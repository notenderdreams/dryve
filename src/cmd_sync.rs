use crate::download::{self, collect_tasks, download_file};
use crate::drive::fetch_from_id;
use crate::node::{Diff, Node, NodeType};
use crate::pool::FileTask;
use crate::selector::{self, ItemKind};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

struct SyncRoots {
    /// (node, base_path, kind) triples fed directly into the selector.
    entries: Vec<(Node, PathBuf, ItemKind)>,
}

impl SyncRoots {
    fn from_diff(diff: &Diff) -> Self {
        let mut entries = Vec::new();

        for (path, node) in &diff.added {
            let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            entries.push((node.clone(), base, ItemKind::Added));
        }
        for (path, _old, new_node) in &diff.updated {
            let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            entries.push((new_node.clone(), base, ItemKind::Modified));
        }
        for (path, node) in &diff.removed {
            let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            entries.push((node.clone(), base, ItemKind::Removed));
        }

        SyncRoots { entries }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn run() -> Result<()> {
    let threads = 8;
    let json_path = Path::new("dryve.json");

    if !json_path.exists() {
        println!("Error: dryve.json not found. Please run 'dryve init' first.");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let old_root: Node = serde_json::from_str(&content)?;

    let root_id = old_root.id.as_deref().context("root node is missing ID")?;
    println!("Fetching remote tree for ID: {}...", root_id);
    let new_root = fetch_from_id(root_id)?;

    let diff = Diff::from_nodes(&old_root, &new_root);
    let sync_roots = SyncRoots::from_diff(&diff);

    if sync_roots.is_empty() {
        println!("Everything is up to date.");
        return Ok(());
    }

    // Show the interactive selector — only changed files are listed.
    let selection = selector::select(&sync_roots.entries)
        .map_err(|e| anyhow::anyhow!("selector error: {}", e))?;

    let selected: HashSet<PathBuf> = match selection {
        Some(s) => s,
        None => {
            println!("Sync cancelled.");
            return Ok(());
        }
    };

    if selected.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    // Split the confirmed selection back into add / update / remove buckets.
    let mut to_add: Vec<FileTask> = Vec::new();
    let mut to_update: Vec<FileTask> = Vec::new();
    let mut to_remove: Vec<PathBuf> = Vec::new();

    for (path, node) in &diff.added {
        let base = path.parent().unwrap_or(Path::new("."));
        let mut tasks = Vec::new();
        collect_tasks(node, base, &mut tasks, false)?;
        for t in tasks {
            if selected.contains(&t.path) {
                if let Some(parent) = t.path.parent() {
                    fs::create_dir_all(parent)?;
                }
                to_add.push(t);
            }
        }
    }

    for (path, _old_node, new_node) in &diff.updated {
        let base = path.parent().unwrap_or(Path::new("."));
        let mut tasks = Vec::new();
        collect_tasks(new_node, base, &mut tasks, false)?;
        for t in tasks {
            if selected.contains(&t.path) {
                if let Some(parent) = t.path.parent() {
                    fs::create_dir_all(parent)?;
                }
                to_update.push(t);
            }
        }
    }

    for (path, node) in &diff.removed {
        // Only add to remove list if the user selected it.
        if selected.contains(path) {
            to_remove.push(path.clone());
            continue;
        }
        // Also check descendant files for folder nodes.
        if matches!(node.node_type, NodeType::Folder) {
            let mut tasks = Vec::new();
            let base = path.parent().unwrap_or(Path::new("."));
            collect_tasks(node, base, &mut tasks, false)?;
            for t in tasks {
                if selected.contains(&t.path) {
                    to_remove.push(t.path);
                }
            }
        }
    }

    // Execute.
    let client = download::build_client()?;

    if !to_add.is_empty() {
        let errors = crate::pool::run(to_add, |task, _| download_file(&client, task), threads);
        for e in &errors {
            eprintln!("Error: {}", e);
        }
    }

    if !to_update.is_empty() {
        let errors = crate::pool::run(
            to_update,
            |task, _| {
                if task.path.exists() {
                    fs::remove_file(&task.path)?;
                }
                download_file(&client, task)
            },
            threads,
        );
        for e in &errors {
            eprintln!("Error: {}", e);
        }
    }

    for path in &to_remove {
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
        println!("Removed: {}", path.display());
    }

    let mut root = new_root;
    root.prune_missing_at_root();

    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;
    println!("Synchronization complete. Updated dryve.json.");

    Ok(())
}
