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

pub fn wildcard_match(text: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }

    let starts_with_star = pattern.starts_with('*');
    let ends_with_star = pattern.ends_with('*');

    let first = parts[0];
    let last = parts[parts.len() - 1];

    if !starts_with_star && !text.starts_with(first) {
        return false;
    }
    if !ends_with_star && !text.ends_with(last) {
        return false;
    }
    if !starts_with_star && !ends_with_star && text.len() < first.len() + last.len() {
        return false;
    }

    let mut rem = text;
    if !starts_with_star {
        rem = &rem[first.len()..];
    }
    if !ends_with_star {
        rem = &rem[..rem.len().saturating_sub(last.len())];
    }

    let middle = &parts[1..parts.len() - 1];
    for &part in middle {
        if part.is_empty() {
            continue;
        }
        match rem.find(part) {
            Some(idx) => {
                rem = &rem[idx + part.len()..];
            }
            None => return false,
        }
    }

    true
}

/// Matches a path string against a pattern, case-insensitively.
/// Supports wildcard matching on either filename or the full path.
pub fn path_matches_pattern(path_str: &str, pattern: &str) -> bool {
    let path_lower = path_str.to_lowercase().replace('\\', "/");
    let pattern_lower = pattern.to_lowercase().replace('\\', "/");

    let filename = path_lower.rsplit('/').next().unwrap_or(&path_lower);

    if pattern_lower.contains('*') {
        if pattern_lower.contains('/') {
            wildcard_match(&path_lower, &pattern_lower)
        } else {
            wildcard_match(filename, &pattern_lower) || wildcard_match(&path_lower, &pattern_lower)
        }
    } else if pattern_lower.contains('/') {
        path_lower.contains(&pattern_lower)
    } else {
        filename == pattern_lower || path_lower.contains(&pattern_lower)
    }
}

/// Checks whether a path passes both include and exclude filters.
/// - If include patterns exist, path must match at least one.
/// - If exclude patterns exist, path matching any pattern is rejected.
pub fn path_passes_filter(path: &std::path::Path, include: &[String], exclude: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    if !include.is_empty() {
        let matched = include
            .iter()
            .any(|pat| path_matches_pattern(&path_str, pat));
        if !matched {
            return false;
        }
    }

    if !exclude.is_empty() {
        let excluded = exclude
            .iter()
            .any(|pat| path_matches_pattern(&path_str, pat));
        if excluded {
            return false;
        }
    }

    true
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

    #[test]
    fn test_path_matches_pattern() {
        assert!(path_matches_pattern("folder/file.pdf", "*.pdf"));
        assert!(path_matches_pattern("folder/file.pdf", "file.pdf"));
        assert!(path_matches_pattern("folder/file.pdf", "folder/*"));
        assert!(path_matches_pattern("folder/file.pdf", "FOLDER/*.pdf"));
        assert!(!path_matches_pattern("folder/file.pdf", "*.txt"));
        assert!(!path_matches_pattern("folder/file.pdf", "archive/*"));
    }

    #[test]
    fn test_path_passes_filter() {
        let p = std::path::Path::new("docs/report.pdf");
        assert!(path_passes_filter(p, &[], &[]));

        let inc = vec!["*.pdf".to_string()];
        assert!(path_passes_filter(p, &inc, &[]));

        let inc_txt = vec!["*.txt".to_string()];
        assert!(!path_passes_filter(p, &inc_txt, &[]));

        let exc = vec!["docs/*".to_string()];
        assert!(!path_passes_filter(p, &inc, &exc));

        let exc_other = vec!["archive/*".to_string()];
        assert!(path_passes_filter(p, &inc, &exc_other));
    }
}
