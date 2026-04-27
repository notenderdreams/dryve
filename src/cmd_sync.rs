use crate::download::{self, collect_tasks, download_file};
use crate::drive::fetch_from_id;
use crate::node::{Diff, Node, NodeType};
use crate::pool::FileTask;
use crate::selector::{self, ItemKind};
use crate::utils::sanitize_name;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Build a kind-override map from the diff: path → ItemKind.
fn build_kind_map(diff: &Diff) -> HashMap<PathBuf, ItemKind> {
    let mut map = HashMap::new();

    for (path, node) in &diff.added {
        insert_node_kinds(&mut map, node, path, ItemKind::Added);
    }
    for (path, _old, new_node) in &diff.updated {
        insert_node_kinds(&mut map, new_node, path, ItemKind::Modified);
    }
    for (path, node) in &diff.removed {
        insert_node_kinds(&mut map, node, path, ItemKind::Removed);
    }

    map
}

/// Recursively insert kind entries for a node and all its descendants.
fn insert_node_kinds(
    map: &mut HashMap<PathBuf, ItemKind>,
    node: &Node,
    path: &Path,
    kind: ItemKind,
) {
    map.insert(path.to_path_buf(), kind);
    for child in &node.children {
        let child_path = path.join(sanitize_name(&child.name));
        insert_node_kinds(map, child, &child_path, kind);
    }
}

/// Prune a tree so only branches leading to paths in `changed` are kept.
/// Returns `None` if the node (and its subtree) contains nothing of interest.
fn prune_to_changed(node: &Node, current_path: &Path, changed: &HashSet<PathBuf>) -> Option<Node> {
    if changed.contains(current_path) {
        // This node itself is changed — keep it with all children.
        return Some(node.clone());
    }

    // For folders, recurse and keep only children that lead to changes.
    if matches!(node.node_type, NodeType::Folder) {
        let mut kept_children = Vec::new();
        for child in &node.children {
            let child_path = current_path.join(sanitize_name(&child.name));
            if let Some(pruned) = prune_to_changed(child, &child_path, changed) {
                kept_children.push(pruned);
            }
        }
        if !kept_children.is_empty() {
            let mut pruned_node = node.clone();
            pruned_node.children = kept_children;
            return Some(pruned_node);
        }
    }

    None
}

/// Build a list of selector roots from the diff, preserving tree structure.
/// Paths match the diff scheme (starting from empty, relative to root folder).
fn build_sync_roots(new_root: &Node, diff: &Diff) -> Vec<(Node, PathBuf, ItemKind)> {
    // Collect all changed paths (matching diff_recursive's path scheme).
    let mut changed_paths: HashSet<PathBuf> = HashSet::new();
    for (path, _) in &diff.added {
        changed_paths.insert(path.clone());
    }
    for (path, _, _) in &diff.updated {
        changed_paths.insert(path.clone());
    }
    for (path, _) in &diff.removed {
        changed_paths.insert(path.clone());
    }

    if changed_paths.is_empty() {
        return Vec::new();
    }

    // Prune each top-level child of the root to only branches with changes.
    // We skip the root node itself because sync runs from inside the root folder,
    // and diff paths don't include the root name.
    let base = PathBuf::new();
    let mut kept_children: Vec<Node> = Vec::new();

    for child in &new_root.children {
        let child_path = base.join(sanitize_name(&child.name));
        if let Some(pruned) = prune_to_changed(child, &child_path, &changed_paths) {
            kept_children.push(pruned);
        }
    }

    // Graft removed nodes back into the tree (they won't be in new_root).
    for (path, node) in &diff.removed {
        let parent = path.parent();
        if parent.is_none() || parent == Some(Path::new("")) {
            // Top-level removed node
            if !kept_children.iter().any(|c| c.name == node.name) {
                kept_children.push(node.clone());
            }
        } else {
            // Nested removed node — graft into the matching child folder
            for child in &mut kept_children {
                let child_path = PathBuf::from(sanitize_name(&child.name));
                if path.starts_with(&child_path) {
                    graft_node(child, &child_path, path, node);
                    break;
                }
            }
        }
    }

    // Each top-level child becomes a selector root with base = ""
    kept_children
        .into_iter()
        .map(|node| (node, PathBuf::new(), ItemKind::Normal))
        .collect()
}

/// Graft a removed node back into the tree at the correct position.
fn graft_node(tree: &mut Node, tree_path: &Path, target_path: &Path, node: &Node) {
    let parent_path = match target_path.parent() {
        Some(p) => p,
        None => return,
    };

    if parent_path == tree_path {
        let name = node.name.as_str();
        if !tree.children.iter().any(|c| c.name == name) {
            tree.children.push(node.clone());
        }
        return;
    }

    for child in &mut tree.children {
        let child_path = tree_path.join(sanitize_name(&child.name));
        if target_path.starts_with(&child_path) {
            graft_node(child, &child_path, target_path, node);
            return;
        }
    }
}

pub fn run(threads: usize) -> Result<()> {
    let json_path = Path::new("dryve.json");

    if !json_path.exists() {
        println!("Error: dryve.json not found.");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let old_root: Node = serde_json::from_str(&content)?;

    let root_id = old_root.id.as_deref().context("root node is missing ID")?;
    println!("Fetching remote tree for ID: {}...", root_id);
    let new_root = fetch_from_id(root_id)?;

    let diff = Diff::from_nodes(&old_root, &new_root);

    let roots = build_sync_roots(&new_root, &diff);

    if roots.is_empty() {
        println!("Everything is up to date.");
        return Ok(());
    }

    let kind_map = build_kind_map(&diff);

    // Show the interactive selector with the structured tree.
    let selection = selector::select_with_kinds(&roots, &kind_map)
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
