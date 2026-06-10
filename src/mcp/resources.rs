use rmcp::{model::ResourceContents, ErrorData};

use super::AirpMcpServer;

/// DS-4: Parse optional `?offset=N&limit=M` query parameters from a resource URI.
///
/// Returns `(offset, limit)`. Default limit is 100 000 UTF-8 chars (≈ safe MCP message size).
/// The caller is responsible for slicing at character boundaries.
fn parse_offset_limit(uri: &str) -> (usize, usize) {
    const DEFAULT_LIMIT: usize = 100_000;
    let query = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let offset = query
        .split('&')
        .find_map(|p| p.strip_prefix("offset=").and_then(|v| v.parse().ok()))
        .unwrap_or(0);
    let limit = query
        .split('&')
        .find_map(|p| p.strip_prefix("limit=").and_then(|v| v.parse().ok()))
        .unwrap_or(DEFAULT_LIMIT);
    (offset, limit)
}

/// DS-4: Slice text at char boundaries and prepend a header if content was paginated.
///
/// Returns the slice and optionally a preamble line like
/// `[PARTIAL: total=455000, offset=0, limit=100000 — use ?offset=N&limit=M to page]`.
fn paginate_text(text: &str, uri: &str) -> String {
    let (offset, limit) = parse_offset_limit(uri);
    let total = text.len();
    // Ensure offset doesn't exceed length
    let start = offset.min(total);
    // Advance to next char boundary (text is UTF-8)
    let safe_start = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= start)
        .unwrap_or(total);
    let slice = &text[safe_start..];
    // Clamp limit to remaining length
    let end = limit.min(slice.len());
    let safe_end = slice
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= end)
        .unwrap_or(slice.len());
    let page = &slice[..safe_end];
    let needs_header = total > DEFAULT_LIMIT || offset > 0;
    if needs_header {
        format!(
            "[PARTIAL: total={} chars, offset={}, limit={} — append ?offset=N&limit=M to URI to page]\n{}",
            total, offset, limit, page
        )
    } else {
        page.to_owned()
    }
}
const DEFAULT_LIMIT: usize = 100_000;

/// DS-3：递归遍历 `base_dir` 下所有文件，返回相对路径列表（正斜杠，已排序）。
/// `preset.json` 本身排除（它是原料，不是产物）。
fn walk_preset_artifacts(base_dir: &std::path::Path) -> Vec<String> {
    let mut result = Vec::new();
    walk_dir_recursive(base_dir, base_dir, &[], &["preset.json"], &mut result);
    result.sort();
    result
}

/// 递归遍历 `characters/{id}/` 下 Agent 写入的产物文件。
/// 排除系统目录（card/world/history/memory/gating/sessions）和系统根文件（card.json/card.png）。
fn walk_character_artifacts(base_dir: &std::path::Path) -> Vec<String> {
    const SKIP_DIRS: &[&str] = &["card", "world", "history", "memory", "gating", "sessions"];
    const SKIP_ROOT_FILES: &[&str] = &["card.json", "card.png"];
    let mut result = Vec::new();
    walk_dir_recursive(base_dir, base_dir, SKIP_DIRS, SKIP_ROOT_FILES, &mut result);
    result.sort();
    result
}

fn walk_dir_recursive(
    base: &std::path::Path,
    current: &std::path::Path,
    skip_dirs: &[&str],
    skip_root_files: &[&str],
    out: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    let is_root = current == base;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if ft.is_dir() {
            // 仅在根层过滤系统目录
            if is_root && skip_dirs.contains(&name_str.as_ref()) {
                continue;
            }
            walk_dir_recursive(base, &path, skip_dirs, skip_root_files, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(base) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                // 根层系统文件排除
                if is_root && skip_root_files.contains(&rel_str.as_str()) {
                    continue;
                }
                out.push(rel_str);
            }
        }
    }
}

impl AirpMcpServer {
    /// URI 路由：返资源内容（统一 TextResourceContents）。
    ///
    /// 支持 URI:
    /// - `airp://characters` → 角色 ID 列表
    /// - `airp://characters/{id}/card` → card/raw.json 或 card.json
    /// - `airp://characters/{id}/world/lorebook` → world/lorebook.json
    /// - `airp://characters/{id}/state/live` → state/live.json（无则空对象）
    /// - `airp://presets` → 预设 ID 列表
    /// - `airp://presets/{id}/raw` → presets/{id}/preset.json 原文（供 Agent 自主分析）
    /// - `airp://plugins` → 插件命名空间列表（M_PLUGIN_DATA）
    /// - `airp://plugins/{name}/files` → 插件文件相对路径列表（递归）
    /// - `airp://plugins/{name}/data/{path}` → 插件文件内容（UTF-8，支持分页）
    pub(super) fn dispatch_resource(&self, uri: &str) -> Result<Vec<ResourceContents>, ErrorData> {
        // airp://presets
        if uri == "airp://presets" {
            let list = crate::data_dir::list_presets(&self.data_root).map_err(|e| {
                ErrorData::internal_error(format!("list_presets 失败: {}", e), None)
            })?;
            let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
            return Ok(vec![
                ResourceContents::text(json, uri).with_mime_type("application/json")
            ]);
        }

        // airp://presets/{id}/...
        if let Some(rest) = uri.strip_prefix("airp://presets/") {
            let mut split = rest.splitn(2, '/');
            let pid = split
                .next()
                .ok_or_else(|| ErrorData::invalid_params("缺 preset_id".to_string(), None))?;
            let sub_full = split.next().unwrap_or("");
            // DS-4: strip optional ?query string from the sub-path before matching
            let sub = sub_full.split_once('?').map(|(s, _)| s).unwrap_or(sub_full);
            crate::data_dir::validate_id_segment(pid)
                .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;
            match sub {
                "raw" => {
                    // DS-4: supports ?offset=N&limit=M for large files (e.g. test_preset ~455 KB)
                    let path = crate::data_dir::preset_json_path(&self.data_root, pid);
                    if !path.exists() {
                        return Err(ErrorData::invalid_params(
                            format!("预设 {} 无 preset.json", pid),
                            None,
                        ));
                    }
                    let raw = std::fs::read_to_string(&path).map_err(|e| {
                        ErrorData::internal_error(format!("读 preset.json 失败: {}", e), None)
                    })?;
                    let cleaned = crate::data_dir::strip_utf8_bom(&raw);
                    let output = paginate_text(cleaned, uri);
                    return Ok(vec![
                        ResourceContents::text(output, uri).with_mime_type("application/json")
                    ]);
                }
                "artifacts" => {
                    // DS-3：列出 Agent 已写入的产物文件（排除 preset.json 本身）
                    let preset_dir = self.data_root.join("presets").join(pid);
                    let files = walk_preset_artifacts(&preset_dir);
                    let json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string());
                    return Ok(vec![
                        ResourceContents::text(json, uri).with_mime_type("application/json")
                    ]);
                }
                "regex" => {
                    // PR-9：列出 presets/{id}/regex/*.json 的富元数据
                    let req = super::tools::ListPresetRegexScriptsRequest {
                        preset_id: pid.to_string(),
                    };
                    use rmcp::handler::server::wrapper::Parameters;
                    let json = self
                        .list_preset_regex_scripts_impl(Parameters(req))
                        .unwrap_or_else(|_| "[]".to_string());
                    return Ok(vec![
                        ResourceContents::text(json, uri).with_mime_type("application/json")
                    ]);
                }
                other => {
                    return Err(ErrorData::invalid_params(
                        format!("未知预设子资源 '{}' (URI={})", other, uri),
                        None,
                    ));
                }
            }
        }

        // airp://characters
        if uri == "airp://characters" {
            let list = crate::data_dir::list_characters(&self.data_root).map_err(|e| {
                ErrorData::internal_error(format!("list_characters 失败: {}", e), None)
            })?;
            let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
            return Ok(vec![
                ResourceContents::text(json, uri).with_mime_type("application/json")
            ]);
        }

        // M_PLUGIN_DATA: airp://plugins → 插件命名空间列表
        if uri == "airp://plugins" {
            let plugins_dir = self.data_root.join("plugins");
            let mut names: Vec<String> = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        names.push(entry.file_name().to_string_lossy().into_owned());
                    }
                }
            }
            names.sort();
            let json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
            return Ok(vec![
                ResourceContents::text(json, uri).with_mime_type("application/json")
            ]);
        }

        // M_PLUGIN_DATA: airp://plugins/{plugin_name}/...
        if let Some(rest) = uri.strip_prefix("airp://plugins/") {
            let mut split = rest.splitn(2, '/');
            let pname = split
                .next()
                .ok_or_else(|| ErrorData::invalid_params("缺 plugin_name".to_string(), None))?;
            let sub_full = split.next().unwrap_or("");
            let sub = sub_full.split_once('?').map(|(s, _)| s).unwrap_or(sub_full);
            crate::data_dir::validate_id_segment(pname)
                .map_err(|e| ErrorData::invalid_params(format!("非法 plugin_name: {}", e), None))?;
            let plugin_dir = self.data_root.join("plugins").join(pname);

            if sub == "files" {
                // 递归列出插件命名空间下所有文件（无系统目录需排除 — 全部是插件自己的数据）
                let mut files = Vec::new();
                walk_dir_recursive(&plugin_dir, &plugin_dir, &[], &[], &mut files);
                files.sort();
                let json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string());
                return Ok(vec![
                    ResourceContents::text(json, uri).with_mime_type("application/json")
                ]);
            }
            if let Some(rel_path) = sub.strip_prefix("data/") {
                if !plugin_dir.exists() {
                    return Err(ErrorData::invalid_params(
                        format!("插件 `{}` 不存在", pname),
                        None,
                    ));
                }
                let target = crate::data_dir::safe_resolve_for_write(&plugin_dir, rel_path)
                    .map_err(|e| {
                        ErrorData::invalid_params(format!("非法插件数据路径: {}", e), None)
                    })?;
                if !target.is_file() {
                    return Err(ErrorData::invalid_params(
                        format!("文件不存在: plugins/{}/{}", pname, rel_path),
                        None,
                    ));
                }
                let raw = std::fs::read_to_string(&target).map_err(|_| {
                    ErrorData::invalid_params(
                        "文件非 UTF-8 文本，请用 plugin_blob_read 工具以 base64 读取".to_string(),
                        None,
                    )
                })?;
                let output = paginate_text(&raw, uri);
                return Ok(vec![
                    ResourceContents::text(output, uri).with_mime_type("text/plain")
                ]);
            }
            return Err(ErrorData::invalid_params(
                format!("未知插件子资源 '{}' (URI={})", sub, uri),
                None,
            ));
        }

        // M_UP: airp://users
        if uri == "airp://users" {
            let list = crate::data_dir::list_users(&self.data_root)
                .map_err(|e| ErrorData::internal_error(format!("list_users 失败: {}", e), None))?;
            let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string());
            return Ok(vec![
                ResourceContents::text(json, uri).with_mime_type("application/json")
            ]);
        }

        // M_UP: airp://users/{id}/...
        if let Some(rest) = uri.strip_prefix("airp://users/") {
            let mut split = rest.splitn(2, '/');
            let uid = split
                .next()
                .ok_or_else(|| ErrorData::invalid_params("缺 user_id".to_string(), None))?;
            let sub_full = split.next().unwrap_or("");
            let sub = sub_full.split_once('?').map(|(s, _)| s).unwrap_or(sub_full);
            let uid = crate::types::UserId::new(uid)
                .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;
            match sub {
                "persona" => {
                    let path = crate::data_dir::user_persona_path(&self.data_root, &uid);
                    let body = if path.exists() {
                        std::fs::read_to_string(&path)
                            .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
                            .unwrap_or_else(|_| "{}".to_string())
                    } else {
                        // Empty persona indicated by null + locked=false echoed shape
                        "null".to_string()
                    };
                    return Ok(vec![
                        ResourceContents::text(body, uri).with_mime_type("application/json")
                    ]);
                }
                "state/live" => {
                    let path = crate::data_dir::user_state_live_path(&self.data_root, &uid);
                    let body = if path.exists() {
                        std::fs::read_to_string(&path)
                            .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
                            .unwrap_or_else(|_| "{}".to_string())
                    } else {
                        "{}".to_string()
                    };
                    return Ok(vec![
                        ResourceContents::text(body, uri).with_mime_type("application/json")
                    ]);
                }
                other => {
                    return Err(ErrorData::invalid_params(
                        format!("未知用户子资源 '{}' (URI={})", other, uri),
                        None,
                    ));
                }
            }
        }

        // airp://characters/{id}/...
        let rest = uri
            .strip_prefix("airp://characters/")
            .ok_or_else(|| ErrorData::invalid_params(format!("未知 URI: {}", uri), None))?;
        let mut split = rest.splitn(2, '/');
        let cid = split
            .next()
            .ok_or_else(|| ErrorData::invalid_params("缺 character_id".to_string(), None))?;
        let sub = split.next().unwrap_or("");
        crate::data_dir::validate_id_segment(cid)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        match sub {
            "card" => {
                let cdir = self.data_root.join("characters").join(cid);
                let path = if cdir.join("card").join("raw.json").exists() {
                    cdir.join("card").join("raw.json")
                } else if cdir.join("card.json").exists() {
                    cdir.join("card.json")
                } else {
                    return Err(ErrorData::invalid_params(
                        format!("角色 {} 无 card.json", cid),
                        None,
                    ));
                };
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| ErrorData::internal_error(format!("读 card 失败: {}", e), None))?;
                let cleaned = crate::data_dir::strip_utf8_bom(&raw).to_owned();
                Ok(vec![
                    ResourceContents::text(cleaned, uri).with_mime_type("application/json")
                ])
            }
            "world/lorebook" => {
                let lb_path = crate::data_dir::char_world_lorebook_path(&self.data_root, cid);
                if !lb_path.exists() {
                    // 返空 lorebook 而非错误，便于 client 容错探测
                    return Ok(vec![ResourceContents::text(r#"{"entries":[]}"#, uri)
                        .with_mime_type("application/json")]);
                }
                let raw = std::fs::read_to_string(&lb_path).map_err(|e| {
                    ErrorData::internal_error(format!("读 lorebook 失败: {}", e), None)
                })?;
                let cleaned = crate::data_dir::strip_utf8_bom(&raw).to_owned();
                Ok(vec![
                    ResourceContents::text(cleaned, uri).with_mime_type("application/json")
                ])
            }
            "artifacts" => {
                // DS-B：列出 Agent 已写入的角色卡产物（排除系统目录和系统文件）
                let char_dir = self.data_root.join("characters").join(cid);
                let files = walk_character_artifacts(&char_dir);
                let json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string());
                Ok(vec![
                    ResourceContents::text(json, uri).with_mime_type("application/json")
                ])
            }
            "history" => {
                // chat_log.jsonl 路径（CF-2 格式，见 chat_store.rs）
                let jsonl = self
                    .data_root
                    .join("characters")
                    .join(cid)
                    .join("history")
                    .join("chat_log.jsonl");
                if !jsonl.exists() {
                    let empty = serde_json::json!({"character_id": cid, "messages": []});
                    return Ok(vec![ResourceContents::text(empty.to_string(), uri)
                        .with_mime_type("application/json")]);
                }
                let log = crate::chat_store::ChatLog::load_or_create(&self.data_root, cid)
                    .map_err(|e| {
                        ErrorData::internal_error(format!("读 ChatLog 失败: {}", e), None)
                    })?;
                let json = serde_json::to_string(&log)
                    .unwrap_or_else(|_| r#"{"messages":[]}"#.to_string());
                Ok(vec![
                    ResourceContents::text(json, uri).with_mime_type("application/json")
                ])
            }
            "state/live" => {
                // M_LS 实现前，返空 stub
                let state_path = self
                    .data_root
                    .join("characters")
                    .join(cid)
                    .join("state")
                    .join("live.json");
                let body = if state_path.exists() {
                    std::fs::read_to_string(&state_path)
                        .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
                        .unwrap_or_else(|_| "{}".to_string())
                } else {
                    "{}".to_string()
                };
                Ok(vec![
                    ResourceContents::text(body, uri).with_mime_type("application/json")
                ])
            }
            other => Err(ErrorData::invalid_params(
                format!("未知子资源 '{}' (URI={})", other, uri),
                None,
            )),
        }
    }
}
