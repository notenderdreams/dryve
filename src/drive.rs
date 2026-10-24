use crate::node::{Node, NodeType};
use crate::parser::parse_html;
use anyhow::Result;
use colored::Colorize;
use rayon::prelude::*;
use std::io::{self, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct DiscoveryProgress {
    folders: AtomicUsize,
    files: AtomicUsize,
    lock: Mutex<()>,
}

impl DiscoveryProgress {
    fn new() -> Self {
        Self {
            folders: AtomicUsize::new(0),
            files: AtomicUsize::new(0),
            lock: Mutex::new(()),
        }
    }

    fn add_children(&self, children: &[Node]) {
        let mut folders = 0;
        let mut files = 0;
        for c in children {
            match c.node_type {
                NodeType::Folder => folders += 1,
                NodeType::File => files += 1,
            }
        }
        if folders > 0 {
            self.folders.fetch_add(folders, Ordering::Relaxed);
        }
        if files > 0 {
            self.files.fetch_add(files, Ordering::Relaxed);
        }
        self.render();
    }

    fn render(&self) {
        let _guard = self.lock.lock().unwrap();
        let folders = self.folders.load(Ordering::Relaxed);
        let files = self.files.load(Ordering::Relaxed);
        let folder_label = if folders == 1 { "folder" } else { "folders" };
        let file_label = if files == 1 { "file" } else { "files" };
        let mut stdout = io::stdout();
        write!(
            stdout,
            "\r\x1B[2K{}  {} {}, {} {}",
            "discovering".blue(),
            folders.to_string().white(),
            folder_label,
            files.to_string().white(),
            file_label
        )
        .ok();
        stdout.flush().ok();
    }

    fn finish(&self) {
        let _guard = self.lock.lock().unwrap();
        let mut stdout = io::stdout();
        write!(stdout, "\r\x1B[2K").ok();
        stdout.flush().ok();
    }
}

fn fetch_folder(id: &str) -> Result<String> {
    let url = format!("https://drive.google.com/drive/folders/{}?hl=en", id);
    let body = reqwest::blocking::get(url)?.text()?;
    Ok(body)
}

fn fetch_recursive(node: &mut Node, progress: &DiscoveryProgress) -> Result<()> {
    let results: Vec<(String, Vec<Node>)> = node
        .children
        .par_iter()
        .filter_map(|child| {
            if let NodeType::Folder = child.node_type
                && let Some(ref id) = child.id
            {
                let html = fetch_folder(id).ok()?;
                let sub = parse_html(&html).ok()?;
                progress.add_children(&sub.children);
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
            fetch_recursive(child, progress)?;
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

    fetch_from_id(id)
}

pub fn fetch_from_id(id: &str) -> Result<Node> {
    let progress = DiscoveryProgress::new();
    progress.render();

    let res = (|| {
        let html = fetch_folder(id)?;
        let mut root = parse_html(&html)?;
        root.id = Some(id.to_string());

        progress.folders.fetch_add(1, Ordering::Relaxed);
        progress.add_children(&root.children);

        fetch_recursive(&mut root, &progress)?;
        Ok(root)
    })();

    progress.finish();
    res
}
