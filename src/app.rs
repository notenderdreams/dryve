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

    /// Include files matching pattern (e.g. "*.pdf", "docs/*")
    #[arg(long, value_name = "PATTERN", global = true)]
    pub include: Vec<String>,

    /// Exclude files matching pattern (e.g. "*.tmp", "archive/*")
    #[arg(long, value_name = "PATTERN", global = true)]
    pub exclude: Vec<String>,

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
    /// Show sync status and pending remote changes
    #[command(visible_alias = "st")]
    Status {
        /// Optional folder path containing dryve.json
        path: Option<String>,

        /// Include ignored files in status
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
    let include = &cli.include;
    let exclude = &cli.exclude;

    match &cli.command {
        Commands::Pull { url } => {
            crate::cmd_pull::run(url, threads, yes, include, exclude)?;
        }
        Commands::Sync {
            path,
            include_ignored,
        } => {
            crate::cmd_sync::run(
                threads,
                path.as_deref(),
                *include_ignored,
                yes,
                include,
                exclude,
            )?;
        }
        Commands::Status {
            path,
            include_ignored,
        } => {
            crate::cmd_status::run(path.as_deref(), *include_ignored, include, exclude)?;
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

    #[test]
    fn test_cli_status_subcommand() {
        let cli = Cli::try_parse_from(["dryve", "status"]).unwrap();
        match cli.command {
            Commands::Status {
                path,
                include_ignored,
            } => {
                assert_eq!(path, None);
                assert!(!include_ignored);
            }
            _ => panic!("expected Status command"),
        }

        let cli = Cli::try_parse_from(["dryve", "st", "my_folder", "-I"]).unwrap();
        match cli.command {
            Commands::Status {
                path,
                include_ignored,
            } => {
                assert_eq!(path, Some("my_folder".to_string()));
                assert!(include_ignored);
            }
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn test_cli_include_exclude_flags() {
        let cli = Cli::try_parse_from([
            "dryve",
            "pull",
            "https://example.com",
            "--include",
            "*.pdf",
            "--include",
            "docs/*",
            "--exclude",
            "*.tmp",
        ])
        .unwrap();

        assert_eq!(cli.include, vec!["*.pdf".to_string(), "docs/*".to_string()]);
        assert_eq!(cli.exclude, vec!["*.tmp".to_string()]);
    }
}
