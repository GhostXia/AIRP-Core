//! M_MCP AIRP MCP Server。
//!
//! 暴露 AIRP 的角色卡 / 预设 / 世界书 / 卷 / 状态作为 MCP tools / resources /
//! prompts，供 Claude Code / Cursor / Pi 等 MCP-aware client 消费。
//!
//! 实现进度：
//!   - MCP-1 ✅：rmcp 1.7 集成 + `ping` 健康检查
//!   - MCP-2 ✅：3 个 P0 工具 `import_card` / `apply_lorebook` / `start_session`
//!   - MCP-3 ✅：4 资源（静态 characters 列表 + 3 个 templates）
//!   - MCP-4 ✅：3 个 prompt 模板
//!   - MCP-5 ✅：stdio transport
//!   - MCP-6 ✅：Streamable HTTP transport（`/mcp/v1`）
//!   - MCP-7 ✅：资源订阅（`airp://characters/{id}/state/live` subscribe/unsubscribe + push）
//!   - MCP-8 ✅：全量 tools/resources/prompts
//!   - MCP-9/10 🟨：Claude Code 联调 / 验收

mod prompts;
mod resources;
mod tools;
pub mod transport_http;

use prompts::{analyze_character_card_prompt, analyze_preset_prompt, filter_text_prompt, state_update_prompt};
pub use tools::{
    AppendMessageRequest, ApplyLorebookRequest, GetRecentContextRequest,
    GetStateHistoryRequest, ImportCardRequest, ImportPresetRequest,
    ListPresetRegexScriptsRequest, ListSessionsRequest, RemovePresetRegexScriptRequest,
    RollbackMessagesRequest, SetPresetRegexEnabledRequest, PingRequest,
    StartSessionRequest, UpdateStateRequest,
    WriteCharacterArtifactRequest, WritePresetArtifactRequest,
};
pub use transport_http::mcp_http_router;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceTemplate,
        ServerCapabilities, ServerInfo,
        SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::{Peer, RequestContext},
    tool, tool_handler, tool_router,
    ErrorData,
    RoleServer,
    ServerHandler,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// MCP-7: resource subscription registry — (uri, peer) pairs.
/// Shared between `AirpMcpServer` instances and `DaemonState` so the
/// chat-pipeline finalizer can push `notifications/resources/updated`.
pub type StateSubs = Arc<Mutex<Vec<(String, Peer<RoleServer>)>>>;

/// AIRP MCP Server 句柄。持有 data_root；所有 tool / resource 调用通过此句柄访问业务逻辑。
#[derive(Debug, Clone)]
pub struct AirpMcpServer {
    pub data_root: PathBuf,
    // 由 #[tool_handler] 宏在编译期读取，dead_code lint 看不到宏访问
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    /// MCP-7: 资源订阅注册表，与 DaemonState + FinalizerCtx 共享同一 Arc。
    pub state_subs: StateSubs,
}

impl AirpMcpServer {
    /// 构造独立实例（测试 / stdio 使用，订阅不跨进程共享）。
    pub fn new(data_root: PathBuf) -> Self {
        Self::new_with_subs(data_root, Arc::new(Mutex::new(Vec::new())))
    }

    /// 构造共享订阅实例（daemon HTTP transport 使用，state_subs 由 DaemonState 持有）。
    pub fn new_with_subs(data_root: PathBuf, state_subs: StateSubs) -> Self {
        Self {
            data_root,
            tool_router: Self::tool_router(),
            state_subs,
        }
    }
}

// ── MCP-1/2: Tool wrappers（宏生成的 tool_router() 须在此模块，故 thin wrapper 留在此）──

#[tool_router]
impl AirpMcpServer {
    /// MCP-1：健康检查。返回版本号 + 数据根目录路径。
    #[tool(
        description = "AIRP MCP 健康检查。返回版本号与数据根目录。",
        annotations(title = "ping", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn ping(&self, params: Parameters<PingRequest>) -> String {
        self.ping_impl(params)
    }

    /// MCP-2.1：导入角色卡（JSON 字符串或 PNG base64）。
    #[tool(
        description = "导入 SillyTavern V2 角色卡。传 card_json (JSON 字符串) 或 card_png_base64 (PNG base64)。\
                       自动解包 greetings + world/lorebook.json。返回 {character_id, card_format, lorebook_entries, greetings_count}。",
        annotations(title = "import_card", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn import_card(&self, params: Parameters<ImportCardRequest>) -> Result<String, ErrorData> {
        self.import_card_impl(params)
    }

    /// MCP-2.2：lorebook 关键词扫描，返回触发条目。
    #[tool(
        description = "扫描文本中的 lorebook 关键词，返回触发的条目内容（用于注入 LLM context）。\
                       依赖角色已导入并具备 world/lorebook.json。",
        annotations(title = "apply_lorebook", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn apply_lorebook(
        &self,
        params: Parameters<ApplyLorebookRequest>,
    ) -> Result<String, ErrorData> {
        self.apply_lorebook_impl(params)
    }

    /// MCP-2.3：启动 RP 会话，返回 system prompt + greetings。
    #[tool(
        description = "启动 RP 会话。返回 {system_prompt, greetings, session_dir} JSON。\
                       Client 拿到 system_prompt 后自行调 LLM，再用 append_message 记录对话。",
        annotations(title = "start_session", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    fn start_session(&self, params: Parameters<StartSessionRequest>) -> Result<String, ErrorData> {
        self.start_session_impl(params)
    }

    /// DS-6：读取角色 ChatLog 最近 N 条消息，用于构建 LLM context。
    #[tool(
        description = "读取角色历史对话最近 N 条消息（默认 10）。\
                       返回 {character_id, total_messages, returned, messages} JSON。\
                       messages 数组元素含 {role, content}（role: user/assistant/system）。",
        annotations(title = "get_recent_context", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn get_recent_context(
        &self,
        params: Parameters<GetRecentContextRequest>,
    ) -> Result<String, ErrorData> {
        self.get_recent_context_impl(params)
    }

    /// DS-7：向角色 ChatLog 追加一条消息，持久化 RP 对话历史。
    #[tool(
        description = "向角色 ChatLog 追加一条消息（role: user/assistant/system）。\
                       每次 LLM 回复后调此工具将消息写入 history/chat_log.jsonl（O(1) append）。\
                       返回 {character_id, role, total_messages} JSON。",
        annotations(title = "append_message", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    fn append_message(
        &self,
        params: Parameters<AppendMessageRequest>,
    ) -> Result<String, ErrorData> {
        self.append_message_impl(params)
    }

    /// DS-8：直接更新角色实时状态（state/live.json），供 MCP RP 工作流使用。
    #[tool(
        description = "更新角色实时状态（state/live.json）。\
                       state_json 须为合法 JSON 对象。\
                       overwrite=false（默认）合并到现有状态；overwrite=true 全量替换。\
                       同步追加快照到 state/history.jsonl。\
                       返回 {character_id, overwrite, fields_updated, state} JSON。",
        annotations(title = "update_state", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    fn update_state(
        &self,
        params: Parameters<UpdateStateRequest>,
    ) -> Result<String, ErrorData> {
        self.update_state_impl(params)
    }

    /// DS-5：导入预设 JSON，写入 presets/{preset_id}/preset.json。
    /// 写入后即可通过 `airp://presets/{id}/raw` 读取，供 Agent 分析。
    #[tool(
        description = "导入 SillyTavern 预设 JSON。写入 presets/{preset_id}/preset.json。\
                       成功后可通过 airp://presets/{id}/raw 读取原文，write_preset_artifact 写入产物。\
                       返回 {preset_id, path, bytes_written} JSON。",
        annotations(title = "import_preset", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn import_preset(&self, params: Parameters<ImportPresetRequest>) -> Result<String, ErrorData> {
        self.import_preset_impl(params)
    }

    /// DS-B：写入 Agent 生成的预设产物文件。
    /// Agent 通过 `airp://presets/{id}/raw` 读取预设，自主分析后调此工具写入产物。
    #[tool(
        description = "写入预设产物文件（Agent 分析预设后调用）。\
                       artifact_path 为相对路径（如 regex/display_layer.json），\
                       受限于 presets/{preset_id}/ 目录（路径穿越防护）。\
                       返回 {preset_id, artifact_path, bytes_written} JSON。",
        annotations(title = "write_preset_artifact", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn write_preset_artifact(
        &self,
        params: Parameters<WritePresetArtifactRequest>,
    ) -> Result<String, ErrorData> {
        self.write_preset_artifact_impl(params)
    }

    /// PR-5：列出预设关联的所有正则脚本（含 `_filename`、`scriptName`、`findRegex`、`disabled` 等字段）。
    #[tool(
        description = "列出预设 presets/{preset_id}/regex/ 下的所有正则脚本文件及其字段。\
                       返回 JSON 数组，每条含 _filename / scriptName / findRegex / disabled 等字段。\
                       目录不存在时返回空数组 []。",
        annotations(title = "list_preset_regex_scripts", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn list_preset_regex_scripts(
        &self,
        params: Parameters<ListPresetRegexScriptsRequest>,
    ) -> Result<String, ErrorData> {
        self.list_preset_regex_scripts_impl(params)
    }

    /// PR-6：删除预设正则脚本文件。
    #[tool(
        description = "删除 presets/{preset_id}/regex/{filename} 脚本文件。\
                       filename 仅限叶文件名（如 hide_thoughts.json），不允许路径分隔符或 `..`。\
                       文件不存在时返回错误；成功返回 {preset_id, filename, removed: true}。",
        annotations(title = "remove_preset_regex_script", read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    fn remove_preset_regex_script(
        &self,
        params: Parameters<RemovePresetRegexScriptRequest>,
    ) -> Result<String, ErrorData> {
        self.remove_preset_regex_script_impl(params)
    }

    /// PR-7：启用或禁用单条正则脚本（修改 `disabled` 字段并写回文件）。
    #[tool(
        description = "启用（enabled=true）或禁用（enabled=false）一条正则脚本。\
                       读取 presets/{preset_id}/regex/{filename}，修改 disabled 字段后写回。\
                       支持单对象及数组格式。返回 {preset_id, filename, enabled, disabled}。",
        annotations(title = "set_preset_regex_enabled", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn set_preset_regex_enabled(
        &self,
        params: Parameters<SetPresetRegexEnabledRequest>,
    ) -> Result<String, ErrorData> {
        self.set_preset_regex_enabled_impl(params)
    }

    /// DS-B：写入 Agent 生成的角色卡产物文件。
    /// Agent 通过 `airp://characters/{id}/card` 读取角色卡，自主分析后调此工具写入产物。
    /// 典型产物：`analysis/profile.md`（角色分析）、`style/guide.md`（文风提炼）、
    /// `cot/strategy.md`（CoT 策略）、`schema/output_schema.json`（输出结构）。
    #[tool(
        description = "写入角色卡产物文件（Agent 分析角色卡后调用）。\
                       artifact_path 为相对路径（如 analysis/profile.md），\
                       受限于 characters/{character_id}/ 目录（路径穿越防护）。\
                       返回 {character_id, artifact_path, bytes_written} JSON。",
        annotations(title = "write_character_artifact", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn write_character_artifact(
        &self,
        params: Parameters<WriteCharacterArtifactRequest>,
    ) -> Result<String, ErrorData> {
        self.write_character_artifact_impl(params)
    }

    /// DS-9：回滚角色 ChatLog 最后 N 条消息（默认 1）。
    #[tool(
        description = "回滚角色 ChatLog 最后 N 条消息（默认 n=1）。\
                       用于撤销错误追加、重新生成上一轮对话。\
                       n 自动 clamp 到 [1, 1000]，超出总消息数时清空全部。\
                       返回 {character_id, requested, removed, total_messages} JSON。",
        annotations(title = "rollback_messages", read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    fn rollback_messages(
        &self,
        params: Parameters<RollbackMessagesRequest>,
    ) -> Result<String, ErrorData> {
        self.rollback_messages_impl(params)
    }

    /// DS-10：列出角色的所有具名 session。
    #[tool(
        description = "列出角色在 characters/{id}/sessions/ 下的所有具名 session ID。\
                       不含 legacy 默认 session（memory/ 下的卷系统）。\
                       返回 {character_id, sessions: [...], count} JSON。\
                       可配合 start_session(session_id=...) 切换 session。",
        annotations(title = "list_sessions", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn list_sessions(
        &self,
        params: Parameters<ListSessionsRequest>,
    ) -> Result<String, ErrorData> {
        self.list_sessions_impl(params)
    }

    /// DS-11：读取角色实时状态历史快照。
    #[tool(
        description = "读取角色 state/history.jsonl 中最近 N 条状态快照（默认 10，newest-first）。\
                       每条快照含 ts（ISO-8601 时间戳）+ 状态字段（hp/mp/location 等）。\
                       文件不存在时返回空数组。返回 {character_id, entries: [...], count} JSON。",
        annotations(title = "get_state_history", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    fn get_state_history(
        &self,
        params: Parameters<GetStateHistoryRequest>,
    ) -> Result<String, ErrorData> {
        self.get_state_history_impl(params)
    }
}

// ── MCP-3/4: ServerHandler（资源 + Prompts）────────────────────────────────

#[tool_handler]
impl ServerHandler for AirpMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "AIRP — Stateful Streaming Gateway for RP Character Cards. \
             Tools: ping / import_card / import_preset / apply_lorebook / start_session \
             / get_recent_context / append_message / update_state / rollback_messages \
             / list_sessions / get_state_history \
             / write_preset_artifact / write_character_artifact \
             / list_preset_regex_scripts / remove_preset_regex_script / set_preset_regex_enabled. \
             Resources: airp://characters (list), airp://presets (list), \
             templates: characters/{id}/card + lorebook + artifacts + history + state/live \
             + presets/{id}/raw + presets/{id}/artifacts + presets/{id}/regex. \
             Prompts: build_system_prompt / filter_text / state_update_instruction / analyze_character_card / analyze_preset. \
             RP workflow: import_card -> start_session -> [get_recent_context -> LLM -> append_message(user) -> LLM -> append_message(assistant) -> update_state] loop. \
             Preset workflow: import_preset -> read airp://presets/{id}/raw -> analyze -> write_preset_artifact -> check airp://presets/{id}/artifacts. \
             Card analysis: import_card -> read airp://characters/{id}/card -> analyze -> write_character_artifact -> check airp://characters/{id}/artifacts.",
        )
    }

    // ── MCP-3: Resources ─────────────────────────────────────────────────────

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let mk = |uri: &str, name: &str, desc: &str| -> Resource {
            let mut raw = RawResource::new(uri, name);
            raw.description = Some(desc.to_string());
            raw.mime_type = Some("application/json".to_string());
            raw.no_annotation()
        };
        let resources = vec![
            mk("airp://characters", "AIRP Characters", "已导入角色卡列表（JSON 数组）"),
            mk("airp://presets", "AIRP Presets", "已导入预设列表（JSON 数组）"),
        ];
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let mk = |uri: &str, name: &str, desc: &str, mime: &str| -> ResourceTemplate {
            let mut raw = RawResourceTemplate::new(uri, name);
            raw.description = Some(desc.to_string());
            raw.mime_type = Some(mime.to_string());
            raw.no_annotation()
        };
        let templates = vec![
            mk(
                "airp://characters/{character_id}/card",
                "AIRP Character Card",
                "角色卡完整 JSON（TavernV2 格式）",
                "application/json",
            ),
            mk(
                "airp://characters/{character_id}/world/lorebook",
                "AIRP Lorebook",
                "角色关联世界书 JSON（CF-7 导入时自动提取）",
                "application/json",
            ),
            mk(
                "airp://characters/{character_id}/history",
                "AIRP Chat History",
                "角色默认 session 聊天记录（最近 1000 条消息）。返回 ChatLog JSON（含 messages 数组）。文件不存在时返回空数组。",
                "application/json",
            ),
            mk(
                "airp://characters/{character_id}/artifacts",
                "AIRP Character Artifacts",
                "Agent 已写入的角色卡产物文件列表（排除系统目录 card/world/history/memory/gating/sessions）。返回相对路径 JSON 数组。",
                "application/json",
            ),
            mk(
                "airp://characters/{character_id}/state/live",
                "AIRP Live State",
                "实时状态快照（JSON 对象）。LLM 输出 <state>{...}</state> 时 finalizer 自动写 state/live.json。文件不存在返空对象 {}。",
                "application/json",
            ),
            mk(
                "airp://presets/{preset_id}/raw",
                "AIRP Preset Raw",
                "预设 JSON 原文（preset.json）。Agent 读取后自主分析，调 write_preset_artifact 写入产物。",
                "application/json",
            ),
            mk(
                "airp://presets/{preset_id}/artifacts",
                "AIRP Preset Artifacts",
                "Agent 已写入的预设产物文件列表（排除 preset.json 本身）。返回相对路径 JSON 数组。",
                "application/json",
            ),
            mk(
                "airp://presets/{preset_id}/regex",
                "AIRP Preset Regex Scripts",
                "预设 regex/ 目录下所有脚本的富元数据列表（含 _filename / scriptName / findRegex / disabled 等字段）。目录不存在返回 []。",
                "application/json",
            ),
        ];
        Ok(ListResourceTemplatesResult {
            resource_templates: templates,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri;
        let contents = self.dispatch_resource(&uri)?;
        Ok(ReadResourceResult::new(contents))
    }

    // ── MCP-7: Resource subscription ──────────────────────────────────────────

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let uri = &request.uri;
        // Only support state/live subscriptions for now
        if uri.starts_with("airp://characters/") && uri.ends_with("/state/live") {
            let mut subs = self.state_subs.lock().unwrap_or_else(|e| e.into_inner());
            subs.push((uri.clone(), context.peer.clone()));
            tracing::debug!(uri = %uri, "MCP-7: client subscribed to resource");
        }
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        let uri = &request.uri;
        let mut subs = self.state_subs.lock().unwrap_or_else(|e| e.into_inner());
        let before = subs.len();
        subs.retain(|(u, _)| u != uri);
        let after = subs.len();
        tracing::debug!(uri = %uri, removed = before - after, "MCP-7: client unsubscribed from resource");
        Ok(())
    }

    // ── MCP-4: Prompts ────────────────────────────────────────────────────────

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let prompts = vec![
            Prompt::new(
                "build_system_prompt",
                Some("装配 RP 会话的 system prompt（含 card / preset / lorebook / gating / volume）"),
                Some(vec![
                    PromptArgument::new("character_id")
                        .with_description(
                            "已导入角色 ID（characters/{id}/card/raw.json 必须存在）",
                        )
                        .with_required(true),
                    PromptArgument::new("preset_id")
                        .with_description(
                            "可选预设 ID。指定时合并 preset prompts 进 system prompt。",
                        )
                        .with_required(false),
                    PromptArgument::new("user_name")
                        .with_description("用户显示名，宏 {{user}} 替换用")
                        .with_required(false),
                ]),
            ),
            Prompt::new(
                "filter_text",
                Some(
                    "文本筛选 Agent prompt。剥除 <think>/<thought>/<status>/[卷评估]/[OOC] 等元数据。",
                ),
                None::<Vec<PromptArgument>>,
            ),
            Prompt::new(
                "state_update_instruction",
                Some(
                    "指示 AI 在 <state>{...JSON...}</state> 标签内输出本轮状态更新（M_LS 集成）",
                ),
                None::<Vec<PromptArgument>>,
            ),
            // M_CA: Agent-driven character / preset analysis prompts
            Prompt::new(
                "analyze_character_card",
                Some("分析角色卡 Agent workflow prompt。读 card + lorebook → 输出 profile/tier/style/cot 产物。"),
                Some(vec![
                    PromptArgument::new("character_id")
                        .with_description("已导入角色 ID")
                        .with_required(true),
                ]),
            ),
            Prompt::new(
                "analyze_preset",
                Some("分析预设 Agent workflow prompt。读 preset raw JSON → 输出 summary/regex/style 产物。"),
                Some(vec![
                    PromptArgument::new("preset_id")
                        .with_description("已导入预设 ID")
                        .with_required(true),
                ]),
            ),
        ];
        Ok(ListPromptsResult {
            prompts,
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        match request.name.as_str() {
            "build_system_prompt" => {
                let args = request.arguments.unwrap_or_default();
                let cid = args
                    .get("character_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ErrorData::invalid_params("缺 character_id".to_string(), None)
                    })?;
                let preset_id = args.get("preset_id").and_then(|v| v.as_str());
                let user_name = args
                    .get("user_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("User");
                let sp = self.assemble_system_prompt(cid, preset_id, user_name)?;
                let msg = PromptMessage::new_text(PromptMessageRole::User, sp);
                Ok(GetPromptResult::new(vec![msg])
                    .with_description(format!("System prompt for character {}", cid)))
            }
            "filter_text" => {
                let text = filter_text_prompt();
                Ok(
                    GetPromptResult::new(vec![PromptMessage::new_text(
                        PromptMessageRole::User,
                        text,
                    )])
                    .with_description("Text filter Agent prompt"),
                )
            }
            "state_update_instruction" => {
                let text = state_update_prompt();
                Ok(
                    GetPromptResult::new(vec![PromptMessage::new_text(
                        PromptMessageRole::User,
                        text,
                    )])
                    .with_description("State update instruction"),
                )
            }
            "analyze_character_card" => {
                let args = request.arguments.unwrap_or_default();
                let cid = args
                    .get("character_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ErrorData::invalid_params("缺 character_id".to_string(), None)
                    })?;
                let text = analyze_character_card_prompt(cid);
                Ok(
                    GetPromptResult::new(vec![PromptMessage::new_text(
                        PromptMessageRole::User,
                        text,
                    )])
                    .with_description(format!("Analyze character card: {}", cid)),
                )
            }
            "analyze_preset" => {
                let args = request.arguments.unwrap_or_default();
                let pid = args
                    .get("preset_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ErrorData::invalid_params("缺 preset_id".to_string(), None)
                    })?;
                let text = analyze_preset_prompt(pid);
                Ok(
                    GetPromptResult::new(vec![PromptMessage::new_text(
                        PromptMessageRole::User,
                        text,
                    )])
                    .with_description(format!("Analyze preset: {}", pid)),
                )
            }
            other => Err(ErrorData::invalid_params(
                format!("未知 prompt: {}", other),
                None,
            )),
        }
    }
}

/// M_LS LS-9: test-only shim.
#[cfg(test)]
pub fn state_update_prompt_for_test() -> String {
    state_update_prompt()
}

#[cfg(test)]
mod tests;
