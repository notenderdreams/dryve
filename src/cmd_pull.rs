use crate::download::download_tree;
use crate::drive::fetch_drive;
use crate::selector::{self, ItemKind};
use crate::utils::sanitize_name;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(url: &str, threads: usize) -> Result<()> {
    let root = fetch_drive(url)?;
    let base = Path::new(".");
    let roots = vec![(root.clone(), base.to_path_buf(), ItemKind::Normal)];

    let selection =
        selector::select(&roots).map_err(|e| anyhow::anyhow!("selector error: {}", e))?;

    let selection = match selection {
        Some(s) => s,
        None => {
            println!("Download cancelled.");
            return Ok(());
        }
    };

    if selection.selected.is_empty() && selection.ignored.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    if !selection.selected.is_empty() {
        download_tree(&root, base, threads, &selection.selected)?;
    } else {
        println!("No files selected for download.");
    }

    let mut root = root;
    root.apply_ignored(base, &selection.ignored);
    root.prune_missing(base);

    let json_path = base.join(sanitize_name(&root.name)).join("dryve.json");
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    println!("Pull complete.");
    Ok(())
}
