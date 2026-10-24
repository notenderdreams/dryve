use crate::node::Node;
use crate::utils::open_in_browser;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(folder: Option<&str>) -> Result<()> {
    if let Some(dir) = folder {
        std::env::set_current_dir(dir).context("Failed to change working directory")?;
    }

    let json_path = Path::new("dryve.json");
    if !json_path.exists() {
        println!("Error: dryve.json not found.");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let root: Node = serde_json::from_str(&content)?;

    let id = root.id.as_deref().context("root node is missing ID")?;
    let url = format!("https://drive.google.com/drive/folders/{}", id);

    println!("Opening: {}", url);
    open_in_browser(&url)?;

    Ok(())
}
