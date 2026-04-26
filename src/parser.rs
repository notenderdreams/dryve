use crate::node::{Node, NodeType};
use anyhow::Result;


pub fn parse_html(html: &str) -> Result<Node> {
    // 1. Extract folder name from <title>
    let mut folder_name = "GDrive Folder".to_string();
    if let Some(start) = html.find("<title>")
        && let Some(end) = html[start..].find("</title>")
    {
        let title = &html[start + 7..start + end];
        folder_name = title.replace(" - Google Drive", "");
    }

    let mut root = Node::new(folder_name, NodeType::Folder, None);

    // 2. Find _DRIVE_ivd
    if let Some(idx) = html.find("_DRIVE_ivd") {
        let mut curr = idx;
        let mut quotes_found = 0;
        // Move past two single quotes to reach the raw data string
        while quotes_found < 2 {
            if let Some(next_quote) = html[curr + 1..].find('\'') {
                curr += next_quote + 1;
                quotes_found += 1;
            } else {
                break;
            }
        }

        if quotes_found == 2 {
            let start = curr + 1;
            if let Some(end_offset) = html[start..].find('\'') {
                let end = start + end_offset;
                let raw_data = &html[start..end];
                let decoded_bytes = decode_drive_data(raw_data);
                let data = String::from_utf8_lossy(&decoded_bytes);

                let mut pos = 0;
                while let Some(entry_start) = data[pos..].find("[\"") {
                    let entry_start = pos + entry_start;
                    pos = entry_start + 1;

                    // Item ID
                    let s1 = entry_start + 1;
                    let e1 = match data[s1 + 1..].find('"') {
                        Some(e) => s1 + 1 + e,
                        None => continue,
                    };
                    let item_id = &data[s1 + 1..e1];
                    if item_id.len() < 10 {
                        continue;
                    }

                    // Skip nested array (e.g. ["...", [...], ...])
                    let p_array_start = match data[e1..].find(",[") {
                        Some(s) => e1 + s,
                        None => continue,
                    };
                    let mut bracket_depth = 1;
                    let mut p_array_end = p_array_start + 2;
                    let data_bytes = data.as_bytes();
                    while bracket_depth > 0 && p_array_end < data_bytes.len() {
                        if data_bytes[p_array_end] == b'[' {
                            bracket_depth += 1;
                        } else if data_bytes[p_array_end] == b']' {
                            bracket_depth -= 1;
                        }
                        p_array_end += 1;
                    }

                    // Item name
                    let s2 = match data[p_array_end..].find('"') {
                        Some(s) => p_array_end + s,
                        None => continue,
                    };
                    let e2 = match data[s2 + 1..].find('"') {
                        Some(e) => s2 + 1 + e,
                        None => continue,
                    };
                    let name = &data[s2 + 1..e2];

                    // MIME type
                    let s3 = match data[e2 + 1..].find('"') {
                        Some(s) => e2 + 1 + s,
                        None => continue,
                    };
                    let e3 = match data[s3 + 1..].find('"') {
                        Some(e) => s3 + 1 + e,
                        None => continue,
                    };
                    let type_str = &data[s3 + 1..e3];

                    if !name.is_empty() && !name.starts_with("application/") {
                        let is_folder = type_str == "application/vnd.google-apps.folder";

                        let exists = root.children.iter().any(|c| c.name == name);
                        if !exists {
                            let child = Node::new(
                                name.to_string(),
                                if is_folder {
                                    NodeType::Folder
                                } else {
                                    NodeType::File
                                },
                                Some(item_id.to_string()),
                            );
                            root.children.push(child);
                        }
                    }
                    pos = e3 + 1;
                }
            }
        }
    }

    Ok(root)
}

fn decode_drive_data(data: &str) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(data.len());
    let bytes = data.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'x'
            && let Ok(byte) = u8::from_str_radix(&data[i + 2..i + 4], 16)
        {
            decoded.push(byte);
            i += 4;
            continue;
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => {
                    decoded.push(b'"');
                    i += 2;
                    continue;
                }
                b'/' => {
                    decoded.push(b'/');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    decoded
}
