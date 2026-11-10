use crate::download::{build_client, download_file, get_part_path, is_google_workspace_mime};
use crate::node::{Node, NodeType};
use crate::pool::{self, FileTask};
use crate::selector::{self, ItemKind};
use crate::utils::{path_passes_filter, sanitize_name};
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, PartialEq, Eq)]
enum State {
    Ok,
    Missing,
    SizeMismatch { expected: u64, actual: u64 },
    NotFile,
    UnknownSize,
}

impl State {
    fn needs_repair(&self) -> bool {
        matches!(self, Self::Missing | Self::SizeMismatch { .. })
    }

    fn is_problem(&self) -> bool {
        self.needs_repair() || matches!(self, Self::NotFile)
    }
}

struct Entry {
    path: PathBuf,
    node: Node,
    state: State,
}

// Drive's size for a Workspace document is not the size of its PDF export.
fn expected_size(node: &Node) -> Option<u64> {
    if is_google_workspace_mime(node.mime_type.as_deref()) {
        None
    } else {
        node.size
    }
}

/// Reject links and non-directory ancestors rather than inspecting or repairing
/// a different location through them. A missing ancestor means a missing file.
fn inspect(base: &Path, path: &Path, expected: Option<u64>) -> Result<State> {
    let mut current = base.to_path_buf();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        current.push(component);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(State::Missing),
            Err(err) => {
                return Err(err).with_context(|| format!("Cannot check {}", current.display()));
            }
        };
        if metadata.file_type().is_symlink()
            || (components.peek().is_some() && !metadata.is_dir())
            || (components.peek().is_none() && !metadata.is_file())
        {
            return Ok(State::NotFile);
        }
        if components.peek().is_none() {
            return Ok(match expected {
                Some(expected) if metadata.len() != expected => State::SizeMismatch {
                    expected,
                    actual: metadata.len(),
                },
                Some(_) => State::Ok,
                None => State::UnknownSize,
            });
        }
    }
    Ok(State::NotFile)
}

fn collect_entries(
    node: &Node,
    base: &Path,
    relative: &Path,
    include_ignored: bool,
    include: &[String],
    exclude: &[String],
    entries: &mut Vec<Entry>,
) -> Result<()> {
    if node.ignored && !include_ignored {
        return Ok(());
    }
    match node.node_type {
        NodeType::Folder => {
            for child in &node.children {
                let name = sanitize_name(&child.name);
                anyhow::ensure!(
                    !name.is_empty(),
                    "Invalid tracked filename: {:?}",
                    child.name
                );
                collect_entries(
                    child,
                    base,
                    &relative.join(name),
                    include_ignored,
                    include,
                    exclude,
                    entries,
                )?;
            }
        }
        NodeType::File if path_passes_filter(relative, include, exclude) => {
            anyhow::ensure!(
                !relative
                    .to_string_lossy()
                    .eq_ignore_ascii_case("dryve.json"),
                "Tracked file conflicts with dryve.json"
            );
            entries.push(Entry {
                path: relative.to_path_buf(),
                node: node.clone(),
                state: inspect(base, relative, expected_size(node))?,
            });
        }
        NodeType::File => {}
    }
    Ok(())
}

fn report(entries: &[Entry]) {
    for entry in entries {
        let detail = match entry.state {
            State::Ok => continue,
            State::Missing => "missing".to_string(),
            State::SizeMismatch { expected, actual } => {
                format!("size mismatch: expected {expected} bytes, found {actual}")
            }
            State::NotFile => {
                "blocked by a directory, symbolic link, or non-directory parent; resolve manually"
                    .to_string()
            }
            State::UnknownSize => "present; no expected size available".to_string(),
        };
        println!("  {}: {}", entry.path.display(), detail);
    }
    println!(
        "Checked {} files: {} OK, {} missing, {} size mismatches, {} blocked, {} without size information.",
        entries.len(),
        entries.iter().filter(|e| e.state == State::Ok).count(),
        entries.iter().filter(|e| e.state == State::Missing).count(),
        entries
            .iter()
            .filter(|e| matches!(e.state, State::SizeMismatch { .. }))
            .count(),
        entries.iter().filter(|e| e.state == State::NotFile).count(),
        entries
            .iter()
            .filter(|e| e.state == State::UnknownSize)
            .count(),
    );
}

fn repair_branch(node: &Node, path: &Path, affected: &HashSet<PathBuf>) -> Option<Node> {
    let mut branch = node.clone();
    branch.ignored = false;
    match node.node_type {
        NodeType::File => affected.contains(path).then_some(branch),
        NodeType::Folder => {
            branch.children = node
                .children
                .iter()
                .filter_map(|child| {
                    repair_branch(child, &path.join(sanitize_name(&child.name)), affected)
                })
                .collect();
            (!branch.children.is_empty()).then_some(branch)
        }
    }
}

pub fn run(
    folder: Option<&Path>,
    repair: bool,
    include_ignored: bool,
    threads: usize,
    yes: bool,
    include: &[String],
    exclude: &[String],
) -> Result<()> {
    let base = folder.unwrap_or(Path::new("."));
    let metadata_path = base.join("dryve.json");
    let root: Node = serde_json::from_str(
        &fs::read_to_string(&metadata_path)
            .with_context(|| format!("Cannot read {}", metadata_path.display()))?,
    )
    .context("Invalid dryve.json")?;
    let mut entries = Vec::new();
    collect_entries(
        &root,
        base,
        Path::new(""),
        include_ignored,
        include,
        exclude,
        &mut entries,
    )?;
    let unique_paths: HashSet<_> = entries.iter().map(|entry| &entry.path).collect();
    anyhow::ensure!(
        unique_paths.len() == entries.len(),
        "Multiple tracked files map to the same local path"
    );
    println!("Checking {}…", base.display());
    report(&entries);

    if !entries.iter().any(|e| e.state.is_problem()) {
        return Ok(());
    }
    if !repair {
        println!(
            "Use 'dryve check --repair' with the same path and filters to repair affected files."
        );
        bail!("Local download check failed");
    }

    // Only affected files appear; ignored files explicitly included with -I are
    // selectable without changing their persisted ignore setting.
    let affected: HashSet<_> = entries
        .iter()
        .filter(|e| e.state.needs_repair())
        .map(|e| e.path.clone())
        .collect();
    let roots: Vec<_> = root
        .children
        .iter()
        .filter_map(|child| {
            repair_branch(child, Path::new(&sanitize_name(&child.name)), &affected)
                .map(|node| (node, PathBuf::new(), ItemKind::Modified))
        })
        .collect();
    if roots.is_empty() {
        bail!("Blocked paths must be resolved manually");
    }
    println!(
        "Repairs fetch the current remote contents. Size mismatches may be local edits; selected files will be replaced."
    );
    let selection = if yes {
        selector::select_all(&roots, &[], &[])
    } else {
        match selector::select(&roots, &[], &[])? {
            Some(selection) => selection,
            None => {
                println!("Repair cancelled.");
                return Ok(());
            }
        }
    };
    let candidates: Vec<_> = entries
        .iter()
        .filter(|e| selection.selected.contains(&e.path))
        .collect();
    if candidates.is_empty() {
        bail!("No files selected for repair");
    }
    let mut expected = HashMap::new();
    let mut tasks = Vec::new();
    for entry in candidates {
        let id = entry
            .node
            .id
            .clone()
            .with_context(|| format!("No Drive ID saved for {}", entry.path.display()))?;
        let path = base.join(&entry.path);
        expected.insert(path.clone(), expected_size(&entry.node));
        tasks.push(FileTask {
            id,
            name: entry.path.display().to_string(),
            path,
            mime_type: entry.node.mime_type.clone(),
        });
    }
    let client = build_client()?;
    let errors = pool::run(
        tasks,
        |task, _| {
            let relative = task.path.strip_prefix(base)?;
            let state = inspect(base, relative, expected[&task.path])?;
            anyhow::ensure!(state != State::NotFile, "Repair path is blocked");
            replace_download(task, expected[&task.path], |staged| {
                download_file(&client, staged)
            })
        },
        threads,
    );
    for error in &errors {
        eprintln!("Repair failed: {error}");
    }
    for entry in &mut entries {
        entry.state = inspect(base, &entry.path, expected_size(&entry.node))?;
    }
    report(&entries);
    if !errors.is_empty() || entries.iter().any(|e| e.state.is_problem()) {
        bail!("Some files remain unrepaired");
    }
    println!("Repair complete.");
    Ok(())
}

/// A private sibling directory ensures no existing file or stale partial is
/// resumed. Rename publishes a completed replacement on the same filesystem.
struct StagingDirectory(PathBuf);

impl StagingDirectory {
    fn create(parent: &Path) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let path = parent.join(format!(
                ".dryve-repair-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err.into()),
            }
        }
        bail!("Cannot create a repair staging directory")
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn replace_download(
    task: &FileTask,
    expected: Option<u64>,
    download: impl FnOnce(&FileTask) -> Result<u64>,
) -> Result<u64> {
    let parent = task
        .path
        .parent()
        .context("Repair target has no parent directory")?;
    fs::create_dir_all(parent)?;
    let staging = StagingDirectory::create(parent)?;
    let staged = FileTask {
        id: task.id.clone(),
        name: task.name.clone(),
        path: staging.0.join("download"),
        mime_type: task.mime_type.clone(),
    };
    let bytes = download(&staged)?;
    let metadata = fs::symlink_metadata(&staged.path)?;
    anyhow::ensure!(metadata.is_file(), "Replacement is not a regular file");
    if let Some(expected) = expected {
        anyhow::ensure!(
            metadata.len() == expected,
            "Remote download size differs from saved metadata; run dryve sync to review remote changes"
        );
    }
    // A stale partial would otherwise be preferred over the repaired file by a
    // subsequent download. Discard it only after the replacement is validated.
    match fs::remove_file(get_part_path(&task.path)) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err).context("Cannot remove stale partial download"),
    }
    fs::rename(&staged.path, &task.path).context("Cannot replace local file")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: Option<u64>) -> Node {
        let mut node = Node::new(name.into(), NodeType::File, Some(name.into()));
        node.size = size;
        node
    }

    fn fixture() -> StagingDirectory {
        StagingDirectory::create(&std::env::temp_dir()).unwrap()
    }

    #[test]
    fn checks_missing_size_unknown_and_blocked_files() {
        let dir = fixture();
        fs::write(dir.0.join("ok"), b"abc").unwrap();
        fs::write(dir.0.join("short"), b"ab").unwrap();
        fs::write(dir.0.join("long"), b"abcd").unwrap();
        fs::write(dir.0.join("empty"), b"").unwrap();
        fs::create_dir(dir.0.join("directory")).unwrap();
        let inspect_file =
            |path: &str, expected| inspect(&dir.0, Path::new(path), expected).unwrap();
        assert_eq!(inspect_file("ok", Some(3)), State::Ok);
        assert_eq!(inspect_file("empty", Some(0)), State::Ok);
        assert_eq!(
            inspect_file("short", Some(3)),
            State::SizeMismatch {
                expected: 3,
                actual: 2
            }
        );
        assert_eq!(
            inspect_file("long", Some(3)),
            State::SizeMismatch {
                expected: 3,
                actual: 4
            }
        );
        assert_eq!(inspect_file("ok", None), State::UnknownSize);
        assert_eq!(inspect_file("missing/sub/file", Some(1)), State::Missing);
        assert_eq!(inspect_file("directory", Some(3)), State::NotFile);
        assert_eq!(inspect_file("ok/child", Some(3)), State::NotFile);
    }

    #[test]
    fn honors_inherited_ignores_and_filters() {
        let dir = fixture();
        let mut root = Node::new(
            "Remote folder".into(),
            NodeType::Folder,
            Some("root".into()),
        );
        let mut ignored = Node::new("archive".into(), NodeType::Folder, None);
        ignored.ignored = true;
        ignored.children = vec![file("old.pdf", Some(3))];
        root.children = vec![
            file("new.pdf", Some(3)),
            file("notes.txt", Some(3)),
            ignored,
        ];
        let mut entries = Vec::new();
        collect_entries(
            &root,
            &dir.0,
            Path::new(""),
            false,
            &["*.pdf".into()],
            &[],
            &mut entries,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, Path::new("new.pdf"));
        assert_eq!(entries[0].state, State::Missing);

        entries.clear();
        collect_entries(
            &root,
            &dir.0,
            Path::new(""),
            true,
            &["*.pdf".into()],
            &["new*".into()],
            &mut entries,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, Path::new("archive/old.pdf"));
        assert!(root.children[2].ignored);
    }

    #[test]
    fn workspace_exports_have_unknown_size() {
        let mut document = file("document.pdf", Some(0));
        document.mime_type = Some("application/vnd.google-apps.document".into());
        assert_eq!(expected_size(&document), None);
        assert_eq!(expected_size(&file("empty.txt", Some(0))), Some(0));
    }

    #[test]
    fn repair_selection_preserves_hierarchy_and_includes_only_affected_files() {
        let mut folder = Node::new("docs".into(), NodeType::Folder, None);
        folder.ignored = true;
        folder.children = vec![file("missing.pdf", Some(3)), file("ok.pdf", Some(3))];
        let affected = HashSet::from([PathBuf::from("docs/missing.pdf")]);
        let branch = repair_branch(&folder, Path::new("docs"), &affected).unwrap();
        assert!(!branch.ignored);
        assert_eq!(branch.children.len(), 1);
        let selection =
            selector::select_all(&[(branch, PathBuf::new(), ItemKind::Modified)], &[], &[]);
        assert_eq!(selection.selected, affected);
        assert!(selection.ignored.is_empty());
        assert!(folder.ignored);
        assert_eq!(folder.children.len(), 2);
    }

    #[test]
    fn read_only_check_works_offline_and_leaves_metadata_unchanged() {
        let dir = fixture();
        let mut root = Node::new("Remote folder".into(), NodeType::Folder, None);
        root.children = vec![file("data.bin", Some(3))];
        let json = serde_json::to_string(&root).unwrap();
        let metadata_path = dir.0.join("dryve.json");
        fs::write(&metadata_path, &json).unwrap();
        assert!(run(Some(&dir.0), false, false, 2, true, &[], &[]).is_err());
        assert!(!dir.0.join("data.bin").exists());
        fs::write(dir.0.join("data.bin"), b"abc").unwrap();
        run(Some(&dir.0), false, false, 2, true, &[], &[]).unwrap();
        assert_eq!(fs::read_to_string(metadata_path).unwrap(), json);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn failed_repairs_preserve_original_and_clean_up_staging() {
        let dir = fixture();
        let path = dir.0.join("data.bin");
        fs::write(&path, b"original").unwrap();
        let task = FileTask {
            id: "data".into(),
            name: "data.bin".into(),
            path: path.clone(),
            mime_type: None,
        };
        let error = replace_download(&task, Some(3), |staged| {
            fs::write(&staged.path, b"ab")?;
            bail!("transfer interrupted")
        });
        assert!(error.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);

        let error = replace_download(&task, Some(3), |staged| {
            fs::write(&staged.path, b"different remote version")?;
            Ok(24)
        });
        assert!(error.unwrap_err().to_string().contains("sync"));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn repairs_replace_fresh_contents_and_ignore_stale_partials() {
        let dir = fixture();
        let path = dir.0.join("nested/data.bin");
        let task = FileTask {
            id: "data".into(),
            name: "data.bin".into(),
            path: path.clone(),
            mime_type: None,
        };
        for original_exists in [false, true] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if original_exists {
                fs::write(&path, b"corrupt").unwrap();
            }
            let partial = path.with_file_name("data.bin.dryve.part");
            fs::write(&partial, b"bad partial").unwrap();
            replace_download(&task, Some(3), |staged| {
                assert!(!staged.path.exists());
                fs::write(&staged.path, b"abc")?;
                Ok(3)
            })
            .unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"abc");
            assert!(!partial.exists());
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symbolic_links_and_linked_parent_directories() {
        use std::os::unix::fs::symlink;
        let dir = fixture();
        let other = fixture();
        fs::write(other.0.join("data"), b"abc").unwrap();
        symlink(other.0.join("data"), dir.0.join("linked-file")).unwrap();
        symlink(&other.0, dir.0.join("linked-folder")).unwrap();
        assert_eq!(
            inspect(&dir.0, Path::new("linked-file"), Some(3)).unwrap(),
            State::NotFile
        );
        assert_eq!(
            inspect(&dir.0, Path::new("linked-folder/data"), Some(3)).unwrap(),
            State::NotFile
        );
        assert_eq!(fs::read(other.0.join("data")).unwrap(), b"abc");
    }
}
