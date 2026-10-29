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

                // Attempt structured JSON parse first
                if !parse_json_items(&data, &mut root) {
                    // Fallback to manual slice parsing
                    parse_fallback_items(&data, &mut root);
                }
            }
        }
    }

    Ok(root)
}

fn parse_json_items(data: &str, root: &mut Node) -> bool {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(data) else {
        return false;
    };

    let entries = if let Some(arr) = val.as_array() {
        if let Some(first) = arr.first().and_then(|v| v.as_array()) {
            first
        } else {
            arr
        }
    } else {
        return false;
    };

    let mut found_any = false;
    for item in entries {
        let Some(item_arr) = item.as_array() else {
            continue;
        };
        if item_arr.len() < 4 {
            continue;
        }

        let Some(item_id) = item_arr[0].as_str() else {
            continue;
        };
        if item_id.len() < 10 {
            continue;
        }

        let Some(name) = item_arr[2].as_str() else {
            continue;
        };
        let Some(type_str) = item_arr[3].as_str() else {
            continue;
        };

        if !name.is_empty() && !name.starts_with("application/") {
            let is_folder = type_str == "application/vnd.google-apps.folder";

            let size = if is_folder {
                None
            } else {
                item_arr
                    .get(13)
                    .and_then(|v| v.as_u64())
                    .or_else(|| item_arr.get(27).and_then(|v| v.as_u64()))
            };

            let exists = root.children.iter().any(|c| c.name == name);
            if !exists {
                let mut child = Node::new(
                    name.to_string(),
                    if is_folder {
                        NodeType::Folder
                    } else {
                        NodeType::File
                    },
                    Some(item_id.to_string()),
                );
                child.size = size;
                root.children.push(child);
                found_any = true;
            }
        }
    }

    found_any
}

fn parse_fallback_items(data: &str, root: &mut Node) {
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

            let size = if is_folder {
                None
            } else if let Some(bracket_end) = data[pos..].find("],[\"") {
                let slice = &data[entry_start..pos + bracket_end + 1];
                serde_json::from_str::<Vec<serde_json::Value>>(slice)
                    .ok()
                    .and_then(|arr| {
                        arr.get(13)
                            .and_then(|v| v.as_u64())
                            .or_else(|| arr.get(27).and_then(|v| v.as_u64()))
                    })
            } else {
                None
            };

            let exists = root.children.iter().any(|c| c.name == name);
            if !exists {
                let mut child = Node::new(
                    name.to_string(),
                    if is_folder {
                        NodeType::Folder
                    } else {
                        NodeType::File
                    },
                    Some(item_id.to_string()),
                );
                child.size = size;
                root.children.push(child);
            }
        }
        pos = e3 + 1;
    }
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
                b'\\' => {
                    decoded.push(b'\\');
                    i += 2;
                    continue;
                }
                b'n' => {
                    decoded.push(b'\n');
                    i += 2;
                    continue;
                }
                b'r' => {
                    decoded.push(b'\r');
                    i += 2;
                    continue;
                }
                b't' => {
                    decoded.push(b'\t');
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_html_with_file_sizes() {
        let html = r#"<html><head><title>My Drive Folder - Google Drive</title></head>
        <body><script>
        window['_DRIVE_ivd'] = '[[[\"folder_12345678\",[1],\"Subfolder\",\"application/vnd.google-apps.folder\",0,null,0,0,0,100,100],[\"file_1234567890\",[1],\"report.pdf\",\"application/pdf\",0,null,0,0,0,100,100,null,null,2516582]]]';
        </script></body></html>"#;

        let node = parse_html(html).expect("parse should succeed");
        assert_eq!(node.name, "My Drive Folder");
        assert_eq!(node.children.len(), 2);

        let folder = &node.children[0];
        assert_eq!(folder.name, "Subfolder");
        assert_eq!(folder.size, None);

        let file = &node.children[1];
        assert_eq!(file.name, "report.pdf");
        assert_eq!(file.size, Some(2516582));
    }
}
