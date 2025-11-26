use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

mod http_connect;

#[derive(Parser, Debug)]
#[command(name = "bw-relay")]
#[command(about = "Network relay for bw-claude sandbox - bridges localhost proxies to UDS")]
struct Args {
    /// SOCKS5 listening port
    #[arg(long, default_value = "1080")]
    socks_port: u16,

    /// HTTP CONNECT listening port
    #[arg(long, default_value = "3128")]
    http_port: u16,

    /// Unix domain socket path to connect to
    #[arg(long)]
    socket: PathBuf,

    /// Enable debug logging
    #[arg(long, short = 'v')]
    verbose: bool,

    /// Target command and arguments (use -- to separate from bw-relay options)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    target_command: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging - only if BW_LOG env var or verbose flag
    let _ = if args.verbose || std::env::var("BW_LOG").is_ok() {
        let filter = std::env::var("BW_LOG").unwrap_or_else(|_| "debug".to_string());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init()
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .with_writer(std::io::stderr)
            .try_init()
    };

    tracing::info!(
        "Starting bw-relay: SOCKS5 on :{}, HTTP on :{}, UDS at {:?}",
        args.socks_port,
        args.http_port,
        args.socket
    );

    // Spawn SOCKS5 server
    let uds_path_socks = args.socket.clone();
    let socks_handle = tokio::spawn(async move {
        run_socks5_server("127.0.0.1", args.socks_port, &uds_path_socks).await
    });

    // Spawn HTTP server
    let uds_path_http = args.socket.clone();
    let http_handle = tokio::spawn(async move {
        run_http_server("127.0.0.1", args.http_port, &uds_path_http).await
    });

    // If a target command is provided, execute it after a brief delay to allow servers to start
    if !args.target_command.is_empty() {
        // Wait a bit for servers to bind and start listening
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        tracing::info!("Executing target command: {:?}", args.target_command);

        // Set up proxy environment variables
        let http_proxy = format!("http://127.0.0.1:{}", args.http_port);
        let socks_server = format!("127.0.0.1:{}", args.socks_port);

        // Execute the target command with proxy env vars
        let status = execute_command(&args.target_command, &http_proxy, &socks_server)?;

        // Drop the handles to stop the servers
        socks_handle.abort();
        http_handle.abort();

        std::process::exit(status.code().unwrap_or(1));
    }

    // If no target command, wait for servers to run forever
    tokio::select! {
        res = socks_handle => {
            res??
        },
        res = http_handle => {
            res??
        },
    };

    Ok(())
}

/// Execute a target command and wait for it to complete
///
/// The child process inherits the parent's signal handlers, so signals
/// (SIGTERM, SIGINT, etc.) will be delivered to both parent and child.
/// The child's exit status is propagated back to the caller.
///
/// Sets proxy environment variables for the child process via Command builder.
fn execute_command(cmd_parts: &[String], http_proxy: &str, socks_server: &str) -> anyhow::Result<std::process::ExitStatus> {
    if cmd_parts.is_empty() {
        anyhow::bail!("No command provided");
    }

    let mut cmd = Command::new(&cmd_parts[0]);
    if cmd_parts.len() > 1 {
        cmd.args(&cmd_parts[1..]);
    }

    // Set proxy environment variables for the child process
    cmd.env("HTTP_PROXY", http_proxy);
    cmd.env("http_proxy", http_proxy);
    cmd.env("HTTPS_PROXY", http_proxy);
    cmd.env("https_proxy", http_proxy);
    cmd.env("SOCKS5_SERVER", socks_server);
    cmd.env("socks5_server", socks_server);

    // Inherit stdio from parent so output goes to console
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    // Spawn the child process and wait for it
    let mut child = cmd.spawn()?;
    let status = child.wait()?;

    Ok(status)
}

async fn run_socks5_server(host: &str, port: u16, uds_path: &PathBuf) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port)
        .parse::<std::net::SocketAddr>()?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("SOCKS5 server listening on {}", addr);

    let uds_path = uds_path.clone();
    loop {
        let (socket, peer_addr) = listener.accept().await?;
        tracing::debug!("SOCKS5 client connected: {}", peer_addr);

        let uds_path = uds_path.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_socks5_client(socket, &uds_path).await {
                tracing::warn!("Error handling SOCKS5 client {}: {}", peer_addr, e);
            }
        });
    }
}

async fn handle_socks5_client(mut client: tokio::net::TcpStream, uds_path: &PathBuf) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Read SOCKS5 greeting
    let mut buf = [0u8; 256];
    let n = client.read(&mut buf).await?;

    if n < 2 || buf[0] != 5 {
        anyhow::bail!("Invalid SOCKS5 greeting");
    }

    // Send back "no authentication required" (method 0)
    client.write_all(&[5, 0]).await?;

    // Read CONNECT request
    let n = client.read(&mut buf).await?;
    if n < 4 || buf[0] != 5 || buf[1] != 1 {
        client.write_all(&[5, 1]).await?; // General SOCKS server failure
        anyhow::bail!("Invalid SOCKS5 request");
    }

    // Parse address
    let addr_type = buf[3];
    let (host, port_offset) = match addr_type {
        1 => {
            // IPv4
            if n < 10 {
                anyhow::bail!("Invalid IPv4 address");
            }
            let host = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
            (host, 8)
        }
        3 => {
            // Domain name
            let len = buf[4] as usize;
            if n < 5 + len + 2 {
                anyhow::bail!("Invalid domain name");
            }
            let host = String::from_utf8(buf[5..5 + len].to_vec())?;
            (host, 5 + len)
        }
        4 => {
            // IPv6
            if n < 22 {
                anyhow::bail!("Invalid IPv6 address");
            }
            let host = format!(
                "{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}",
                buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11],
                buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19]
            );
            (host, 20)
        }
        _ => {
            client.write_all(&[5, 8]).await?; // Address type not supported
            anyhow::bail!("Unsupported address type");
        }
    };

    let port = u16::from_be_bytes([buf[port_offset], buf[port_offset + 1]]);

    tracing::info!("SOCKS5 CONNECT request: {}:{}", host, port);

    // Connect to bw-proxy via UDS and forward request
    match forward_to_proxy(&mut client, uds_path, &host, port).await {
        Ok(_) => {
            // Proxy handling took over - connection is now tunneling
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Failed to forward to proxy: {}", e);
            client.write_all(&[5, 1]).await?; // General SOCKS server failure
            Err(e)
        }
    }
}

async fn forward_to_proxy(
    client: &mut tokio::net::TcpStream,
    uds_path: &PathBuf,
    host: &str,
    port: u16,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    // Connect to bw-proxy
    let mut proxy = UnixStream::connect(uds_path).await?;

    // Forward the SOCKS5 CONNECT request to proxy
    // For now, use a simple format: "CONNECT host port\n"
    // TODO: Use proper SOCKS5 protocol to bw-proxy

    let request = format!("CONNECT {} {}\n", host, port);
    proxy.write_all(request.as_bytes()).await?;
    proxy.flush().await?;

    // Read response from proxy
    let mut response = [0u8; 256];
    let n = proxy.read(&mut response).await?;

    if n == 0 {
        anyhow::bail!("No response from proxy");
    }

    let response_str = String::from_utf8_lossy(&response[..n]);
    if response_str.starts_with("OK") {
        // Send SOCKS5 success response to client
        let mut reply = vec![5, 0, 0, 1]; // version, success, reserved, IPv4
        reply.extend_from_slice(&[127, 0, 0, 1]); // 127.0.0.1
        reply.extend_from_slice(&port.to_be_bytes());
        client.write_all(&reply).await?;

        // TODO: Implement bidirectional tunneling between client and proxy
        // For now, just keep connection open
        Ok(())
    } else {
        anyhow::bail!("Proxy connection failed: {}", response_str);
    }
}

async fn run_http_server(host: &str, port: u16, uds_path: &PathBuf) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port)
        .parse::<std::net::SocketAddr>()?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP CONNECT server listening on {}", addr);

    let uds_path = uds_path.clone();
    loop {
        let (socket, peer_addr) = listener.accept().await?;
        tracing::debug!("HTTP CONNECT client connected: {}", peer_addr);

        let uds_path = uds_path.clone();
        // Spawn a task to handle this connection
        tokio::spawn(async move {
            if let Err(e) = handle_http_client(socket, &uds_path).await {
                tracing::warn!("Error handling HTTP client {}: {}", peer_addr, e);
            }
        });
    }
}

/// Handle an HTTP CONNECT client connection
async fn handle_http_client(mut client: tokio::net::TcpStream, uds_path: &PathBuf) -> anyhow::Result<()> {
    // Parse the CONNECT request
    let (host, port) = http_connect::parse_connect_request(&mut client).await?;

    tracing::info!("HTTP CONNECT request to {}:{}", host, port);

    // Forward to bw-proxy via SOCKS5 over UDS
    match forward_to_proxy_socks5(&mut client, uds_path, &host, port).await {
        Ok(_) => {
            tracing::debug!("Tunnel closed for {}:{}", host, port);
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Failed to forward to proxy: {}", e);
            http_connect::send_error_response(&mut client, 502, "Bad Gateway").await?;
            Err(e)
        }
    }
}

/// Forward HTTP CONNECT request to bw-proxy via SOCKS5 over UDS
async fn forward_to_proxy_socks5(
    client: &mut tokio::net::TcpStream,
    uds_path: &PathBuf,
    host: &str,
    port: u16,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    // Connect to bw-proxy via UDS
    tracing::debug!("Connecting to proxy at {:?}", uds_path);
    let mut proxy = UnixStream::connect(uds_path).await?;
    tracing::debug!("Connected to proxy via UDS");

    // Send SOCKS5 CONNECT request over UDS to proxy
    // Format: SOCKS5 greeting, then CONNECT request
    // For simplicity, use a text-based protocol: "CONNECT host port\n"
    // TODO: Use proper fast-socks5 SOCKS5 protocol

    let request = format!("CONNECT {} {}\n", host, port);
    tracing::debug!("Sending request to proxy: {:?}", request);
    proxy.write_all(request.as_bytes()).await?;
    proxy.flush().await?;
    tracing::debug!("Request sent to proxy");

    // Read response from proxy
    let mut response = [0u8; 256];
    tracing::debug!("Waiting for response from proxy");
    let n = proxy.read(&mut response).await?;
    tracing::debug!("Received {} bytes from proxy", n);

    if n == 0 {
        anyhow::bail!("No response from proxy");
    }

    let response_str = String::from_utf8_lossy(&response[..n]);
    tracing::debug!("Proxy response: {:?}", response_str);
    if response_str.starts_with("OK") {
        // Send HTTP 200 Connection Established to client
        http_connect::send_connect_success(client).await?;

        // Tunnel data bidirectionally between client and proxy
        tracing::debug!("Starting tunnel between client and proxy");
        tokio::io::copy_bidirectional(&mut *client, &mut proxy).await?;
        tracing::debug!("Tunnel closed");

        Ok(())
    } else {
        anyhow::bail!("Proxy connection failed: {}", response_str);
    }
}
