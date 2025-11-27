//! HTTP CONNECT protocol implementation for transparent proxy tunneling

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use url::Url;

/// Request type: either CONNECT tunneling (HTTPS) or forward proxy (HTTP)
#[derive(Debug, Clone)]
pub enum RequestType {
    /// CONNECT method for HTTPS tunneling
    Connect { host: String, port: u16 },
    /// Regular HTTP request to be forwarded
    Forward { host: String, port: u16, request: Vec<u8> },
}

/// Convert HTTP request from absolute-form (http://host/path) to origin-form (/path)
/// This is needed when forwarding through a tunnel instead of a forward proxy
fn convert_to_origin_form(request: &[u8]) -> Vec<u8> {
    let request_str = String::from_utf8_lossy(request);
    let lines: Vec<&str> = request_str.lines().collect();

    if lines.is_empty() {
        return request.to_vec();
    }

    let first_line = lines[0];
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.len() < 3 {
        return request.to_vec();
    }

    let method = parts[0];
    let url = parts[1];
    let http_version = parts[2];

    // Convert absolute URL to origin-form
    let origin_path = if url.starts_with("http://") {
        // Extract just the path from the URL
        if let Ok(parsed_url) = Url::parse(url) {
            let path = parsed_url.path();
            let query = parsed_url.query().unwrap_or("");
            if query.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{query}")
            }
        } else {
            url.to_string()
        }
    } else if url.starts_with("https://") {
        // HTTPS absolute form - extract path
        if let Ok(parsed_url) = Url::parse(url) {
            let path = parsed_url.path();
            let query = parsed_url.query().unwrap_or("");
            if query.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{query}")
            }
        } else {
            url.to_string()
        }
    } else {
        // Already in origin-form
        url.to_string()
    };

    // Rebuild the request with origin-form URL
    let mut result = format!("{method} {origin_path} {http_version}\r\n");

    // Add the rest of the headers
    for line in &lines[1..] {
        result.push_str(line);
        result.push_str("\r\n");
    }

    result.into_bytes()
}

/// Parse an HTTP CONNECT request or regular HTTP request
///
/// Handles two formats:
/// - CONNECT method (HTTPS tunneling): CONNECT host:port HTTP/1.1\r\n\r\n
/// - Regular HTTP method (HTTP requests): GET http://host:port/path HTTP/1.1\r\nHost: host\r\n\r\n
pub async fn parse_connect_request(
    stream: &mut TcpStream,
) -> anyhow::Result<(RequestType, Vec<u8>)> {
    let mut buffer = vec![0u8; 1024];
    let n = stream.read(&mut buffer).await?;

    if n == 0 {
        anyhow::bail!("Connection closed before request received");
    }

    let request_bytes = buffer[..n].to_vec();
    let request = String::from_utf8_lossy(&request_bytes);

    let req_type = parse_request_line(&request)?;
    Ok((req_type, request_bytes))
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
            Ok(RequestType::Forward {
                host,
                port,
                request: request.as_bytes().to_vec(),
            })
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
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\n\r\n",
        status, message
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
        let (host, port) = parse_connect_line(request).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_connect_with_whitespace() {
        let request = "CONNECT  example.com:8443  HTTP/1.1\r\n\r\n";
        let (host, port) = parse_connect_line(request).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
    }

    #[test]
    fn test_parse_connect_invalid_port() {
        let request = "CONNECT example.com:invalid HTTP/1.1\r\n\r\n";
        let result = parse_connect_line(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_connect_no_port() {
        let request = "CONNECT example.com HTTP/1.1\r\n\r\n";
        let result = parse_connect_line(request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_connect_wrong_method() {
        let request = "GET example.com:443 HTTP/1.1\r\n\r\n";
        let result = parse_connect_line(request);
        assert!(result.is_err());
    }
}
