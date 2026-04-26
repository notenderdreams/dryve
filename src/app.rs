use anyhow::Result;
use clap::{Parser, Subcommand};
use crate::drive::fetch_drive;
use std::fs;

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
            let root = fetch_drive(url)?;
            root.print();
            
            // Write root to dryve.json
            let json = serde_json::to_string_pretty(&root)?;
            fs::write("dryve.json", json)?;
            println!("Root structure written to dryve.json");
        }
        Commands::Sync => {
            println!("Syncing with drive...");
        }
    }

    Ok(())
}
