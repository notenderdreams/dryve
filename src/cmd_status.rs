use crate::drive::fetch_from_id;
use crate::node::{Diff, Node, NodeType};
use crate::utils::fmt_size_compact;
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeItem {
    pub kind: ChangeKind,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub ignored: bool,
}

fn count_files(node: &Node) -> usize {
    match node.node_type {
        NodeType::File => 1,
        NodeType::Folder => node.children.iter().map(count_files).sum(),
    }
}

fn collect_node_changes(
    node: &Node,
    path: &Path,
    kind: ChangeKind,
    out: &mut Vec<ChangeItem>,
) {
    match node.node_type {
        NodeType::File => {
            out.push(ChangeItem {
                kind,
                path: path.to_path_buf(),
                size: node.size,
                ignored: node.ignored,
            });
        }
        NodeType::Folder => {
            if node.children.is_empty() {
                out.push(ChangeItem {
                    kind,
                    path: path.to_path_buf(),
                    size: None,
                    ignored: node.ignored,
                });
            } else {
                for child in &node.children {
                    let child_path = path.join(crate::utils::sanitize_name(&child.name));
                    collect_node_changes(child, &child_path, kind, out);
                }
            }
        }
    }
}

pub fn collect_changes(diff: &Diff) -> Vec<ChangeItem> {
    let mut changes = Vec::new();

    for (path, node) in &diff.added {
        collect_node_changes(node, path, ChangeKind::Added, &mut changes);
    }
    for (path, _old, new_node) in &diff.updated {
        changes.push(ChangeItem {
            kind: ChangeKind::Modified,
            path: path.clone(),
            size: new_node.size,
            ignored: new_node.ignored,
        });
    }
    for (path, node) in &diff.removed {
        collect_node_changes(node, path, ChangeKind::Removed, &mut changes);
    }

    changes
}

pub fn run(folder: Option<&str>, include_ignored: bool) -> Result<()> {
    if let Some(dir) = folder {
        std::env::set_current_dir(dir).context("Failed to change working directory")?;
    }

    let json_path = Path::new("dryve.json");
    if !json_path.exists() {
        println!("Error: dryve.json not found.");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let old_root: Node = serde_json::from_str(&content)?;

    let root_id = old_root.id.as_deref().context("root node is missing ID")?;
    println!("Fetching remote tree for ID: {}...", root_id);
    let mut new_root = fetch_from_id(root_id)?;

    // Inherit ignore settings from old metadata
    let mut ignored_ids = HashSet::new();
    let mut ignored_paths = HashSet::new();
    old_root.collect_ignored(Path::new(""), &mut ignored_ids, &mut ignored_paths);
    new_root.inherit_ignored(Path::new(""), false, &ignored_ids, &ignored_paths);

    let diff = Diff::from_nodes_with_ignored(&old_root, &new_root, include_ignored);
    let changes = collect_changes(&diff);

    let total_files = count_files(&new_root);
    let total_size = new_root.total_size();
    let items_str = if total_files == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", total_files)
    };
    let drive_name = if new_root.name.is_empty() {
        &old_root.name
    } else {
        &new_root.name
    };

    println!();
    println!(
        "Drive: {} ({}, {})",
        drive_name,
        items_str,
        fmt_size_compact(total_size)
    );
    println!(
        "Remote: https://drive.google.com/drive/folders/{}",
        root_id
    );
    println!();

    if changes.is_empty() {
        println!("Everything is up to date. Working tree clean.");
        return Ok(());
    }

    let download_bytes: u64 = changes
        .iter()
        .filter(|c| !c.ignored && matches!(c.kind, ChangeKind::Added | ChangeKind::Modified))
        .filter_map(|c| c.size)
        .sum();

    let files_str = if changes.len() == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", changes.len())
    };

    let changes_header = if download_bytes > 0 {
        format!(
            "Changes ({}, {}):",
            files_str,
            fmt_size_compact(download_bytes)
        )
    } else {
        format!("Changes ({}):", files_str)
    };

    println!("{}", changes_header);

    let added: Vec<_> = changes
        .iter()
        .filter(|c| !c.ignored && c.kind == ChangeKind::Added)
        .collect();
    let modified: Vec<_> = changes
        .iter()
        .filter(|c| !c.ignored && c.kind == ChangeKind::Modified)
        .collect();
    let removed: Vec<_> = changes
        .iter()
        .filter(|c| !c.ignored && c.kind == ChangeKind::Removed)
        .collect();
    let ignored: Vec<_> = changes.iter().filter(|c| c.ignored).collect();

    if !added.is_empty() {
        println!();
        println!("{}", "+ Added:".green());
        for c in added {
            let size_str = c
                .size
                .map(|s| format!(" ({})", fmt_size_compact(s)))
                .unwrap_or_default();
            println!("    {}{}", c.path.display(), size_str);
        }
    }

    if !modified.is_empty() {
        println!();
        println!("{}", "~ Modified:".yellow());
        for c in modified {
            let size_str = c
                .size
                .map(|s| format!(" ({})", fmt_size_compact(s)))
                .unwrap_or_default();
            println!("    {}{}", c.path.display(), size_str);
        }
    }

    if !removed.is_empty() {
        println!();
        println!("{}", "- Removed:".red());
        for c in removed {
            println!("    {}", c.path.display());
        }
    }

    if !ignored.is_empty() {
        println!();
        println!("{}", "! Ignored:".bright_black());
        for c in ignored {
            println!(
                "    {} {}",
                c.path.display(),
                "(skipped)".bright_black()
            );
        }
    }

    println!();
    println!("Use 'dryve sync' to apply changes.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_changes() {
        let mut diff = Diff::new();
        let mut added_file = Node::new("new_spec.pdf".to_string(), NodeType::File, Some("id1".into()));
        added_file.size = Some(2_500_000);

        let mut ignored_file = Node::new("data.zip".to_string(), NodeType::File, Some("id2".into()));
        ignored_file.ignored = true;

        diff.added.push((PathBuf::from("docs/new_spec.pdf"), added_file));
        diff.added.push((PathBuf::from("archive/data.zip"), ignored_file));

        let old_file = Node::new("report.docx".to_string(), NodeType::File, Some("old_id".into()));
        let mut updated_file = Node::new("report.docx".to_string(), NodeType::File, Some("new_id".into()));
        updated_file.size = Some(12_000_000);
        diff.updated.push((PathBuf::from("report.docx"), old_file, updated_file));

        let removed_file = Node::new("notes.txt".to_string(), NodeType::File, Some("id3".into()));
        diff.removed.push((PathBuf::from("notes.txt"), removed_file));

        let changes = collect_changes(&diff);
        assert_eq!(changes.len(), 4);

        assert_eq!(changes[0].kind, ChangeKind::Added);
        assert_eq!(changes[0].path, PathBuf::from("docs/new_spec.pdf"));
        assert_eq!(changes[0].size, Some(2_500_000));
        assert!(!changes[0].ignored);

        assert_eq!(changes[1].kind, ChangeKind::Added);
        assert!(changes[1].ignored);

        assert_eq!(changes[2].kind, ChangeKind::Modified);
        assert_eq!(changes[2].path, PathBuf::from("report.docx"));
        assert_eq!(changes[2].size, Some(12_000_000));

        assert_eq!(changes[3].kind, ChangeKind::Removed);
        assert_eq!(changes[3].path, PathBuf::from("notes.txt"));
    }
}
