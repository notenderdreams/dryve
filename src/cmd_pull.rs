use crate::drive::fetch_drive;
use crate::download::download_tree;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn run(url: &str) -> Result<()> {
    let root = fetch_drive(url)?;
    root.print();

    let json = serde_json::to_string_pretty(&root)?;
    fs::write("dryve.json", json)?;
    println!("Root structure written to dryve.json");

    download_tree(&root, Path::new("."))?;

    Ok(())
}