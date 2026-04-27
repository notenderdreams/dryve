use crate::node::{Node, NodeType};
use crate::utils::sanitize_name;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, ClearType},
};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Normal,
    Added,
    Modified,
    Removed,
}

struct Item {
    depth: usize,
    path: PathBuf,
    name: String,
    node_type: NodeType,
    kind: ItemKind,
    is_last: bool,
    prefix: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Full,
    None,
    Partial,
}

pub fn select(roots: &[(Node, PathBuf, ItemKind)]) -> io::Result<Option<HashSet<PathBuf>>> {
    let items = build_items(roots);
    if items.is_empty() {
        return Ok(Some(HashSet::new()));
    }

    let mut selected: HashSet<PathBuf> = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File))
        .map(|i| i.path.clone())
        .collect();

    let mut cursor: usize = 0;
    let mut scroll: usize = 0;

    // Setup
    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    queue!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    out.flush()?;

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let mut o = io::stdout();
            let _ = queue!(o, terminal::LeaveAlternateScreen, cursor::Show);
            let _ = o.flush();
            let _ = terminal::disable_raw_mode();
        }
    }
    let _guard = Guard;

    // Initial paint.
    draw(&mut out, &items, cursor, scroll, &selected)?;

    // Event loop
    let result = loop {
        let (_, rows) = terminal::size()?;
        let list_height = rows.saturating_sub(1) as usize;

        let mut dirty = false;

        match event::read()? {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if kind != event::KeyEventKind::Press {
                    continue;
                }
                if modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(code, KeyCode::Char('c') | KeyCode::Char('d'))
                {
                    break None;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') if cursor > 0 => {
                        cursor -= 1;
                        if cursor < scroll {
                            scroll = cursor;
                        }
                        dirty = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') if cursor + 1 < items.len() => {
                        cursor += 1;
                        if cursor >= scroll + list_height {
                            scroll = cursor + 1 - list_height;
                        }
                        dirty = true;
                    }
                    KeyCode::Char(' ') => {
                        toggle(&items, cursor, &mut selected);
                        dirty = true;
                    }
                    KeyCode::Char('a') => {
                        let all: HashSet<PathBuf> = items
                            .iter()
                            .filter(|i| matches!(i.node_type, NodeType::File))
                            .map(|i| i.path.clone())
                            .collect();
                        if selected == all {
                            selected.clear();
                        } else {
                            selected = all;
                        }
                        dirty = true;
                    }
                    KeyCode::Enter => break Some(selected),
                    KeyCode::Char('q') | KeyCode::Esc => break None,
                    _ => {}
                }
            }
            Event::Resize(_, _) => {
                dirty = true;
            }
            _ => {}
        }

        if dirty {
            draw(&mut out, &items, cursor, scroll, &selected)?;
        }
    };

    Ok(result)
}

/// Variant of `select` that uses per-path kind overrides instead of inheriting
/// the root's kind for every descendant.
pub fn select_with_kinds(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: &HashMap<PathBuf, ItemKind>,
) -> io::Result<Option<HashSet<PathBuf>>> {
    let items = build_items_with_kinds(roots, kind_map);
    if items.is_empty() {
        return Ok(Some(HashSet::new()));
    }

    let mut selected: HashSet<PathBuf> = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File))
        .map(|i| i.path.clone())
        .collect();

    let mut cursor: usize = 0;
    let mut scroll: usize = 0;

    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    queue!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    out.flush()?;

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let mut o = io::stdout();
            let _ = queue!(o, terminal::LeaveAlternateScreen, cursor::Show);
            let _ = o.flush();
            let _ = terminal::disable_raw_mode();
        }
    }
    let _guard = Guard;

    draw(&mut out, &items, cursor, scroll, &selected)?;

    let result = loop {
        let (_, rows) = terminal::size()?;
        let list_height = rows.saturating_sub(1) as usize;

        let mut dirty = false;

        match event::read()? {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if kind != event::KeyEventKind::Press {
                    continue;
                }
                if modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(code, KeyCode::Char('c') | KeyCode::Char('d'))
                {
                    break None;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') if cursor > 0 => {
                        cursor -= 1;
                        if cursor < scroll {
                            scroll = cursor;
                        }
                        dirty = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') if cursor + 1 < items.len() => {
                        cursor += 1;
                        if cursor >= scroll + list_height {
                            scroll = cursor + 1 - list_height;
                        }
                        dirty = true;
                    }
                    KeyCode::Char(' ') => {
                        toggle(&items, cursor, &mut selected);
                        dirty = true;
                    }
                    KeyCode::Char('a') => {
                        let all: HashSet<PathBuf> = items
                            .iter()
                            .filter(|i| matches!(i.node_type, NodeType::File))
                            .map(|i| i.path.clone())
                            .collect();
                        if selected == all {
                            selected.clear();
                        } else {
                            selected = all;
                        }
                        dirty = true;
                    }
                    KeyCode::Enter => break Some(selected),
                    KeyCode::Char('q') | KeyCode::Esc => break None,
                    _ => {}
                }
            }
            Event::Resize(_, _) => {
                dirty = true;
            }
            _ => {}
        }

        if dirty {
            draw(&mut out, &items, cursor, scroll, &selected)?;
        }
    };

    Ok(result)
}

fn build_items(roots: &[(Node, PathBuf, ItemKind)]) -> Vec<Item> {
    let mut out = Vec::new();
    let n = roots.len();
    for (i, (node, base, kind)) in roots.iter().enumerate() {
        flatten(node, base, *kind, 0, i + 1 == n, "", &mut out);
    }
    out
}

fn build_items_with_kinds(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: &HashMap<PathBuf, ItemKind>,
) -> Vec<Item> {
    let mut out = Vec::new();
    let n = roots.len();
    for (i, (node, base, default_kind)) in roots.iter().enumerate() {
        flatten_with_kinds(
            node,
            base,
            *default_kind,
            kind_map,
            0,
            i + 1 == n,
            "",
            &mut out,
        );
    }
    out
}

fn flatten(
    node: &Node,
    base: &Path,
    kind: ItemKind,
    depth: usize,
    is_last: bool,
    prefix: &str,
    out: &mut Vec<Item>,
) {
    let path = base.join(sanitize_name(&node.name));
    out.push(Item {
        depth,
        path: path.clone(),
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        kind,
        is_last,
        prefix: prefix.to_string(),
    });
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        flatten(
            child,
            &path,
            kind,
            depth + 1,
            i + 1 == n,
            &child_prefix,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn flatten_with_kinds(
    node: &Node,
    base: &Path,
    default_kind: ItemKind,
    kind_map: &HashMap<PathBuf, ItemKind>,
    depth: usize,
    is_last: bool,
    prefix: &str,
    out: &mut Vec<Item>,
) {
    let path = base.join(sanitize_name(&node.name));
    let kind = kind_map.get(&path).copied().unwrap_or(default_kind);
    out.push(Item {
        depth,
        path: path.clone(),
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        kind,
        is_last,
        prefix: prefix.to_string(),
    });
    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        flatten_with_kinds(
            child,
            &path,
            default_kind,
            kind_map,
            depth + 1,
            i + 1 == n,
            &child_prefix,
            out,
        );
    }
}

fn descendants(items: &[Item], idx: usize) -> Vec<usize> {
    let depth = items[idx].depth;
    items
        .iter()
        .enumerate()
        .skip(idx + 1)
        .take_while(|(_, it)| it.depth > depth)
        .map(|(i, _)| i)
        .collect()
}

fn check_state(items: &[Item], idx: usize, sel: &HashSet<PathBuf>) -> Check {
    let kids = descendants(items, idx);
    if kids.is_empty() {
        return if sel.contains(&items[idx].path) {
            Check::Full
        } else {
            Check::None
        };
    }
    let files: Vec<usize> = kids
        .iter()
        .copied()
        .filter(|&i| matches!(items[i].node_type, NodeType::File))
        .collect();
    if files.is_empty() {
        let states: Vec<Check> = kids
            .iter()
            .copied()
            .filter(|&i| matches!(items[i].node_type, NodeType::Folder))
            .map(|i| check_state(items, i, sel))
            .collect();
        return if states.iter().all(|&s| s == Check::Full) {
            Check::Full
        } else if states.iter().all(|&s| s == Check::None) {
            Check::None
        } else {
            Check::Partial
        };
    }
    let n = files
        .iter()
        .filter(|&&i| sel.contains(&items[i].path))
        .count();
    if n == 0 {
        Check::None
    } else if n == files.len() {
        Check::Full
    } else {
        Check::Partial
    }
}

fn toggle(items: &[Item], idx: usize, sel: &mut HashSet<PathBuf>) {
    let do_sel = check_state(items, idx, sel) != Check::Full;
    let kids = descendants(items, idx);
    if kids.is_empty() {
        if do_sel {
            sel.insert(items[idx].path.clone());
        } else {
            sel.remove(&items[idx].path);
        }
    } else {
        for ci in kids {
            if matches!(items[ci].node_type, NodeType::File) {
                if do_sel {
                    sel.insert(items[ci].path.clone());
                } else {
                    sel.remove(&items[ci].path);
                }
            }
        }
    }
}

// Rendering:
//
// Uses the alternate screen buffer entered once at startup.
// Every frame: MoveTo to each row, overwrite content, clear rest of line.
// Nothing is ever printed incrementally — the whole frame is atomic.

fn draw(
    out: &mut impl Write,
    items: &[Item],
    cursor: usize,
    scroll: usize,
    sel: &HashSet<PathBuf>,
) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let list_height = rows.saturating_sub(1) as usize;
    let cols = cols as usize;

    // Item rows
    let visible_count = items.len().saturating_sub(scroll).min(list_height);
    for (row, (i, item)) in items
        .iter()
        .enumerate()
        .skip(scroll)
        .take(list_height)
        .enumerate()
    {
        let is_cur = i == cursor;
        let state = check_state(items, i, sel);

        let check_sym = match (item.kind, state) {
            (ItemKind::Added, Check::Full) => "[+]",
            (ItemKind::Removed, Check::Full) => "[-]",
            (_, Check::Full) => "[x]",
            (_, Check::None) => "[ ]",
            (_, Check::Partial) => "[~]",
        };

        queue!(out, cursor::MoveTo(0, row as u16))?;

        // Cursor marker.
        if is_cur {
            queue!(
                out,
                SetForegroundColor(Color::White),
                SetAttribute(Attribute::Bold),
                Print("> "),
                ResetColor,
            )?;
        } else {
            queue!(out, Print("  "))?;
        }

        // Checkbox.
        let (check_fg, check_bold) = match state {
            Check::Full => (Color::White, true),
            Check::None => (Color::DarkGrey, false),
            Check::Partial => (Color::Yellow, false),
        };
        queue!(out, SetForegroundColor(check_fg))?;
        if check_bold {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }
        queue!(out, Print(check_sym), ResetColor)?;

        // Tree chrome.
        let connector = if item.is_last {
            "└── "
        } else {
            "├── "
        };
        queue!(
            out,
            Print(" "),
            SetForegroundColor(Color::DarkGrey),
            Print(&item.prefix),
            Print(connector),
            ResetColor,
        )?;

        // Name colour by kind; dim when deselected.
        let name_fg = match item.kind {
            ItemKind::Added => Color::Green,
            ItemKind::Modified => Color::Rgb {
                r: 255,
                g: 165,
                b: 0,
            },
            ItemKind::Removed => Color::Red,
            ItemKind::Normal => match item.node_type {
                NodeType::Folder => Color::Magenta,
                NodeType::File => Color::White,
            },
        };
        let effective_fg = if matches!(state, Check::None) {
            Color::DarkGrey
        } else {
            name_fg
        };
        queue!(out, SetForegroundColor(effective_fg))?;
        if matches!(item.node_type, NodeType::Folder) {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }

        // Fixed chars used so far: 2 (marker) + 3 (checkbox) + 1 (sp)
        //   + prefix.len() + connector.len() = overhead
        let overhead = 2 + 3 + 1 + item.prefix.chars().count() + connector.chars().count();
        let name = trunc(&item.name, cols.saturating_sub(overhead));
        queue!(
            out,
            Print(name),
            ResetColor,
            terminal::Clear(ClearType::UntilNewLine),
        )?;
    }

    // Clear any leftover rows below the visible items (but above the status bar)
    for row in visible_count..list_height {
        queue!(
            out,
            cursor::MoveTo(0, row as u16),
            terminal::Clear(ClearType::CurrentLine),
        )?;
    }

    // Scroll position (top-right)
    if items.len() > list_height {
        let hint = format!(" {}/{} ", cursor + 1, items.len());
        let x = cols.saturating_sub(hint.len()) as u16;
        queue!(
            out,
            cursor::MoveTo(x, 0),
            SetForegroundColor(Color::DarkGrey),
            Print(&hint),
            ResetColor,
        )?;
    }

    // Status bar pinned to last row
    let sel_count = sel.len();
    let total_files = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File))
        .count();

    let left = "  [Space] Toggle  [a] All  [j/k] ↑↓  [Enter] Confirm  [q] Quit";
    let right = format!("  {}/{} selected  ", sel_count, total_files);
    let pad = cols.saturating_sub(left.chars().count() + right.chars().count());
    let bar = trunc(&format!("{}{}{}", left, " ".repeat(pad), right), cols);

    queue!(
        out,
        cursor::MoveTo(0, rows - 1),
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::Black),
        SetAttribute(Attribute::Bold),
        Print(bar),
        ResetColor,
    )?;

    // Single flush — the terminal sees one complete frame.
    out.flush()
}

fn trunc(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let mut out = String::with_capacity(max);
    let mut n = 0;
    while n < max {
        match chars.next() {
            Some(c) => {
                out.push(c);
                n += 1;
            }
            None => break,
        }
    }
    out
}
