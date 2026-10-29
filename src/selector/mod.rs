pub mod model;
pub mod view;

pub use model::{ItemKind, Selection};

use crate::node::Node;
use crate::utils::path_passes_filter;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    queue, terminal,
};
use model::{
    build_items, build_items_with_kinds, collect_initial_ignored, first_file_or_first,
    rebuild_items, toggle, toggle_all_visible, toggle_ignore,
};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::PathBuf;
use view::{adjust_scroll, draw};

pub fn select(
    roots: &[(Node, PathBuf, ItemKind)],
    include: &[String],
    exclude: &[String],
) -> io::Result<Option<Selection>> {
    run_selector(roots, None, include, exclude)
}

/// Select all non-ignored files passing include/exclude filters without interactive prompt.
pub fn select_all(
    roots: &[(Node, PathBuf, ItemKind)],
    include: &[String],
    exclude: &[String],
) -> Selection {
    let all_items = build_items(roots);
    let ignored = collect_initial_ignored(roots);
    let selected = all_items
        .into_iter()
        .filter(|i| {
            matches!(i.node_type, crate::node::NodeType::File)
                && !ignored.contains(&i.path)
                && path_passes_filter(&i.path, include, exclude)
        })
        .map(|i| i.path)
        .collect();
    Selection { selected, ignored }
}

/// Variant of `select` that uses per-path kind overrides instead of inheriting
/// the root's kind for every descendant.
pub fn select_with_kinds(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: &HashMap<PathBuf, ItemKind>,
    include: &[String],
    exclude: &[String],
) -> io::Result<Option<Selection>> {
    run_selector(roots, Some(kind_map), include, exclude)
}

fn run_selector(
    roots: &[(Node, PathBuf, ItemKind)],
    kind_map: Option<&HashMap<PathBuf, ItemKind>>,
    include: &[String],
    exclude: &[String],
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
        .filter(|i| {
            matches!(i.node_type, crate::node::NodeType::File)
                && !ignored.contains(&i.path)
                && path_passes_filter(&i.path, include, exclude)
        })
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

    let result = loop {
        let ev = event::read()?;
        let (_, rows) = terminal::size()?;
        let list_height = rows.saturating_sub(1) as usize;
        let mut dirty = false;

        match ev {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    break None;
                }

                if filter_mode {
                    match code {
                        KeyCode::Esc => {
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

#[cfg(test)]
mod tests {
    use super::model::*;
    use super::*;
    use crate::node::{Node, NodeType};
    use crate::utils::wildcard_match;
    use std::collections::HashSet;
    use std::path::PathBuf;

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
    fn test_flatten_computes_folder_size() {
        let mut file1 = Node::new("report.pdf".to_string(), NodeType::File, None);
        file1.size = Some(2_500_000);

        let mut file2 = Node::new("notes.txt".to_string(), NodeType::File, None);
        file2.size = Some(16_582);

        let mut subfolder = Node::new("sub".to_string(), NodeType::Folder, None);
        subfolder.children = vec![file2];

        let mut folder = Node::new("docs".to_string(), NodeType::Folder, None);
        folder.children = vec![file1, subfolder];

        let roots = vec![(folder, PathBuf::new(), ItemKind::Normal)];
        let items = build_items(&roots);

        assert_eq!(items.len(), 4);
        assert_eq!(items[0].node_type, NodeType::Folder);
        assert_eq!(items[0].size, Some(2_516_582));
        assert_eq!(items[1].node_type, NodeType::File);
        assert_eq!(items[1].size, Some(2_500_000));
        assert_eq!(items[2].node_type, NodeType::Folder);
        assert_eq!(items[2].size, Some(16_582));
        assert_eq!(items[3].node_type, NodeType::File);
        assert_eq!(items[3].size, Some(16_582));
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

    #[test]
    fn test_select_all() {
        let file1 = Node::new("file1.txt".to_string(), NodeType::File, None);
        let mut file2 = Node::new("file2.txt".to_string(), NodeType::File, None);
        file2.ignored = true;

        let mut folder = Node::new("docs".to_string(), NodeType::Folder, None);
        folder.children = vec![file1, file2];

        let roots = vec![(folder, PathBuf::new(), ItemKind::Normal)];
        let selection = select_all(&roots, &[], &[]);

        assert!(
            selection
                .selected
                .contains(&PathBuf::from("docs/file1.txt"))
        );
        assert!(
            !selection
                .selected
                .contains(&PathBuf::from("docs/file2.txt"))
        );
        assert!(selection.ignored.contains(&PathBuf::from("docs/file2.txt")));
    }

    #[test]
    fn test_select_all_with_filters() {
        let file1 = Node::new("report.pdf".to_string(), NodeType::File, None);
        let file2 = Node::new("data.csv".to_string(), NodeType::File, None);
        let file3 = Node::new("temp.pdf".to_string(), NodeType::File, None);

        let mut folder = Node::new("docs".to_string(), NodeType::Folder, None);
        folder.children = vec![file1, file2, file3];

        let roots = vec![(folder, PathBuf::new(), ItemKind::Normal)];
        let inc = vec!["*.pdf".to_string()];
        let exc = vec!["*temp*".to_string()];

        let selection = select_all(&roots, &inc, &exc);
        assert!(
            selection
                .selected
                .contains(&PathBuf::from("docs/report.pdf"))
        );
        assert!(!selection.selected.contains(&PathBuf::from("docs/data.csv")));
        assert!(!selection.selected.contains(&PathBuf::from("docs/temp.pdf")));
    }
}
