use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dryve")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Pull content from a drive
    Pull {
        /// The URL to pull content from
        url: String,
    },
    /// Synchronize local files with the drive
    Sync,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Pull { url } => {
            crate::cmd_pull::run(url)?;
        }
        Commands::Sync => {
            crate::cmd_sync::run()?;
        }
    }

    Ok(())
}
