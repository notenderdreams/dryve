use crate::download::{self, collect_tasks};
use crate::drive::fetch_from_id;
use crate::node::{Diff, Node};
use crate::pool::FileTask;
use crate::utils::prompt_confirmation;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

struct SyncPlan {
    added: Vec<FileTask>,
    updated: Vec<FileTask>,
    removed: Vec<FileTask>,
}

impl SyncPlan {
    fn from_diff(diff: &Diff) -> Result<Self> {
        let mut added = Vec::new();
        for (path, node) in &diff.added {
            if let Some(base) = path.parent() {
                collect_tasks(node, base, &mut added, true)?;
            }
        }

        let mut removed = Vec::new();
        for (path, node) in &diff.removed {
            if let Some(base) = path.parent() {
                collect_tasks(node, base, &mut removed, false)?;
            }
        }

        let mut updated = Vec::new();
        for (path, _old_node, new_node) in &diff.updated {
            if let Some(base) = path.parent() {
                collect_tasks(new_node, base, &mut updated, false)?;
            }
        }

        Ok(SyncPlan {
            added,
            updated,
            removed,
        })
    }

    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }

    fn print(&self) {
        println!("Sync Plan:");
        for task in &self.added {
            println!("  [+] Added:    {}", task.path.display());
        }
        for task in &self.updated {
            println!("  [*] Modified: {}", task.path.display());
        }
        for task in &self.removed {
            println!("  [-] Removed:  {}", task.path.display());
        }
    }
}

pub fn run() -> Result<()> {
    let threads = 8;
    let json_path = Path::new("dryve.json");

    if !json_path.exists() {
        println!("Error: dryve.json not found. Please run 'dryve init' first.");
        return Ok(());
    }

    let content = fs::read_to_string(json_path)?;
    let old_root: Node = serde_json::from_str(&content)?;

    let root_id = old_root.id.as_deref().context("root node is missing ID")?;
    println!("Fetching remote tree for ID: {}...", root_id);
    let new_root = fetch_from_id(root_id)?;

    let diff = Diff::from_nodes(&old_root, &new_root);
    let plan = SyncPlan::from_diff(&diff)?;

    if plan.is_empty() {
        println!("Everything is up to date.");
        return Ok(());
    }

    plan.print();

    let client = download::build_client()?;

    // Process new files
    execute_tasks(
        &client,
        plan.added,
        "Download new files?",
        threads,
        download::download_file,
    )?;

    // Process modified files
    execute_tasks(
        &client,
        plan.updated,
        "Replace modified files?",
        threads,
        |c, t| {
            if t.path.exists() {
                fs::remove_file(&t.path)?;
            }
            download::download_file(c, t)
        },
    )?;

    // Process removed files/folders
    if !plan.removed.is_empty() && prompt_confirmation("Delete removed files?")? {
        for t in &plan.removed {
            if t.path.is_dir() {
                fs::remove_dir_all(&t.path)?;
            } else if t.path.exists() {
                fs::remove_file(&t.path)?;
            }
            println!("Removed: {}", t.path.display());
        }
    }

    let json = serde_json::to_string_pretty(&new_root)?;
    fs::write(json_path, json)?;
    println!("Synchronization complete. Updated dryve.json.");

    Ok(())
}

fn execute_tasks<F>(
    client: &reqwest::blocking::Client,
    tasks: Vec<FileTask>,
    prompt: &str,
    threads: usize,
    f: F,
) -> Result<()>
where
    F: Fn(&reqwest::blocking::Client, &FileTask) -> Result<u64> + Sync + Send,
{
    if tasks.is_empty() {
        return Ok(());
    }

    if prompt_confirmation(prompt)? {
        let errors = crate::pool::run(tasks, |task, _| f(client, task), threads);
        for e in &errors {
            eprintln!("Error: failed to process: {}", e);
        }
    }
    Ok(())
}
