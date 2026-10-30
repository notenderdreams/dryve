use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Folder,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub node_type: NodeType,
    pub id: Option<String>,
    pub children: Vec<Node>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

impl Node {
    pub fn new(name: String, node_type: NodeType, id: Option<String>) -> Self {
        Self {
            name,
            node_type,
            id,
            children: Vec::new(),
            ignored: false,
            size: None,
            mime_type: None,
        }
    }

    pub fn total_size(&self) -> u64 {
        match self.node_type {
            NodeType::File => self.size.unwrap_or(0),
            NodeType::Folder => self.children.iter().map(|c| c.total_size()).sum(),
        }
    }

    pub fn prune_missing(&mut self, base: &Path) -> bool {
        let current_path = base.join(crate::utils::sanitize_name(&self.name));

        if self.ignored {
            return true;
        }

        match self.node_type {
            NodeType::File => current_path.exists(),
            NodeType::Folder => {
                self.children
                    .retain_mut(|child| child.prune_missing(&current_path));
                current_path.is_dir() || !self.children.is_empty()
            }
        }
    }

    pub fn prune_missing_at_root(&mut self) {
        self.children
            .retain_mut(|child| child.prune_missing(Path::new(".")));
    }

    /// Recursively apply the ignored state from `ignored_paths` starting from `base`.
    pub fn apply_ignored(&mut self, base: &Path, ignored_paths: &HashSet<PathBuf>) {
        let current_path = base.join(crate::utils::sanitize_name(&self.name));
        let is_self_ignored = ignored_paths.contains(&current_path);
        self.ignored = is_self_ignored;

        for child in &mut self.children {
            child.apply_ignored_recursive(&current_path, is_self_ignored, ignored_paths);
        }
    }

    fn apply_ignored_recursive(
        &mut self,
        base: &Path,
        parent_ignored: bool,
        ignored_paths: &HashSet<PathBuf>,
    ) {
        let current_path = base.join(crate::utils::sanitize_name(&self.name));
        self.ignored = parent_ignored || ignored_paths.contains(&current_path);

        for child in &mut self.children {
            child.apply_ignored_recursive(&current_path, self.ignored, ignored_paths);
        }
    }

    /// Collect all ignored IDs and relative paths in the subtree.
    pub fn collect_ignored(
        &self,
        current_path: &Path,
        ids: &mut HashSet<String>,
        paths: &mut HashSet<PathBuf>,
    ) {
        let my_path = current_path.join(crate::utils::sanitize_name(&self.name));
        if self.ignored {
            if let Some(ref id) = self.id {
                ids.insert(id.clone());
            }
            paths.insert(my_path.clone());
        }
        for child in &self.children {
            child.collect_ignored(&my_path, ids, paths);
        }
    }

    /// Inherit ignored flags from `old_root`'s collected IDs and paths.
    pub fn inherit_ignored(
        &mut self,
        current_path: &Path,
        parent_ignored: bool,
        ids: &HashSet<String>,
        paths: &HashSet<PathBuf>,
    ) {
        let my_path = current_path.join(crate::utils::sanitize_name(&self.name));
        let match_id = self.id.as_ref().map(|id| ids.contains(id)).unwrap_or(false);
        let match_path = paths.contains(&my_path);
        let is_ignored = parent_ignored || self.ignored || match_id || match_path;
        self.ignored = is_ignored;

        for child in &mut self.children {
            child.inherit_ignored(&my_path, is_ignored, ids, paths);
        }
    }

    /// Collect all relative paths present in the subtree starting from `base`.
    pub fn collect_paths(&self, base: &Path, out: &mut HashSet<PathBuf>) {
        let current_path = base.join(crate::utils::sanitize_name(&self.name));
        out.insert(current_path.clone());
        for child in &self.children {
            child.collect_paths(&current_path, out);
        }
    }

    /// Update ignored state for nodes present in `selector_paths`, while preserving
    /// existing ignore state for nodes that were not shown in the selector.
    pub fn apply_selector_changes(
        &mut self,
        base: &Path,
        parent_ignored: bool,
        selector_paths: &HashSet<PathBuf>,
        ignored_paths: &HashSet<PathBuf>,
    ) {
        let current_path = base.join(crate::utils::sanitize_name(&self.name));
        if selector_paths.contains(&current_path) {
            self.ignored = parent_ignored || ignored_paths.contains(&current_path);
        } else {
            self.ignored = parent_ignored || self.ignored;
        }

        for child in &mut self.children {
            child.apply_selector_changes(
                &current_path,
                self.ignored,
                selector_paths,
                ignored_paths,
            );
        }
    }
}

#[derive(Debug)]
pub struct Diff {
    pub added: Vec<(PathBuf, Node)>,
    pub removed: Vec<(PathBuf, Node)>,
    pub updated: Vec<(PathBuf, Node, Node)>,
}

impl Diff {
    pub fn new() -> Self {
        Self {
            added: Vec::new(),
            removed: Vec::new(),
            updated: Vec::new(),
        }
    }
    #[allow(dead_code)]
    pub fn from_nodes(old: &Node, new: &Node) -> Self {
        Self::from_nodes_with_ignored(old, new, false)
    }
    pub fn from_nodes_with_ignored(old: &Node, new: &Node, include_ignored: bool) -> Self {
        let mut diff = Diff::new();
        diff_recursive(old, new, &PathBuf::new(), &mut diff, include_ignored);
        diff
    }
}

fn diff_recursive(
    old: &Node,
    new: &Node,
    current_path: &Path,
    diff: &mut Diff,
    include_ignored: bool,
) {
    let mut old_map: HashMap<&str, &Node> =
        old.children.iter().map(|n| (n.name.as_str(), n)).collect();

    for new_child in new.children.iter() {
        let child_path = current_path.join(crate::utils::sanitize_name(&new_child.name));

        if new_child.ignored {
            if !include_ignored {
                old_map.remove(new_child.name.as_str());
                continue;
            }

            // When include_ignored is true, include ignored items in the diff
            let old_child_opt = old_map.remove(new_child.name.as_str());
            if matches!(new_child.node_type, NodeType::Folder) {
                if let Some(old_child) = old_child_opt {
                    diff_recursive(old_child, new_child, &child_path, diff, include_ignored);
                } else {
                    let empty_old = Node::new(
                        new_child.name.clone(),
                        NodeType::Folder,
                        new_child.id.clone(),
                    );
                    diff_recursive(&empty_old, new_child, &child_path, diff, include_ignored);
                }
            } else {
                diff.added.push((child_path, new_child.clone()));
            }
            continue;
        }

        if let Some(&old_child) = old_map.get(new_child.name.as_str()) {
            old_map.remove(new_child.name.as_str());

            // If old child was ignored and new child is now un-ignored, report as added
            if old_child.ignored {
                diff.added.push((child_path, new_child.clone()));
                continue;
            }

            match (&old_child.node_type, &new_child.node_type) {
                (NodeType::File, NodeType::File) => {
                    if old_child.id != new_child.id {
                        diff.updated.push((
                            child_path.clone(),
                            old_child.clone(),
                            new_child.clone(),
                        ));
                    }
                }
                (NodeType::Folder, NodeType::Folder) => {
                    diff_recursive(old_child, new_child, &child_path, diff, include_ignored);
                }
                _ => {
                    // Node type changed (e.g. folder -> file)
                    diff.removed.push((child_path.clone(), old_child.clone()));
                    diff.added.push((child_path, new_child.clone()));
                }
            }
        } else {
            diff.added.push((child_path, new_child.clone()));
        }
    }

    for (_, old_child) in old_map.into_iter() {
        if old_child.ignored && !include_ignored {
            continue;
        }
        let child_path = current_path.join(crate::utils::sanitize_name(&old_child.name));
        if old_child.ignored && !child_path.exists() {
            continue;
        }
        diff.removed.push((child_path, old_child.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_skips_ignored() {
        let old = Node::new("root".to_string(), NodeType::Folder, None);
        let mut new = Node::new("root".to_string(), NodeType::Folder, None);

        let mut ignored_file = Node::new(
            "ignored.txt".to_string(),
            NodeType::File,
            Some("id1".into()),
        );
        ignored_file.ignored = true;

        let normal_file = Node::new("normal.txt".to_string(), NodeType::File, Some("id2".into()));

        new.children.push(ignored_file);
        new.children.push(normal_file.clone());

        let diff = Diff::from_nodes(&old, &new);
        // Only normal.txt should be in diff.added; ignored.txt must be skipped!
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].1.name, "normal.txt");
    }

    #[test]
    fn test_diff_include_ignored() {
        let old = Node::new("root".to_string(), NodeType::Folder, None);
        let mut new = Node::new("root".to_string(), NodeType::Folder, None);

        let mut ignored_file = Node::new(
            "ignored.txt".to_string(),
            NodeType::File,
            Some("id1".into()),
        );
        ignored_file.ignored = true;

        new.children.push(ignored_file);

        // When include_ignored is false: skipped
        let diff_without = Diff::from_nodes_with_ignored(&old, &new, false);
        assert_eq!(diff_without.added.len(), 0);

        // When include_ignored is true: included
        let diff_with = Diff::from_nodes_with_ignored(&old, &new, true);
        assert_eq!(diff_with.added.len(), 1);
        assert_eq!(diff_with.added[0].1.name, "ignored.txt");
    }

    #[test]
    fn test_inherit_ignored() {
        let mut old = Node::new("root".to_string(), NodeType::Folder, None);
        let mut old_child = Node::new(
            "movie.mkv".to_string(),
            NodeType::File,
            Some("movie_id".into()),
        );
        old_child.ignored = true;
        old.children.push(old_child);

        let mut new = Node::new("root".to_string(), NodeType::Folder, None);
        let new_child = Node::new(
            "movie.mkv".to_string(),
            NodeType::File,
            Some("movie_id".into()),
        );
        new.children.push(new_child);

        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        old.collect_ignored(Path::new(""), &mut ids, &mut paths);

        new.inherit_ignored(Path::new(""), false, &ids, &paths);
        assert!(new.children[0].ignored);
    }

    #[test]
    fn test_apply_selector_changes_preserves_unshown_ignored() {
        let mut root = Node::new("root".to_string(), NodeType::Folder, None);
        let mut unshown_ignored =
            Node::new("already_ignored.mp4".to_string(), NodeType::File, None);
        unshown_ignored.ignored = true;
        let newly_changed = Node::new("new_file.txt".to_string(), NodeType::File, None);

        root.children.push(unshown_ignored);
        root.children.push(newly_changed);

        // Only "new_file.txt" was shown in the selector
        let mut selector_paths = HashSet::new();
        selector_paths.insert(PathBuf::from("new_file.txt"));

        // User chose to ignore "new_file.txt"
        let mut ignored_paths = HashSet::new();
        ignored_paths.insert(PathBuf::from("new_file.txt"));

        for child in &mut root.children {
            child.apply_selector_changes(
                Path::new(""),
                root.ignored,
                &selector_paths,
                &ignored_paths,
            );
        }

        // Both files should now be ignored!
        assert!(root.children[0].ignored);
        assert!(root.children[1].ignored);
    }

    #[test]
    fn test_prune_missing_preserves_ignored_nodes() {
        let mut root = Node::new("root".to_string(), NodeType::Folder, None);
        let mut ignored_file =
            Node::new("non_existent_ignored.bin".to_string(), NodeType::File, None);
        ignored_file.ignored = true;
        let missing_file = Node::new(
            "non_existent_unignored.bin".to_string(),
            NodeType::File,
            None,
        );

        root.children.push(ignored_file);
        root.children.push(missing_file);

        // Path pointing to a non-existent directory
        root.prune_missing(Path::new("/definitely/does/not/exist"));

        // The ignored file should be retained; the unignored missing file should be pruned!
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "non_existent_ignored.bin");
    }

    #[test]
    fn test_node_serde_roundtrip() {
        let mut node = Node::new("file.txt".to_string(), NodeType::File, Some("123".into()));
        node.ignored = true;
        node.size = Some(1048576);

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"ignored\":true"));
        assert!(json.contains("\"size\":1048576"));

        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert!(deserialized.ignored);
        assert_eq!(deserialized.size, Some(1048576));

        // Older dryve.json without "ignored" or "size" field defaults to false/None
        let legacy_json = r#"{"name":"legacy.txt","node_type":"File","id":"456","children":[]}"#;
        let legacy_node: Node = serde_json::from_str(legacy_json).unwrap();
        assert!(!legacy_node.ignored);
        assert_eq!(legacy_node.size, None);
    }

    #[test]
    fn test_node_total_size() {
        let mut file1 = Node::new("a.txt".to_string(), NodeType::File, None);
        file1.size = Some(100);
        let mut file2 = Node::new("b.txt".to_string(), NodeType::File, None);
        file2.size = Some(250);

        let mut folder = Node::new("dir".to_string(), NodeType::Folder, None);
        folder.children = vec![file1, file2];

        assert_eq!(folder.total_size(), 350);
    }
}
