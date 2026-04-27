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

    let selected = match selection {
        Some(s) => s,
        None => {
            println!("Download cancelled.");
            return Ok(());
        }
    };

    if selected.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    download_tree(&root, base, threads, &selected)?;

    let mut root = root;
    root.prune_missing(base);

    let json_path = base.join(sanitize_name(&root.name)).join("dryve.json");
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    println!("Pull complete.");
    Ok(())
}
