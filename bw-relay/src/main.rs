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

    /// Unix domain socket path to connect to (optional - if not provided, just executes target command)
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Enable debug logging
    #[arg(long, short = 'v')]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Split arguments at `--` separator to separate bw-relay options from target command
    let args_vec: Vec<String> = std::env::args().collect();
    let separator_idx = args_vec.iter().position(|arg| arg == "--");

    let (relay_args, target_command) = if let Some(idx) = separator_idx {
        // Split at `--`: everything before goes to clap, everything after is target_command
        (args_vec[..=idx].to_vec(), args_vec[idx + 1..].to_vec())
    } else {
        // No `--` separator found - check if there are extra arguments
        // If the last arg looks like it might be a target command, require `--`
        (args_vec.clone(), vec![])
    };

    // Parse bw-relay's own arguments
    let args = Args::try_parse_from(&relay_args)?;

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

    // Handle proxy mode (when socket is provided)
    if let Some(ref socket_path) = args.socket {
        tracing::info!(
            "Starting bw-relay: HTTP on :{}, UDS at {:?}",
            args.http_port,
            socket_path
        );

        // Spawn HTTP server for proxy mode
        let uds_path_http = socket_path.clone();
        let http_handle = tokio::spawn(async move {
            run_http_server("127.0.0.1", args.http_port, &uds_path_http).await
        });

        // If a target command is provided, execute it after a brief delay to allow servers to start
        if !target_command.is_empty() {
            // Wait a bit for servers to bind and start listening
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            tracing::info!("Executing target command: {:?}", target_command);

            // Set up proxy environment variables
            let http_proxy = format!("http://127.0.0.1:{}", args.http_port);

            // Execute the target command with proxy env vars
            let status = execute_command(&target_command, &http_proxy)?;

            // Drop the handle to stop the server
            http_handle.abort();

            std::process::exit(status.code().unwrap_or(1));
        }

        // If no target command, wait for the server to run forever
        http_handle.await??;
    } else {
        // Non-proxy mode: just execute the target command if provided
        if !target_command.is_empty() {
            tracing::info!("Executing target command (non-proxy mode): {:?}", target_command);

            // Execute the target command without proxy env vars
            let status = execute_command(&target_command, "")?;
            std::process::exit(status.code().unwrap_or(1));
        } else {
            anyhow::bail!("No target command provided and no socket for proxy mode");
        }
    }

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
        tracing::debug!("HTTP CONNECT client connected: {peer_addr}");

        let uds_path = uds_path.clone();
        // Spawn a task to handle this connection
        tokio::spawn(async move {
            if let Err(e) = handle_http_client(socket, &uds_path).await {
                tracing::warn!("Error handling HTTP client {peer_addr}: {e}");
            }
        });
    }
}

/// Handle an HTTP client connection
async fn handle_http_client(client: tokio::net::TcpStream, uds_path: &PathBuf) -> anyhow::Result<()> {
    // Parse the request (consumes client to extract buffered data)
    let (req_type, headers, buffered_extra, mut client) = http_connect::parse_connect_request(client).await?;

    // Forward to bw-proxy via UDS
    match forward_to_proxy(&mut client, uds_path, req_type, headers, buffered_extra).await {
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
    headers: Vec<u8>,
    buffered_extra: Vec<u8>,
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
            // Send CONNECT to UDS proxy (space-separated format: CONNECT host port)
            let proxy_request = format!("CONNECT {host} {port}\n");
            tracing::debug!("Sending CONNECT to proxy: {proxy_request:?}");
            proxy.write_all(proxy_request.as_bytes()).await?;
            proxy.flush().await?;

            // Read OK/BLOCKED/etc response from proxy
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

                // Write any pipelined data (e.g., TLS handshake) to proxy first
                if !buffered_extra.is_empty() {
                    tracing::debug!("Writing {len} bytes of pipelined data to proxy", len = buffered_extra.len());
                    proxy.write_all(&buffered_extra).await?;
                }

                // Tunnel bidirectionally between client and proxy (unbuffered)
                tracing::debug!("Starting CONNECT tunnel between client and proxy");
                tokio::io::copy_bidirectional(client, &mut proxy).await?;
                tracing::debug!("Tunnel closed");

                Ok(())
            } else {
                anyhow::bail!("Proxy rejected CONNECT: {response_str}");
            }
        }
        http_connect::RequestType::Forward { host, port } => {
            // HTTP forward proxy - use CONNECT to establish tunnel, then forward request
            let proxy_request = format!("CONNECT {host} {port}\n");
            tracing::debug!("Sending CONNECT to proxy for HTTP: {proxy_request:?}");
            proxy.write_all(proxy_request.as_bytes()).await?;
            proxy.flush().await?;

            // Read OK/BLOCKED/etc response from proxy
            let mut response = [0u8; 256];
            let n = proxy.read(&mut response).await?;

            if n == 0 {
                anyhow::bail!("No response from proxy");
            }

            let response_str = String::from_utf8_lossy(&response[..n]);
            tracing::debug!("Proxy response: {response_str:?}");
            if response_str.starts_with("OK") {
                // Forward the entire HTTP request headers to destination
                tracing::debug!("Writing {len} bytes of HTTP headers to proxy", len = headers.len());
                proxy.write_all(&headers).await?;

                // Write any pipelined request body data to proxy
                if !buffered_extra.is_empty() {
                    tracing::debug!("Writing {len} bytes of pipelined body to proxy", len = buffered_extra.len());
                    proxy.write_all(&buffered_extra).await?;
                }

                // Tunnel bidirectionally for response and any remaining data
                tracing::debug!("Starting HTTP forward tunnel between client and proxy");
                tokio::io::copy_bidirectional(client, &mut proxy).await?;
                tracing::debug!("Tunnel closed");

                Ok(())
            } else {
                anyhow::bail!("Proxy rejected CONNECT for HTTP: {response_str}");
            }
        }
    }
}
