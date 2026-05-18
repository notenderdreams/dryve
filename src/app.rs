use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dryve", version, about = "Drive sync tool")]
pub struct Cli {
    /// Number of threads to use for syncing (default: 8)
    #[arg(short = 'j', long, default_value_t = 8, global = true)]
    pub threads: usize,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Pull content from a drive
    #[command(visible_alias = "p")]
    Pull {
        /// The URL to pull content from
        url: String,
    },
    /// Synchronize local files with the drive
    #[command(visible_alias = "s")]
    Sync {
        /// Optional folder path containing dryve.json
        path: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let threads = cli.threads;

    match &cli.command {
        Commands::Pull { url } => {
            crate::cmd_pull::run(url, threads)?;
        }
        Commands::Sync { path } => {
            crate::cmd_sync::run(threads, path.as_deref())?;
        }
    }

    Ok(())
}
