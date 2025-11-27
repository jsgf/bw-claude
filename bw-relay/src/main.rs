use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

mod http_connect;

#[derive(Parser, Debug)]
#[command(name = "bw-relay")]
#[command(about = "HTTP relay for bw-claude sandbox - forwards HTTP/HTTPS to policy proxy via UDS")]
struct Args {
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
        "Starting bw-relay: HTTP on :{}, UDS at {:?}",
        args.http_port,
        args.socket
    );

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

        // Execute the target command with proxy env vars
        let status = execute_command(&args.target_command, &http_proxy)?;

        // Drop the handle to stop the server
        http_handle.abort();

        std::process::exit(status.code().unwrap_or(1));
    }

    // If no target command, wait for the server to run forever
    http_handle.await??;

    Ok(())
}

/// Execute a target command and wait for it to complete
///
/// The child process inherits the parent's signal handlers, so signals
/// (SIGTERM, SIGINT, etc.) will be delivered to both parent and child.
/// The child's exit status is propagated back to the caller.
///
/// Sets HTTP proxy environment variables for the child process via Command builder.
fn execute_command(cmd_parts: &[String], http_proxy: &str) -> anyhow::Result<std::process::ExitStatus> {
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

    // Inherit stdio from parent so output goes to console
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    // Spawn the child process and wait for it
    let mut child = cmd.spawn()?;
    let status = child.wait()?;

    Ok(status)
}

async fn run_http_server(host: &str, port: u16, uds_path: &PathBuf) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port)
        .parse::<std::net::SocketAddr>()?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP proxy listening on {addr}");

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

/// Handle an HTTP client connection
async fn handle_http_client(mut client: tokio::net::TcpStream, uds_path: &PathBuf) -> anyhow::Result<()> {
    // Parse the request
    let (req_type, _req_bytes) = http_connect::parse_connect_request(&mut client).await?;

    // Forward to bw-proxy via UDS
    match forward_to_proxy(&mut client, uds_path, req_type).await {
        Ok(_) => {
            tracing::debug!("Request handled");
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Failed to forward to proxy: {e}");
            http_connect::send_error_response(&mut client, 502, "Bad Gateway").await?;
            Err(e)
        }
    }
}

/// Forward HTTP request to bw-proxy via UDS
async fn forward_to_proxy(
    client: &mut tokio::net::TcpStream,
    uds_path: &PathBuf,
    req_type: http_connect::RequestType,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    // Connect to bw-proxy via UDS
    tracing::debug!("Connecting to proxy at {uds_path:?}");
    let mut proxy = UnixStream::connect(uds_path).await?;
    tracing::debug!("Connected to proxy via UDS");

    match req_type {
        http_connect::RequestType::Connect { host, port } => {
            // HTTPS tunneling via CONNECT method
            let request = format!("CONNECT {host} {port}\n");
            tracing::debug!("Sending CONNECT request to proxy: {request:?}");
            proxy.write_all(request.as_bytes()).await?;
            proxy.flush().await?;

            // Read response from proxy
            let mut response = [0u8; 256];
            let n = proxy.read(&mut response).await?;

            if n == 0 {
                anyhow::bail!("No response from proxy");
            }

            let response_str = String::from_utf8_lossy(&response[..n]);
            tracing::debug!("Proxy response: {response_str:?}");
            if response_str.starts_with("OK") {
                // Send HTTP 200 Connection Established to client
                http_connect::send_connect_success(client).await?;

                // Tunnel data bidirectionally between client and proxy
                tracing::debug!("Starting CONNECT tunnel between client and proxy");
                tokio::io::copy_bidirectional(&mut *client, &mut proxy).await?;
                tracing::debug!("Tunnel closed");

                Ok(())
            } else {
                anyhow::bail!("Proxy rejected CONNECT: {response_str}");
            }
        }
        http_connect::RequestType::Forward { host, port, request } => {
            // HTTP forward proxy - same protocol as CONNECT, just different origin
            let proxy_request = format!("CONNECT {host} {port}\n");
            tracing::debug!("Sending CONNECT request to proxy for HTTP: {proxy_request:?}");
            proxy.write_all(proxy_request.as_bytes()).await?;
            proxy.flush().await?;

            // Read response from proxy
            let mut response = [0u8; 256];
            let n = proxy.read(&mut response).await?;

            if n == 0 {
                anyhow::bail!("No response from proxy");
            }

            let response_str = String::from_utf8_lossy(&response[..n]);
            tracing::debug!("Proxy response: {response_str:?}");
            if response_str.starts_with("OK") {
                // For HTTP forward proxy, send the original request then tunnel
                proxy.write_all(&request).await?;
                proxy.flush().await?;

                // Tunnel remaining data bidirectionally between client and proxy
                tracing::debug!("Starting HTTP forward tunnel between client and proxy");
                tokio::io::copy_bidirectional(&mut *client, &mut proxy).await?;
                tracing::debug!("Tunnel closed");

                Ok(())
            } else {
                anyhow::bail!("Proxy rejected CONNECT for HTTP: {response_str}");
            }
        }
    }
}
