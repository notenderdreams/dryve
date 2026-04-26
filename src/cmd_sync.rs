use crate::node::{Node, Diff};
use crate::drive::fetch_from_id;
use crate::download::collect_tasks;
use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

pub fn run() -> Result<()> {
    let json_path = Path::new("dryve.json");
    
    if !json_path.exists() {
        println!("not found");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let root: Node = serde_json::from_str(&content)?;
    
    let root_id = root.id.as_deref().context("root node is missing ID")?;
    
    let new_root = fetch_from_id(root_id)?;

    let diff = Diff::from_nodes(&root, &new_root);

    let mut added_tasks = Vec::new();
    for (path, node) in &diff.added {
        if let Some(base) = path.parent() {
            collect_tasks(node, base, &mut added_tasks, false)?;
        }
    }
    
    let mut removed_tasks = Vec::new();
    for (path, node) in &diff.removed {
        if let Some(base) = path.parent() {
            collect_tasks(node, base, &mut removed_tasks, false)?;
        }
    }

    let mut updated_tasks = Vec::new();
    for (path, _old_node, new_node) in &diff.updated {
        if let Some(base) = path.parent() {
            collect_tasks(new_node, base, &mut updated_tasks, false)?;
        }
    }

    println!("Sync Plan:");
    for task in &added_tasks {
        println!("Added: {}", task.path.display());
    }
    for task in &updated_tasks {
        println!("Modified: {}", task.path.display());
    }
    for task in &removed_tasks {
        println!("Removed: {}", task.path.display());
    }

    Ok(())
}
