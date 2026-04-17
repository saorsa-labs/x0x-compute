use std::path::PathBuf;

use clap::{Parser, Subcommand};
use x0x_compute::{CapabilityAnnouncement, ComputeConfig, ComputeDaemon};

#[derive(Debug, Parser)]
#[command(author, version, about = "Trusted-friends compute mesh built on x0x")]
struct Cli {
    /// Optional path to a config file.
    #[arg(long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the local x0x-compute daemon.
    Start,
    /// Check that configuration loads and x0x identity can be resolved.
    Check,
    /// Print the x0x identity snapshot reused by x0x-compute.
    Identity,
    /// Print the local capability snapshot.
    Capability,
    /// Print the active configuration or write a default config to disk.
    PrintConfig {
        /// Write the default config to disk before printing it.
        #[arg(long)]
        write_default: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Start);
    let config = ComputeConfig::load_or_default(cli.config.as_deref())?;

    match command {
        Command::Start => {
            let daemon = ComputeDaemon::from_config(config).await?;
            daemon.run().await?;
        }
        Command::Check => {
            let agent = x0x_compute::build_agent(&config).await?;
            let identity = x0x_compute::AgentIdentitySnapshot::from_agent(&agent);
            println!("configuration: ok");
            println!("agent_id: {}", identity.agent_id);
            println!("machine_id: {}", identity.machine_id);
            println!(
                "user_id: {}",
                identity.user_id.unwrap_or_else(|| "<none>".to_string())
            );
        }
        Command::Identity => {
            let agent = x0x_compute::build_agent(&config).await?;
            let identity = x0x_compute::AgentIdentitySnapshot::from_agent(&agent);
            println!("{}", serde_json::to_string_pretty(&identity)?);
        }
        Command::Capability => {
            let agent = x0x_compute::build_agent(&config).await?;
            let identity = x0x_compute::AgentIdentitySnapshot::from_agent(&agent);
            let capability = CapabilityAnnouncement::local(&config, identity);
            println!("{}", serde_json::to_string_pretty(&capability)?);
        }
        Command::PrintConfig { write_default } => {
            if write_default {
                let path = ComputeConfig::write_default(cli.config.as_deref())?;
                eprintln!("wrote default config to {}", path.display());
            }
            println!("{}", toml::to_string_pretty(&config)?);
        }
    }

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
