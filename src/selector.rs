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
    size: Option<u64>,
    kind: ItemKind,
    is_last: bool,
    prefix: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    Full,
    None,
    Partial,
    Ignored,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub selected: HashSet<PathBuf>,
    pub ignored: HashSet<PathBuf>,
}

pub fn select(roots: &[(Node, PathBuf, ItemKind)]) -> io::Result<Option<Selection>> {
    run_selector(roots, None)
}

/// Variant of `select` that uses per-path kind overrides instead of inheriting
/// the root's kind for every descendant.
pub fn select_with_kinds(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: &HashMap<PathBuf, ItemKind>,
) -> io::Result<Option<Selection>> {
    run_selector(roots, Some(kind_map))
}

fn adjust_scroll(cursor: usize, scroll: &mut usize, list_height: usize) {
    if list_height == 0 {
        return;
    }
    if cursor < *scroll {
        *scroll = cursor;
    } else if cursor >= *scroll + list_height {
        *scroll = cursor + 1 - list_height;
    }
}

fn wildcard_match(text: &str, pattern: &str) -> bool {
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

fn term_matches(path_lower: &str, term: &str) -> bool {
    let filename = path_lower.rsplit('/').next().unwrap_or(path_lower);
    if term.contains('*') {
        if term.contains('/') {
            wildcard_match(path_lower, term)
        } else {
            wildcard_match(filename, term) || wildcard_match(path_lower, term)
        }
    } else {
        path_lower.contains(term)
    }
}

fn matches_query(path: &str, query: &str) -> bool {
    let path_lower = path.to_lowercase();
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();
    if terms.is_empty() {
        return true;
    }
    terms.iter().all(|term| term_matches(&path_lower, term))
}

fn filter_node(node: &Node, parent_path: &str, query: &str, is_root: bool) -> Option<Node> {
    let current_path = if parent_path.is_empty() || is_root {
        node.name.clone()
    } else {
        format!("{}/{}", parent_path, node.name)
    };

    let path_matches = !is_root && matches_query(&current_path, query);

    match node.node_type {
        NodeType::File => {
            if path_matches {
                Some(node.clone())
            } else {
                None
            }
        }
        NodeType::Folder => {
            let filtered_children: Vec<Node> = node
                .children
                .iter()
                .filter_map(|child| filter_node(child, &current_path, query, false))
                .collect();

            if !filtered_children.is_empty() {
                let mut filtered = node.clone();
                filtered.children = filtered_children;
                Some(filtered)
            } else if path_matches {
                Some(node.clone())
            } else {
                None
            }
        }
    }
}

fn filter_roots(
    roots: &[(Node, PathBuf, ItemKind)],
    query: &str,
) -> Vec<(Node, PathBuf, ItemKind)> {
    if query.trim().is_empty() {
        return roots.to_vec();
    }
    let query_lower = query.to_lowercase();
    roots
        .iter()
        .filter_map(|(node, base, kind)| {
            filter_node(node, "", &query_lower, true)
                .map(|filtered| (filtered, base.clone(), *kind))
        })
        .collect()
}

fn rebuild_items(
    roots: &[(Node, PathBuf, ItemKind)],
    query: &str,
    kind_map: Option<&HashMap<PathBuf, ItemKind>>,
) -> Vec<Item> {
    if query.trim().is_empty() {
        if let Some(km) = kind_map {
            build_items_with_kinds(roots, km)
        } else {
            build_items(roots)
        }
    } else {
        let filtered = filter_roots(roots, query);
        if let Some(km) = kind_map {
            build_items_with_kinds(&filtered, km)
        } else {
            build_items(&filtered)
        }
    }
}

fn first_file_or_first(items: &[Item]) -> usize {
    items
        .iter()
        .position(|i| matches!(i.node_type, NodeType::File))
        .unwrap_or(0)
}

fn toggle_all_visible(items: &[Item], selected: &mut HashSet<PathBuf>, ignored: &HashSet<PathBuf>) {
    let active_visible_files: HashSet<PathBuf> = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File) && !ignored.contains(&i.path))
        .map(|i| i.path.clone())
        .collect();
    if active_visible_files.is_empty() {
        return;
    }
    if active_visible_files.is_subset(selected) {
        for f in &active_visible_files {
            selected.remove(f);
        }
    } else {
        for f in active_visible_files {
            selected.insert(f);
        }
    }
}

fn collect_initial_ignored(roots: &[(Node, PathBuf, ItemKind)]) -> HashSet<PathBuf> {
    let mut ignored = HashSet::new();
    for (node, base, _) in roots {
        collect_node_ignored(node, base, false, &mut ignored);
    }
    ignored
}

fn collect_node_ignored(
    node: &Node,
    base: &Path,
    parent_ignored: bool,
    out: &mut HashSet<PathBuf>,
) {
    let path = base.join(crate::utils::sanitize_name(&node.name));
    let is_ignored = parent_ignored || node.ignored;
    if is_ignored {
        out.insert(path.clone());
    }
    for child in &node.children {
        collect_node_ignored(child, &path, is_ignored, out);
    }
}

fn is_item_ignored(items: &[Item], idx: usize, ignored: &HashSet<PathBuf>) -> bool {
    let path = &items[idx].path;
    if ignored.contains(path) {
        return true;
    }
    let kids = descendants(items, idx);
    let files: Vec<usize> = kids
        .into_iter()
        .filter(|&ci| matches!(items[ci].node_type, NodeType::File))
        .collect();

    if files.is_empty() {
        false
    } else {
        files.iter().all(|&ci| ignored.contains(&items[ci].path))
    }
}

fn toggle_ignore(
    items: &[Item],
    idx: usize,
    selected: &mut HashSet<PathBuf>,
    ignored: &mut HashSet<PathBuf>,
) {
    let is_ign = is_item_ignored(items, idx, ignored);
    let kids = descendants(items, idx);

    if kids.is_empty() {
        let path = &items[idx].path;
        if is_ign {
            ignored.remove(path);
        } else {
            ignored.insert(path.clone());
            selected.remove(path);
        }
    } else {
        if is_ign {
            // Un-ignore this folder and all its descendants
            ignored.remove(&items[idx].path);
            for ci in kids {
                ignored.remove(&items[ci].path);
            }
        } else {
            // Ignore this folder and all its descendants
            ignored.insert(items[idx].path.clone());
            selected.remove(&items[idx].path);
            for ci in kids {
                ignored.insert(items[ci].path.clone());
                selected.remove(&items[ci].path);
            }
        }
    }
}

fn run_selector(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: Option<&HashMap<PathBuf, ItemKind>>,
) -> io::Result<Option<Selection>> {
    let all_items = if let Some(km) = kind_map {
        build_items_with_kinds(roots, km)
    } else {
        build_items(roots)
    };
    if all_items.is_empty() {
        return Ok(Some(Selection::default()));
    }

    let mut ignored = collect_initial_ignored(roots);

    let mut selected: HashSet<PathBuf> = all_items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File) && !ignored.contains(&i.path))
        .map(|i| i.path.clone())
        .collect();

    let mut items = all_items;
    let mut cursor: usize = 0;
    let mut scroll: usize = 0;
    let mut filter_mode = false;
    let mut query = String::new();

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
    draw(
        &mut out,
        &items,
        cursor,
        scroll,
        &selected,
        &ignored,
        filter_mode,
        &query,
    )?;

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

                if filter_mode {
                    match code {
                        KeyCode::Esc => {
                            if !query.is_empty() {
                                query.clear();
                                items = rebuild_items(roots, &query, kind_map);
                                cursor = 0;
                                scroll = 0;
                            }
                            filter_mode = false;
                            dirty = true;
                        }
                        KeyCode::Enter => {
                            filter_mode = false;
                            dirty = true;
                        }
                        KeyCode::Backspace => {
                            query.pop();
                            items = rebuild_items(roots, &query, kind_map);
                            cursor = first_file_or_first(&items);
                            scroll = 0;
                            adjust_scroll(cursor, &mut scroll, list_height);
                            dirty = true;
                        }
                        KeyCode::Tab | KeyCode::BackTab => {
                            if !items.is_empty() {
                                toggle(&items, cursor, &mut selected, &ignored);
                                dirty = true;
                            }
                        }
                        KeyCode::Up => {
                            if cursor > 0 {
                                cursor -= 1;
                                adjust_scroll(cursor, &mut scroll, list_height);
                                dirty = true;
                            }
                        }
                        KeyCode::Down => {
                            if cursor + 1 < items.len() {
                                cursor += 1;
                                adjust_scroll(cursor, &mut scroll, list_height);
                                dirty = true;
                            }
                        }
                        KeyCode::Char('a') if modifiers.contains(KeyModifiers::CONTROL) => {
                            toggle_all_visible(&items, &mut selected, &ignored);
                            dirty = true;
                        }
                        KeyCode::Char('j') if modifiers.contains(KeyModifiers::CONTROL) => {
                            if cursor + 1 < items.len() {
                                cursor += 1;
                                adjust_scroll(cursor, &mut scroll, list_height);
                                dirty = true;
                            }
                        }
                        KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => {
                            if cursor > 0 {
                                cursor -= 1;
                                adjust_scroll(cursor, &mut scroll, list_height);
                                dirty = true;
                            }
                        }
                        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                            query.clear();
                            items = rebuild_items(roots, &query, kind_map);
                            cursor = 0;
                            scroll = 0;
                            dirty = true;
                        }
                        KeyCode::Char(c) => {
                            query.push(c);
                            items = rebuild_items(roots, &query, kind_map);
                            cursor = first_file_or_first(&items);
                            scroll = 0;
                            adjust_scroll(cursor, &mut scroll, list_height);
                            dirty = true;
                        }
                        _ => {}
                    }
                } else {
                    match code {
                        KeyCode::Up | KeyCode::Char('k') if cursor > 0 => {
                            cursor -= 1;
                            adjust_scroll(cursor, &mut scroll, list_height);
                            dirty = true;
                        }
                        KeyCode::Down | KeyCode::Char('j') if cursor + 1 < items.len() => {
                            cursor += 1;
                            adjust_scroll(cursor, &mut scroll, list_height);
                            dirty = true;
                        }
                        KeyCode::Char(' ') | KeyCode::Tab => {
                            if !items.is_empty() {
                                toggle(&items, cursor, &mut selected, &ignored);
                                dirty = true;
                            }
                        }
                        KeyCode::Char('i') => {
                            if !items.is_empty() {
                                toggle_ignore(&items, cursor, &mut selected, &mut ignored);
                                dirty = true;
                            }
                        }
                        KeyCode::Char('a') => {
                            toggle_all_visible(&items, &mut selected, &ignored);
                            dirty = true;
                        }
                        KeyCode::Char('/') => {
                            filter_mode = true;
                            dirty = true;
                        }
                        KeyCode::Esc => {
                            if !query.is_empty() {
                                query.clear();
                                items = rebuild_items(roots, &query, kind_map);
                                cursor = 0;
                                scroll = 0;
                                dirty = true;
                            } else {
                                break None;
                            }
                        }
                        KeyCode::Enter => break Some(Selection { selected, ignored }),
                        KeyCode::Char('q') => break None,
                        _ => {}
                    }
                }
            }
            Event::Resize(_, _) => {
                dirty = true;
            }
            _ => {}
        }

        if dirty {
            draw(
                &mut out,
                &items,
                cursor,
                scroll,
                &selected,
                &ignored,
                filter_mode,
                &query,
            )?;
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
        size: node.size,
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
        size: node.size,
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

fn check_state(
    items: &[Item],
    idx: usize,
    sel: &HashSet<PathBuf>,
    ign: &HashSet<PathBuf>,
) -> Check {
    let kids = descendants(items, idx);
    if kids.is_empty() {
        if ign.contains(&items[idx].path) {
            return Check::Ignored;
        }
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
        if ign.contains(&items[idx].path) {
            return Check::Ignored;
        }
        let states: Vec<Check> = kids
            .iter()
            .copied()
            .filter(|&i| matches!(items[i].node_type, NodeType::Folder))
            .map(|i| check_state(items, i, sel, ign))
            .collect();
        return if !states.is_empty() && states.iter().all(|&s| s == Check::Ignored) {
            Check::Ignored
        } else if !states.is_empty() && states.iter().all(|&s| s == Check::Full) {
            Check::Full
        } else if states.iter().all(|&s| s == Check::None) {
            Check::None
        } else {
            Check::Partial
        };
    }

    let ign_count = files
        .iter()
        .filter(|&&i| ign.contains(&items[i].path))
        .count();

    if ign_count == files.len() || ign.contains(&items[idx].path) {
        return Check::Ignored;
    }

    let non_ign_files: Vec<usize> = files
        .iter()
        .copied()
        .filter(|&i| !ign.contains(&items[i].path))
        .collect();

    let sel_count = non_ign_files
        .iter()
        .filter(|&&i| sel.contains(&items[i].path))
        .count();

    if sel_count == 0 && ign_count == 0 {
        Check::None
    } else if sel_count == non_ign_files.len() && ign_count == 0 {
        Check::Full
    } else {
        Check::Partial
    }
}

fn toggle(items: &[Item], idx: usize, sel: &mut HashSet<PathBuf>, ign: &HashSet<PathBuf>) {
    let kids = descendants(items, idx);
    if kids.is_empty() {
        if ign.contains(&items[idx].path) {
            return;
        }
        if sel.contains(&items[idx].path) {
            sel.remove(&items[idx].path);
        } else {
            sel.insert(items[idx].path.clone());
        }
    } else {
        let active_files: Vec<usize> = kids
            .into_iter()
            .filter(|&ci| {
                matches!(items[ci].node_type, NodeType::File) && !ign.contains(&items[ci].path)
            })
            .collect();

        if active_files.is_empty() {
            return;
        }

        let all_selected = active_files.iter().all(|&ci| sel.contains(&items[ci].path));
        if all_selected {
            for ci in active_files {
                sel.remove(&items[ci].path);
            }
        } else {
            for ci in active_files {
                sel.insert(items[ci].path.clone());
            }
        }
    }
}

// Rendering:
//
// Uses the alternate screen buffer entered once at startup.
// Every frame: MoveTo to each row, overwrite content, clear rest of line.
// Nothing is ever printed incrementally — the whole frame is atomic.

#[allow(clippy::too_many_arguments)]
fn draw(
    out: &mut impl Write,
    items: &[Item],
    cursor: usize,
    scroll: usize,
    sel: &HashSet<PathBuf>,
    ign: &HashSet<PathBuf>,
    filter_mode: bool,
    query: &str,
) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let list_height = rows.saturating_sub(1) as usize;
    let cols = cols as usize;

    let max_lead_len = items
        .iter()
        .filter(|i| i.size.is_some())
        .map(|i| i.prefix.chars().count() + 4 + i.name.chars().count())
        .max()
        .unwrap_or(20);
    let max_target = cols.saturating_sub(25).max(20);
    let target_col = (max_lead_len + 3).min(max_target);

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
        let state = check_state(items, i, sel, ign);

        let check_sym = match state {
            Check::Ignored => "[i]",
            Check::Full => match item.kind {
                ItemKind::Added => "[+]",
                ItemKind::Removed => "[-]",
                _ => "[x]",
            },
            Check::None => "[ ]",
            Check::Partial => "[~]",
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
            Check::Ignored => (Color::DarkGrey, true),
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

        // Name colour by kind; dim when deselected or ignored.
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
        let effective_fg = match state {
            Check::Ignored | Check::None => Color::DarkGrey,
            _ => name_fg,
        };
        queue!(out, SetForegroundColor(effective_fg))?;
        if matches!(item.node_type, NodeType::Folder) {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }

        let overhead = 2 + 3 + 1 + item.prefix.chars().count() + connector.chars().count();
        let lead_len =
            item.prefix.chars().count() + connector.chars().count() + item.name.chars().count();

        let has_room_for_badge = cols.saturating_sub(overhead) >= 15;
        if let Some(bytes) = item.size.filter(|_| has_room_for_badge) {
            let pad_len = if target_col > lead_len {
                target_col - lead_len
            } else {
                1
            };
            let badge_overhead = pad_len + 11 + if state == Check::Ignored { 10 } else { 0 };
            let max_name_len = cols.saturating_sub(overhead + badge_overhead);
            let name = trunc(&item.name, max_name_len);

            queue!(out, Print(name), ResetColor)?;
            queue!(out, Print(" ".repeat(pad_len)))?;

            queue!(
                out,
                SetForegroundColor(Color::DarkGrey),
                Print(crate::utils::fmt_size_badge(bytes)),
                ResetColor,
            )?;

            if state == Check::Ignored {
                queue!(
                    out,
                    SetForegroundColor(Color::DarkGrey),
                    Print(" (ignored)"),
                    ResetColor,
                )?;
            }
        } else {
            let display_name = if state == Check::Ignored {
                format!("{} (ignored)", item.name)
            } else {
                item.name.clone()
            };
            let name = trunc(&display_name, cols.saturating_sub(overhead));
            queue!(out, Print(name), ResetColor)?;
        }
        queue!(out, terminal::Clear(ClearType::UntilNewLine))?;
    }

    // Clear any leftover rows below the visible items (but above the status bar)
    for row in visible_count..list_height {
        queue!(
            out,
            cursor::MoveTo(0, row as u16),
            terminal::Clear(ClearType::CurrentLine),
        )?;
    }

    if items.is_empty() {
        queue!(
            out,
            cursor::MoveTo(2, 1),
            SetForegroundColor(Color::DarkGrey),
            Print("No matching files or folders"),
            ResetColor,
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
    let ign_count = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File) && ign.contains(&i.path))
        .count();
    let visible_selected = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File) && sel.contains(&i.path))
        .count();

    let total_bytes: u64 = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File))
        .filter_map(|i| i.size)
        .sum();
    let sel_bytes: u64 = items
        .iter()
        .filter(|i| matches!(i.node_type, NodeType::File) && sel.contains(&i.path))
        .filter_map(|i| i.size)
        .sum();

    let payload_str = if total_bytes > 0 {
        format!(
            " ({} / {})",
            crate::utils::fmt_size_compact(sel_bytes),
            crate::utils::fmt_size_compact(total_bytes)
        )
    } else {
        String::new()
    };

    let (left, right, bg, fg) = if filter_mode {
        let prompt = if items.is_empty() && !query.is_empty() {
            format!(" /{} (no match)", query)
        } else {
            format!(" /{}", query)
        };
        let controls = "  [Tab] Toggle  [↑↓] Move  [Enter] Done  [Esc] Clear  ".to_string();
        (prompt, controls, Color::White, Color::Black)
    } else if !query.is_empty() {
        let filter_info = format!(
            "  Filter: \"{}\"  [Space] Toggle  [i] Ignore  [a] All  [/] Filter  [Esc] Clear  [Enter] Confirm  [q] Quit",
            query
        );
        let stats = if ign_count > 0 {
            format!(
                "  {}/{} visible{} ({} total, {} ignored)  ",
                visible_selected, total_files, payload_str, sel_count, ign_count
            )
        } else {
            format!(
                "  {}/{} visible{} ({} total)  ",
                visible_selected, total_files, payload_str, sel_count
            )
        };
        (filter_info, stats, Color::DarkGrey, Color::Black)
    } else {
        let controls = "  [Space] Toggle  [i] Ignore  [a] All  [/] Filter  [j/k] ↑↓  [Enter] Confirm  [q] Quit"
            .to_string();
        let stats = if ign_count > 0 {
            format!(
                "  {}/{} selected{} ({} ignored)  ",
                sel_count, total_files, payload_str, ign_count
            )
        } else {
            format!("  {}/{} selected{}  ", sel_count, total_files, payload_str)
        };
        (controls, stats, Color::DarkGrey, Color::Black)
    };

    let right_len = right.chars().count();
    let bar = if cols >= right_len {
        let max_left = cols - right_len;
        let left_truncated = trunc(&left, max_left);
        let pad = cols.saturating_sub(left_truncated.chars().count() + right_len);
        format!("{}{}{}", left_truncated, " ".repeat(pad), right)
    } else {
        trunc(&right, cols)
    };

    queue!(
        out,
        cursor::MoveTo(0, rows - 1),
        SetBackgroundColor(bg),
        SetForegroundColor(fg),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match() {
        assert!(wildcard_match("document.pdf", "*.pdf"));
        assert!(wildcard_match("smthing.txt", "smthing.*"));
        assert!(wildcard_match("my_draft_copy.doc", "*draft*"));
        assert!(wildcard_match("report_2024_final.pdf", "report*2024*.pdf"));
        assert!(!wildcard_match("document.txt", "*.pdf"));
        assert!(!wildcard_match("foo", "foo*foo"));
        assert!(wildcard_match("anything", "*"));
    }

    #[test]
    fn test_matches_query() {
        assert!(matches_query("src/selector.rs", "selector"));
        assert!(matches_query("src/selector.rs", "src rs"));
        assert!(matches_query("src/selector.rs", "RS SRC"));
        assert!(!matches_query("src/selector.rs", "main"));
        assert!(matches_query("src/selector.rs", ""));

        // Wildcard support
        assert!(matches_query("docs/report.pdf", "*.pdf"));
        assert!(matches_query("docs/smthing.txt", "smthing.*"));
        assert!(matches_query("docs/smthing.txt", "docs *.txt"));
        assert!(!matches_query("docs/smthing.txt", "*.pdf"));
    }

    #[test]
    fn test_filter_node() {
        let file1 = Node::new("alpha.txt".to_string(), NodeType::File, None);
        let file2 = Node::new("beta.txt".to_string(), NodeType::File, None);
        let mut subfolder = Node::new("sub".to_string(), NodeType::Folder, None);
        subfolder.children = vec![file1, file2];

        let mut root = Node::new("root".to_string(), NodeType::Folder, None);
        root.children = vec![subfolder];

        // Searching "alpha" keeps sub/alpha.txt, drops beta.txt
        let filtered = filter_node(&root, "", "alpha", true).expect("should match");
        assert_eq!(filtered.children.len(), 1);
        let sub = &filtered.children[0];
        assert_eq!(sub.name, "sub");
        assert_eq!(sub.children.len(), 1);
        assert_eq!(sub.children[0].name, "alpha.txt");

        // Searching non-existent returns None
        assert!(filter_node(&root, "", "nonexistent", true).is_none());
    }

    #[test]
    fn test_toggle_all_visible() {
        let items = vec![
            Item {
                depth: 0,
                path: PathBuf::from("a.txt"),
                name: "a.txt".to_string(),
                node_type: NodeType::File,
                size: Some(1024),
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
            Item {
                depth: 0,
                path: PathBuf::from("b.txt"),
                name: "b.txt".to_string(),
                node_type: NodeType::File,
                size: Some(2048),
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
        ];

        let mut selected = HashSet::new();
        let ignored = HashSet::new();
        // Toggle when none selected -> selects all
        toggle_all_visible(&items, &mut selected, &ignored);
        assert_eq!(selected.len(), 2);

        // Toggle when all selected -> deselects all
        toggle_all_visible(&items, &mut selected, &ignored);
        assert_eq!(selected.len(), 0);
    }

    #[test]
    fn test_toggle_all_visible_skips_ignored() {
        let items = vec![
            Item {
                depth: 0,
                path: PathBuf::from("a.txt"),
                name: "a.txt".to_string(),
                node_type: NodeType::File,
                size: Some(1024),
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
            Item {
                depth: 0,
                path: PathBuf::from("b.txt"),
                name: "b.txt".to_string(),
                node_type: NodeType::File,
                size: Some(2048),
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
        ];

        let mut selected = HashSet::new();
        let mut ignored = HashSet::new();
        ignored.insert(PathBuf::from("b.txt"));

        // Only a.txt should be selected
        toggle_all_visible(&items, &mut selected, &ignored);
        assert_eq!(selected.len(), 1);
        assert!(selected.contains(&PathBuf::from("a.txt")));
        assert!(!selected.contains(&PathBuf::from("b.txt")));
    }

    #[test]
    fn test_toggle_ignore() {
        let items = vec![
            Item {
                depth: 0,
                path: PathBuf::from("folder"),
                name: "folder".to_string(),
                node_type: NodeType::Folder,
                size: None,
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
            Item {
                depth: 1,
                path: PathBuf::from("folder/file.txt"),
                name: "file.txt".to_string(),
                node_type: NodeType::File,
                size: Some(512),
                kind: ItemKind::Normal,
                is_last: true,
                prefix: "    ".to_string(),
            },
        ];

        let mut selected = HashSet::new();
        selected.insert(PathBuf::from("folder/file.txt"));
        let mut ignored = HashSet::new();

        // Toggling ignore on the folder ignores folder & child, and removes child from selected
        toggle_ignore(&items, 0, &mut selected, &mut ignored);
        assert!(ignored.contains(&PathBuf::from("folder")));
        assert!(ignored.contains(&PathBuf::from("folder/file.txt")));
        assert!(!selected.contains(&PathBuf::from("folder/file.txt")));

        // Toggling ignore on the folder again un-ignores both
        toggle_ignore(&items, 0, &mut selected, &mut ignored);
        assert!(!ignored.contains(&PathBuf::from("folder")));
        assert!(!ignored.contains(&PathBuf::from("folder/file.txt")));
    }

    #[test]
    fn test_first_file_or_first() {
        let items = vec![
            Item {
                depth: 0,
                path: PathBuf::from("folder"),
                name: "folder".to_string(),
                node_type: NodeType::Folder,
                size: None,
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
            Item {
                depth: 1,
                path: PathBuf::from("folder/file.txt"),
                name: "file.txt".to_string(),
                node_type: NodeType::File,
                size: Some(512),
                kind: ItemKind::Normal,
                is_last: true,
                prefix: "    ".to_string(),
            },
        ];

        // Should skip folder at index 0 and pick file at index 1
        assert_eq!(first_file_or_first(&items), 1);

        // Empty items fallback to 0
        assert_eq!(first_file_or_first(&[]), 0);
    }

    #[test]
    fn test_flatten_preserves_size() {
        let mut file = Node::new("report.pdf".to_string(), NodeType::File, None);
        file.size = Some(2_516_582);

        let mut folder = Node::new("docs".to_string(), NodeType::Folder, None);
        folder.children = vec![file];

        let roots = vec![(folder, PathBuf::new(), ItemKind::Normal)];
        let items = build_items(&roots);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].node_type, NodeType::Folder);
        assert_eq!(items[0].size, None);
        assert_eq!(items[1].node_type, NodeType::File);
        assert_eq!(items[1].size, Some(2_516_582));
    }

    #[test]
    fn test_payload_size_calculation() {
        let items = [
            Item {
                depth: 0,
                path: PathBuf::from("report.pdf"),
                name: "report.pdf".to_string(),
                node_type: NodeType::File,
                size: Some(2_500_000), // ~2.4 MB
                kind: ItemKind::Normal,
                is_last: false,
                prefix: String::new(),
            },
            Item {
                depth: 0,
                path: PathBuf::from("video.mp4"),
                name: "video.mp4".to_string(),
                node_type: NodeType::File,
                size: Some(1_200_000_000), // ~1.1 GB
                kind: ItemKind::Normal,
                is_last: true,
                prefix: String::new(),
            },
        ];

        let mut sel = HashSet::new();
        sel.insert(PathBuf::from("report.pdf"));

        let total_bytes: u64 = items
            .iter()
            .filter(|i| matches!(i.node_type, NodeType::File))
            .filter_map(|i| i.size)
            .sum();
        let sel_bytes: u64 = items
            .iter()
            .filter(|i| matches!(i.node_type, NodeType::File) && sel.contains(&i.path))
            .filter_map(|i| i.size)
            .sum();

        assert_eq!(total_bytes, 1_202_500_000);
        assert_eq!(sel_bytes, 2_500_000);
    }

    #[test]
    fn test_adjust_scroll() {
        let mut scroll = 0;
        let list_height = 5;

        // Downward jump beyond viewport
        adjust_scroll(7, &mut scroll, list_height);
        assert_eq!(scroll, 3); // 7 + 1 - 5 = 3

        // Upward jump above viewport
        adjust_scroll(1, &mut scroll, list_height);
        assert_eq!(scroll, 1);
    }
}
