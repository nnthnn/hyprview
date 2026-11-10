use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use super::{ClientInfo, WorkspaceInfo};

fn socket_path() -> Option<String> {
    let xdg = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    Some(format!("{xdg}/hypr/{sig}/.socket.sock"))
}

fn hypr_request(cmd: &str) -> Option<String> {
    let path = socket_path()?;
    let mut stream = UnixStream::connect(&path).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .ok();
    let msg = format!("j/{cmd}");
    stream.write_all(msg.as_bytes()).ok()?;
    stream.shutdown(Shutdown::Write).ok();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).ok()?;
    Some(resp)
}

// Minimal JSON helpers — avoids pulling in a JSON library for Hyprland's flat JSON.

/// Decode the escapes that can appear inside a JSON string. Hyprland normally
/// emits raw UTF-8, but window titles do contain escaped quotes and backslashes.
fn unescape_json_string(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            // Unknown escapes (including \u) are kept verbatim.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn json_string(json: &str, key: &str) -> String {
    let needle = format!("\"{key}\":");
    let Some(pos) = json.find(&needle) else {
        return String::new();
    };
    let rest = json[pos + needle.len()..].trim_start_matches([' ', '\t']);
    let Some(inner) = rest.strip_prefix('"') else {
        return String::new();
    };

    // Walk to the closing quote, skipping escaped characters.
    let mut escaped = false;
    for (i, c) in inner.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return unescape_json_string(&inner[..i]);
        }
    }
    String::new()
}

fn json_int(json: &str, key: &str) -> i64 {
    let needle = format!("\"{key}\":");
    let Some(pos) = json.find(&needle) else {
        return 0;
    };
    let rest = json[pos + needle.len()..].trim_start_matches([' ', '\t']);
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    num.parse().unwrap_or(0)
}

fn json_array_objects(json: &str) -> Vec<&str> {
    let mut objs = Vec::new();
    let bytes = json.as_bytes();
    let mut depth = 0i32;
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &c) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    objs.push(&json[start..=i]);
                }
            }
            _ => {}
        }
    }
    objs
}

fn parse_address(json: &str) -> u64 {
    let s = json_string(json, "address");
    let hex = s.strip_prefix("0x").unwrap_or(&s);
    u64::from_str_radix(hex, 16).unwrap_or(0)
}

fn parse_array_pair(json: &str, key: &str) -> (i32, i32) {
    let needle = format!("\"{key}\":");
    let Some(pos) = json.find(&needle) else {
        return (0, 0);
    };
    let rest = &json[pos + needle.len()..];
    let Some(bracket) = rest.find('[') else {
        return (0, 0);
    };
    let inner = &rest[bracket + 1..];
    let a: i32 = inner
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    let Some(comma) = inner.find(',') else {
        return (a, 0);
    };
    let b: i32 = inner[comma + 1..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    (a, b)
}

pub fn get_workspaces() -> Vec<WorkspaceInfo> {
    let ws_json = hypr_request("workspaces").unwrap_or_default();
    let cl_json = hypr_request("clients").unwrap_or_default();

    let ws_objs = json_array_objects(&ws_json);
    let cl_objs = json_array_objects(&cl_json);

    let mut workspaces: Vec<WorkspaceInfo> = ws_objs
        .iter()
        .filter_map(|wj| {
            let id = json_int(wj, "id") as i32;
            if id < 1 {
                return None; // skip special workspaces
            }
            Some(WorkspaceInfo {
                id,
                name: json_string(wj, "name"),
                monitor_id: json_int(wj, "monitorID") as i32,
                clients: Vec::new(),
            })
        })
        .collect();

    workspaces.sort_by_key(|w| w.id);

    for cj in &cl_objs {
        let mut workspace_id = json_int(cj, "workspace") as i32;

        // Hyprland may use nested "workspace":{"id":1,"name":"1"}
        if workspace_id == 0 {
            if let Some(pos) = cj.find("\"workspace\":") {
                if let Some(brace) = cj[pos..].find('{') {
                    let inner_start = pos + brace;
                    if let Some(end) = cj[inner_start..].find('}') {
                        let inner = &cj[inner_start..inner_start + end + 1];
                        workspace_id = json_int(inner, "id") as i32;
                    }
                }
            }
        }

        let (x, y) = parse_array_pair(cj, "at");
        let (w, h) = parse_array_pair(cj, "size");

        let ci = ClientInfo {
            class_name: json_string(cj, "class"),
            title: json_string(cj, "title"),
            initial_title: json_string(cj, "initialTitle"),
            address: parse_address(cj),
            workspace_id,
            x,
            y,
            w,
            h,
        };

        if let Some(ws) = workspaces.iter_mut().find(|w| w.id == workspace_id) {
            ws.clients.push(ci);
        }
    }

    workspaces
}

fn dispatch(cmd: &str) {
    let Some(path) = socket_path() else { return };
    let Ok(mut stream) = UnixStream::connect(&path) else {
        return;
    };
    let msg = format!("/dispatch {cmd}");
    let _ = stream.write_all(msg.as_bytes());
    let _ = stream.shutdown(Shutdown::Write);
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
}

pub fn switch_workspace(id: i32) {
    dispatch(&format!("hl.dsp.focus({{ workspace = \"{id}\" }})"));
}

pub fn move_window_to_workspace(window_address: u64, workspace_id: i32) {
    dispatch(&format!(
        "hl.dsp.window.move({{ workspace = {workspace_id}, window = \"address:0x{window_address:x}\" }})"
    ));
}

pub fn focus_window(_workspace_id: i32, address: u64, _cx: i32, _cy: i32) {
    dispatch(&format!(
        "hl.dsp.focus({{ window = \"address:0x{address:x}\"}})"
    ));
    // Query the window's actual on-screen position after focus+scroll, then warp cursor there.
    if let Some(json) = hypr_request("activewindow") {
        let (x, y) = parse_array_pair(&json, "at");
        let (w, h) = parse_array_pair(&json, "size");
        let cx = x + w / 2;
        let cy = y + h / 2;
        dispatch(&format!("hl.dsp.cursor.move({{x = {cx}, y = {cy}}})"));
    }
}

pub fn get_active_workspace_name() -> String {
    let json = hypr_request("activeworkspace").unwrap_or_default();
    json_string(&json, "name")
}

/// Returns the address of the currently focused window, or 0 if none.
pub fn close_window(address: u64) {
    dispatch(&format!(
        "hl.dsp.window.close({{ window = \"address:0x{address:x}\" }})"
    ));
}

pub fn get_active_window_address() -> u64 {
    let json = hypr_request("activewindow").unwrap_or_default();
    // Hyprland returns `{}` when no window is focused
    if json.trim() == "{}" {
        return 0;
    }
    parse_address(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_reads_plain_value() {
        let json = r#"{"class":"firefox","title":"GitHub"}"#;
        assert_eq!(json_string(json, "class"), "firefox");
        assert_eq!(json_string(json, "title"), "GitHub");
    }

    #[test]
    fn json_string_handles_escaped_quotes() {
        let json = r#"{"title":"say \"hi\"","class":"foot"}"#;
        assert_eq!(json_string(json, "title"), r#"say "hi""#);
        assert_eq!(json_string(json, "class"), "foot");
    }

    #[test]
    fn json_string_handles_escaped_backslash_before_quote() {
        // Two backslashes encode one literal backslash, so the following quote
        // is the real terminator, not an escaped quote.
        let json = r#"{"class":"a\\","title":"b"}"#;
        assert_eq!(json_string(json, "class"), r"a\");
        assert_eq!(json_string(json, "title"), "b");
    }

    #[test]
    fn json_string_handles_control_escapes() {
        let json = r#"{"title":"a\nb\tc"}"#;
        assert_eq!(json_string(json, "title"), "a\nb\tc");
    }

    #[test]
    fn json_array_objects_ignores_braces_inside_strings() {
        let json = r#"[{"title":"a \"}\" b"},{"id":2}]"#;
        let objs = json_array_objects(json);
        assert_eq!(objs.len(), 2);
        assert!(objs[0].contains(r#""title":"a \"}\" b""#));
    }

    #[test]
    fn json_array_objects_handles_escaped_backslash_before_brace() {
        let json = r#"[{"title":"a\\"},{"id":2}]"#;
        let objs = json_array_objects(json);
        assert_eq!(objs.len(), 2);
        assert_eq!(objs[1], r#"{"id":2}"#);
    }

    #[test]
    fn parse_address_accepts_prefixed_and_bare_hex() {
        assert_eq!(parse_address(r#"{"address":"0xabc"}"#), 0xabc);
        assert_eq!(parse_address(r#"{"address":"abc"}"#), 0xabc);
    }
}
