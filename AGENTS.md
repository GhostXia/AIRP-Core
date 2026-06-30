# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

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

# Lint (0 warnings enforced on lib+bins; tests have 3 pre-existing warnings in config.rs)
cargo clippy --lib --bins -- -D warnings

# Start daemon on port 8000 (SSE/HTTP compatibility layer)
cargo run -- daemon --port 8000

# MCP stdio mode (primary interface for Codex)
cargo run -- mcp --data-dir ./data

# Single-shot CLI run
cargo run -- run --message "hello" --filters "<thought>[\s\S]*?<\/thought>"
```

Convenience scripts: `run_daemon.bat` (incremental build + launch), `run_tests.bat` (unit tests).

## Architecture

AIRP-Core is an **MCP-first** RP data management server. The primary interface is the MCP protocol (stdio transport for Codex, Streamable HTTP for remote agents). The original SSE/HTTP gateway (`daemon` mode) is retained as a compatibility layer.

### Primary: MCP Interface

```
Codex / MCP client
  → airp-core mcp (stdio) OR POST /mcp/v1 (HTTP)
  → src/mcp/mod.rs: AirpMcpServer (rmcp #[tool_router])
      ├─ tools.rs: 39 tool implementations (import_card, start_session, …)
      ├─ resources.rs: static + template resources (airp://characters/…)
      └─ prompts.rs: Agent workflow prompts (analyze_character_card, …)
```

**MCP RP Workflow (recommended):**
```
import_card(character_id, card_json)
  → start_session(character_id, preset_id, user_name)  → system_prompt + greetings
  → loop:
      get_recent_context(character_id, n=20)  → build message array for LLM
      [Codex calls LLM]                      → reply text
      append_message(character_id, "user", user_input)
      append_message(character_id, "assistant", llm_reply)
      update_state(character_id, state_json)    → persist HP/MP/location/…
      [on regen] rollback_messages(character_id, n=2)  → undo last user+assistant pair
```

### Compatibility: SSE/HTTP Gateway

```
POST /v1/chat/completions
  → daemon/mod.rs: chat_completion_handler
  → chat_pipeline.rs: prepare_pipeline
      ├─ Validate ID newtypes (CharacterId / PresetId / SessionId)
      ├─ Load character card (PNG via png_parser or inline JSON)
      ├─ Orchestrator builds system prompt:
      │    character personality → preset prompts → checkpoint gating
      │    → known context → volume context → lorebook matches
      ├─ R-04: Auto-inject ChatLog history if messages_history is None
      └─ Persist user message (chat_store.rs, O(1) JSONL append)
  → adapter.rs: call_streaming_api (OpenAI-compatible)
  → Stream processing:
      ├─ fsm.rs: char-level regex filter (stateful)
      └─ xml_unpacker.rs: extract <think>/<action> tags → SSE events
  → Spawned finalizer task:
      ├─ Persist assistant message (JSONL append)
      ├─ Volume sealing check (soft/hard token thresholds → vol_XXX.md)
      └─ Cross-volume maintenance (entity promotion if ≥3 volumes)
```

### Key Modules

| Module | Role |
|---|---|
| `mcp/mod.rs` | MCP server: `AirpMcpServer`, `#[tool_router]`, resource/prompt handlers |
| `mcp/tools.rs` | 39 tool `_impl` methods (character + preset + RP workflow + plugin data) |
| `mcp/resources.rs` | Static + template resources (`airp://characters/…`) |
| `mcp/prompts.rs` | Agent workflow prompts (`analyze_character_card`, etc.) |
| `mcp/transport_http.rs` | Streamable HTTP MCP transport (`POST/GET /mcp/v1`) |
| `daemon/mod.rs` | axum router, HTTP handlers, `DaemonState` with `RwLock<MutableConfig>` |
| `chat_pipeline.rs` | Three-phase stream: prepare → stream → finalize |
| `orchestrator/` | Prompt assembly: card, lorebook, preset, gating, volume injection |
| `chat_store.rs` | ChatLog JSONL persistence with O(1) append |
| `adapter.rs` | Provider enum, `ProviderConfig`, `GenerationParams`, `MessageRole` |
| `fsm.rs` | Char-level streaming FSM for regex filtering (`pub(crate)`) |
| `xml_unpacker.rs` | `<think>`/`<action>` tag extraction (`pub(crate)`) |
| `volume_store.rs` | current.md / vol_XXX.md / index.md I/O (`pub(crate)`) |
| `volume_manager.rs` | Volume sealing workflow, `run_maintenance` (`pub(crate)`) |
| `config.rs` | 3-layer merge: default → settings.json → env → request |
| `types.rs` | Newtype IDs: `CharacterId`, `PresetId`, `SessionId` with serde deserialization validation |
| `data_dir.rs` | Path resolution + security primitives (`resolve_session_dir`, `validate_id_segment`) |

### Key Design Invariants

- **MCP-first** — `airp-core mcp` is the recommended entrypoint; daemon HTTP SSE is compatibility layer.
- **`pub(crate)` internals** — `fsm`, `xml_unpacker`, `volume_store`, `volume_manager`, `index_parser`, `auto_converter` are implementation details, not public API.
- **No Arc<Mutex> on hot path** — `MutableConfig` uses `std::sync::RwLock` (not tokio), settings hot-reload via `POST /v1/settings`.
- **JSONL chat logs** — one JSON line per message; `OpenOptions::append` is the only write path, ensuring O(1) append.
- **ID newtypes** — validation happens at serde deserialization time; downstream code can trust IDs are valid.
- **`estimate_tokens`** — ±30% approximation, not real tiktoken. Volume thresholds tolerate this imprecision.

### MCP Tools (39 total)

> Count is single-sourced from `AirpMcpServer::tool_count()` / `airp-core list-tools`. Table below is a representative subset; run `airp-core list-tools --format summary` for the full list.

| Tool | Purpose |
|---|---|
| `ping` | Health check |
| `import_card` | Import SillyTavern V2 card (JSON or PNG) |
| `import_preset` | Import SillyTavern preset JSON |
| `apply_lorebook` | Scan text for lorebook keyword matches |
| `start_session` | Build system prompt + load greetings |
| `get_recent_context` | Load last N chat messages |
| `append_message` | O(1) JSONL append to chat log |
| `update_state` | Merge/overwrite `state/live.json` |
| `rollback_messages` | Delete last N messages from chat log |
| `list_sessions` | List named sessions for a character |
| `get_state_history` | Read recent state snapshots (newest-first) |
| `list_preset_regex_scripts` | List preset regex filter scripts |
| `remove_preset_regex_script` | Delete a regex script file |
| `set_preset_regex_enabled` | Toggle regex script enabled/disabled |
| `write_preset_artifact` | Write analysis artifact to preset dir |
| `write_character_artifact` | Write analysis artifact to character dir |
| `plugin_kv_get` / `plugin_kv_set` | Zero-schema plugin KV (M_PLUGIN_DATA, `data/plugins/{name}/`) |
| `plugin_jsonl_append` / `plugin_jsonl_read` | Plugin JSONL event log (O(1) append) |
| `plugin_blob_write` / `plugin_blob_read` | Plugin arbitrary file I/O (base64 or UTF-8 text) |

### HTTP API (compatibility layer)

| Method | Path | Notes |
|---|---|---|
| POST | `/mcp/v1` | MCP Streamable HTTP (JSON-RPC) |
| GET | `/mcp/v1` | MCP SSE subscription |
| POST | `/v1/chat/completions` | Rate-limited: 10 req/s, burst 20/IP (tower_governor) |
| POST | `/v1/chat/history` / `/v1/chat/rollback` / `/v1/chat/regen` | |
| GET | `/v1/characters` | |
| POST | `/v1/characters/import` | R-01 |
| GET/POST | `/v1/sessions/:character_id` | Multi-session (M5.2) |
| GET/POST | `/v1/settings` | Hot-reload MutableConfig |
| GET | `/` | Web UI |

## Authority Document

`REFACTOR_PLAN.md` tracks the full M0–M6 + M_MCP refactor history and pending milestones. Check it before making structural changes to understand the design decisions and what work is verified vs. pending.

MCP integration reference: `docs/mcp.md`. Codex config template: `mcp_config.example.json`.
