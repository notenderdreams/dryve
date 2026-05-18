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
[![Version](https://img.shields.io/badge/version-v0.1.0-1f2937)](#)
[![Google Drive](https://img.shields.io/badge/Google%20Drive-34A853?logo=googledrive&logoColor=white)](https://drive.google.com/)

</td>
</tr>
</table>
</div>

`dryve` is a Rust CLI for pulling and syncing publicly shared Google Drive folders.

It fetches a Drive folder tree, lets you select files in an interactive terminal UI, downloads them locally, and stores metadata in `dryve.json` so later `sync` runs can apply remote changes.

## Features

- Pull an entire Google Drive folder tree from a shared folder URL.
- Interactive terminal selector for choosing files/folders before download.
- Multi-threaded downloads using a Rayon thread pool.
- Sync mode that detects added, updated, and removed files.
- Safe filename sanitization for cross-platform local paths.

## Installation

Use the installer scripts for your platform:

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/notenderdreams/dryve/main/installer/install.sh | bash
```

Optional: install a specific version by passing the tag name:

```bash
curl -fsSL https://raw.githubusercontent.com/notenderdreams/dryve/main/installer/install.sh | bash -s v0.1.0
```

### Windows

```powershell
irm https://raw.githubusercontent.com/notenderdreams/dryve/main/installer/install.ps1 | iex
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
```

What it does:

- Fetches the remote folder tree recursively.
- Opens an interactive selector (`j/k` move, `Space` toggle, `a` toggle all, `Enter` confirm).
- Downloads selected files in parallel.
- Writes metadata to `<root-folder>/dryve.json`.

### `sync`

Alias: `s`

Synchronize local files against remote changes:

```bash
dryve sync [path]
# alias
dryve s [path]
```

Important:

- `sync` expects `dryve.json` in the specified directory, or the current directory if no path is provided.
- After `pull`, run `sync` from inside the downloaded root folder, or pass the folder path to the command.

Sync behavior:

- Compares old metadata with the current remote tree.
- Lists only changed items (added/modified/removed) in the selector.
- Applies only selected changes.
- Updates `dryve.json` after completion.

## Example Workflow

```bash
# 1) Pull from a Drive folder
dryve pull "https://drive.google.com/drive/folders/<FOLDER_ID>"
# or
dryve p "https://drive.google.com/drive/folders/<FOLDER_ID>"

# 2) Synchronize later
# Option A: Move into the folder and sync
cd "<downloaded-folder>"
dryve sync
# Option B: Sync by passing the folder path
dryve sync "<downloaded-folder>"
```

## Project Layout

- `src/app.rs`: CLI entry and subcommand routing.
- `src/cmd_pull.rs`: Pull flow + selector + initial metadata write.
- `src/cmd_sync.rs`: Diff + selective sync + metadata refresh.
- `src/drive.rs`: Google Drive folder discovery and recursive traversal.
- `src/parser.rs`: HTML parsing for Drive listing data.
- `src/download.rs`: Task collection and file download logic.
- `src/selector.rs`: Interactive TUI selector.
- `src/pool.rs`: Parallel task execution and progress integration.
- `src/node.rs`: Tree model and diff engine.

## Notes

- Works with publicly accessible Google Drive folder links.
- If no items are selected in the selector, no files are downloaded/synced.

## Development

```bash
cargo check
cargo test
```

## Disclaimer

Google Drive page structure can change over time. Since parsing depends on HTML structure, adjustments may be needed if Drive markup changes.
