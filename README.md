<div align="center">
<table>
<tr>
<th>
<img src="assets/dryve.svg" alt="dryve logo" width="400" />
</th>
</tr>

<tr>
<td align="center">

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Version](https://img.shields.io/badge/version-v0.2.1-1f2937)](#)
[![Google Drive](https://img.shields.io/badge/Google%20Drive-34A853?logo=googledrive&logoColor=white)](https://drive.google.com/)

</td>
</tr>
</table>
</div>

`dryve` is a Rust CLI for pulling and syncing publicly shared Google Drive folders.

It fetches a Drive folder tree, lets you select files in an interactive terminal UI, downloads them locally, and stores metadata in `dryve.json` so later `sync` runs can apply remote changes.

## Features

- Pull an entire Google Drive folder tree from a shared folder URL.
- Interactive terminal selector with human-readable file sizes, live selected payload tally, and search filters.
- Multi-threaded downloads using a Rayon thread pool.
- Ignore items (`i` in selector) to persistently skip them across future syncs.
- Sync mode that detects added, updated, and removed files while honoring ignored items.
- Safe filename sanitization for cross-platform local paths.

## Installation

### Homebrew (macOS / Linux)

```bash
brew install notenderdreams/tap/dryve
```

### Shell script (Linux / macOS)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/notenderdreams/dryve/releases/latest/download/dryve-installer.sh | sh
```

### PowerShell (Windows)

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/notenderdreams/dryve/releases/latest/download/dryve-installer.ps1 | iex"
```

Build from source if you prefer:

```bash
cargo build --release
```

## Commands

### `pull`

Alias: `p`

Pull and download from a Google Drive folder URL:

```bash
dryve pull "https://drive.google.com/drive/folders/<FOLDER_ID>"
# alias
dryve p "https://drive.google.com/drive/folders/<FOLDER_ID>"

# non-interactive (skip tree selector)
dryve pull "https://drive.google.com/drive/folders/<FOLDER_ID>" -y

# filter specific file patterns
dryve pull "https://drive.google.com/drive/folders/<FOLDER_ID>" -y --include "*.pdf" --exclude "*temp*"
```

What it does:

- Fetches the remote folder tree recursively.
- Opens an interactive selector with file sizes and live payload total (`j/k` move, `Space` toggle, `i` toggle ignore, `a` toggle all, `/` filter with `*` wildcards, `Tab` toggle in filter, `Esc` clear, `Enter` confirm), or skips it if `-y`/`--yes` is passed. When `--include` / `--exclude` are provided, matching files are pre-selected in the TUI (or directly selected under `-y`).
- Downloads selected files in parallel (skipping ignored items).
- Writes metadata to `<root-folder>/dryve.json` (persisting ignored files/folders).

### `sync`

Alias: `s`

Synchronize local files against remote changes:

```bash
dryve sync [path]
# alias
dryve s [path]

# include ignored files in sync
dryve sync [path] --include-ignored
# or short flag
dryve s [path] -I

# non-interactive (skip tree selector)
dryve sync [path] -y
```

Important:

- `sync` expects `dryve.json` in the specified directory, or the current directory if no path is provided.
- After `pull`, run `sync` from inside the downloaded root folder, or pass the folder path to the command.

Options:

- `-y, --yes`: Non-interactive mode, automatically accept default selection without selector prompt.
- `--include <PATTERN>`: Only select files matching pattern (repeatable, e.g. `--include "*.pdf"`).
- `--exclude <PATTERN>`: Exclude files matching pattern (repeatable, e.g. `--exclude "archive/*"`).
- `-I, --include-ignored`: Include ignored files and folders in the sync selector to review or un-ignore them.

Sync behavior:

- Compares old metadata with the current remote tree.
- Lists only changed items (added/modified/removed) in the selector (ignoring items marked with `[I]` unless `-I` / `--include-ignored` is provided).
- Applies only selected changes.
- Updates `dryve.json` after completion.

### `status`

Alias: `st`

Check sync status and preview remote changes without applying them:

```bash
dryve status [path]
# alias
dryve st [path]

# include ignored files in status check
dryve status [path] -I
```

Example output:

```text
Drive: Project Backup (12 items, 240 MB)
Remote: https://drive.google.com/drive/folders/<FOLDER_ID>

Changes (3 files, 14.2 MB):

+ Added:
    docs/new_spec.pdf (2.4 MB)

~ Modified:
    report.docx (11.8 MB)

- Removed:
    notes.txt

! Ignored:
    archive/data.zip (skipped)

Use 'dryve sync' to apply changes.
```

### `open`

Alias: `o`

Open the Drive folder in your default web browser:

```bash
dryve open [path]
# alias
dryve o [path]
```

Important:

- `open` expects `dryve.json` in the specified directory, or the current directory if no path is provided.
- Extracts the remote folder ID from `dryve.json` and launches the folder link in your browser.

## Example Workflow

```bash
# 1) Pull from a Drive folder
dryve pull "https://drive.google.com/drive/folders/<FOLDER_ID>"
# or
dryve p "https://drive.google.com/drive/folders/<FOLDER_ID>"

# 2) Open in browser
cd "<downloaded-folder>"
dryve open
# or pass folder path directly
dryve open "<downloaded-folder>"

# 3) Synchronize later
# Option A: Move into the folder and sync
cd "<downloaded-folder>"
dryve sync
# Option B: Sync by passing the folder path
dryve sync "<downloaded-folder>"
```

## Project Layout

- `src/app.rs`: CLI entry and subcommand routing.
- `src/cmd_open.rs`: Open command flow + browser launch.
- `src/cmd_pull.rs`: Pull flow + selector + initial metadata write.
- `src/cmd_status.rs`: Status command flow + change summary.
- `src/cmd_sync.rs`: Diff + selective sync + metadata refresh.
- `src/drive.rs`: Google Drive folder discovery and recursive traversal.
- `src/parser.rs`: HTML parsing for Drive listing data.
- `src/download.rs`: Task collection and file download logic.
- `src/selector/`: Interactive TUI selector (event loop, tree model, and terminal renderer).
- `src/pool.rs`: Parallel task execution and progress integration.
- `src/node.rs`: Tree model and diff engine.

## Notes

- Works with publicly accessible Google Drive folder links.
- Google Docs, Sheets, Slides, and Drawings automatically export to PDF (`.pdf`) on download.
- If no items are selected in the selector, no files are downloaded/synced.

## Development

```bash
cargo check
cargo test
```

## Disclaimer

Google Drive page structure can change over time. Since parsing depends on HTML structure, adjustments may be needed if Drive markup changes.
