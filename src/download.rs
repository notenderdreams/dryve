use crate::node::{Node, NodeType};
use crate::pool::{FileTask, run};
use crate::utils::sanitize_name;
use anyhow::Result;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_RANGE, RANGE, RETRY_AFTER};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
struct FatalError(String);

impl std::fmt::Display for FatalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FatalError {}

#[derive(Debug)]
struct RetryableError {
    status: StatusCode,
    retry_after: Option<u64>,
}

impl std::fmt::Display for RetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retryable status: {}", self.status)
    }
}

impl std::error::Error for RetryableError {}

fn get_part_path(path: &Path) -> PathBuf {
    let mut part_name = path.file_name().unwrap_or_default().to_os_string();
    part_name.push(".dryve.part");
    path.with_file_name(part_name)
}

fn parse_content_range_total(header_val: &str) -> Option<u64> {
    let slash_idx = header_val.rfind('/')?;
    let total_str = header_val[slash_idx + 1..].trim();
    if total_str == "*" {
        return None;
    }
    total_str.parse::<u64>().ok()
}

fn calculate_backoff(attempt: u32, retry_after: Option<u64>) -> Duration {
    if let Some(secs) = retry_after {
        return Duration::from_secs(secs);
    }
    let base_ms = (500u64 * (1 << attempt.min(5))).min(10_000);
    let jitter_ms = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        % 500) as u64;
    Duration::from_millis(base_ms + jitter_ms)
}

fn is_fatal_error(err: &anyhow::Error) -> bool {
    if err.downcast_ref::<FatalError>().is_some() {
        return true;
    }
    if let Some(re) = err.downcast_ref::<reqwest::Error>()
        && let Some(status) = re.status()
        && (status == StatusCode::NOT_FOUND
            || status == StatusCode::FORBIDDEN
            || status == StatusCode::UNAUTHORIZED)
    {
        return true;
    }
    if let Some(ioe) = err.downcast_ref::<std::io::Error>() {
        match ioe.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                return true;
            }
            _ => {}
        }
    }
    false
}

pub fn is_google_workspace_mime(mime_type: Option<&str>) -> bool {
    let Some(mime) = mime_type else {
        return false;
    };
    matches!(
        mime,
        "application/vnd.google-apps.document"
            | "application/vnd.google-apps.spreadsheet"
            | "application/vnd.google-apps.presentation"
            | "application/vnd.google-apps.drawing"
    )
}

pub fn google_workspace_export_filename(mime_type: Option<&str>, name: &str) -> String {
    if !is_google_workspace_mime(mime_type) {
        return name.to_string();
    }
    let ext = ".pdf";
    if name.to_lowercase().ends_with(ext) {
        name.to_string()
    } else {
        format!("{}{}", name, ext)
    }
}

pub fn google_workspace_export_url(mime_type: Option<&str>, id: &str) -> Option<String> {
    let mime = mime_type?;
    match mime {
        "application/vnd.google-apps.document" => Some(format!(
            "https://docs.google.com/document/d/{}/export?format=pdf",
            id
        )),
        "application/vnd.google-apps.spreadsheet" => Some(format!(
            "https://docs.google.com/spreadsheets/d/{}/export?format=pdf",
            id
        )),
        "application/vnd.google-apps.presentation" => Some(format!(
            "https://docs.google.com/presentation/d/{}/export/pdf",
            id
        )),
        "application/vnd.google-apps.drawing" => Some(format!(
            "https://docs.google.com/drawings/d/{}/export/pdf",
            id
        )),
        _ => None,
    }
}

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
                    mime_type: node.mime_type.clone(),
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

fn check_status(response: &reqwest::blocking::Response) -> Result<()> {
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(RetryableError {
            status,
            retry_after,
        }
        .into());
    }

    if status == StatusCode::NOT_FOUND
        || status == StatusCode::FORBIDDEN
        || status == StatusCode::UNAUTHORIZED
    {
        return Err(FatalError(format!("Google Drive error: HTTP {}", status)).into());
    }

    Ok(())
}

fn download_file_attempt(client: &Client, task: &FileTask, part_path: &Path) -> Result<u64> {
    let export_url = google_workspace_export_url(task.mime_type.as_deref(), &task.id);

    let start_pos = if export_url.is_some() {
        // Google Docs export endpoints don't reliably support HTTP Range resume
        0
    } else if part_path.exists() {
        fs::metadata(part_path).map(|m| m.len()).unwrap_or(0)
    } else if task.path.exists() {
        fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let url = if let Some(ref export_url) = export_url {
        export_url.clone()
    } else {
        format!("https://drive.google.com/uc?export=download&id={}", task.id)
    };

    let mut req = client.get(&url);
    if start_pos > 0 {
        req = req.header(RANGE, format!("bytes={}-", start_pos));
    }
    let response = req.send()?;
    check_status(&response)?;

    let mut response = {
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if export_url.is_some() && content_type.contains("text/html") {
            anyhow::bail!(
                "Google Workspace export failed: received HTML response for {}",
                task.name
            );
        } else if content_type.contains("text/html") {
            let html = response.text()?;
            let confirm_url = extract_confirm_url(&html, &task.id)?;
            let mut confirm_req = client.get(&confirm_url);
            if start_pos > 0 {
                confirm_req = confirm_req.header(RANGE, format!("bytes={}-", start_pos));
            }
            let confirm_resp = confirm_req.send()?;
            check_status(&confirm_resp)?;
            confirm_resp
        } else {
            response
        }
    };

    let status = response.status();
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        let range_header = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Some(total) = parse_content_range_total(range_header)
            && start_pos == total
        {
            if part_path.exists() {
                fs::rename(part_path, &task.path)?;
            }
            return Ok(0);
        }
        if part_path.exists() {
            fs::remove_file(part_path).ok();
        }
        if task.path.exists() {
            fs::remove_file(&task.path).ok();
        }
        anyhow::bail!("HTTP 416 Range Not Satisfiable; offset reset to 0");
    }

    if !status.is_success() {
        anyhow::bail!("HTTP download failed with status {}", status);
    }

    let is_partial = status == StatusCode::PARTIAL_CONTENT;

    if is_partial && !part_path.exists() && task.path.exists() {
        fs::rename(&task.path, part_path)?;
    }

    let expected_total = if is_partial {
        response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range_total)
            .or_else(|| response.content_length().map(|len| start_pos + len))
    } else if export_url.is_some() {
        // Google Docs export endpoints can omit content-length or send 0 on redirects
        None
    } else {
        response.content_length()
    };

    let mut file = if is_partial {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)?
    } else {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(part_path)?
    };

    let mut buf = [0u8; 65536];
    let mut downloaded = 0u64;

    loop {
        let n = match response.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                file.flush().ok();
                return Err(e.into());
            }
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            file.flush().ok();
            return Err(e.into());
        }
        downloaded += n as u64;
    }

    file.flush()?;

    let final_len = file.metadata()?.len();
    if let Some(expected) = expected_total
        && final_len != expected
    {
        anyhow::bail!(
            "Incomplete download: got {} bytes, expected {} bytes",
            final_len,
            expected
        );
    }

    fs::rename(part_path, &task.path)?;
    Ok(downloaded)
}

pub fn download_file(client: &Client, task: &FileTask) -> Result<u64> {
    if let Some(parent) = task.path.parent() {
        fs::create_dir_all(parent)?;
    }

    let part_path = get_part_path(&task.path);

    let initial_disk_bytes = if task.path.exists() {
        fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0)
    } else {
        fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0)
    };

    let max_attempts = 5;
    for attempt in 0..max_attempts {
        match download_file_attempt(client, task, &part_path) {
            Ok(bytes) => {
                let final_disk_bytes = fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0);
                let added = final_disk_bytes.saturating_sub(initial_disk_bytes);
                if added == 0 && bytes > 0 {
                    return Ok(bytes);
                }
                return Ok(added);
            }
            Err(e) => {
                if is_fatal_error(&e) || attempt + 1 >= max_attempts {
                    return Err(e);
                }

                let retry_after = e
                    .downcast_ref::<RetryableError>()
                    .and_then(|r| r.retry_after);
                let delay = calculate_backoff(attempt, retry_after);
                std::thread::sleep(delay);
            }
        }
    }

    let final_disk_bytes = fs::metadata(&task.path).map(|m| m.len()).unwrap_or(0);
    Ok(final_disk_bytes.saturating_sub(initial_disk_bytes))
}

pub fn download_tree(
    node: &Node,
    base: &Path,
    threads: usize,
    filter: &HashSet<PathBuf>,
) -> Result<()> {
    let mut all_tasks = Vec::new();
    collect_tasks(node, base, &mut all_tasks, false)?;

    let tasks: Vec<FileTask> = all_tasks
        .into_iter()
        .filter(|t| filter.contains(&t.path))
        .collect();

    // Ensure parent directories exist for the filtered files.
    for t in &tasks {
        if let Some(parent) = t.path.parent() {
            fs::create_dir_all(parent)?;
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_content_range_total() {
        assert_eq!(parse_content_range_total("bytes 200-1000/1001"), Some(1001));
        assert_eq!(parse_content_range_total("bytes */5000"), Some(5000));
        assert_eq!(parse_content_range_total("bytes 0-100/*"), None);
        assert_eq!(parse_content_range_total("invalid"), None);
    }

    #[test]
    fn test_get_part_path() {
        assert_eq!(
            get_part_path(Path::new("downloads/file.txt")),
            PathBuf::from("downloads/file.txt.dryve.part")
        );
        assert_eq!(
            get_part_path(Path::new("video.mp4")),
            PathBuf::from("video.mp4.dryve.part")
        );
    }

    #[test]
    fn test_calculate_backoff() {
        assert_eq!(calculate_backoff(0, Some(12)), Duration::from_secs(12));
        let b0 = calculate_backoff(0, None);
        assert!(b0 >= Duration::from_millis(500) && b0 < Duration::from_millis(1000));
        let b1 = calculate_backoff(1, None);
        assert!(b1 >= Duration::from_millis(1000) && b1 < Duration::from_millis(1500));
    }

    #[test]
    fn test_is_fatal_error() {
        let fatal = FatalError("fatal error".to_string());
        assert!(is_fatal_error(&fatal.into()));

        let retryable = RetryableError {
            status: StatusCode::TOO_MANY_REQUESTS,
            retry_after: None,
        };
        assert!(!is_fatal_error(&retryable.into()));

        let perm_denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(is_fatal_error(&perm_denied.into()));

        let conn_reset = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        assert!(!is_fatal_error(&conn_reset.into()));
    }

    #[test]
    fn test_extract_confirm_url() {
        let html = r#"<a href="/uc?export=download&id=abc&confirm=t_1234">Download</a>"#;
        let url = extract_confirm_url(html, "abc").unwrap();
        assert_eq!(
            url,
            "https://drive.google.com/uc?export=download&id=abc&confirm=t_1234"
        );
    }

    #[test]
    fn test_google_workspace_export_filename() {
        assert_eq!(
            google_workspace_export_filename(
                Some("application/vnd.google-apps.document"),
                "Project Plan"
            ),
            "Project Plan.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(
                Some("application/vnd.google-apps.document"),
                "Project Plan.pdf"
            ),
            "Project Plan.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(
                Some("application/vnd.google-apps.spreadsheet"),
                "Financials"
            ),
            "Financials.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(
                Some("application/vnd.google-apps.presentation"),
                "Pitch Deck"
            ),
            "Pitch Deck.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(
                Some("application/vnd.google-apps.drawing"),
                "Diagram"
            ),
            "Diagram.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(Some("application/pdf"), "report.pdf"),
            "report.pdf"
        );
        assert_eq!(
            google_workspace_export_filename(None, "notes.txt"),
            "notes.txt"
        );
    }

    #[test]
    fn test_google_workspace_export_url() {
        assert_eq!(
            google_workspace_export_url(Some("application/vnd.google-apps.document"), "doc123"),
            Some("https://docs.google.com/document/d/doc123/export?format=pdf".to_string())
        );
        assert_eq!(
            google_workspace_export_url(
                Some("application/vnd.google-apps.spreadsheet"),
                "sheet123"
            ),
            Some("https://docs.google.com/spreadsheets/d/sheet123/export?format=pdf".to_string())
        );
        assert_eq!(
            google_workspace_export_url(
                Some("application/vnd.google-apps.presentation"),
                "pres123"
            ),
            Some("https://docs.google.com/presentation/d/pres123/export/pdf".to_string())
        );
        assert_eq!(
            google_workspace_export_url(Some("application/vnd.google-apps.drawing"), "draw123"),
            Some("https://docs.google.com/drawings/d/draw123/export/pdf".to_string())
        );
        assert_eq!(
            google_workspace_export_url(Some("application/pdf"), "pdf123"),
            None
        );
        assert_eq!(google_workspace_export_url(None, "none123"), None);
    }
}
