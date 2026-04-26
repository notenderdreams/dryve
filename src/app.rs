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
            println!("Pulling content from: {}", url);
            let html = crate::drive::fetch_drive(url)?;
            let root = crate::drive::parse_html(&html)?;

            root.print();
        }
        Commands::Sync => {
            println!("Syncing with drive...");
        }
    }

    Ok(())
}
