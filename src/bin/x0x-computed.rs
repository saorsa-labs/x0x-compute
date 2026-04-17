use std::path::PathBuf;

use clap::Parser;
use x0x_compute::{ComputeConfig, ComputeDaemon};

#[derive(Debug, Parser)]
#[command(author, version, about = "x0x-compute daemon")]
struct Cli {
    /// Optional path to a config file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Validate config and identity, then exit.
    #[arg(long)]
    check: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let cli = Cli::parse();
    let config = ComputeConfig::load_or_default(cli.config.as_deref())?;

    if cli.check {
        let agent = x0x_compute::build_agent(&config).await?;
        let identity = x0x_compute::AgentIdentitySnapshot::from_agent(&agent);
        println!("daemon configuration ok");
        println!("agent_id: {}", identity.agent_id);
        println!("machine_id: {}", identity.machine_id);
        return Ok(());
    }

    let daemon = ComputeDaemon::from_config(config).await?;
    daemon.run().await?;
    Ok(())
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}
