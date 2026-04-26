use crate::drive::fetch_drive;
use crate::download::download_tree;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(url: &str) -> Result<()> {
    let root = fetch_drive(url)?;
    root.print();

    download_tree(&root, Path::new("."))?;

    let json_path = Path::new(".").join(&root.name).join("dryve.json");
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    Ok(())
}