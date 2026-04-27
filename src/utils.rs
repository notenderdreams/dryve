use colored::Colorize;
use std::io;
use std::io::Write;

pub fn fmt_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

pub fn fmt_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    let mut val = bytes as f64;
    while val >= 1024.0 && i < units.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", val, units[i])
}

pub fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    sanitized.trim_end_matches(&[' ', '.'][..]).to_string()
}

pub fn prompt_confirmation(prompt: &str) -> anyhow::Result<bool> {
    print!("{}{}", prompt, " [Y/n]:".blue());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let response = input.trim().to_lowercase();
    Ok(matches!(response.as_str(), "y" | "yes" | ""))
}
