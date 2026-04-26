use crate::node::{Node,NodeType};
use anyhow::Result;
use reqwest::blocking::Client;
use std::fs;
use std::path::Path;
use std::io::Write;

fn download_file(client: &Client, id: &str, dest: &Path) -> Result<()> {
    let url = format!("https://drive.google.com/uc?export=download&id={}", id);
    let response = client.get(&url).send()?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let bytes = if content_type.contains("text/html") {
        let html = response.text()?;
        let confirm_url = extract_confirm_url(&html,id)?;
        client.get(&confirm_url).send()?.bytes()?
    } else {
        response.bytes()?
    };

    let mut file = fs::File::create(dest)?;
    file.write_all(&bytes)?;
    Ok(())
}


fn extract_confirm_url(html: &str,id: &str) -> Result<String> {
    let token = html
        .find("confirm=")
        .and_then(|i| {
            let rest = &html[i + 8..];
            rest.find(|c:char| !c.is_alphanumeric() && c!='_' && c!='-')
                .map(|e| &rest[..e])
        })
        .unwrap_or("t");

    Ok(format!(
        "https://drive.google.com/uc?export=download&id={}&confirm={}",
        id, token
    ))
}

pub fn download_tree(node: &Node, base: &Path) ->Result<()> {
    match node.node_type {
        NodeType::Folder => {
            let dir = base.join(&node.name);
            fs::create_dir_all(&dir)?;
            for c in &node.children {
                download_tree(c,&dir)?;
            }
        }
        NodeType::File => {
            if let Some(ref id) = node.id {
                let dest = base.join(&node.name);
                println!("Downloading: {}", id);
                let client = Client::builder()
                    .cookie_store(true)
                    .user_agent("Mozilla/5.0")
                    .build()?;
                download_file(&client, id, &dest)?;
            }
        }
    }
    Ok(())
}