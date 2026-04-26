use crate::node::{Node, NodeType};
use crate::parser::parse_html;
use anyhow::Result;
use rayon::prelude::*;

fn fetch_folder(id: &str) -> Result<String> {
    let url = format!("https://drive.google.com/drive/folders/{}?hl=en", id);
    let body = reqwest::blocking::get(url)?.text()?;
    Ok(body)
}

fn fetch_recursive(node: &mut Node) -> Result<()> {
    let results: Vec<(String, Vec<Node>)> = node
        .children
        .par_iter()
        .filter_map(|child| {
            if let NodeType::Folder = child.node_type
                && let Some(ref id) = child.id
            {
                let html = fetch_folder(id).ok()?;
                let sub = parse_html(&html).ok()?;
                return Some((id.clone(), sub.children));
            }
            None
        })
        .collect();

    for (id, children) in results {
        if let Some(child) = node
            .children
            .iter_mut()
            .find(|c| c.id.as_deref() == Some(&id))
        {
            child.children = children;
        }
    }

    for child in &mut node.children {
        if let NodeType::Folder = child.node_type {
            fetch_recursive(child)?;
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
    root.id = Some(id.to_string());
    fetch_recursive(&mut root)?;
    Ok(root)
}
