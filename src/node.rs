use colored::*;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum NodeType {
    Folder,
    File,
}

#[derive(Debug, Serialize, Deserialize)]
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
            NodeType::File => self.name .normal(),
        };

        println!("{}{}{}", prefix.bright_black(), connector, name);

        let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
        for (i, child) in self.children.iter().enumerate() {
            let last_child = i == self.children.len() - 1;
            child.print_tree(&new_prefix, last_child);
        }
    }
}
