# AIRP-Core

> **Full version** of the AIRP RP data substrate.
> Includes MCP server + SSE/HTTP compatibility layer + Web UI + SillyTavern/OpenAI SDK adapters.
>
> Looking for the lean MCP-only version? → [GhostXia/AIRP-MCP-Server](https://github.com/GhostXia/AIRP-MCP-Server)
>
> Both versions share the same data contract (character cards, presets, lorebooks, JSONL chat format)
> and the same `data/` directory layout — you can switch between them with zero migration.

---

**AIRP-Core 是 MCP-native RP 数据底座。** 提供角色卡 / 世界书 / 预设 / 会话 / 状态 / 卷封存的持久化原语，通过 Model Context Protocol（stdio + HTTP）和 OpenAI 兼容 SSE 网关两条入口暴露给 AI Agent。

**不调用 LLM、不跑后台循环、不强加 RP 业务逻辑。** 所有推理、叙事推进、状态决策由 client / Agent 完成。AIRP 只管数据形态正确 + 不变式守护 + 原语组合性强。

---

## 设计理念

- **License**：MIT OR Apache-2.0，商用 / fork / 集成无限制
- **协议**：MCP 标准（Anthropic 发布；Claude Code / Cursor / Continue / Pi 原生支持）
- **服务端零 LLM 调度**：所有推理由 client / Agent 完成，AIRP 只持久化数据
- **部署**：单 Rust 二进制
- **数据格式**：SillyTavern V2 角色卡 / lorebook / preset 直读

---

## 四条架构戒律

支配所有功能取舍的硬性原则：

1. **拒绝任何 server 侧 loop / 决策 / 自动副作用** — 不跑 agent loop，不调 LLM，不解析模型输出语义
2. **欢迎把 RP 数据形态固化成原语** — 角色卡 / 世界书 / 卷 / gating / 多角色场景都是合法扩展
3. **疑虑替 Agent 决定怎么玩的工具** — 不内置 dice / combat / economy 模块；Agent 用通用 `update_state` 自己写
4. **开放接入** — 任何插件、任何语言、无需特殊适配即可接入；零特化、零注册、零 schema 强制

---

## 当前状态

| 项 | 值 |
|---|---|
| 测试 | **447** passing（lib 437 + integration 10），1 ignored |
| Clippy `--lib --bins -- -D warnings` | **0** warning |
| MCP 工具 | 39（含 5 user persona + 4 P0 读 + delete_character + 4 scene CRUD + 3 volume ops + 6 plugin data，全部带 ToolAnnotations） |
| MCP 资源 | 4 静态 + 12 模板 |
| MCP Prompts | 5 |
| 已完成里程碑 | M0–M3 / M_CF / M_PR (PR-1~10) / M_MS / M_MCP / M_DX / M_LS / M_CA / **M_HARDEN (13/13)** / **M_PLUGIN_DATA** |
| 进行中 | — |

---

## 快速开始

### Windows 构建环境

```powershell
$env:RUSTUP_HOME = "D:\.rustup"
$env:PATH = "D:\msys64\mingw64\bin;" + $env:PATH
```

目标三元组 `x86_64-pc-windows-gnu`（见 `.cargo/config.toml`）。Linux CI 通过 `CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu` 覆盖。

### 构建

```powershell
cargo build --release
```

### 三种使用模式

**1. MCP stdio（推荐：Claude Code / Cursor / Pi）**

```powershell
cargo run -- mcp --data-dir ./data
```

`mcp_config.example.json` 是 Claude Code MCP 配置模板。

**2. MCP HTTP + SSE 网关（同进程多 client）**

```powershell
cargo run -- daemon --port 8000
```

- MCP Streamable HTTP：`POST/GET /mcp/v1`
- OpenAI 兼容 SSE：`POST /v1/chat/completions`
- Web UI：`http://127.0.0.1:8000`

**3. 单次 CLI 渲染（脚本管线）**

```powershell
cargo run -- run --message "你好" --filters "<thought>[\s\S]*?<\/thought>"
```

**4. 一次性 MCP 工具调用（agent shell 自动化 / CI 自检）**

不起 server，单次调用单个 MCP 工具并返回 JSON 到 stdout。

```bash
airp-core tool ping
airp-core tool list_sessions --json '{"character_id":"alice"}'
airp-core tool append_message --json '{"character_id":"alice","role":"user","content":"hi"}'
airp-core tool get_recent_context --json '{"character_id":"alice","n":10}'
```

退出码 0 = 成功（结果在 stdout）；退出码 1 = 错误（消息在 stderr）。

**5. 一键全景诊断（用户报 bug 时跑这条）**

```bash
airp-core diagnose                        # 全量 JSON
airp-core diagnose --format summary       # 人可读简表
airp-core diagnose --character-id alice   # 聚焦单个角色
```

输出含：data root 健康、settings 字段（敏感字段仅 `*_set` 布尔，**永不明文**）、所有角色 / 预设 / 场景概要（卡片状态、lorebook 条目数、chat 行数、卷数、当前 CP 等）。用户复制粘贴 → 维护者立即定位问题。

便捷脚本：`run_daemon.bat`（增量编译 + 启动）、`run_tests.bat`。

---

## MCP 工具表

全部 39 个工具带 MCP 标准 `ToolAnnotations` 元数据，harness 可据此自动判断 "静默调 vs 需用户确认"。

> 工具数以 `airp-core list-tools` / `AirpMcpServer::tool_count()` 为单一真相源。下表为常用子集示例，完整列表跑 `airp-core list-tools --format summary`。

| 工具 | 用途 | side_effect |
|---|---|---|
| `ping` | 健康检查 | readonly |
| `import_card` | 导入 V2 角色卡（JSON 或 base64 PNG） | mutate / idempotent |
| `import_preset` | 导入 SillyTavern 预设 | mutate / idempotent |
| `apply_lorebook` | 扫描文本，返回触发条目 | readonly |
| `start_session` | 启动 RP 会话 → system_prompt + greetings | mutate |
| `get_recent_context` | 读 ChatLog 最近 N 条 | readonly |
| `append_message` | 追加消息到 chat.jsonl（O(1) append） | append |
| `update_state` | 写 `state/live.json` + 追加 `history.jsonl` 快照 | mutate |
| `rollback_messages` | 回滚最后 N 条消息 | destructive |
| `list_sessions` | 列具名 sessions | readonly |
| `get_state_history` | 读状态快照（newest-first） | readonly |
| `write_preset_artifact` | Agent 写预设分析产物 | mutate / idempotent |
| `write_character_artifact` | Agent 写角色卡分析产物 | mutate / idempotent |
| `list_preset_regex_scripts` | 列预设正则脚本 | readonly |
| `remove_preset_regex_script` | 删一条正则脚本 | destructive |
| `set_preset_regex_enabled` | 启/禁用正则脚本 | mutate / idempotent |
| `import_user_persona` | 导入用户人设元设定（可封存） | mutate / idempotent |
| `lock_user_persona` | 封存用户 persona（写 persona.lock） | mutate / idempotent |
| `get_user_persona` | 读 base + state + drift_keys | readonly |
| `update_user_state` | 更新用户变量设定（drift overlay） | mutate |
| `get_user_state_history` | 用户状态历史快照 | readonly |
| `list_characters` | 列出全部角色 ID | readonly |
| `list_users` | 列出全部用户 ID | readonly |
| `get_character` | 取角色卡 + 元数据 | readonly |
| `get_live_state` | 读角色当前 state/live.json | readonly |
| `delete_character` | 删除整个角色目录（默认 dry-run） | destructive |
| `list_scenes` | 列出全部场景 ID | readonly |
| `get_scene` | 读场景完整配置 | readonly |
| `create_scene` | 从 JSON 创建/覆盖场景 | mutate / idempotent |
| `add_scene_character` | 向场景追加角色 | mutate |
| `list_volumes` | 列出角色已封存卷 | readonly |
| `read_volume` | 读取指定编号卷内容 | readonly |
| `seal_volume` | 封存 current.md 为下一卷（纯文件操作，不调 LLM） | mutate / idempotent |
| `plugin_kv_get` | 读插件 KV（plugins/{name}/{key}.json） | readonly |
| `plugin_kv_set` | 写插件 KV（任意 JSON 值，零 schema） | mutate / idempotent |
| `plugin_jsonl_append` | 插件 JSONL 追加（O(1) append） | append |
| `plugin_jsonl_read` | 插件 JSONL 分页读取 | readonly |
| `plugin_blob_write` | 插件任意文件写入（base64 / UTF-8 文本） | mutate / idempotent |
| `plugin_blob_read` | 插件任意文件读取（上限 4 MiB） | readonly |

**M_PLUGIN_DATA 零 schema 插件数据（戒律 4）：**
- 任何语言的 MCP client 取一个 `plugin_name` 命名空间即可存取自己的数据 — 无 manifest、无注册、无 schema 强制
- 数据落地 `data/plugins/{plugin_name}/`，完全任意文件树，AIRP 不解析语义
- 三个写工具均推送 `airp://plugins/{name}/data/{path}` 资源变更通知 — 可把 AIRP 当零代码事件总线

**User persona 双层模型（M_UP）：**
- **元设定 / Base**（`users/{id}/persona.json`）：初始人设，可通过 `persona.lock` 封存为只读契约
- **变量设定 / Drift**（`users/{id}/state/live.json`）：剧情推进中累积的变化（学会新技能、心情变化等）
- Server **不判定语义冲突**（戒律 1）— `get_user_persona` 返回完整 base + drift + drift_keys，Agent 自行推断「不会打篮球（base）vs 学会了打篮球（drift）」这类冲突

详细 schema 见 `docs/mcp.md`。

---

## HTTP API（兼容层）

| 方法 | 路径 | 用途 | 限流 |
|---|---|---|---|
| GET / POST | `/mcp/v1` | MCP Streamable HTTP 入口 | — |
| GET | `/` | Web UI（embedded `index.html`） | — |
| POST | `/v1/chat/completions` | OpenAI 兼容 SSE 流 | 10 req/s + burst 20/IP |
| POST | `/v1/chat/history` | 拉 ChatLog | — |
| POST | `/v1/chat/rollback` | 回滚到指定 index | — |
| POST | `/v1/chat/regen` | 删最后一条 | — |
| GET | `/v1/characters` | 列角色 | — |
| POST | `/v1/characters/import` | 导入角色卡 | — |
| GET / POST | `/v1/sessions/:character_id` | 多 session 管理 | — |
| GET / POST | `/v1/settings` | 配置热重载 | — |

**ID 类型契约**：`character_id` / `preset_id` 反序列化即 `validate_id_segment`（拒路径分隔符、`..`、空字节、`.` 开头）；`session_id` 必须合法 UUID v4。

可选 API key 鉴权：env `AIRP_ACCESS_KEY` 设置后所有 `/v1/*` 路径要求 `Authorization: Bearer <key>`（`/mcp/v1` 不受影响）。

---

## 架构

### MCP 主入口（推荐路径）

```
Claude Code / Cursor / 任何 MCP client
  → airp-core mcp (stdio) 或 POST /mcp/v1 (HTTP)
  → src/mcp/mod.rs: AirpMcpServer (#[tool_router])
      ├─ tools.rs：33 个工具实现
      ├─ resources.rs：静态 + 模板资源（airp://characters/...）
      └─ prompts.rs：Agent 工作流提示词
```

**典型 RP 工作流**：
```
import_card → start_session → 循环:
    get_recent_context → [client 调 LLM] →
    append_message(user) → append_message(assistant) →
    update_state（可选）
```

### SSE 兼容层（旧酒馆生态）

```
POST /v1/chat/completions
  → daemon::chat_completion_handler
  → chat_pipeline::prepare_pipeline
      ├─ 校验 ID newtype
      ├─ 加载角色卡 + Orchestrator 装配 system prompt
      │    (card → preset → checkpoint gating → known context → 卷 → lorebook)
      └─ 持久化 user 消息（JSONL append）
  → adapter::call_streaming_api（OpenAI 兼容上游）
  → 流处理：
      ├─ fsm.rs：char 级正则过滤（Aho-Corasick 加速）
      └─ xml_unpacker.rs：<think>/<action> 拆包
  → 派生 finalizer（JoinSet）：
      ├─ 持久化 assistant 消息
      ├─ 卷封存（soft / hard token 阈值 → vol_XXX.md）
      └─ 跨卷维护（≥3 卷晋升入 index）
```

### 关键模块

| 模块 | 职责 |
|---|---|
| `mcp/mod.rs` | MCP server：`AirpMcpServer`、`#[tool_router]`、资源 / 提示词 handler |
| `mcp/tools.rs` | 33 个 `_impl` 实现 |
| `mcp/resources.rs` | 静态 + 模板资源 |
| `mcp/prompts.rs` | Agent 工作流提示词 |
| `mcp/transport_http.rs` | Streamable HTTP MCP transport（`/mcp/v1`） |
| `daemon/mod.rs` | axum router + HTTP handlers + `RwLock<MutableConfig>` |
| `chat_pipeline.rs` | 三阶段流：prepare → stream → finalize |
| `orchestrator/` | 提示词装配（card / lorebook / preset / gating / volume_inject） |
| `chat_store.rs` | ChatLog JSONL 持久化（O(1) append） |
| `adapter.rs` | `Provider` enum、`ProviderConfig`、`GenerationParams` |
| `fsm.rs` | char 级流过滤 FSM（`pub(crate)`） |
| `xml_unpacker.rs` | `<think>` / `<action>` 拆包（`pub(crate)`） |
| `volume_store.rs` | current.md / vol_XXX.md / index.md I/O（`pub(crate)`） |
| `volume_manager.rs` | 封卷流程 + `run_maintenance`（`pub(crate)`） |
| `config.rs` | 三层合并：default → settings.json → env → request |
| `types.rs` | newtype ID（serde 反序列化时校验） |
| `data_dir/` | 路径解析 + 安全原语（`resolve_session_dir`、`validate_id_segment`） |
| `scene.rs` | 多角色场景（M_MS） |

### 工程不变式

- **`pub(crate)` 内部模块** — `fsm` / `xml_unpacker` / `volume_store` / `volume_manager` / `index_parser` / `auto_converter` 不对外暴露
- **热路径无 `Arc<Mutex>`** — `MutableConfig` 用 `std::sync::RwLock`
- **JSONL chat logs** — `OpenOptions::append` 唯一写路径，O(1)
- **newtype ID** — 反序列化时校验，下游免重复 `validate_id_segment`
- **`estimate_tokens` ±30% 近似** — 非真实 tiktoken；卷阈值容忍此精度

### Rust 原生加速点

| 路径 | 技术 | 效果 |
|---|---|---|
| 关键词扫描（`lorebook` + `volume_inject`） | `aho-corasick` 单次 DFA | **11.37× 实测加速**（500 entries × 3 keys × 4 KiB） |
| 流式 FSM（`fsm.rs`） | char-level 状态机 + `special_first_chars` HashSet 快进 + `mem::take/replace` 零 clone | 消除 N 次 `String::from(c)` 分配 |
| XML 拆包（`xml_unpacker.rs`） | 本地 buf 批量 + `mem::take` flush | 消除每字符 `Vec::push` |
| HTTP client | `reqwest::Client` 共享于 `DaemonState`（`Arc<ConnectionPool>`） | 跨请求复用，免 TLS 握手 |
| 流任务管理 | `tokio::task::JoinSet` | finalize await 全部子任务，无遗弃 JoinHandle |
| ChatLog | `chat_log.jsonl` 行式 + `OpenOptions::append` | O(1) 追加，仅滚动/回滚整体重写 |

---

## 数据目录

```
data/
├── settings.json
├── characters/{character_id}/
│   ├── card/                     (CF-1 文件夹形态：card.json + card.png)
│   ├── greetings/                (greetings 文件夹)
│   ├── world/lorebook.json       (CF-8 自动发现)
│   ├── analysis/                 (analyze_character_card 产物)
│   ├── state/
│   │   ├── live.json             (当前实时状态)
│   │   ├── history.jsonl         (状态快照时序，newest-last append)
│   │   └── schema.json           (M_LS-7 可选 schema)
│   ├── gating/checkpoints.json
│   ├── memory/                   (legacy session 卷系统：current.md / index.md / volumes/vol_*.md)
│   └── sessions/{session_id}/    (M5.1 显式 SessionId)
│       ├── meta.json
│       ├── chat.jsonl
│       └── memory/               (同 legacy memory/ 结构)
├── presets/{preset_id}/
│   ├── preset.json               (M_PR 目录化)
│   ├── preset.md
│   ├── regex/*.json              (PR-4 SillyTavern 正则脚本)
│   └── analysis/                 (analyze_preset 产物)
├── scenes/{scene_id}/            (M_MS 多角色场景)
│   ├── scene.json
│   ├── memory/                   (场景级独立卷系统)
│   └── world/lorebook.json       (场景级世界书)
└── plugins/{plugin_name}/        (M_PLUGIN_DATA 零 schema 插件数据)
    └── {arbitrary_file_tree}     (完全任意结构，AIRP 不解析)
```

---

## 测试基础设施

- **单元测试** — 435 用例覆盖配置三层合并 / 卷系统隔离 / FSM 状态转换 / Orchestrator 装配 / ChatLog 持久化 / 各 MCP 工具 / 场景多角色装配
- **集成测试** — `tests/sse_wiremock.rs` + `tests/openai_compat.rs` 用 `wiremock` mock 上游 SSE，5 端到端场景
- **Property test** — `fsm.rs` proptest 验证 chunk 边界独立性 / 任意 UTF-8 不 panic / 变量替换 chunk 独立 / `<卷评估/>` 自闭合标签 chunk 独立
- **CI** — `.github/workflows/ci.yml` Ubuntu 跑 test + clippy + fmt（全部必过）+ `cargo-llvm-cov` 覆盖率

---

## 配置三层合并

优先级：`default → data/settings.json → AIRP_* env → request body`

| 字段 | env 变量 |
|---|---|
| `provider` | `AIRP_PROVIDER` |
| `endpoint` | `AIRP_ENDPOINT` |
| `api_key` | `AIRP_API_KEY` |
| `model` | `AIRP_MODEL` |
| `daemon_port` | `AIRP_DAEMON_PORT` |
| `access_api_key`（鉴权） | `AIRP_ACCESS_KEY` |

合并完成后 `AppConfig::validate()` fast-fail（如 `VolumeConfig.soft >= hard` 拦截）。

---

## 部署

```powershell
# Docker
docker-compose up --build -d
```

`Dockerfile` 多阶段构建，`docker-compose.yml` 单服务。详见 `docs/deploy.md`。

---

## 路线图与决策

**已完成里程碑：**
- M0–M3：Rust 质量审计 + 安全 + 错误统一 + 流管线 + 三层配置
- M_CF：角色卡文件夹分层
- M_PR：预设目录化 + SillyTavern 正则脚本（PR-1~10）
- M_MS：多角色场景
- M_MCP：MCP 协议全量集成（33 工具 + 资源 + 提示词 + stdio + HTTP）
- M_DX：API key 鉴权 + Docker 部署
- M_LS：实时状态系统 + schema 推断
- M_CA：Agent-driven 分析提示词
- **M_HARDEN：13/13 子任务全部完成**（鉴权扩展到 /mcp/v1、SceneId newtype 全量 retrofit、tool side_effect 元数据、resource subscribe emit、idempotency keys、stdio 优雅停机、/version 端点、rmcp pin、卷封存/跨卷维护软提示、list-tools CLI、safe_resolve property test、RwLock 决策验证）
- **M_PLUGIN_DATA：零 schema 三原语**（plugin_kv_get/set + plugin_jsonl_append/read + plugin_blob_write/read，6 工具 + 3 资源 URI + 订阅推送；戒律 4 开放接入落地）

**预留里程碑：**
- M_HELPERS（airp-mcp-helpers Rust crate，生态杠杆）
- M_ARTIFACTS_UNIFIED（通用 artifact 工具组）
- M_MODES（三档 prompt mode：compat / enhanced / bare）
- M_REGEN / M_MEMORY_ENTRIES / M_AUDIT_LOG / M_WORLD_EVENTS

---

## 已知限制

- **`estimate_tokens` ±30% 偏差** — 启发式而非真实 tokenizer。卷阈值容忍
- **Windows-GNU 本地覆盖率不可跑** — `profiler_builtins` runtime 缺失；`cargo llvm-cov` 仅 Linux CI 跑得通
- **错误响应中文为主** — 跨语言 API client 解析不便（未来 M_I18N 规划）
- **Web UI 未对接全部新 API** — 部分 session / 多角色场景管理仍待 UI 集成
- **角色卡仅支持 PNG / JSON** — PNG 覆盖 `tEXt` / `zTXt` / `iTXt`（含 zlib 压缩），`ccv3`(V3) 优先回退 `chara`(V2)，v1 平铺卡自动归一化为 v2。WEBP / JPEG 非 SillyTavern 标准导出格式，暂不支持（未来扩展：EXIF/XMP 字节扫描解卡）

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
