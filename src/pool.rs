use anyhow::Result;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::path::PathBuf;

pub struct FileTask {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

pub fn run<F>(tasks: Vec<FileTask>, f: F, threads: usize) -> Vec<String>
where
    F: Fn(&FileTask) -> Result<()> + Sync + Send,
{
    let pool = ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("failed to build thread pool");

    pool.install(|| {
        tasks
            .par_iter()
            .filter_map(|task| f(task).err().map(|e| format!("{}: {}", task.name, e)))
            .collect()
    })
}
