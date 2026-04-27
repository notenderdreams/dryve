<div align="center">

<table>
	<tr>
		<td align="center" width="50%">
			<img src="assets/dryve.svg" alt="dryve logo" width="400" />
		</td>
		<td align="center" width="50%">
			<a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust" alt="Rust" /></a><br />
			<a href="#"><img src="https://img.shields.io/badge/version-v0.1.0-1f2937" alt="Version" /></a><br />
			<a href="https://drive.google.com/"><img src="https://img.shields.io/badge/Google%20Drive-34A853?logo=googledrive&logoColor=white" alt="Google Drive" /></a>
		</td>
	</tr>
</table>

</div>

`dryve` is a Rust CLI for Google Drive folders that keeps local files updated over time, like working with a GitHub repo.

It builds a local tree, lets you choose what to download in an interactive selector, and stores metadata in `dryve.json` so future sync runs can detect and apply remote changes.

## Features

- Pull an entire Google Drive folder tree from a shared folder URL.
- Interactive terminal selector for choosing files/folders before download.
- Multi-threaded downloads using a Rayon thread pool.
- Sync mode that detects added, updated, and removed files.
- Safe filename sanitization for cross-platform local paths.

## Requirements

- Rust toolchain (edition 2024 capable).
- Network access to Google Drive public folder URLs.

## Installation

Build from source:

```bash
cargo build --release
```

Run directly with Cargo:

```bash
cargo run -- <command>
```

Use compiled binary:

```bash
./target/release/dryve <command>
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
dryve sync
# alias
dryve s
```

Important:

- `sync` expects `dryve.json` in the current directory.
- After `pull`, run `sync` from inside the downloaded root folder (the one containing `dryve.json`).

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

# 2) Move into the downloaded root folder (created from Drive folder name)
cd "<downloaded-folder>"

# 3) Synchronize later
dryve sync
# or
dryve s
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
