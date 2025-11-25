use clap::Parser;
use std::path::PathBuf;
use std::process::Command;
use tracing_subscriber::filter::EnvFilter;

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

    // Initialize logging
    let env_filter = if args.verbose {
        EnvFilter::from_default_env()
            .add_directive(tracing_subscriber::filter::LevelFilter::DEBUG.into())
    } else {
        EnvFilter::from_default_env()
            .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();

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
        std::env::set_var("HTTP_PROXY", format!("http://127.0.0.1:{}", args.http_port));
        std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{}", args.http_port));
        std::env::set_var("SOCKS5_SERVER", format!("127.0.0.1:{}", args.socks_port));

        // Execute the target command
        let status = execute_command(&args.target_command)?;

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
fn execute_command(cmd_parts: &[String]) -> anyhow::Result<std::process::ExitStatus> {
    if cmd_parts.is_empty() {
        anyhow::bail!("No command provided");
    }

    let mut cmd = Command::new(&cmd_parts[0]);
    if cmd_parts.len() > 1 {
        cmd.args(&cmd_parts[1..]);
    }

    // Inherit stdio from parent so output goes to console
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    // Spawn the child process and wait for it
    let mut child = cmd.spawn()?;
    let status = child.wait()?;

    Ok(status)
}

async fn run_socks5_server(host: &str, port: u16, _uds_path: &PathBuf) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port)
        .parse::<std::net::SocketAddr>()?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("SOCKS5 server listening on {}", addr);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        tracing::debug!("SOCKS5 client connected: {}", peer_addr);

        // TODO: Implement SOCKS5 protocol handling
        // For Phase 2, just accept and close
        drop(socket);
    }
}

async fn run_http_server(host: &str, port: u16, _uds_path: &PathBuf) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port)
        .parse::<std::net::SocketAddr>()?;

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP CONNECT server listening on {}", addr);

    loop {
        let (socket, peer_addr) = listener.accept().await?;
        tracing::debug!("HTTP CONNECT client connected: {}", peer_addr);

        // TODO: Implement HTTP CONNECT protocol handling
        // For Phase 2, just accept and close
        drop(socket);
    }
}
