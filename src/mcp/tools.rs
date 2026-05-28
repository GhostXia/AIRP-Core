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

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
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
    /// AUDIT-12: 幂等键。同一 key 重复调用会返回缓存的首次结果，
    /// 不再追加消息。用于 harness retry 防重。
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateStateRequest {
    /// 角色 ID。
    pub character_id: String,
    /// 状态 JSON 字符串（必须是合法 JSON 对象）。
    /// overwrite=false 时与现有 live.json 合并（字段覆盖）；overwrite=true 时全量替换。
    pub state_json: String,
    /// true = 覆盖全部状态；false（默认）= 合并到现有状态。
    #[serde(default)]
    pub overwrite: bool,
    /// AUDIT-12: 幂等键。同一 key 重复调用返回缓存结果，避免重复写盘 + 重复
    /// 追加 history.jsonl 快照。
    #[serde(default)]
    pub idempotency_key: Option<String>,
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

// ── P0 read-side parity tools (Agent symmetry with diagnose / resources) ──

/// List all character IDs under data/characters/.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ListCharactersRequest {}

/// List all user IDs under data/users/.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ListUsersRequest {}

/// Fetch a character's card JSON + basic presence metadata.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct GetCharacterRequest {
    pub character_id: String,
}

/// Fetch a character's current `state/live.json` value (or empty `{}`).
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct GetLiveStateRequest {
    pub character_id: String,
}

// ── M_UP: User Persona request types ─────────────────────────────────────

/// M_UP-1: 导入用户人设（元设定，base persona）。
///
/// 若已存在 `persona.lock` 则拒绝（base 已封存）。`persona_json` 是用户自由
/// 定义的 JSON 对象，必须含 `name` 字段；其余字段（description / traits /
/// 任何额外 key）由 Agent 自定义。
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ImportUserPersonaRequest {
    /// 用户 ID（对应 `users/{user_id}/` 目录）。
    pub user_id: String,
    /// 用户 persona JSON 字符串（必须是合法 JSON 对象且含 `name`）。
    pub persona_json: String,
    /// 导入后立即封存（写 persona.lock）。默认 false（允许后续更新 base）。
    #[serde(default)]
    pub lock: bool,
    /// AUDIT-12: 幂等键。
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// M_UP-2: 封存用户 persona（创建 `persona.lock`，禁止后续 base 修改）。
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct LockUserPersonaRequest {
    pub user_id: String,
}

/// M_UP-3: 读取用户 persona 全貌。
///
/// 返回 `{user_id, persona, locked, current_state, drift_keys}`；
/// `drift_keys` 是 current_state 与 persona 顶层键的差集，供 Agent 比对元设定
/// 与变量设定的偏移。
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct GetUserPersonaRequest {
    pub user_id: String,
}

/// M_UP-4: 更新用户 state（变量设定 / 漂移层）。
///
/// 语义与 `update_state`（角色）一致：默认 merge，`overwrite=true` 全量替换。
/// **不修改 persona.json**（即使 persona 未封存也仅由 import 路径写）。
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateUserStateRequest {
    pub user_id: String,
    /// 状态 JSON 字符串（必须是合法 JSON 对象）。
    pub state_json: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// M_UP-5: 读取用户 state 快照历史。
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct GetUserStateHistoryRequest {
    pub user_id: String,
    #[serde(default = "default_state_history_n")]
    pub n: usize,
}

// ── 工具实现（业务逻辑）─────────────────────────────────────────────────────
// #[tool_router] 宏生成的 fn tool_router() 是 mod.rs 私有，故 thin wrappers 留在 mod.rs。
// 此处只存放实际业务逻辑（供 mod.rs wrappers 调用，pub(super) 可见）。

/// AUDIT-13: Emit `notifications/resources/updated` to all subscribers of a URI.
///
/// Spawned as fire-and-forget so synchronous tool handlers don't block on
/// network I/O. Idempotent + no-op when no subscribers match (common path).
pub(super) fn emit_resource_updated(state_subs: &super::StateSubs, uri: String) {
    let peers: Vec<rmcp::service::Peer<rmcp::service::RoleServer>> = {
        let guard = state_subs.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .iter()
            .filter(|(u, _)| u == &uri)
            .map(|(_, p)| p.clone())
            .collect()
    };
    if peers.is_empty() {
        return;
    }
    let param = rmcp::model::ResourceUpdatedNotificationParam::new(uri.clone());
    for peer in peers {
        let param = param.clone();
        let uri_dbg = uri.clone();
        tokio::spawn(async move {
            if let Err(e) = peer.notify_resource_updated(param).await {
                tracing::debug!(uri = %uri_dbg, err = %e, "AUDIT-13: notify_resource_updated failed (client disconnected?)");
            }
        });
    }
}

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

        // AUDIT-12: idempotency short-circuit. If client supplies a key and
        // we've seen it within the TTL window, return the cached result and
        // skip the write entirely.
        if let Some(ref key) = req.idempotency_key {
            if let Some(cached) = self.idempotency.get("append_message", key) {
                return Ok(cached);
            }
        }

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

        // AUDIT-6 / AUDIT-7: emit soft hints to the agent. Server never
        // takes action (戒律 1 forbids automatic side-effects) — we just
        // surface the data so the client can decide to call seal_volume /
        // perform cross-volume maintenance.
        let mut hints: Vec<serde_json::Value> = Vec::new();

        // AUDIT-6: estimated total tokens across all chat messages. When it
        // exceeds the configured soft threshold (default 2500), suggest the
        // agent seal current.md into a new volume.
        let total_chat_tokens: usize = log
            .messages
            .iter()
            .map(|m| crate::volume_store::estimate_tokens(&m.content))
            .sum();
        let soft = crate::config::VolumeConfig::default().soft_threshold_tokens;
        if total_chat_tokens > soft {
            hints.push(serde_json::json!({
                "kind": "volume_seal_recommended",
                "current_tokens": total_chat_tokens,
                "soft_threshold": soft,
                "message": "Chat tokens exceed soft threshold; consider archiving current.md as a new volume."
            }));
        }

        // AUDIT-7: count archived volumes under the legacy session memory dir.
        // When 3+ volumes exist, suggest cross-volume entity maintenance.
        // Read-only — we do NOT create the directory if it's missing.
        let memory_dir = self
            .data_root
            .join("characters")
            .join(&req.character_id)
            .join("memory");
        let vol_count = crate::volume_store::list_volume_numbers(&memory_dir).len();
        if vol_count >= 3 {
            hints.push(serde_json::json!({
                "kind": "volume_maintenance_available",
                "volume_count": vol_count,
                "message": "Three or more volumes present; cross-volume entity maintenance may improve memory recall."
            }));
        }

        let response = serde_json::json!({
            "character_id": req.character_id,
            "role": req.role,
            "total_messages": log.messages.len(),
            "hints": hints,
        })
        .to_string();

        // AUDIT-12: store the successful response for future retries.
        if let Some(ref key) = req.idempotency_key {
            self.idempotency.store("append_message", key, response.clone());
        }

        Ok(response)
    }

    /// DS-8 实现：直接更新角色实时状态（state/live.json）。
    /// 供 MCP client 在 RP 对话后调用，持久化状态变更（HP/MP/位置等）。
    pub(super) fn update_state_impl(
        &self,
        Parameters(req): Parameters<UpdateStateRequest>,
    ) -> Result<String, ErrorData> {
        // AUDIT-12: short-circuit on idempotency key cache hit
        if let Some(ref key) = req.idempotency_key {
            if let Some(cached) = self.idempotency.get("update_state", key) {
                return Ok(cached);
            }
        }

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

        // AUDIT-13: emit notification to subscribers of state/live resource.
        emit_resource_updated(
            &self.state_subs,
            format!("airp://characters/{}/state/live", req.character_id),
        );

        let response = serde_json::json!({
            "character_id": req.character_id,
            "overwrite": req.overwrite,
            "fields_updated": delta.as_object().map(|m| m.len()).unwrap_or(0),
            "state": merged,
        })
        .to_string();

        // AUDIT-12: cache result for retry safety.
        if let Some(ref key) = req.idempotency_key {
            self.idempotency.store("update_state", key, response.clone());
        }

        Ok(response)
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

    // ── P0 read-side parity implementations ───────────────────────────────

    /// List all character IDs.
    pub(super) fn list_characters_impl(
        &self,
        _params: Parameters<ListCharactersRequest>,
    ) -> Result<String, ErrorData> {
        let list = crate::data_dir::list_characters(&self.data_root)
            .map_err(|e| ErrorData::internal_error(format!("list_characters 失败: {}", e), None))?;
        Ok(serde_json::json!({
            "count": list.len(),
            "characters": list,
        })
        .to_string())
    }

    /// List all user IDs.
    pub(super) fn list_users_impl(
        &self,
        _params: Parameters<ListUsersRequest>,
    ) -> Result<String, ErrorData> {
        let list = crate::data_dir::list_users(&self.data_root)
            .map_err(|e| ErrorData::internal_error(format!("list_users 失败: {}", e), None))?;
        Ok(serde_json::json!({
            "count": list.len(),
            "users": list,
        })
        .to_string())
    }

    /// Fetch a character's card JSON + metadata.
    pub(super) fn get_character_impl(
        &self,
        Parameters(req): Parameters<GetCharacterRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;
        let cdir = self.data_root.join("characters").join(&req.character_id);

        // Same lookup logic as the `airp://characters/{id}/card` resource:
        // prefer CF-1 folder form, fall back to legacy flat layout.
        let card_folder_raw = cdir.join("card").join("raw.json");
        let card_legacy = cdir.join("card.json");
        let (card_path, card_format) = if card_folder_raw.exists() {
            (Some(card_folder_raw), "v2_folder")
        } else if card_legacy.exists() {
            (Some(card_legacy), "v2_legacy")
        } else {
            (None, "missing")
        };

        let card_json: Option<serde_json::Value> = card_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok());

        Ok(serde_json::json!({
            "character_id": req.character_id,
            "card_present": card_path.is_some(),
            "card_format": card_format,
            "card": card_json,
        })
        .to_string())
    }

    /// Fetch a character's current state/live.json (or empty object).
    pub(super) fn get_live_state_impl(
        &self,
        Parameters(req): Parameters<GetLiveStateRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.character_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 character_id: {}", e), None))?;
        let live_path = crate::data_dir::char_state_dir(&self.data_root, &req.character_id)
            .join("live.json");
        let state: serde_json::Value = if live_path.exists() {
            std::fs::read_to_string(&live_path)
                .ok()
                .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        Ok(serde_json::json!({
            "character_id": req.character_id,
            "present": live_path.exists(),
            "state": state,
        })
        .to_string())
    }

    // ── M_UP: User Persona implementations ─────────────────────────────────

    /// M_UP-1 implementation: write user persona JSON (元设定).
    pub(super) fn import_user_persona_impl(
        &self,
        Parameters(req): Parameters<ImportUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        // AUDIT-12: idempotency
        if let Some(ref key) = req.idempotency_key {
            if let Some(cached) = self.idempotency.get("import_user_persona", key) {
                return Ok(cached);
            }
        }

        crate::data_dir::validate_id_segment(&req.user_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;

        // Reject if locked
        let lock_path = crate::data_dir::user_persona_lock_path(&self.data_root, &req.user_id);
        if lock_path.exists() {
            return Err(ErrorData::invalid_params(
                format!(
                    "user `{}` persona 已封存（persona.lock 存在），拒绝重写",
                    req.user_id
                ),
                None,
            ));
        }

        // Validate JSON + require `name`
        let parsed: serde_json::Value = serde_json::from_str(&req.persona_json).map_err(|e| {
            ErrorData::invalid_params(format!("persona_json 非合法 JSON: {}", e), None)
        })?;
        if !parsed.is_object() {
            return Err(ErrorData::invalid_params(
                "persona_json 必须是 JSON 对象（{...}）".to_string(),
                None,
            ));
        }
        let name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    "persona_json 必须含非空 `name` 字段".to_string(),
                    None,
                )
            })?
            .to_string();

        // Write
        let user_dir = crate::data_dir::user_dir(&self.data_root, &req.user_id);
        std::fs::create_dir_all(&user_dir).map_err(|e| {
            ErrorData::internal_error(format!("创建 users/{} 目录失败: {}", req.user_id, e), None)
        })?;
        let persona_path = crate::data_dir::user_persona_path(&self.data_root, &req.user_id);
        let pretty = serde_json::to_string_pretty(&parsed).unwrap_or(req.persona_json.clone());
        std::fs::write(&persona_path, pretty.as_bytes()).map_err(|e| {
            ErrorData::internal_error(format!("写 persona.json 失败: {}", e), None)
        })?;

        let locked = if req.lock {
            std::fs::write(&lock_path, b"locked\n").map_err(|e| {
                ErrorData::internal_error(format!("写 persona.lock 失败: {}", e), None)
            })?;
            true
        } else {
            false
        };

        let response = serde_json::json!({
            "user_id": req.user_id,
            "name": name,
            "locked": locked,
            "persona_path": persona_path.display().to_string(),
        })
        .to_string();

        if let Some(ref key) = req.idempotency_key {
            self.idempotency.store("import_user_persona", key, response.clone());
        }

        Ok(response)
    }

    /// M_UP-2 implementation: seal user persona (write `persona.lock`).
    pub(super) fn lock_user_persona_impl(
        &self,
        Parameters(req): Parameters<LockUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.user_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;

        let persona_path = crate::data_dir::user_persona_path(&self.data_root, &req.user_id);
        if !persona_path.exists() {
            return Err(ErrorData::invalid_params(
                format!("user `{}` persona.json 不存在；先调 import_user_persona", req.user_id),
                None,
            ));
        }

        let lock_path = crate::data_dir::user_persona_lock_path(&self.data_root, &req.user_id);
        let was_locked = lock_path.exists();
        if !was_locked {
            std::fs::write(&lock_path, b"locked\n").map_err(|e| {
                ErrorData::internal_error(format!("写 persona.lock 失败: {}", e), None)
            })?;
        }

        Ok(serde_json::json!({
            "user_id": req.user_id,
            "locked": true,
            "was_already_locked": was_locked,
        })
        .to_string())
    }

    /// M_UP-3 implementation: read base + current_state + drift_keys.
    pub(super) fn get_user_persona_impl(
        &self,
        Parameters(req): Parameters<GetUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.user_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;

        let persona_path = crate::data_dir::user_persona_path(&self.data_root, &req.user_id);
        let persona: Option<serde_json::Value> = if persona_path.exists() {
            std::fs::read_to_string(&persona_path)
                .ok()
                .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok())
        } else {
            None
        };
        let locked = crate::data_dir::user_persona_lock_path(&self.data_root, &req.user_id).exists();

        let live_path = crate::data_dir::user_state_live_path(&self.data_root, &req.user_id);
        let current_state: Option<serde_json::Value> = if live_path.exists() {
            std::fs::read_to_string(&live_path)
                .ok()
                .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok())
        } else {
            None
        };

        // drift_keys = current_state keys NOT present in persona (top-level only).
        // Server does NOT decide semantic conflicts — Agent reasons over the full
        // base + state below to decide whether e.g. "skills:basketball" in state
        // contradicts "skills:none" in persona.
        let drift_keys: Vec<String> = match (&persona, &current_state) {
            (Some(p), Some(s)) => match (p.as_object(), s.as_object()) {
                (Some(pm), Some(sm)) => sm
                    .keys()
                    .filter(|k| !pm.contains_key(k.as_str()))
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            },
            (None, Some(s)) => s
                .as_object()
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        Ok(serde_json::json!({
            "user_id": req.user_id,
            "persona": persona,
            "locked": locked,
            "current_state": current_state,
            "drift_keys": drift_keys,
        })
        .to_string())
    }

    /// M_UP-4 implementation: merge / overwrite live state + append snapshot.
    pub(super) fn update_user_state_impl(
        &self,
        Parameters(req): Parameters<UpdateUserStateRequest>,
    ) -> Result<String, ErrorData> {
        if let Some(ref key) = req.idempotency_key {
            if let Some(cached) = self.idempotency.get("update_user_state", key) {
                return Ok(cached);
            }
        }

        crate::data_dir::validate_id_segment(&req.user_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;

        let delta: serde_json::Value = serde_json::from_str(&req.state_json).map_err(|e| {
            ErrorData::invalid_params(format!("state_json 非合法 JSON: {}", e), None)
        })?;
        if !delta.is_object() {
            return Err(ErrorData::invalid_params(
                "state_json 必须是 JSON 对象（{...}）".to_string(),
                None,
            ));
        }

        let state_dir = crate::data_dir::user_state_dir(&self.data_root, &req.user_id)
            .map_err(|e| ErrorData::internal_error(format!("创建 state/ 目录失败: {}", e), None))?;
        let live_path = state_dir.join("live.json");

        let merged: serde_json::Value = if req.overwrite {
            delta.clone()
        } else {
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

        let pretty = serde_json::to_string_pretty(&merged)
            .map_err(|e| ErrorData::internal_error(format!("序列化失败: {}", e), None))?;
        std::fs::write(&live_path, pretty.as_bytes())
            .map_err(|e| ErrorData::internal_error(format!("写 live.json 失败: {}", e), None))?;

        // Append snapshot
        let history_path =
            crate::data_dir::user_state_history_path(&self.data_root, &req.user_id);
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

        // AUDIT-13 parity: emit notification to subscribers of user state/live resource.
        emit_resource_updated(
            &self.state_subs,
            format!("airp://users/{}/state/live", req.user_id),
        );

        let response = serde_json::json!({
            "user_id": req.user_id,
            "overwrite": req.overwrite,
            "fields_updated": delta.as_object().map(|m| m.len()).unwrap_or(0),
            "state": merged,
        })
        .to_string();

        if let Some(ref key) = req.idempotency_key {
            self.idempotency.store("update_user_state", key, response.clone());
        }

        Ok(response)
    }

    /// M_UP-5 implementation: recent state history snapshots (newest-first).
    pub(super) fn get_user_state_history_impl(
        &self,
        Parameters(req): Parameters<GetUserStateHistoryRequest>,
    ) -> Result<String, ErrorData> {
        crate::data_dir::validate_id_segment(&req.user_id)
            .map_err(|e| ErrorData::invalid_params(format!("非法 user_id: {}", e), None))?;

        let history_path =
            crate::data_dir::user_state_history_path(&self.data_root, &req.user_id);
        let n = req.n.clamp(1, 1000);
        let entries: Vec<serde_json::Value> = if history_path.exists() {
            std::fs::read_to_string(&history_path)
                .ok()
                .map(|s| {
                    s.lines()
                        .filter(|l| !l.trim().is_empty())
                        .filter_map(|l| serde_json::from_str(l).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        // Newest-first
        let mut rev: Vec<_> = entries.into_iter().rev().take(n).collect();
        // Reverse back so user can read top-down newest -> older? Actually convention
        // for state_history was newest-first; align with character get_state_history.
        let count = rev.len();
        let _ = rev.iter_mut(); // no-op to keep mut binding warning-free
        Ok(serde_json::json!({
            "user_id": req.user_id,
            "entries": rev,
            "count": count,
        })
        .to_string())
    }
}
