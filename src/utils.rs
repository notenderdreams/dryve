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

pub fn fmt_size_compact(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    let mut val = bytes as f64;
    while val >= 1024.0 && i < units.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", val, units[i])
    }
}

pub fn fmt_size_badge(bytes: u64) -> String {
    format!("[ {:>7} ]", fmt_size_compact(bytes))
}

pub fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    sanitized.trim_end_matches(&[' ', '.'][..]).to_string()
}

pub fn open_in_browser(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = std::process::Command::new("xdg-open");

    let status = cmd.arg(url).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            println!("Failed to open browser automatically. Link: {}", url);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_size_badge() {
        assert_eq!(fmt_size_badge(512), "[   512 B ]");
        assert_eq!(fmt_size_badge(2_516_582), "[  2.4 MB ]");
        assert_eq!(fmt_size_badge(1_288_490_188), "[  1.2 GB ]");
        assert_eq!(fmt_size_badge(9_126_805_504), "[  8.5 GB ]");
    }
}
