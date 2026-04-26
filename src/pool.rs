use crate::utils::{Progress, ProgressHandle};
use anyhow::Result;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::io::{self, Write};
use std::path::PathBuf;

pub struct FileTask {
    pub id:   String,
    pub name: String,
    pub path: PathBuf,
}

pub fn run<F>(tasks: Vec<FileTask>, f: F, threads: usize) -> Vec<String>
where
    F: Fn(&FileTask, &ProgressHandle) -> Result<u64> + Sync + Send,
{
    let progress = Progress::new(tasks.len());

    print!(
        "progress  [{}]  0/{} files  0 active  0.0/s",
        " ".repeat(28),
        tasks.len()
    );
    io::stdout().flush().ok();

    let pool = ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("failed to build thread pool");

    let errors = pool.install(|| {
        tasks
            .par_iter()
            .filter_map(|task| {
                let handle = progress.handle();
                handle.start(&task.name);

                let result = f(task, &handle);

                match result {
                    Ok(size) => {
                        handle.complete(&task.name, size);
                        None
                    }
                    Err(e) => {
                        handle.complete_err(&task.name);
                        Some(format!("{}: {}", task.name, e))
                    }
                }
            })
            .collect()
    });

    progress.finish();
    errors
}