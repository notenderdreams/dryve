use crate::node::{Node, NodeType};
use crate::utils::sanitize_name;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Normal,
    Added,
    Modified,
    Removed,
}

pub struct Item {
    pub depth: usize,
    pub path: PathBuf,
    pub name: String,
    pub node_type: NodeType,
    pub size: Option<u64>,
    pub kind: ItemKind,
    pub is_last: bool,
    pub prefix: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Check {
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

pub fn term_matches(path_lower: &str, term: &str) -> bool {
    let filename = path_lower.rsplit('/').next().unwrap_or(path_lower);
    if term.contains('*') {
        if term.contains('/') {
            crate::utils::wildcard_match(path_lower, term)
        } else {
            crate::utils::wildcard_match(filename, term)
                || crate::utils::wildcard_match(path_lower, term)
        }
    } else {
        path_lower.contains(term)
    }
}

pub fn matches_query(path: &str, query: &str) -> bool {
    let path_lower = path.to_lowercase();
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();
    if terms.is_empty() {
        return true;
    }
    terms.iter().all(|term| term_matches(&path_lower, term))
}

pub fn filter_node(node: &Node, parent_path: &str, query: &str, is_root: bool) -> Option<Node> {
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
                let mut filtered = node.clone();
                filtered.children = Vec::new();
                Some(filtered)
            } else {
                None
            }
        }
    }
}

pub fn filter_roots(
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

pub fn rebuild_items(
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

pub fn first_file_or_first(items: &[Item]) -> usize {
    items
        .iter()
        .position(|i| matches!(i.node_type, NodeType::File))
        .unwrap_or(0)
}

pub fn toggle_all_visible(
    items: &[Item],
    selected: &mut HashSet<PathBuf>,
    ignored: &HashSet<PathBuf>,
) {
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

pub fn collect_initial_ignored(roots: &[(Node, PathBuf, ItemKind)]) -> HashSet<PathBuf> {
    let mut ignored = HashSet::new();
    for (node, base, _) in roots {
        collect_node_ignored(node, base, false, &mut ignored);
    }
    ignored
}

pub fn collect_node_ignored(
    node: &Node,
    base: &Path,
    parent_ignored: bool,
    out: &mut HashSet<PathBuf>,
) {
    let path = base.join(sanitize_name(&node.name));
    let is_ignored = parent_ignored || node.ignored;
    if is_ignored {
        out.insert(path.clone());
    }
    for child in &node.children {
        collect_node_ignored(child, &path, is_ignored, out);
    }
}

pub fn is_item_ignored(items: &[Item], idx: usize, ignored: &HashSet<PathBuf>) -> bool {
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

pub fn toggle_ignore(
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
    } else if is_ign {
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

pub fn build_items(roots: &[(Node, PathBuf, ItemKind)]) -> Vec<Item> {
    let mut out = Vec::new();
    let n = roots.len();
    for (i, (node, base, kind)) in roots.iter().enumerate() {
        flatten(node, base, *kind, 0, i + 1 == n, "", &mut out);
    }
    out
}

pub fn build_items_with_kinds(
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

pub fn flatten(
    node: &Node,
    base: &Path,
    kind: ItemKind,
    depth: usize,
    is_last: bool,
    prefix: &str,
    out: &mut Vec<Item>,
) {
    let path = base.join(sanitize_name(&node.name));
    let size = match node.node_type {
        NodeType::File => node.size,
        NodeType::Folder => node.size.or_else(|| Some(node.total_size())),
    };
    out.push(Item {
        depth,
        path: path.clone(),
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        size,
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
pub fn flatten_with_kinds(
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
    let size = match node.node_type {
        NodeType::File => node.size,
        NodeType::Folder => node.size.or_else(|| Some(node.total_size())),
    };
    out.push(Item {
        depth,
        path: path.clone(),
        name: node.name.clone(),
        node_type: node.node_type.clone(),
        size,
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

pub fn descendants(items: &[Item], idx: usize) -> Vec<usize> {
    let depth = items[idx].depth;
    items
        .iter()
        .enumerate()
        .skip(idx + 1)
        .take_while(|(_, it)| it.depth > depth)
        .map(|(i, _)| i)
        .collect()
}

pub fn check_state(
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

pub fn toggle(items: &[Item], idx: usize, sel: &mut HashSet<PathBuf>, ign: &HashSet<PathBuf>) {
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
