# AGENTS.md

This file provides guidance to AI coding agents working in this repository.

## Environment Setup

This project requires a non-standard toolchain location due to disk constraints:

```powershell
$env:RUSTUP_HOME = "D:\.rustup"
$env:PATH = "D:\msys64\mingw64\bin;" + $env:PATH
```

Target triple is pinned to `x86_64-pc-windows-gnu` in `.cargo/config.toml`. CI can override via `CARGO_BUILD_TARGET`.

## Commands

```powershell
# Build
cargo build --release

# Run all tests
cargo test

# Lint (0 warnings enforced on lib+bins)
cargo clippy --lib --bins -- -D warnings

# Start daemon SSE gateway on port 8000
cargo run -- daemon --port 8000

# Single-shot CLI run (stream to stdout)
cargo run -- run --message "hello" --filters "<thought>[\s\S]*?<\/thought>"
```

Convenience scripts: `run_daemon.bat` (incremental build + launch), `run_tests.bat` (unit tests).

## Project Identity

**AIRP-Core 是纯 Agent 端 — 自调 LLM 的流式 RP 后端。** 乐高式独立块，不耦合生态其他仓库：

| 想要 | 用 |
|---|---|
| 纯 MCP 数据工具面（不调 LLM） | [AIRP-MCP-Server](https://github.com/GhostXia/AIRP-MCP-Server) |
| 协议桥 / AgentBus（HTTP/SSE ↔ MCP） | [AIRP-Gateway](https://github.com/GhostXia/AIRP-Gateway) |
| UI + State Protocol 契约 | [AIRP-State-Protocol](https://github.com/GhostXia/AIRP-State-Protocol) |
| **自调 LLM 的流式 RP 后端（本仓）** | **AIRP-Core** |

Core 自身调用 LLM（`adapter.rs`），装配上下文（`orchestrator/`），流式过滤（`fsm` + `xml_unpacker`），落库封卷（`chat_store` + `volume_*`）。**Core 已演进为独立 Agent 后端**（M_AGENT 系列）：在有界戒律内跑 server-side agent loop（`src/agent/`），把单回合流式管线当 loop 的一个动作复用，不重写。底座纯度移交 AIRP-MCP-Server 守护。

## Agent Loop（M_AGENT-1 已落地）

- 入口：`POST /v1/agent/run`（SSE）。`/v1/chat/completions` ≡ `max_steps=1` 退化情形（向后兼容）。
- 协调器：`src/agent/mod.rs` `AgentLoop::run`。每步在「派生纯净 subagent / 调工具 / 收敛」间选择；四道闸（step cap + token 预算 + 墙钟 + CancellationToken）。
- 工具：`src/agent/tools.rs` `Tool` trait + `ToolRegistry`。M_AGENT-1 仅 mock `echo`；M_AGENT-2 注入 built-in（chat_store/volume_*/scene/...）。
- 复用纪律：派生 subagent = `chat_pipeline::prepare_pipeline` + `run_generation_step`（新增，跑生成返回累积不 finalize）。一行 SSE/provider/拆包都不重写。
- 戒律#6 两平面隔离：角色平面（subagent 上下文）只由 RP 数据装配；控制平面（协调器多步状态）不注入 system prompt。`tests::subagent_context_has_no_orchestrator_noise` 守护。

进度：M_AGENT-0 ✅ · M_AGENT-1 ✅ · M_AGENT-2~7 待推（见 `AGENT_BACKEND_PLAN.md`）。

## Architecture

```
前端 (HTTP/SSE) → POST /v1/chat/completions
  → daemon::chat_completion_handler
  → chat_pipeline::prepare_pipeline (装配上下文 + 持久化 user 消息)
  → adapter::call_streaming_api_auto (OpenAI / Anthropic 双 provider)
  → 流处理: fsm.rs (正则过滤) + xml_unpacker.rs (immersive/<action>/<state> 拆包)
  → finalizer: 持久化 assistant 消息 + 落盘 state + 卷封存
```

多角色场景走 `prepare_scene_pipeline` 分支。

### Key Modules

| Module | Role |
|---|---|
| `agent/mod.rs` | M_AGENT-1: `AgentLoop::run` orchestrator (bounded loop, 4 gates, SSE events) |
| `agent/tools.rs` | `Tool` trait + `ToolRegistry` + mock `echo` (M_AGENT-2 will add built-ins) |
| `daemon/mod.rs` | axum router, HTTP handlers, `DaemonState` with `RwLock<MutableConfig>`, auth + rate-limit |
| `daemon/handlers.rs` | HTTP endpoint implementations (chat/characters/sessions/scenes/settings) |
| `chat_pipeline.rs` | Three-phase stream: prepare → stream → finalize |
| `orchestrator/` | Prompt assembly: card, lorebook, preset, gating, volume_inject, multi-char scene |
| `adapter.rs` | `BackendEngine` (Direct/AnthropicMessages/ClaudeCodeSdk stub), `ProviderConfig`, `GenerationParams`, dual-format SSE |
| `chat_store.rs` | ChatLog JSONL persistence, O(1) append |
| `fsm.rs` / `xml_unpacker.rs` | Char-level streaming FSM + tag extraction (`pub(crate)`) |
| `volume_store.rs` / `volume_manager.rs` | current.md / vol_XXX.md / index.md I/O + sealing workflow (`pub(crate)`) |
| `config.rs` | 3-layer merge: default → settings.json → env → request |
| `types.rs` | Newtype IDs: `CharacterId` / `PresetId` / `SessionId` / `SceneId` (serde-validated) |
| `data_dir/` | Path resolution + security primitives (`resolve_session_dir`, `validate_id_segment`) |
| `scene.rs` | Multi-character `SceneConfig` |
| `png_parser.rs` | SillyTavern V2 PNG card parsing |

### Key Design Invariants

- **乐高独立** — 不依赖生态其他仓库；数据层自带，`data/` 格式与 AIRP-MCP-Server 兼容可互换。
- **`pub(crate)` internals** — `fsm`, `xml_unpacker`, `volume_store`, `volume_manager`, `index_parser`, `auto_converter`, `preset_regex` are implementation details, not public API.
- **No Arc<Mutex> on hot path** — `MutableConfig` uses `std::sync::RwLock` (not tokio), settings hot-reload via `POST /v1/settings`.
- **JSONL chat logs** — one JSON line per message; `OpenOptions::append` is the only write path, O(1) append.
- **ID newtypes** — validation at serde deserialization; downstream code trusts IDs are valid.
- **`estimate_tokens`** — ±30% approximation, not real tiktoken. Volume thresholds tolerate this.
- **Bounded agent loop** — `src/agent/` runs a server-side loop under six Agent戒律 (step cap + token budget + wall clock + cancel + allowlist + context purity). Single-turn = `max_steps=1` degenerate case. See `AGENT_BACKEND_PLAN.md`.

### HTTP API

| Method | Path | Notes |
|---|---|---|
| POST | `/v1/chat/completions` | Rate-limited: 10 req/s, burst 20/IP (tower_governor) |
| POST | `/v1/agent/run` | M_AGENT-1: multi-step agent loop (SSE); `max_steps=1` ≡ chat/completions |
| POST | `/v1/chat/history` / `/v1/chat/rollback` / `/v1/chat/regen` | |
| GET | `/v1/characters` · POST `/v1/characters/import` | |
| GET/POST | `/v1/sessions/:character_id` | Multi-session |
| GET/POST | `/v1/scenes` · `/v1/scenes/:id` · `/v1/scenes/:id/characters` | Multi-character scenes |
| GET | `/v1/characters/:id/avatar` · `/state` · `/state/schema` · `/state/history` | |
| GET/POST | `/v1/settings` | Hot-reload MutableConfig |
| GET | `/v1/models` | Proxy upstream provider /models |
| GET | `/version` | Build metadata (name + version) |

`AIRP_ACCESS_KEY` env var enables bearer auth on all `/v1/*` paths.
