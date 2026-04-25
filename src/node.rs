#[derive(Debug)]
pub enum NodeType {
    Folder,
    File,
}

#[derive(Debug)]
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
}
