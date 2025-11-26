//! HTTP CONNECT protocol implementation for transparent proxy tunneling

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Parse an HTTP CONNECT request
///
/// Format: CONNECT host:port HTTP/1.1\r\n\r\n
pub async fn parse_connect_request(
    stream: &mut TcpStream,
) -> anyhow::Result<(String, u16)> {
    let mut buffer = vec![0u8; 1024];
    let n = stream.read(&mut buffer).await?;

    if n == 0 {
        anyhow::bail!("Connection closed before request received");
    }

    let request = String::from_utf8_lossy(&buffer[..n]);
    parse_connect_line(&request)
}

/// Parse the CONNECT request line (e.g., "CONNECT example.com:443 HTTP/1.1\r\n")
fn parse_connect_line(request: &str) -> anyhow::Result<(String, u16)> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty request"))?;

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("Invalid CONNECT request format");
    }

    if parts[0] != "CONNECT" {
        anyhow::bail!("Expected CONNECT method, got {}", parts[0]);
    }

    let host_port = parts[1];
    let (host, port_str) = host_port
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid host:port format"))?;

    let port: u16 = port_str.parse().map_err(|_| {
        anyhow::anyhow!("Invalid port number: {}", port_str)
    })?;

    Ok((host.to_string(), port))
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
