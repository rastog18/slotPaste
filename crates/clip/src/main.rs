use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::process::Command;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "clip")]
#[command(about = "Clip - macOS clipboard manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the agent in foreground
    Start,
    /// Check system configuration
    Doctor,
    /// Install system hooks
    Install,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start => start_agent()?,
        Commands::Doctor => {
            println!("not implemented yet");
            std::process::exit(0);
        }
        Commands::Install => {
            println!("not implemented yet");
            std::process::exit(0);
        }
    }

    Ok(())
}

fn find_agent_path() -> Result<std::path::PathBuf> {
    if let Ok(path) = which::which("clip-agent") {
        return Ok(path);
    }
    // Fallback: look next to the clip binary (for cargo run / development)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let agent = dir.join("clip-agent");
            if agent.exists() {
                return Ok(agent);
            }
        }
    }
    anyhow::bail!("clip-agent not found");
}

fn start_agent() -> Result<()> {
    info!("Starting clip agent...");

    let agent_path = match find_agent_path() {
        Ok(path) => path,
        Err(_) => {
            error!("clip-agent binary not found");
            eprintln!(
                "Error: clip-agent binary not found in PATH or next to clip binary.\n\
                Suggestions:\n\
                1. Build the agent: cargo build -p clip-agent\n\
                2. Run with cargo: cargo run -p clip -- start\n\
                3. Install both binaries: cargo install --path crates/clip --path crates/clip-agent"
            );
            std::process::exit(1);
        }
    };

    info!("Found agent at: {}", agent_path.display());

    let mut child = Command::new(&agent_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to spawn agent process: {}", agent_path.display()))?;

    ctrlc::set_handler(move || {
        info!("Received interrupt signal, stopping agent...");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    let status = child.wait().context("Failed to wait for agent process")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
