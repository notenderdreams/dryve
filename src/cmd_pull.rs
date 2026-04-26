use crate::download::download_tree;
use crate::drive::fetch_drive;
use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub fn run(url: &str) -> Result<()> {
    let root = fetch_drive(url)?;
    root.print();

    if !confirm_download()? {
        println!("Download cancelled.");
        return Ok(());
    }

    download_tree(&root, Path::new("."), 8)?;

    let json_path = Path::new(".").join(&root.name).join("dryve.json");
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    Ok(())
}

fn confirm_download() -> Result<bool> {
    print!("Start download?{}", " [y/N]:".blue());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let response = input.trim().to_lowercase();
    Ok(matches!(response.as_str(), "y" | "yes"))
}
