use crate::utils;
use colored::Colorize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct StdoutState {
    active_names: Vec<String>,
    drawn_active: usize,
}

struct ProgressShared {
    total: usize,              // total work units (read-only, no sync needed)
    done: AtomicUsize,         // completed units
    total_bytes: AtomicUsize,  // total bytes downloaded
    started: Instant,          // start time (read-only)
    bar_width: usize,          // width of the progress bar in chars
    state: Mutex<StdoutState>, // exclusive access to state & stdout
}

impl ProgressShared {
    fn bar_str(&self, done: usize, active: usize) -> String {
        let total = self.total;
        let pct = done as f64 / total as f64;
        let filled = (pct * self.bar_width as f64) as usize;
        let empty = self.bar_width - filled;

        format!(
            "progress  [{}{}]  {}/{} files  {} active",
            "=".repeat(filled).blue(),
            " ".repeat(empty),
            done.to_string().white(),
            total.to_string().bright_black(),
            active.to_string().yellow(),
        )
    }

    fn update_stdout<F>(&self, f: F)
    where
        F: FnOnce(&mut StdoutState) -> Option<String>,
    {
        let mut state = self.state.lock().unwrap();
        let prev_active = state.drawn_active;

        let msg = f(&mut state);

        let active = state.active_names.len();
        state.drawn_active = active;

        let done = self.done.load(Ordering::Relaxed);

        let mut buf = String::new();
        buf.push('\r');
        if prev_active > 0 {
            buf.push_str(&format!("\x1B[{}A", prev_active));
        }
        buf.push_str("\x1B[J");

        if let Some(m) = msg {
            buf.push_str(&m);
            buf.push('\n');
        }

        for name in &state.active_names {
            buf.push_str(&format!(
                "  {} {}\n",
                "downloading".bright_black(),
                name.white()
            ));
        }

        buf.push_str(&self.bar_str(done, active));

        let mut stdout = io::stdout();
        write!(stdout, "{}", buf).ok();
        stdout.flush().ok();
    }
}

pub struct Progress {
    inner: Arc<ProgressShared>,
}

impl Progress {
    pub fn new(total: usize) -> Self {
        Self {
            inner: Arc::new(ProgressShared {
                total,
                done: AtomicUsize::new(0),
                total_bytes: AtomicUsize::new(0),
                started: Instant::now(),
                bar_width: 28,
                state: Mutex::new(StdoutState {
                    active_names: Vec::new(),
                    drawn_active: 0,
                }),
            }),
        }
    }

    pub fn handle(&self) -> ProgressHandle {
        ProgressHandle {
            inner: Arc::clone(&self.inner),
        }
    }

    pub fn finish(&self) {
        let elapsed = self.inner.started.elapsed().as_secs_f64().max(0.001);
        let bytes = self.inner.total_bytes.load(Ordering::Relaxed) as u64;

        let state = self.inner.state.lock().unwrap();
        let prev_active = state.drawn_active;

        let mut buf = String::new();
        buf.push('\r');
        if prev_active > 0 {
            buf.push_str(&format!("\x1B[{}A", prev_active));
        }
        buf.push_str("\x1B[J");

        buf.push_str(&format!(
            "{}  size: {}  time: {}\n",
            "download successful".blue(),
            utils::fmt_size(bytes).yellow(),
            utils::fmt_duration(elapsed as u64).cyan(),
        ));

        let mut stdout = io::stdout();
        write!(stdout, "{}", buf).ok();
        stdout.flush().ok();
    }
}

pub struct ProgressHandle {
    inner: Arc<ProgressShared>,
}

impl ProgressHandle {
    pub fn start(&self, name: &str) {
        let name_owned = name.to_string();
        self.inner.update_stdout(|state| {
            state.active_names.push(name_owned);
            None
        });
    }

    pub fn complete(&self, name: &str, size: u64) {
        self.inner.done.fetch_add(1, Ordering::Relaxed);
        self.inner
            .total_bytes
            .fetch_add(size as usize, Ordering::Relaxed);
        let name_owned = name.to_string();
        self.inner.update_stdout(|state| {
            state.active_names.retain(|n| n != &name_owned);
            Some(format!("  {} {}", "downloaded ".green(), name.white()))
        });
    }

    pub fn complete_err(&self, name: &str) {
        self.inner.done.fetch_add(1, Ordering::Relaxed);
        let name_owned = name.to_string();
        self.inner.update_stdout(|state| {
            state.active_names.retain(|n| n != &name_owned);
            Some(format!("  {} {}", "error      ".red(), name.white()))
        });
    }
}
