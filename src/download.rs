use crate::node::{Node, NodeType};
use crate::pool::{FileTask, run};
use crate::utils::sanitize_name;
use anyhow::Result;
use reqwest::blocking::Client;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub fn collect_tasks(
    node: &Node,
    base: &Path,
    tasks: &mut Vec<FileTask>,
    create_dirs: bool,
) -> Result<()> {
    match node.node_type {
        NodeType::Folder => {
            let dir = base.join(sanitize_name(&node.name));
            if create_dirs {
                fs::create_dir_all(&dir)?;
            }
            for child in &node.children {
                collect_tasks(child, &dir, tasks, create_dirs)?;
            }
        }
        NodeType::File => {
            if let Some(ref id) = node.id {
                tasks.push(FileTask {
                    id: id.clone(),
                    name: node.name.clone(),
                    path: base.join(sanitize_name(&node.name)),
                });
            }
        }
    }
    Ok(())
}

pub fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .cookie_store(true)
        .user_agent("Mozilla/5.0")
        .build()?)
}

fn extract_confirm_url(html: &str, id: &str) -> Result<String> {
    let token = html
        .find("confirm=")
        .and_then(|i| {
            let rest = &html[i + 8..];
            rest.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                .map(|e| &rest[..e])
        })
        .unwrap_or("t");

    Ok(format!(
        "https://drive.google.com/uc?export=download&id={}&confirm={}",
        id, token
    ))
}

pub fn download_file(client: &Client, task: &FileTask) -> Result<u64> {
    let url = format!("https://drive.google.com/uc?export=download&id={}", task.id);
    let response = client.get(&url).send()?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut response = if content_type.contains("text/html") {
        let html = response.text()?;
        let confirm_url = extract_confirm_url(&html, &task.id)?;
        client.get(&confirm_url).send()?
    } else {
        response
    };

    let mut file = fs::File::create(&task.path)?;
    let mut buf = [0u8; 16384];
    let mut downloaded = 0;
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
    }

    Ok(downloaded)
}

pub fn download_tree(node: &Node, base: &Path, threads: usize) -> Result<()> {
    let mut tasks = Vec::new();
    collect_tasks(node, base, &mut tasks, true)?;

    let errors = run(
        tasks,
        |task, _| {
            let client = build_client()?;
            download_file(&client, task)
        },
        threads,
    );

    for e in &errors {
        eprintln!("failed: {}", e);
    }

    Ok(())
}
