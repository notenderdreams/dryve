use crate::download::download_tree;
use crate::drive::fetch_drive;
use crate::selector::{self, ItemKind};
use crate::utils::sanitize_name;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(
    url: &str,
    output: Option<&Path>,
    threads: usize,
    yes: bool,
    include: &[String],
    exclude: &[String],
) -> Result<()> {
    let root = fetch_drive(url)?;
    let base = Path::new(".");
    let destination = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(sanitize_name(&root.name)));
    let roots = vec![(root.clone(), base.to_path_buf(), ItemKind::Normal)];

    let selection = if yes {
        selector::select_all(&roots, include, exclude)
    } else {
        let sel = selector::select(&roots, include, exclude)
            .map_err(|e| anyhow::anyhow!("selector error: {}", e))?;
        match sel {
            Some(s) => s,
            None => {
                println!("Download cancelled.");
                return Ok(());
            }
        }
    };

    if selection.selected.is_empty() && selection.ignored.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    if !selection.selected.is_empty() {
        let selected = destination_paths(&root, base, &destination, &selection.selected)?;
        download_tree(&root, &destination, threads, &selected)?;
    } else {
        println!("No files selected for download.");
    }

    let mut root = root;
    root.apply_ignored(base, &selection.ignored);
    root.children
        .retain_mut(|child| child.prune_missing(&destination));

    let json_path = destination.join("dryve.json");
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&root)?;
    fs::write(json_path, json)?;

    println!("Pull complete.");
    Ok(())
}

/// Keep selector paths independent of the chosen download directory.
fn destination_paths(
    root: &crate::node::Node,
    base: &Path,
    destination: &Path,
    selected: &HashSet<PathBuf>,
) -> Result<HashSet<PathBuf>> {
    let root_path = base.join(sanitize_name(&root.name));
    selected
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root_path)
                .context("Selected path is outside the remote folder")?;
            Ok(destination.join(relative))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::collect_tasks;
    use crate::node::{Node, NodeType};

    #[test]
    fn selected_paths_match_download_tasks_in_destination() {
        let mut root = Node::new(
            "Remote: Folder".into(),
            NodeType::Folder,
            Some("root".into()),
        );
        let mut docs = Node::new("docs".into(), NodeType::Folder, Some("docs".into()));
        docs.children = vec![
            Node::new("report.pdf".into(), NodeType::File, Some("report".into())),
            Node::new("skip.txt".into(), NodeType::File, Some("skip".into())),
        ];
        root.children.push(docs);
        let base = Path::new(".");
        let roots = vec![(root.clone(), base.to_path_buf(), ItemKind::Normal)];
        let selection = selector::select_all(&roots, &["*.pdf".into()], &[]);

        for destination in [
            PathBuf::from("./my materials"),
            std::env::temp_dir().join("dryve-materials"),
            PathBuf::from("."),
            PathBuf::from("../materials"),
            base.join(sanitize_name(&root.name)),
        ] {
            let selected =
                destination_paths(&root, base, &destination, &selection.selected).unwrap();
            assert_eq!(
                selected,
                HashSet::from([destination.join("docs/report.pdf")])
            );

            let mut tasks = Vec::new();
            for child in &root.children {
                collect_tasks(child, &destination, &mut tasks, false).unwrap();
            }
            let selected_tasks: Vec<_> = tasks
                .iter()
                .filter(|task| selected.contains(&task.path))
                .collect();
            assert_eq!(selected_tasks.len(), 1);
            assert_eq!(selected_tasks[0].id, "report");
            assert_eq!(selected_tasks[0].path, destination.join("docs/report.pdf"));
        }
        assert_eq!(root.name, "Remote: Folder");
    }
}
