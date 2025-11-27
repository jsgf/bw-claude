//! HTTP CONNECT protocol implementation for transparent proxy tunneling

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use url::Url;

/// Request type: either CONNECT (HTTPS tunneling) or Forward (HTTP)
#[derive(Debug, Clone)]
pub enum RequestType {
    /// CONNECT method for HTTPS tunneling
    Connect { host: String, port: u16 },
    /// Regular HTTP request to be forwarded
    Forward { host: String, port: u16 },
}

/// Parse HTTP request from a stream
///
/// Handles two formats:
/// - CONNECT method (HTTPS tunneling): CONNECT host:port HTTP/1.1\r\n\r\n
/// - Regular HTTP method (HTTP requests): GET http://host:port/path HTTP/1.1\r\nHost: host\r\n\r\n
///
/// Returns: (RequestType, header_bytes, buffered_extra_bytes, stream)
/// where buffered_extra_bytes contains any data read beyond the headers
pub async fn parse_connect_request(
    stream: TcpStream,
) -> anyhow::Result<(RequestType, Vec<u8>, Vec<u8>, TcpStream)> {
    // Wrap stream in buffered reader (16KB buffer for large headers)
    let mut reader = BufReader::with_capacity(16384, stream);
    let mut headers = Vec::new();

    // Read headers until we find \r\n\r\n (end of headers)
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).await?;

        if n == 0 {
            anyhow::bail!("Connection closed before request received");
        }

        headers.extend_from_slice(&line);

        // Check if we've reached the end of headers
        if headers.ends_with(b"\r\n\r\n") {
            break;
        }

        // Safety limit: 16KB for headers
        if headers.len() > 16384 {
            anyhow::bail!("HTTP headers too large (>16KB)");
        }
    }

    // Parse headers to extract request type
    let headers_str = String::from_utf8_lossy(&headers);
    let req_type = parse_request_line(&headers_str)?;

    // Extract any buffered data beyond headers (pipelined data)
    let buffered_extra = reader.buffer().to_vec();

    // Unwrap the stream to get it back for tunneling
    let stream = reader.into_inner();

    Ok((req_type, headers, buffered_extra, stream))
}

/// Parse either a CONNECT request or a regular HTTP request
fn parse_request_line(request: &str) -> anyhow::Result<RequestType> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty request"))?;

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid request format");
    }

    match parts[0] {
        "CONNECT" => {
            // CONNECT method: CONNECT host:port HTTP/1.1
            let host_port = parts[1];
            let (host, port_str) = host_port
                .rsplit_once(':')
                .ok_or_else(|| anyhow::anyhow!("Invalid host:port format"))?;

            let port: u16 = port_str.parse().map_err(|_| {
                anyhow::anyhow!("Invalid port number: {port_str}")
            })?;

            Ok(RequestType::Connect {
                host: host.to_string(),
                port,
            })
        }
        _ => {
            // Regular HTTP method (GET, POST, etc.)
            let (host, port) = parse_http_request(request)?;
            Ok(RequestType::Forward { host, port })
        }
    }
}

/// Parse a regular HTTP request (GET, POST, etc.)
/// Extracts host and port from either the request URL or Host header
fn parse_http_request(request: &str) -> anyhow::Result<(String, u16)> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty request"))?;

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid HTTP request format");
    }

    let url_str = parts[1];

    // Try to parse as absolute URL (http://host:port/path or https://host:port/path)
    if let Ok(url) = Url::parse(url_str) {
        let host = url.host_str().ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
        let port = url.port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("Unknown URL scheme: {}", url.scheme()))?;
        return Ok((host.to_string(), port));
    }

    // URL is in origin-form (just path) - extract from Host header
    for line in request.lines().skip(1) {
        if line.starts_with("Host:") || line.starts_with("host:") {
            let host_value = line[5..].trim();
            // Host header might be "host:port" or just "host"
            if let Some((host, port_str)) = host_value.rsplit_once(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Ok((host.to_string(), port));
                }
            }
            // No port specified - use default port 80
            return Ok((host_value.to_string(), 80));
        }
    }

    anyhow::bail!("Cannot determine host and port from HTTP request")
}

/// Send HTTP 200 response to indicate successful CONNECT
pub async fn send_connect_success(stream: &mut TcpStream) -> anyhow::Result<()> {
    let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Send HTTP error response
pub async fn send_error_response(
    stream: &mut TcpStream,
    status: u16,
    message: &str,
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {message}\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connect_request() {
        let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let req_type = parse_request_line(request).unwrap();
        match req_type {
            RequestType::Connect { host, port } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected Connect request type"),
        }
    }

    #[test]
    fn test_parse_connect_with_whitespace() {
        let request = "CONNECT  example.com:8443  HTTP/1.1\r\n\r\n";
        let req_type = parse_request_line(request).unwrap();
        match req_type {
            RequestType::Connect { host, port } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 8443);
            }
            _ => panic!("Expected Connect request type"),
        }
    }

    #[test]
    fn test_parse_connect_invalid_port() {
        let request = "CONNECT example.com:invalid HTTP/1.1\r\n\r\n";
        let result = parse_request_line(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_connect_no_port() {
        let request = "CONNECT example.com HTTP/1.1\r\n\r\n";
        let result = parse_request_line(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_http_request() {
        let request = "GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let req_type = parse_request_line(request).unwrap();
        match req_type {
            RequestType::Forward { host, port } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 80);
            }
            _ => panic!("Expected Forward request type"),
        }
    }

    #[test]
    fn test_large_headers_parsing() {
        // Test that we handle large headers correctly
        let mut large_request = String::from("CONNECT example.com:443 HTTP/1.1\r\n");
        // Add many headers
        for i in 0..100 {
            large_request.push_str(&format!("X-Custom-Header-{i}: value\r\n"));
        }
        large_request.push_str("\r\n");

        let req_type = parse_request_line(&large_request).unwrap();
        match req_type {
            RequestType::Connect { host, port } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 443);
            }
            _ => panic!("Expected Connect request type"),
        }
    }
}
