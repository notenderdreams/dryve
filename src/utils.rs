use colored::Colorize;
use std::io::{self, Write};
use std::time::Instant;

pub struct Progress {
    total: Option<u64>,
    downloaded: u64,
    started: Instant,
    bar_width: usize,
}

impl Progress {
    pub fn new(total: Option<u64>) -> Self {
        Self {
            total,
            downloaded: 0,
            started: Instant::now(),
            bar_width: 24,
        }
    }

    pub fn update(&mut self, chunk: u64) {
        self.downloaded += chunk;
        self.render();
    }

    pub fn finish(&self) {
        println!();
    }

    fn render(&self) {
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        let rate = self.downloaded as f64 / elapsed;

        let bar = match self.total {
            Some(total) if total > 0 => {
                let pct = (self.downloaded as f64 / total as f64).min(1.0);
                let filled = (pct * self.bar_width as f64) as usize;
                let empty = self.bar_width - filled;
                format!(
                    "[{}{}]",
                    "=".repeat(filled).blue(),
                    " ".repeat(empty)
                )
            }
            _ => {
                let pos = (self.downloaded / 8192) as usize % (self.bar_width + 1);
                let mut buf = vec![b' '; self.bar_width];
                if pos < self.bar_width {
                    buf[pos] = b'=';
                }
                format!("[{}]", String::from_utf8_lossy(&buf).to_string().blue())
            }
        };

        let size_str = fmt_bytes(self.downloaded);
        let total_str = self.total.map(fmt_bytes).unwrap_or_default();
        let rate_str = fmt_bytes(rate as u64);

        let right = match self.total {
            Some(_) => format!(" {} / {}  {}ps", size_str, total_str, rate_str),
            None    => format!(" {}  {}ps", size_str, rate_str),
        };

        print!("\r  {}{}", bar, right);
        io::stdout().flush().ok();
    }
}

fn fmt_bytes(b: u64) -> String {
    match b {
        0..=999         => format!("{} B",     b),
        1_000..=999_999 => format!("{:.1} KB", b as f64 / 1_000.0),
        _               => format!("{:.1} MB", b as f64 / 1_000_000.0),
    }
}