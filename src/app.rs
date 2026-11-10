use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dryve", version, about = "Drive sync tool")]
pub struct Cli {
    /// Number of parallel download threads
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

        /// Download directly into this directory (default: remote folder name)
        #[arg(short, long, value_name = "DIR")]
        output: Option<PathBuf>,
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
    /// Check local downloads for missing files and unexpected sizes
    #[command(visible_alias = "ck")]
    Check {
        /// Optional folder path containing dryve.json
        path: Option<PathBuf>,

        /// Download selected missing or damaged files again
        #[arg(long)]
        repair: bool,

        /// Check ignored files too
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
        Commands::Pull { url, output } => {
            crate::cmd_pull::run(url, output.as_deref(), threads, yes, include, exclude)?;
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
        Commands::Check {
            path,
            repair,
            include_ignored,
        } => {
            crate::cmd_check::run(
                path.as_deref(),
                *repair,
                *include_ignored,
                threads,
                yes,
                include,
                exclude,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_command_and_flags() {
        for command in ["check", "ck"] {
            let cli = Cli::try_parse_from([
                "dryve",
                command,
                "./materials",
                "--repair",
                "-y",
                "-I",
                "-j",
                "2",
                "--include",
                "*.pdf",
                "--exclude",
                "archive/*",
            ])
            .unwrap();
            assert!(cli.yes);
            assert_eq!(cli.threads, 2);
            assert_eq!(cli.include, ["*.pdf"]);
            assert_eq!(cli.exclude, ["archive/*"]);
            assert!(matches!(cli.command, Commands::Check {
                path: Some(path), repair: true, include_ignored: true,
            } if path == std::path::Path::new("./materials")));
        }
        let cli = Cli::try_parse_from(["dryve", "check"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Check {
                path: None,
                repair: false,
                include_ignored: false,
            }
        ));
    }

    #[test]
    fn test_pull_output() {
        for flag in ["--output", "-o"] {
            let cli = Cli::try_parse_from([
                "dryve",
                "p",
                "https://example.com",
                flag,
                "./my materials",
                "-y",
            ])
            .unwrap();
            assert!(cli.yes);
            match cli.command {
                Commands::Pull { output, .. } => {
                    assert_eq!(output, Some(PathBuf::from("./my materials")));
                }
                _ => panic!("expected Pull command"),
            }
        }

        let cli = Cli::try_parse_from(["dryve", "pull", "https://example.com"]).unwrap();
        assert!(matches!(cli.command, Commands::Pull { output: None, .. }));
        assert!(Cli::try_parse_from(["dryve", "pull", "https://example.com", "--output"]).is_err());
        assert!(Cli::try_parse_from(["dryve", "sync", "--output", "materials"]).is_err());
    }

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
