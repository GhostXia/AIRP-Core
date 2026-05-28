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

mod idempotency;
mod prompts;
mod resources;
mod tools;
pub mod transport_http;

pub use idempotency::IdempotencyCache;

use prompts::{
    analyze_character_card_prompt, analyze_preset_prompt, filter_text_prompt, state_update_prompt,
};
pub use tools::{
    // P1: Scene CRUD
    AddSceneCharacterRequest,
    AppendMessageRequest,
    ApplyLorebookRequest,
    CreateSceneRequest,
    // P1: delete character
    DeleteCharacterRequest,
    // P0 read-side parity
    GetCharacterRequest,
    GetLiveStateRequest,
    GetRecentContextRequest,
    GetSceneRequest,
    GetStateHistoryRequest,
    // M_UP: User Persona
    GetUserPersonaRequest,
    GetUserStateHistoryRequest,
    ImportCardRequest,
    ImportPresetRequest,
    ImportUserPersonaRequest,
    ListCharactersRequest,
    ListPresetRegexScriptsRequest,
    ListScenesRequest,
    ListSessionsRequest,
    ListUsersRequest,
    // P1: Volume management
    ListVolumesRequest,
    LockUserPersonaRequest,
    PingRequest,
    ReadVolumeRequest,
    RemovePresetRegexScriptRequest,
    RollbackMessagesRequest,
    SealVolumeRequest,
    SetPresetRegexEnabledRequest,
    StartSessionRequest,
    UpdateStateRequest,
    UpdateUserStateRequest,
    WriteCharacterArtifactRequest,
    WritePresetArtifactRequest,
};
pub use transport_http::mcp_http_router;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceTemplate,
        ServerCapabilities, ServerInfo, SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::{Peer, RequestContext},
    tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler,
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
    /// AUDIT-12: idempotency cache shared across all tool handlers.
    pub idempotency: Arc<IdempotencyCache>,
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
            idempotency: IdempotencyCache::shared(),
        }
    }

    /// AUDIT-3: enumerate all registered MCP tools with annotations.
    ///
    /// Returns the same `Tool` list that `tools/list` JSON-RPC method would
    /// emit, including name / description / input schema / annotations.
    /// Used by the `list-tools` CLI subcommand so docs (README, REFACTOR_PLAN)
    /// can be cross-checked against the code as a single source of truth.
    pub fn enumerate_tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// CLI dispatcher: invoke a single MCP tool synchronously by name and
    /// return its JSON response as a `String`. Enables shell-driven agent
    /// automation without spawning the long-running stdio MCP server.
    ///
    /// `args_json` is the raw JSON object passed as the tool's `Parameters`.
    /// Unknown tool names return an error.
    pub fn call_tool_sync(&self, name: &str, args_json: &str) -> Result<String, String> {
        use rmcp::handler::server::wrapper::Parameters;
        use tools::*;

        // Helper: parse args_json into the request struct and short-circuit on
        // serde errors with a uniform message.
        macro_rules! parse_args {
            ($t:ty) => {{
                serde_json::from_str::<$t>(args_json)
                    .map_err(|e| format!("parse args for `{}` failed: {}", name, e))?
            }};
        }
        macro_rules! to_err {
            ($r:expr) => {
                $r.map_err(|e: rmcp::ErrorData| e.message.to_string())
            };
        }

        match name {
            "ping" => {
                let req = parse_args!(PingRequest);
                Ok(self.ping_impl(Parameters(req)))
            }
            "import_card" => {
                to_err!(self.import_card_impl(Parameters(parse_args!(ImportCardRequest))))
            }
            "import_preset" => {
                to_err!(self.import_preset_impl(Parameters(parse_args!(ImportPresetRequest))))
            }
            "apply_lorebook" => {
                to_err!(self.apply_lorebook_impl(Parameters(parse_args!(ApplyLorebookRequest))))
            }
            "start_session" => {
                to_err!(self.start_session_impl(Parameters(parse_args!(StartSessionRequest))))
            }
            "get_recent_context" => to_err!(
                self.get_recent_context_impl(Parameters(parse_args!(GetRecentContextRequest)))
            ),
            "append_message" => {
                to_err!(self.append_message_impl(Parameters(parse_args!(AppendMessageRequest))))
            }
            "update_state" => {
                to_err!(self.update_state_impl(Parameters(parse_args!(UpdateStateRequest))))
            }
            "rollback_messages" => to_err!(
                self.rollback_messages_impl(Parameters(parse_args!(RollbackMessagesRequest)))
            ),
            "list_sessions" => {
                to_err!(self.list_sessions_impl(Parameters(parse_args!(ListSessionsRequest))))
            }
            "get_state_history" => {
                to_err!(self.get_state_history_impl(Parameters(parse_args!(GetStateHistoryRequest))))
            }
            "list_preset_regex_scripts" => to_err!(self.list_preset_regex_scripts_impl(
                Parameters(parse_args!(ListPresetRegexScriptsRequest))
            )),
            "remove_preset_regex_script" => to_err!(self.remove_preset_regex_script_impl(
                Parameters(parse_args!(RemovePresetRegexScriptRequest))
            )),
            "set_preset_regex_enabled" => to_err!(self.set_preset_regex_enabled_impl(Parameters(
                parse_args!(SetPresetRegexEnabledRequest)
            ))),
            "write_preset_artifact" => to_err!(self
                .write_preset_artifact_impl(Parameters(parse_args!(WritePresetArtifactRequest)))),
            "write_character_artifact" => to_err!(self.write_character_artifact_impl(Parameters(
                parse_args!(WriteCharacterArtifactRequest)
            ))),
            // M_UP: user persona
            "import_user_persona" => {
                to_err!(self
                    .import_user_persona_impl(Parameters(parse_args!(ImportUserPersonaRequest))))
            }
            "lock_user_persona" => {
                to_err!(self.lock_user_persona_impl(Parameters(parse_args!(LockUserPersonaRequest))))
            }
            "get_user_persona" => {
                to_err!(self.get_user_persona_impl(Parameters(parse_args!(GetUserPersonaRequest))))
            }
            "update_user_state" => {
                to_err!(self.update_user_state_impl(Parameters(parse_args!(UpdateUserStateRequest))))
            }
            "get_user_state_history" => to_err!(self
                .get_user_state_history_impl(Parameters(parse_args!(GetUserStateHistoryRequest)))),
            // P0 read-side parity
            "list_characters" => {
                to_err!(self.list_characters_impl(Parameters(parse_args!(ListCharactersRequest))))
            }
            "list_users" => {
                to_err!(self.list_users_impl(Parameters(parse_args!(ListUsersRequest))))
            }
            "get_character" => {
                to_err!(self.get_character_impl(Parameters(parse_args!(GetCharacterRequest))))
            }
            "get_live_state" => {
                to_err!(self.get_live_state_impl(Parameters(parse_args!(GetLiveStateRequest))))
            }
            "delete_character" => {
                to_err!(self.delete_character_impl(Parameters(parse_args!(DeleteCharacterRequest))))
            }
            // P1: scene CRUD
            "list_scenes" => {
                to_err!(self.list_scenes_impl(Parameters(parse_args!(ListScenesRequest))))
            }
            "get_scene" => to_err!(self.get_scene_impl(Parameters(parse_args!(GetSceneRequest)))),
            "create_scene" => {
                to_err!(self.create_scene_impl(Parameters(parse_args!(CreateSceneRequest))))
            }
            "add_scene_character" => {
                to_err!(self
                    .add_scene_character_impl(Parameters(parse_args!(AddSceneCharacterRequest))))
            }
            // P1: volume management
            "list_volumes" => {
                to_err!(self.list_volumes_impl(Parameters(parse_args!(ListVolumesRequest))))
            }
            "read_volume" => {
                to_err!(self.read_volume_impl(Parameters(parse_args!(ReadVolumeRequest))))
            }
            "seal_volume" => {
                to_err!(self.seal_volume_impl(Parameters(parse_args!(SealVolumeRequest))))
            }
            other => Err(format!(
                "unknown tool: `{}` (use `airp-core list-tools --format summary` to enumerate)",
                other
            )),
        }
    }
}

// ── MCP-1/2: Tool wrappers（宏生成的 tool_router() 须在此模块，故 thin wrapper 留在此）──

#[tool_router]
impl AirpMcpServer {
    /// MCP-1：健康检查。返回版本号 + 数据根目录路径。
    #[tool(
        description = "AIRP MCP 健康检查。返回版本号与数据根目录。",
        annotations(
            title = "ping",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn ping(&self, params: Parameters<PingRequest>) -> String {
        self.ping_impl(params)
    }

    /// MCP-2.1：导入角色卡（JSON 字符串或 PNG base64）。
    #[tool(
        description = "导入 SillyTavern V2 角色卡。传 card_json (JSON 字符串) 或 card_png_base64 (PNG base64)。\
                       自动解包 greetings + world/lorebook.json。返回 {character_id, card_format, lorebook_entries, greetings_count}。",
        annotations(
            title = "import_card",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn import_card(&self, params: Parameters<ImportCardRequest>) -> Result<String, ErrorData> {
        self.import_card_impl(params)
    }

    /// MCP-2.2：lorebook 关键词扫描，返回触发条目。
    #[tool(
        description = "扫描文本中的 lorebook 关键词，返回触发的条目内容（用于注入 LLM context）。\
                       依赖角色已导入并具备 world/lorebook.json。",
        annotations(
            title = "apply_lorebook",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "start_session",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn start_session(&self, params: Parameters<StartSessionRequest>) -> Result<String, ErrorData> {
        self.start_session_impl(params)
    }

    /// DS-6：读取角色 ChatLog 最近 N 条消息，用于构建 LLM context。
    #[tool(
        description = "读取角色历史对话最近 N 条消息（默认 10）。\
                       返回 {character_id, total_messages, returned, messages} JSON。\
                       messages 数组元素含 {role, content}（role: user/assistant/system）。",
        annotations(
            title = "get_recent_context",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "append_message",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        annotations(
            title = "update_state",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn update_state(&self, params: Parameters<UpdateStateRequest>) -> Result<String, ErrorData> {
        self.update_state_impl(params)
    }

    /// DS-5：导入预设 JSON，写入 presets/{preset_id}/preset.json。
    /// 写入后即可通过 `airp://presets/{id}/raw` 读取，供 Agent 分析。
    #[tool(
        description = "导入 SillyTavern 预设 JSON。写入 presets/{preset_id}/preset.json。\
                       成功后可通过 airp://presets/{id}/raw 读取原文，write_preset_artifact 写入产物。\
                       返回 {preset_id, path, bytes_written} JSON。",
        annotations(
            title = "import_preset",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "write_preset_artifact",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "list_preset_regex_scripts",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "remove_preset_regex_script",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        annotations(
            title = "set_preset_regex_enabled",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "write_character_artifact",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
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
        annotations(
            title = "rollback_messages",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
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
        annotations(
            title = "list_sessions",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_sessions(&self, params: Parameters<ListSessionsRequest>) -> Result<String, ErrorData> {
        self.list_sessions_impl(params)
    }

    /// DS-11：读取角色实时状态历史快照。
    #[tool(
        description = "读取角色 state/history.jsonl 中最近 N 条状态快照（默认 10，newest-first）。\
                       每条快照含 ts（ISO-8601 时间戳）+ 状态字段（hp/mp/location 等）。\
                       文件不存在时返回空数组。返回 {character_id, entries: [...], count} JSON。",
        annotations(
            title = "get_state_history",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_state_history(
        &self,
        params: Parameters<GetStateHistoryRequest>,
    ) -> Result<String, ErrorData> {
        self.get_state_history_impl(params)
    }

    /// M_UP-1：导入用户人设（元设定）。已封存的 persona 拒绝重写。
    #[tool(
        description = "导入用户 persona（元设定 / immutable base）。persona_json 必须为合法 JSON 对象且含 `name`。\
                       若已存在 persona.lock 则拒绝。lock=true 时导入后立即封存。\
                       返回 {user_id, name, locked, persona_path} JSON。",
        annotations(
            title = "import_user_persona",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn import_user_persona(
        &self,
        params: Parameters<ImportUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        self.import_user_persona_impl(params)
    }

    /// M_UP-2：封存用户 persona（写 persona.lock，禁止后续 base 修改）。
    #[tool(
        description = "封存 users/{user_id}/persona.json（创建 persona.lock）。\
                       封存后 import_user_persona 将拒绝写入；update_user_state 不受影响。\
                       返回 {user_id, locked, was_already_locked} JSON。",
        annotations(
            title = "lock_user_persona",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn lock_user_persona(
        &self,
        params: Parameters<LockUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        self.lock_user_persona_impl(params)
    }

    /// M_UP-3：读取用户 persona + 当前 state + drift_keys。
    #[tool(
        description = "返回 {user_id, persona, locked, current_state, drift_keys}。\
                       drift_keys 为 current_state 顶层键中不在 persona 顶层的部分，供 Agent 比对\
                       元设定（persona）与变量设定（current_state）的偏移。\
                       Server 不做语义冲突判定 — Agent 读全量数据后自行推断。",
        annotations(
            title = "get_user_persona",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_user_persona(
        &self,
        params: Parameters<GetUserPersonaRequest>,
    ) -> Result<String, ErrorData> {
        self.get_user_persona_impl(params)
    }

    /// M_UP-4：更新用户 state（变量设定）。merge 或 overwrite。
    #[tool(
        description = "更新用户 state（变量设定 / drift overlay）。\
                       overwrite=false（默认）合并到现有 state；overwrite=true 全量替换。\
                       同步追加快照到 state/history.jsonl。\
                       **不修改 persona.json**。\
                       返回 {user_id, overwrite, fields_updated, state} JSON。",
        annotations(
            title = "update_user_state",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn update_user_state(
        &self,
        params: Parameters<UpdateUserStateRequest>,
    ) -> Result<String, ErrorData> {
        self.update_user_state_impl(params)
    }

    /// M_UP-5：读取用户 state 历史快照。
    #[tool(
        description = "读取 users/{user_id}/state/history.jsonl 中最近 N 条快照（默认 10，newest-first）。\
                       每条快照含 ts（ISO-8601 时间戳）+ state 字段。\
                       文件不存在时返回空数组。",
        annotations(
            title = "get_user_state_history",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_user_state_history(
        &self,
        params: Parameters<GetUserStateHistoryRequest>,
    ) -> Result<String, ErrorData> {
        self.get_user_state_history_impl(params)
    }

    /// P0：列出所有角色 ID。
    #[tool(
        description = "列出 data/characters/ 下的所有角色 ID。返回 {count, characters: [...]} JSON。",
        annotations(
            title = "list_characters",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_characters(
        &self,
        params: Parameters<ListCharactersRequest>,
    ) -> Result<String, ErrorData> {
        self.list_characters_impl(params)
    }

    /// P0：列出所有用户 ID。
    #[tool(
        description = "列出 data/users/ 下的所有用户 ID。返回 {count, users: [...]} JSON。",
        annotations(
            title = "list_users",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_users(&self, params: Parameters<ListUsersRequest>) -> Result<String, ErrorData> {
        self.list_users_impl(params)
    }

    /// P0：获取角色卡 + 元数据。
    #[tool(
        description = "返回 {character_id, card_present, card_format, card}。\
                       card_format 取值：v2_folder / v2_legacy / missing。\
                       与 airp://characters/{id}/card 资源等价，但便于 Agent 用 tool call 拿。",
        annotations(
            title = "get_character",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_character(&self, params: Parameters<GetCharacterRequest>) -> Result<String, ErrorData> {
        self.get_character_impl(params)
    }

    /// P0：读取角色当前 state/live.json。
    #[tool(
        description = "返回 {character_id, present, state}。文件不存在时 state={}，present=false。\
                       与 airp://characters/{id}/state/live 资源等价。",
        annotations(
            title = "get_live_state",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_live_state(&self, params: Parameters<GetLiveStateRequest>) -> Result<String, ErrorData> {
        self.get_live_state_impl(params)
    }

    /// P1：删除整个角色目录（递归）。
    #[tool(
        description = "删除 data/characters/{character_id}/ 整目录（递归）。\
                       默认 `confirm=false` 仅 dry-run，返回 would_remove_top_entries 列表。\
                       传 `confirm=true` 才真正删除。**不可撤销**。",
        annotations(
            title = "delete_character",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn delete_character(
        &self,
        params: Parameters<DeleteCharacterRequest>,
    ) -> Result<String, ErrorData> {
        self.delete_character_impl(params)
    }

    /// P1：列出所有场景。
    #[tool(
        description = "列出 data/scenes/ 下的所有场景 ID。返回 {count, scenes: [...]} JSON。",
        annotations(
            title = "list_scenes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_scenes(&self, params: Parameters<ListScenesRequest>) -> Result<String, ErrorData> {
        self.list_scenes_impl(params)
    }

    /// P1：读取场景完整配置。
    #[tool(
        description = "返回 scenes/{scene_id}/scene.json 的完整 SceneConfig（含 characters / narrator_style / lorebook_merge 等）。",
        annotations(
            title = "get_scene",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_scene(&self, params: Parameters<GetSceneRequest>) -> Result<String, ErrorData> {
        self.get_scene_impl(params)
    }

    /// P1：创建或覆盖场景。
    #[tool(
        description = "从 scene_json（完整 SceneConfig JSON 字符串）创建或覆盖 scenes/{scene_id}/scene.json。\
                       scene_id 由 JSON 内部 scene_id 字段决定。返回 {scene_id, characters_count, created} JSON。",
        annotations(
            title = "create_scene",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn create_scene(&self, params: Parameters<CreateSceneRequest>) -> Result<String, ErrorData> {
        self.create_scene_impl(params)
    }

    /// P1：向场景追加角色。
    #[tool(
        description = "向 scenes/{scene_id}/scene.json 的 characters 数组追加一条 {character_id, role, intro}。\
                       role 取 `primary` 或 `npc`（默认 npc）。intro 为可选自由文本。\
                       返回 {scene_id, character_id, characters_count} JSON。",
        annotations(
            title = "add_scene_character",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn add_scene_character(
        &self,
        params: Parameters<AddSceneCharacterRequest>,
    ) -> Result<String, ErrorData> {
        self.add_scene_character_impl(params)
    }

    /// P1：列出角色已封存卷。
    #[tool(
        description = "列出 characters/{character_id}/memory/volumes/ 下已存在的卷编号（升序）。\
                       返回 {character_id, count, volumes: [n1, n2, ...]} JSON。",
        annotations(
            title = "list_volumes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_volumes(&self, params: Parameters<ListVolumesRequest>) -> Result<String, ErrorData> {
        self.list_volumes_impl(params)
    }

    /// P1：读取指定编号卷内容。
    #[tool(
        description = "读取 characters/{character_id}/memory/volumes/vol_{number:03}.md 全文。\
                       返回 {character_id, number, content} JSON。",
        annotations(
            title = "read_volume",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn read_volume(&self, params: Parameters<ReadVolumeRequest>) -> Result<String, ErrorData> {
        self.read_volume_impl(params)
    }

    /// P1：封存 current.md 为下一个卷（纯文件操作，不调 LLM）。
    #[tool(
        description = "将 current.md 内容（或调用方传入的 `content`）写入 vol_{next:03}.md，\
                       清空 current.md。**不调用 LLM** — 若需 summarization，Agent 自行计算后\
                       通过 `content` 参数传入，否则按原文 archive。\
                       返回 {character_id, sealed_number, bytes, used_override_content} JSON。",
        annotations(
            title = "seal_volume",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn seal_volume(&self, params: Parameters<SealVolumeRequest>) -> Result<String, ErrorData> {
        self.seal_volume_impl(params)
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
            mk(
                "airp://characters",
                "AIRP Characters",
                "已导入角色卡列表（JSON 数组）",
            ),
            mk(
                "airp://presets",
                "AIRP Presets",
                "已导入预设列表（JSON 数组）",
            ),
            mk(
                "airp://users",
                "AIRP Users",
                "已导入用户人设列表（M_UP, JSON 数组）",
            ),
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
            // M_UP: user persona resources
            mk(
                "airp://users/{user_id}/persona",
                "AIRP User Persona (Base)",
                "用户元设定（immutable base）。文件不存在返回 null。订阅可监听 import_user_persona 写入。",
                "application/json",
            ),
            mk(
                "airp://users/{user_id}/state/live",
                "AIRP User State (Drift)",
                "用户变量设定（drift overlay）。文件不存在返回 {}。订阅可监听 update_user_state 变更。",
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
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    text,
                )])
                .with_description("Text filter Agent prompt"))
            }
            "state_update_instruction" => {
                let text = state_update_prompt();
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    text,
                )])
                .with_description("State update instruction"))
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
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    text,
                )])
                .with_description(format!("Analyze character card: {}", cid)))
            }
            "analyze_preset" => {
                let args = request.arguments.unwrap_or_default();
                let pid = args
                    .get("preset_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ErrorData::invalid_params("缺 preset_id".to_string(), None))?;
                let text = analyze_preset_prompt(pid);
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    text,
                )])
                .with_description(format!("Analyze preset: {}", pid)))
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
