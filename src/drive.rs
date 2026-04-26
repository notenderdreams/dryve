use crate::node::{Node, NodeType};
use crate::parser::parse_html;
use anyhow::Result;

fn fetch_folder(id: &str) -> Result<String> {
    let url = format!("https://drive.google.com/drive/folders/{}?hl=en", id);
    let body = reqwest::blocking::get(url)?.text()?;
    Ok(body)
}

fn fetch_recursive(node: &mut Node) -> Result<()> {
    for child in &mut node.children {
        if let NodeType::Folder = child.node_type {
            if let Some(ref id) = child.id.clone() {
                let html = fetch_folder(id)?;
                let mut sub = parse_html(&html)?;
                std::mem::swap(&mut child.children, &mut sub.children);
                fetch_recursive(child)?;
            }
        }
    }
    Ok(())
}

pub fn fetch_drive(url: &str) -> Result<Node> {
    let id = url
        .split("/folders/")
        .nth(1)
        .and_then(|s| s.split(['?', '&', '/']).next())
        .ok_or_else(|| anyhow::anyhow!("could not extract folder ID from URL: {}", url))?;

    let html = fetch_folder(id)?;
    let mut root = parse_html(&html)?;
    fetch_recursive(&mut root)?;
    Ok(root)
}