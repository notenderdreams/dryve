use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Node {
    pub fn new(name: String, node_type: NodeType, id: Option<String>) -> Self {
        Self {
            name,
            node_type,
            id,
            children: Vec::new(),
        }
    }

    pub fn print(&self) {
        let name = match self.node_type {
            NodeType::Folder => self.name.magenta().bold(),
            NodeType::File => self.name.normal(),
        };
        println!("{}", name);
        for (i, child) in self.children.iter().enumerate() {
            let is_last = i == self.children.len() - 1;
            child.print_tree("", is_last);
        }
    }

    fn print_tree(&self, prefix: &str, is_last: bool) {
        let connector = if is_last { "└── " } else { "├── " };
        let connector = connector.bright_black();

        let name = match self.node_type {
            NodeType::Folder => self.name.magenta().bold(),
            NodeType::File => self.name.normal(),
        };

        println!("{}{}{}", prefix.bright_black(), connector, name);

        let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        for (i, child) in self.children.iter().enumerate() {
            let last_child = i == self.children.len() - 1;
            child.print_tree(&new_prefix, last_child);
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
    pub fn from_nodes(old: &Node, new: &Node) -> Self {
        let mut diff = Diff::new();
        diff_recursive(old, new, &PathBuf::new(), &mut diff);
        diff
    }
}

fn diff_recursive(old: &Node, new: &Node, current_path: &Path, diff: &mut Diff) {
    let mut old_map: HashMap<&str, &Node> =
        old.children.iter().map(|n| (n.name.as_str(), n)).collect();

    for new_child in new.children.iter() {
        let child_path = current_path.join(crate::utils::sanitize_name(&new_child.name));
        if let Some(&old_child) = old_map.get(new_child.name.as_str()) {
            old_map.remove(new_child.name.as_str());

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
                    diff_recursive(old_child, new_child, &child_path, diff);
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
        let child_path = current_path.join(crate::utils::sanitize_name(&old_child.name));
        diff.removed.push((child_path, old_child.clone()));
    }
}
