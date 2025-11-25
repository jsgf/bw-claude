use bwrap_proxy::{
    ConfigLoader, LearningRecorder, PolicyEngine, ProxyServer, ProxyServerConfig,
};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::filter::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "bwrap-proxy")]
#[command(about = "SOCKS5 proxy with network filtering for bw-claude sandbox")]
struct Args {
    /// Unix domain socket path to listen on
    #[arg(long, short = 's')]
    socket: PathBuf,

    /// Proxy mode: open | learning | restrictive:<policy>
    #[arg(long, short = 'm', default_value = "restrictive:default")]
    mode: String,

    /// Config file path
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Enable debug logging
    #[arg(long, short = 'v')]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Load configuration
    let config = ConfigLoader::load_or_default(args.config)?;

    // Parse proxy mode
    let (policy_engine, learning_recorder) = if args.mode == "open" {
        (None, None)
    } else if args.mode == "learning" {
        (None, Some(Arc::new(LearningRecorder::new())))
    } else if args.mode.starts_with("restrictive:") {
        let policy_name = args.mode.strip_prefix("restrictive:").unwrap_or("default");
        let engine = Arc::new(PolicyEngine::from_policy(
            policy_name,
            &config.network,
        )?);
        (Some(engine), None)
    } else {
        return Err(format!(
            "Invalid proxy mode: {}. Use 'open', 'learning', or 'restrictive:<policy>'",
            args.mode
        ).into());
    };

    // Create server configuration
    let server_config = ProxyServerConfig {
        socket_path: args.socket,
        network_config: Arc::new(config.network),
        policy_engine,
        learning_recorder,
    };

    // Start the proxy server
    let server = ProxyServer::new(server_config);
    server.start().await.map_err(|e| e.into())
}
