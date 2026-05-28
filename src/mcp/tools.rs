use rmcp::{handler::server::wrapper::Parameters, schemars, ErrorData};

use super::AirpMcpServer;

// ── 工具入参 ─────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PingRequest {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImportCardRequest {
    /// 角色 ID（文件夹名）。二选一传 `card_json` 或 `card_png_base64`。
    pub character_id: String,
    /// SillyTavern V2 JSON 字符串（可选）。
    #[serde(default)]
    pub card_json: Option<String>,
    /// PNG 角色卡 base64 编码（可选）。tEXt chara chunk 内 JSON 自动提取。
    #[serde(default)]
    pub card_png_base64: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ApplyLorebookRequest {
    /// 角色 ID。读 `characters/{id}/world/lorebook.json` 自动发现。
    pub character_id: String,
    /// 待扫描文本（通常是用户消息 + 最近对话上下文）。
    pub text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StartSessionRequest {
    pub character_id: String,
    /// 可选 session UUID；为空时使用 legacy 默认 session（CF-3 后位于 `memory/`）。
    #[serde(default)]
    pub session_id: Option<String>,
    /// 可选预设 ID。指定时合并预设 prompts 进 system prompt。
    #[serde(default)]
    pub preset_id: Option<String>,
    /// 用户显示名，宏 `{{user}}` 替换用。
    #[serde(default = "default_user_name")]
    pub user_name: String,
}

pub(super) fn default_user_name() -> String {
    "User".to_string()
}

fn default_context_n() -> usize {
    10
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetRecentContextRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 返回最近 N 条消息（默认 10，最多 1000）。
    #[serde(default = "default_context_n")]
    pub n: usize,
    /// 可选 session UUID；预留字段，当前不影响行为。
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AppendMessageRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 消息角色："user" | "assistant" | "system"。
    pub role: String,
    /// 消息文本内容。
    pub content: String,
    /// 可选 session UUID；预留字段，当前不影响行为。
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateStateRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 状态 JSON 字符串（必须是合法 JSON 对象）。
    /// overwrite=false 时与现有 live.json 合并（字段覆盖）；overwrite=true 时全量替换。
    pub state_json: String,
    /// true = 覆盖全部状态；false（默认）= 合并到现有状态。
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WritePresetArtifactRequest {
    /// 预设 ID（对应 presets/{id}/ 目录）。
    pub preset_id: String,
    /// 产物路径（相对于 presets/{id}/），如 `regex/display_layer.json`。不允许 `..` 或绝对路径。
    pub artifact_path: String,
    /// 文件内容（UTF-8 字符串）。
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WriteCharacterArtifactRequest {
    /// 角色 ID（对应 characters/{id}/ 目录）。
    pub character_id: String,
    /// 产物路径（相对于 characters/{id}/），如 `analysis/profile.md`。不允许 `..` 或绝对路径。
    pub artifact_path: String,
    /// 文件内容（UTF-8 字符串）。
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImportPresetRequest {
    /// 预设 ID（文件夹名，对应 presets/{id}/preset.json）。
    pub preset_id: String,
    /// SillyTavern Preset JSON 完整内容字符串。必须为合法 JSON。
    pub preset_json: String,
}

/// PR-5: 列出预设关联的所有正则脚本。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListPresetRegexScriptsRequest {
    /// 预设 ID（对应 presets/{id}/regex/ 目录）。
    pub preset_id: String,
}

/// PR-6: 删除预设正则脚本文件。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemovePresetRegexScriptRequest {
    /// 预设 ID。
    pub preset_id: String,
    /// 脚本文件名（仅文件名，如 `hide_thoughts.json`）。不允许路径分隔符或 `..`。
    pub filename: String,
}

/// PR-7: 启用或禁用单条正则脚本。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetPresetRegexEnabledRequest {
    /// 预设 ID。
    pub preset_id: String,
    /// 脚本文件名（仅文件名，如 `hide_thoughts.json`）。
    pub filename: String,
    /// true = 启用（disabled=false）；false = 禁用（disabled=true）。
    pub enabled: bool,
}

/// DS-9: 回滚角色 ChatLog 最后 N 条消息。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RollbackMessagesRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 删除最后 N 条消息（最少 1，最多 1000）。默认 1。
    #[serde(default = "default_rollback_n")]
    pub n: usize,
}

fn default_rollback_n() -> usize {
    1
}

/// DS-10: 列出角色的所有具名 session（非 legacy 默认 session）。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListSessionsRequest {
    /// 角色 ID。
    pub character_id: String,
}

/// DS-11: 读取角色实时状态历史快照（state/history.jsonl）。
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetStateHistoryRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 返回最近 N 条快照（默认 10，最多 1000），newest-first。
    #[serde(default = "default_state_history_n")]
    pub n: usize,
}

fn default_state_history_n() -> usize {
    10
}

// ── 工具实现（业务逻辑）─────────────────────────────────────────────────────
// #[tool_router] 宏生成的 fn tool_router() 是 mod.rs 私有，故 thin wrappers 留在 mod.rs。
// 此处只存放实际业务逻辑（供 mod.rs wrappers 调用，pub(super) 可见）。

impl AirpMcpServer {
    /// MCP-1 实现。
    pub(super) fn ping_impl(&self, _params: Parameters<PingRequest>) -> String {
        format!(
            "AIRP MCP Server v{} (data_root={})",
            env!("CARGO_PKG_VERSION"),
            self.data_root.display()
        )
    }

    /// MCP-2.1 实现：导入角色卡。
    pub(super) fn import_card_impl(
        &self,
        Parameters(req): Parameters<ImportCardRequest>,
    ) -> Result<String, ErrorData> {
        let (card_format, _json_str) = crate::daemon::import_card_to_disk(
            &self.data_root,
            &req.character_id,
            req.card_json,
            req.card_png_base64,
        )
        .map_err(|e| ErrorData::internal_error(format!("import_card 失败: {}", e), None))?;

        let greetings_dir = self
            .data_root
            .join("characters")
            .join(&req.character_id)
            .join("card")
            .join("greetings");
        let greetings_count = std::fs::read_dir(&greetings_dir)
            .map(|it| it.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        let lb_path =
            crate::data_dir::char_world_lorebook_path(&self.data_root, &req.character_id);
        let lorebook_entries = if lb_path.exists() {
            std::fs::read_to_string(&lb_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("entries")
                        .and_then(|e| e.as_array())
                        .map(|arr| arr.len())
                })
                .unwrap_or(0)
        } else {
            0
        };

        let result = serde_json::json!({
            "character_id": req.character_id,
            "card_format": card_format,
            "greetings_count": greetings_count,
            "lorebook_entries": lorebook_entries,
        });
        Ok(result.to_string())
    }

    /// MCP-2.2 实现：lorebook 关键词扫描。
    pub(super) fn apply_lorebook_impl(
        &self,
        Parameters(req): Parameters<ApplyLorebookRequest>,
    ) -> Result<String, ErrorData> {
        let lb_path =
            crate::data_dir::char_world_lorebook_path(&self.data_root, &req.character_id);
        if !lb_path.exists() {
            return Ok(String::new());
        }
        let raw = std::fs::read_to_string(&lb_path)
            .map_err(|e| ErrorData::internal_error(format!("读 lorebook 失败: {}", e), None))?;
        let cleaned = crate::data_dir::strip_utf8_bom(&raw).to_owned();
        let orch = crate::orchestrator::Orchestrator::new(None, Some(&cleaned)).map_err(|e| {
            ErrorData::internal_error(format!("Orchestrator 构造失败: {}", e), None)
        })?;
        Ok(orch.trigger_lorebook(&req.text))
    }

    /// MCP-2.3 实现：启动 RP 会话。
    pub(super) fn start_session_impl(
        &self,
        Parameters(req): Parameters<StartSessionRequest>,
    ) -> Result<String, ErrorData> {
        let char_dir = self.data_root.join("characters").join(&req.character_id);
        let card_path = if char_dir.join("card").join("raw.json").exists() {
            char_dir.join("card").join("raw.json")
        } else if char_dir.join("card.json").exists() {
            char_dir.join("card.json")
        } else {
            return Err(ErrorData::invalid_params(
                format!("角色 {} 无 card.json / card/raw.json", req.character_id),
                None,
            ));
        };
        let card_json = std::fs::read_to_string(&card_path)
            .map_err(|e| ErrorData::internal_error(format!("读 card 失败: {}", e), None))?;
        let card_json = crate::data_dir::strip_utf8_bom(&card_json).to_owned();

        let lb_path =
            crate::data_dir::char_world_lorebook_path(&self.data_root, &req.character_id);
        let lorebook_json = if lb_path.exists() {
            std::fs::read_to_string(&lb_path)
                .ok()
                .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
        } else {
            None
        };

        let preset_json = req.preset_id.as_ref().and_then(|pid| {
            let new_path = crate::data_dir::preset_json_path(&self.data_root, pid);
            let legacy = self.data_root.join("presets").join(format!("{}.json", pid));
            let p = if new_path.exists() { new_path } else { legacy };
            std::fs::read_to_string(&p)
                .ok()
                .map(|s| crate::data_dir::strip_utf8_bom(&s).to_owned())
        });

        let orch =
            crate::orchestrator::Orchestrator::new(Some(&card_json), lorebook_json.as_deref())
                .map_err(|e| {
                    ErrorData::internal_error(format!("Orchestrator 构造失败: {}", e), None)
                })?;

        let variables = std::collections::HashMap::new();
        let system_prompt = orch.build_system_prompt_with_preset(
            &self.data_root,
            Some(&req.character_id),
            &req.user_name,
            &variables,
            "",
            preset_json.as_deref(),
            None,
            "",
        );

        let greetings_dir = self
            .data_root
            .join("characters")
            .join(&req.character_id)
            .join("card")
            .join("greetings");
        let mut greetings: Vec<String> = Vec::new();
        if greetings_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(&greetings_dir)
                .ok()
                .into_iter()
                .flat_map(|it| it.filter_map(|e| e.ok()).collect::<Vec<_>>())
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    == Some("md")
                {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        greetings.push(content);
                    }
                }
            }
        }

        let session_id_parsed = req
            .session_id
            .as_deref()
            .and_then(|s| crate::types::SessionId::parse(s).ok());
        let session_dir = crate::data_dir::resolve_session_dir(
            &self.data_root,
            &req.character_id,
            session_id_parsed.as_ref(),
        )
        .map_err(|e| {
            ErrorData::internal_error(format!("session 目录解析失败: {}", e), None)
        })?;

        let result = serde_json::json!({
            "character_id": req.character_id,
            "session_id": req.session_id,
            "session_dir": session_dir.to_string_lossy(),
            "system_prompt": system_prompt,
            "greetings_count": greetings.len(),
            "greetings": greetings,
        });
        Ok(result.to_string())
    }

    /// DS-B 实现：Agent 分析预设后写入产物文件。
    /// `artifact_path` 受限于 `presets/{preset_id}/` 目录（路径穿越攻击防护）。
    pub(super) fn write_preset_artifact_impl(
        &self,
        Parameters(req): Parameters<WritePresetArtifactRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.preset_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;

        let preset_dir = self.data_root.join("presets").join(&req.preset_id);
        // 确保基目录存在，safe_resolve_for_write 需要 canonicalize base_dir
        std::fs::create_dir_all(&preset_dir).map_err(|e| {
            ErrorData::internal_error(format!("创建 presets/{} 目录失败: {}", req.preset_id, e), None)
        })?;
        let artifact_full =
            crate::data_dir::safe_resolve_for_write(&preset_dir, &req.artifact_path)
                .map_err(|e| {
                    ErrorData::invalid_params(format!("非法 artifact_path: {}", e), None)
                })?;

        if let Some(parent) = artifact_full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ErrorData::internal_error(format!("创建目录失败: {}", e), None)
            })?;
        }

        std::fs::write(&artifact_full, req.content.as_bytes()).map_err(|e| {
            ErrorData::internal_error(format!("写文件失败: {}", e), None)
        })?;

        Ok(serde_json::json!({
            "preset_id": req.preset_id,
            "artifact_path": req.artifact_path,
            "bytes_written": req.content.len(),
        })
        .to_string())
    }

    /// DS-5 实现：导入预设 JSON，写入 presets/{preset_id}/preset.json。
    pub(super) fn import_preset_impl(
        &self,
        Parameters(req): Parameters<ImportPresetRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.preset_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;

        // 校验 JSON 合法性（拒绝非 JSON 内容）
        let _ = serde_json::from_str::<serde_json::Value>(&req.preset_json)
            .map_err(|e| ErrorData::invalid_params(format!("preset_json 不是合法 JSON: {}", e), None))?;

        let preset_path = crate::data_dir::preset_json_path(&self.data_root, &req.preset_id);
        if let Some(parent) = preset_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ErrorData::internal_error(format!("创建目录失败: {}", e), None)
            })?;
        }
        let bytes = req.preset_json.as_bytes();
        std::fs::write(&preset_path, bytes)
            .map_err(|e| ErrorData::internal_error(format!("写 preset.json 失败: {}", e), None))?;

        Ok(serde_json::json!({
            "preset_id": req.preset_id,
            "path": preset_path.to_string_lossy(),
            "bytes_written": bytes.len(),
        })
        .to_string())
    }

    /// PR-5 实现：列出预设关联的所有正则脚本（含文件名 + 全字段）。
    pub(super) fn list_preset_regex_scripts_impl(
        &self,
        Parameters(req): Parameters<ListPresetRegexScriptsRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.preset_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;

        let regex_dir = self.data_root.join("presets").join(&req.preset_id).join("regex");
        if !regex_dir.exists() {
            return Ok("[]".to_string());
        }

        let mut scripts: Vec<serde_json::Value> = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&regex_dir)
            .map_err(|e| ErrorData::internal_error(format!("读 regex 目录失败: {}", e), None))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.path().extension().and_then(|s| s.to_str()) == Some("json")
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            let filename = entry.file_name().to_string_lossy().into_owned();
            let raw = match std::fs::read_to_string(&path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(path = ?path, err = %e, "PR-5: 读脚本失败");
                    continue;
                }
            };
            let cleaned = crate::data_dir::strip_utf8_bom(&raw);

            // 解析为 Value 以保留所有原始字段，注入 filename
            let mut v: serde_json::Value = match serde_json::from_str(cleaned) {
                Ok(v) => v,
                Err(_) => {
                    // 数组格式：返回数组中每个条目并附加 filename
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(cleaned) {
                        for mut item in arr {
                            if let Some(obj) = item.as_object_mut() {
                                obj.insert("_filename".to_string(), serde_json::json!(filename));
                            }
                            scripts.push(item);
                        }
                    }
                    continue;
                }
            };
            if let Some(obj) = v.as_object_mut() {
                obj.insert("_filename".to_string(), serde_json::json!(filename));
            }
            scripts.push(v);
        }

        serde_json::to_string(&scripts)
            .map_err(|e| ErrorData::internal_error(format!("序列化失败: {}", e), None))
    }

    /// PR-6 实现：删除预设正则脚本文件。
    pub(super) fn remove_preset_regex_script_impl(
        &self,
        Parameters(req): Parameters<RemovePresetRegexScriptRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.preset_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;

        let preset_dir = self.data_root.join("presets").join(&req.preset_id);
        // safe_resolve_for_write 防路径穿越：确保 regex/{filename} 在 preset_dir 内
        let target =
            crate::data_dir::safe_resolve_for_write(&preset_dir, &format!("regex/{}", req.filename))
                .map_err(|e| ErrorData::invalid_params(format!("非法 filename: {}", e), None))?;

        if !target.exists() {
            return Err(ErrorData::invalid_params(
                format!("脚本文件不存在: {}", req.filename),
                None,
            ));
        }

        std::fs::remove_file(&target)
            .map_err(|e| ErrorData::internal_error(format!("删除失败: {}", e), None))?;

        Ok(serde_json::json!({
            "preset_id": req.preset_id,
            "filename": req.filename,
            "removed": true,
        })
        .to_string())
    }

    /// PR-7 实现：启用或禁用单条正则脚本（写回 disabled 字段）。
    pub(super) fn set_preset_regex_enabled_impl(
        &self,
        Parameters(req): Parameters<SetPresetRegexEnabledRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.preset_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 preset_id: {}", e), None))?;

        let preset_dir = self.data_root.join("presets").join(&req.preset_id);
        let target =
            crate::data_dir::safe_resolve_for_write(&preset_dir, &format!("regex/{}", req.filename))
                .map_err(|e| ErrorData::invalid_params(format!("非法 filename: {}", e), None))?;

        if !target.exists() {
            return Err(ErrorData::invalid_params(
                format!("脚本文件不存在: {}", req.filename),
                None,
            ));
        }

        let raw = std::fs::read_to_string(&target)
            .map_err(|e| ErrorData::internal_error(format!("读文件失败: {}", e), None))?;
        let cleaned = crate::data_dir::strip_utf8_bom(&raw).to_owned();

        // 支持单对象（设 disabled 字段）；数组格式对所有条目设置
        let new_disabled = !req.enabled;
        let updated: String = if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&cleaned) {
            if v.is_object() {
                v["disabled"] = serde_json::json!(new_disabled);
            } else if let Some(arr) = v.as_array_mut() {
                for item in arr.iter_mut() {
                    item["disabled"] = serde_json::json!(new_disabled);
                }
            }
            serde_json::to_string_pretty(&v)
                .map_err(|e| ErrorData::internal_error(format!("序列化失败: {}", e), None))?
        } else {
            return Err(ErrorData::internal_error(
                format!("脚本文件非合法 JSON: {}", req.filename),
                None,
            ));
        };

        std::fs::write(&target, updated.as_bytes())
            .map_err(|e| ErrorData::internal_error(format!("写文件失败: {}", e), None))?;

        Ok(serde_json::json!({
            "preset_id": req.preset_id,
            "filename": req.filename,
            "enabled": req.enabled,
            "disabled": new_disabled,
        })
        .to_string())
    }

    /// DS-6 实现：返回角色 ChatLog 最近 N 条消息。
    pub(super) fn get_recent_context_impl(
        &self,
        Parameters(req): Parameters<GetRecentContextRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let log =
            crate::chat_store::ChatLog::load_or_create(&self.data_root, &req.character_id)
                .map_err(|e| {
                    ErrorData::internal_error(format!("读 ChatLog 失败: {}", e), None)
                })?;

        let msgs = log.recent(req.n);
        serde_json::to_string(&serde_json::json!({
            "character_id": req.character_id,
            "total_messages": log.messages.len(),
            "returned": msgs.len(),
            "messages": msgs,
        }))
        .map_err(|e| ErrorData::internal_error(format!("序列化失败: {}", e), None))
    }

    /// DS-7 实现：向角色 ChatLog 追加一条消息。
    pub(super) fn append_message_impl(
        &self,
        Parameters(req): Parameters<AppendMessageRequest>,
    ) -> Result<String, ErrorData> {
        use crate::adapter::{ChatMessage, MessageRole};

        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let role = match req.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            other => {
                return Err(ErrorData::invalid_params(
                    format!("未知 role '{}': 必须为 user / assistant / system", other),
                    None,
                ))
            }
        };

        let mut log =
            crate::chat_store::ChatLog::load_or_create(&self.data_root, &req.character_id)
                .map_err(|e| {
                    ErrorData::internal_error(format!("读 ChatLog 失败: {}", e), None)
                })?;

        let msg = ChatMessage {
            role,
            content: req.content.clone(),
        };
        log.append(&self.data_root, msg)
            .map_err(|e| ErrorData::internal_error(format!("写 ChatLog 失败: {}", e), None))?;

        Ok(serde_json::json!({
            "character_id": req.character_id,
            "role": req.role,
            "total_messages": log.messages.len(),
        })
        .to_string())
    }

    /// DS-8 实现：直接更新角色实时状态（state/live.json）。
    /// 供 MCP client 在 RP 对话后调用，持久化状态变更（HP/MP/位置等）。
    pub(super) fn update_state_impl(
        &self,
        Parameters(req): Parameters<UpdateStateRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let delta: serde_json::Value = serde_json::from_str(&req.state_json).map_err(|e| {
            ErrorData::invalid_params(format!("state_json 非合法 JSON: {}", e), None)
        })?;

        if !delta.is_object() {
            return Err(ErrorData::invalid_params(
                "state_json 必须是 JSON 对象（{...}）".to_string(),
                None,
            ));
        }

        let state_dir = crate::data_dir::char_state_dir(&self.data_root, &req.character_id);
        std::fs::create_dir_all(&state_dir)
            .map_err(|e| ErrorData::internal_error(format!("创建 state/ 目录失败: {}", e), None))?;
        let live_path = state_dir.join("live.json");

        let merged: serde_json::Value = if req.overwrite {
            delta.clone()
        } else {
            // 读现有状态，合并 delta 字段
            let existing: serde_json::Value = if live_path.exists() {
                std::fs::read_to_string(&live_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            let mut merged = existing;
            if let (Some(m), Some(d)) = (merged.as_object_mut(), delta.as_object()) {
                for (k, v) in d {
                    m.insert(k.clone(), v.clone());
                }
            }
            merged
        };

        let json = serde_json::to_string_pretty(&merged)
            .map_err(|e| ErrorData::internal_error(format!("序列化状态失败: {}", e), None))?;
        std::fs::write(&live_path, json.as_bytes())
            .map_err(|e| ErrorData::internal_error(format!("写 live.json 失败: {}", e), None))?;

        // 追加快照到 state/history.jsonl
        let history_path = crate::data_dir::char_state_history_path(&self.data_root, &req.character_id);
        let snapshot = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "state": merged,
        });
        let mut line = serde_json::to_string(&snapshot).unwrap_or_default();
        line.push('\n');
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&history_path)
        {
            use std::io::Write as _;
            let _ = f.write_all(line.as_bytes());
        }

        Ok(serde_json::json!({
            "character_id": req.character_id,
            "overwrite": req.overwrite,
            "fields_updated": delta.as_object().map(|m| m.len()).unwrap_or(0),
            "state": merged,
        })
        .to_string())
    }

    /// DS-B 实现：Agent 分析角色卡后写入产物文件。
    /// `artifact_path` 受限于 `characters/{character_id}/` 目录（路径穿越攻击防护）。
    pub(super) fn write_character_artifact_impl(
        &self,
        Parameters(req): Parameters<WriteCharacterArtifactRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let char_dir = self.data_root.join("characters").join(&req.character_id);
        // 确保基目录存在，safe_resolve_for_write 需要 canonicalize base_dir
        std::fs::create_dir_all(&char_dir).map_err(|e| {
            ErrorData::internal_error(format!("创建 characters/{} 目录失败: {}", req.character_id, e), None)
        })?;
        let artifact_full =
            crate::data_dir::safe_resolve_for_write(&char_dir, &req.artifact_path)
                .map_err(|e| {
                    ErrorData::invalid_params(format!("非法 artifact_path: {}", e), None)
                })?;

        if let Some(parent) = artifact_full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ErrorData::internal_error(format!("创建目录失败: {}", e), None)
            })?;
        }

        std::fs::write(&artifact_full, req.content.as_bytes()).map_err(|e| {
            ErrorData::internal_error(format!("写文件失败: {}", e), None)
        })?;

        Ok(serde_json::json!({
            "character_id": req.character_id,
            "artifact_path": req.artifact_path,
            "bytes_written": req.content.len(),
        })
        .to_string())
    }

    /// DS-9: 回滚 ChatLog 最后 N 条消息。
    ///
    /// 使用 `ChatLog::delete_last_n` 完整重写 JSONL（不可逆）。
    /// 用于 LLM 生成质量差时撤销最近几轮对话，然后重新 `append_message`。
    pub(super) fn rollback_messages_impl(
        &self,
        Parameters(req): Parameters<RollbackMessagesRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let n = req.n.clamp(1, 1000);

        let mut log =
            crate::chat_store::ChatLog::load_or_create(&self.data_root, &req.character_id)
                .map_err(|e| {
                    ErrorData::internal_error(format!("加载 ChatLog 失败: {}", e), None)
                })?;

        let before = log.messages.len();
        log.delete_last_n(&self.data_root, n).map_err(|e| {
            ErrorData::internal_error(format!("回滚失败: {}", e), None)
        })?;
        let after = log.messages.len();
        let removed = before - after;

        Ok(serde_json::json!({
            "character_id": req.character_id,
            "requested": n,
            "removed": removed,
            "total_messages": after,
        })
        .to_string())
    }

    /// DS-10 实现：列出角色的具名 sessions（characters/{id}/sessions/ 子目录）。
    pub(super) fn list_sessions_impl(
        &self,
        Parameters(req): Parameters<ListSessionsRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;
        let sessions =
            crate::data_dir::list_sessions(&self.data_root, &req.character_id).map_err(|e| {
                ErrorData::internal_error(format!("列举 sessions 失败: {}", e), None)
            })?;
        let ids: Vec<String> = sessions.iter().map(|s| s.to_string()).collect();
        Ok(serde_json::json!({
            "character_id": req.character_id,
            "sessions": ids,
            "count": ids.len(),
        })
        .to_string())
    }

    /// DS-11 实现：读取状态历史快照（state/history.jsonl，newest-first）。
    pub(super) fn get_state_history_impl(
        &self,
        Parameters(req): Parameters<GetStateHistoryRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;

        let n = req.n.clamp(1, 1000);
        let history_path =
            crate::data_dir::char_state_history_path(&self.data_root, &req.character_id);

        if !history_path.exists() {
            return Ok(serde_json::json!({
                "character_id": req.character_id,
                "entries": [],
                "count": 0,
            })
            .to_string());
        }

        let text = std::fs::read_to_string(&history_path).map_err(|e| {
            ErrorData::internal_error(format!("读取 state/history.jsonl 失败: {}", e), None)
        })?;

        let entries: Vec<serde_json::Value> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(n)
            .collect();

        let count = entries.len();
        Ok(serde_json::json!({
            "character_id": req.character_id,
            "entries": entries,
            "count": count,
        })
        .to_string())
    }
}
