use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dryve", version, about = "Drive sync tool")]
pub struct Cli {
    /// Number of threads to use for syncing (default: 8)
    #[arg(short = 'j', long, default_value_t = 8, global = true)]
    pub threads: usize,

    /// Automatically accept default selection without interactive prompt
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

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

        /// Include ignored files in sync
        #[arg(short = 'I', long)]
        include_ignored: bool,
    },
    /// Open the drive link in browser
    #[command(visible_alias = "o")]
    Open {
        /// Optional folder path containing dryve.json
        path: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let threads = cli.threads;
    let yes = cli.yes;

    match &cli.command {
        Commands::Pull { url } => {
            crate::cmd_pull::run(url, threads, yes)?;
        }
        Commands::Sync {
            path,
            include_ignored,
        } => {
            crate::cmd_sync::run(threads, path.as_deref(), *include_ignored, yes)?;
        }
        Commands::Open { path } => {
            crate::cmd_open::run(path.as_deref())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_yes_flag_global_before_subcommand() {
        let cli = Cli::try_parse_from(["dryve", "-y", "pull", "https://example.com"]).unwrap();
        assert!(cli.yes);

        let cli = Cli::try_parse_from(["dryve", "--yes", "sync"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn test_cli_yes_flag_global_after_subcommand() {
        let cli = Cli::try_parse_from(["dryve", "pull", "-y", "https://example.com"]).unwrap();
        assert!(cli.yes);

        let cli = Cli::try_parse_from(["dryve", "pull", "https://example.com", "--yes"]).unwrap();
        assert!(cli.yes);

        let cli = Cli::try_parse_from(["dryve", "sync", "-y"]).unwrap();
        assert!(cli.yes);
    }

    #[test]
    fn test_cli_default_yes_is_false() {
        let cli = Cli::try_parse_from(["dryve", "pull", "https://example.com"]).unwrap();
        assert!(!cli.yes);

        let cli = Cli::try_parse_from(["dryve", "sync"]).unwrap();
        assert!(!cli.yes);
    }
}
