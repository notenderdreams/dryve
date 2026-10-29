use super::model::{Check, Item, ItemKind, check_state};
use crate::node::NodeType;
use crate::utils::{fmt_size_badge, fmt_size_compact};
use crossterm::{
    cursor, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, ClearType},
};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

pub fn adjust_scroll(cursor: usize, scroll: &mut usize, list_height: usize) {
    if list_height == 0 {
        return;
    }
    if cursor < *scroll {
        *scroll = cursor;
    } else if cursor >= *scroll + list_height {
        *scroll = cursor + 1 - list_height;
    }
}

pub fn trunc(s: &str, max: usize) -> String {
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

#[allow(clippy::too_many_arguments)]
pub fn draw(
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

        // Text styling per kind.
        let (name_fg, name_bold) = match state {
            Check::Ignored => (Color::DarkGrey, false),
            _ => match item.kind {
                ItemKind::Added => (Color::Green, true),
                ItemKind::Modified => (Color::Yellow, false),
                ItemKind::Removed => (Color::Red, false),
                ItemKind::Normal => match item.node_type {
                    NodeType::Folder => (Color::Blue, true),
                    NodeType::File => (Color::White, false),
                },
            },
        };

        queue!(out, SetForegroundColor(name_fg))?;
        if name_bold {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }

        let overhead = 2 + 3 + 1 + item.prefix.chars().count() + 4;

        if let Some(bytes) = item.size {
            let badge_overhead = 11;
            let current_col = overhead + item.name.chars().count();
            let pad_len = if current_col < target_col {
                target_col - current_col
            } else {
                1
            };

            let max_name_len = cols.saturating_sub(overhead + badge_overhead);
            let name = trunc(&item.name, max_name_len);

            queue!(out, Print(name), ResetColor)?;
            queue!(out, Print(" ".repeat(pad_len)))?;

            queue!(
                out,
                SetForegroundColor(Color::DarkGrey),
                Print(fmt_size_badge(bytes)),
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
            fmt_size_compact(sel_bytes),
            fmt_size_compact(total_bytes)
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
