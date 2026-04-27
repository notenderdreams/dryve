use crate::download::download_tree;
use crate::drive::fetch_drive;
use crate::utils::{confirm_download, sanitize_name};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(url: &str) -> Result<()> {
    let root = fetch_drive(url)?;
    root.print();

    if !confirm_download("Start Download")? {
        println!("Download cancelled.");
        return Ok(());
    }

    download_tree(&root, Path::new("."), 8)?;

    let json_path = Path::new(".").join(sanitize_name(&root.name)).join("dryve.json");
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    Ok(())
}

