# AIRP MCP Server Reference

AIRP exposes a full MCP (Model Context Protocol) server over two transports:

| Transport | Endpoint | Use case |
|-----------|----------|----------|
| **stdio** | `airp-core mcp` | Claude Desktop / Claude Code (recommended) |
| **HTTP Streamable** | `POST /mcp/v1` + `GET /mcp/v1` SSE | Remote agents, HTTP-based MCP clients |

## Quick Start (Claude Code)

1. Build release binary:
   ```powershell
   cargo build --release
   ```
2. Copy `mcp_config.example.json` to your Claude config directory and adjust paths.
3. Verify with `airp-core mcp` — first JSON-RPC message should respond with server info.

---

## Tools

### `ping`
Health check. Returns version string and data_root path.

**Input:** `{}` (no parameters)

**Output:** `"AIRP MCP Server v0.x.x (data_root=...)"`

---

### `import_card`
Import a SillyTavern V2 character card (JSON or PNG).

**Input:**
```json
{
  "character_id": "凌欺霜",
  "card_json": "{ ... TavernV2 JSON string ... }",
  "card_png_base64": null
}
```
Exactly one of `card_json` / `card_png_base64` must be provided.

**Output:**
```json
{
  "character_id": "凌欺霜",
  "card_format": "json",
  "greetings_count": 3,
  "lorebook_entries": 12
}
```

**Side effects:** Writes `characters/{id}/card/raw.json`, extracts greetings to `card/greetings/00.md…`, extracts `world/lorebook.json`.

---

### `import_preset`
Import a SillyTavern Preset JSON file.

**Input:**
```json
{
  "preset_id": "my_preset",
  "preset_json": "{ ... SillyTavern Preset JSON ... }"
}
```

**Output:**
```json
{ "preset_id": "my_preset", "path": "presets/my_preset/preset.json", "bytes_written": 1234 }
```

---

### `apply_lorebook`
Scan text for lorebook keyword matches; return triggered entries.

**Input:**
```json
{ "character_id": "凌欺霜", "text": "走到天剑阁外的茶摊。" }
```

**Output:** Concatenated lorebook entry content strings (empty string if no match).

---

### `start_session`
Build system prompt + load greetings for a new RP session.

**Input:**
```json
{
  "character_id": "凌欺霜",
  "session_id": null,
  "preset_id": "my_preset",
  "user_name": "玩家"
}
```

**Output:**
```json
{
  "character_id": "凌欺霜",
  "session_id": null,
  "session_dir": "data/characters/凌欺霜/memory",
  "system_prompt": "...",
  "greetings_count": 3,
  "greetings": ["我便是凌欺霜。", "剑光一闪。", "茶摊偶遇。"]
}
```

The client is responsible for calling the LLM with `system_prompt` + chosen greeting as first assistant message.

---

### `get_recent_context`
Return the N most recent messages from a character's chat log.

**Input:**
```json
{ "character_id": "凌欺霜", "n": 10, "session_id": null }
```
`n` defaults to 10. `session_id` is a reserved field (unused in current implementation).

**Output:**
```json
{
  "character_id": "凌欺霜",
  "total_messages": 42,
  "returned": 10,
  "messages": [
    { "role": "user", "content": "..." },
    { "role": "assistant", "content": "..." }
  ]
}
```

**Use case:** Build the message array for the next LLM call (recent context window).

---

### `append_message`
Append one message to a character's chat log (O(1) JSONL append).

**Input:**
```json
{ "character_id": "凌欺霜", "role": "user", "content": "我要挑战你。", "session_id": null }
```
`role` must be `"user"`, `"assistant"`, or `"system"`. `session_id` is reserved.

**Output:**
```json
{ "character_id": "凌欺霜", "role": "user", "total_messages": 43 }
```

**Use case:** After each LLM turn, persist both the user message and the assistant reply to `history/chat_log.jsonl`.

---

### `update_state`
Write or merge fields into `characters/{id}/state/live.json`.

**Input:**
```json
{
  "character_id": "凌欺霜",
  "state_json": "{\"hp\": 80, \"location\": \"天剑阁\"}",
  "overwrite": false
}
```
- `overwrite: false` (default): merge `state_json` fields into existing state (unknown keys preserved).
- `overwrite: true`: replace `live.json` entirely with `state_json`.
- Also appends a timestamped snapshot to `state/history.jsonl`.

**Output:**
```json
{
  "character_id": "凌欺霜",
  "overwrite": false,
  "fields_updated": 2,
  "state": { "hp": 80, "mp": 50, "location": "天剑阁" }
}
```

**Use case:** After a RP turn where the LLM outputs state changes, call `update_state` to persist them. Combines with `airp://characters/{id}/state/live` subscription for live state streaming.

---

### `list_preset_regex_scripts`
List all regex scripts in `presets/{preset_id}/regex/`.

**Input:** `{ "preset_id": "my_preset" }`

**Output:** JSON array of script objects, each enriched with `_filename`. Returns `[]` if directory absent.

---

### `remove_preset_regex_script`
Delete a regex script file from `presets/{preset_id}/regex/`.

**Input:** `{ "preset_id": "my_preset", "filename": "hide_thoughts.json" }`

`filename` must be a bare filename (no path separators or `..`).

**Output:** `{ "preset_id": "my_preset", "filename": "hide_thoughts.json", "removed": true }`

---

### `set_preset_regex_enabled`
Enable or disable a regex script (writes the `disabled` field back to the JSON file).

**Input:**
```json
{ "preset_id": "my_preset", "filename": "hide_thoughts.json", "enabled": true }
```

**Output:** `{ "preset_id": "my_preset", "filename": "hide_thoughts.json", "enabled": true, "disabled": false }`

Supports both single-object and array-format script files.

---

### `write_preset_artifact`
Write an Agent-generated analysis artifact under `presets/{preset_id}/`.

**Input:**
```json
{
  "preset_id": "my_preset",
  "artifact_path": "analysis/summary.md",
  "content": "# Summary\n..."
}
```

- `artifact_path` is relative to `presets/{preset_id}/`. Path traversal (`../`) is blocked.

**Output:**
```json
{ "preset_id": "my_preset", "artifact_path": "analysis/summary.md", "bytes_written": 42 }
```

---

### `write_character_artifact`
Write an Agent-generated analysis artifact under `characters/{character_id}/`.

**Input:**
```json
{
  "character_id": "凌欺霜",
  "artifact_path": "analysis/profile.md",
  "content": "# Profile\n..."
}
```

- `artifact_path` is relative to `characters/{id}/`. Path traversal blocked.
- System directories (`card/`, `world/`, `history/`, `memory/`, `gating/`, `sessions/`) are accessible by path but intentionally excluded from `airp://characters/{id}/artifacts` listing.

**Output:**
```json
{ "character_id": "凌欺霜", "artifact_path": "analysis/profile.md", "bytes_written": 100 }
```

---

### `rollback_messages`
Roll back (delete) the last N messages from a character's ChatLog. Default `n=1`.

Use after an incorrect `append_message` or to redo the last LLM turn.

**Input:**
```json
{
  "character_id": "凌欺霜",
  "n": 1
}
```

- `n` is clamped to `[1, 1000]`. If `n` exceeds the total message count, all messages are removed.
- Rewrites the entire `history/chat_log.jsonl` (not O(1) — avoid on very large logs in tight loops).

**Output:**
```json
{
  "character_id": "凌欺霜",
  "requested": 1,
  "removed": 1,
  "total_messages": 4
}
```

---

### `list_sessions`
List all named sessions for a character (sub-directories under `characters/{id}/sessions/`).

Does NOT include the legacy default session stored directly in `memory/`.

**Input:**
```json
{ "character_id": "凌欺霜" }
```

**Output:**
```json
{
  "character_id": "凌欺霜",
  "sessions": ["20240101_120000_abc123", "20240102_093015_def456"],
  "count": 2
}
```

Use `start_session(character_id, session_id=...)` to resume a specific session.

---

### `get_state_history`
Read the N most recent state snapshots from `state/history.jsonl`, newest-first.

Each snapshot is appended by `update_state`. Useful for reviewing how HP/MP/location evolved during a session.

**Input:**
```json
{
  "character_id": "凌欺霜",
  "n": 10
}
```

- `n` defaults to 10, clamped to `[1, 1000]`.
- Returns empty array if `state/history.jsonl` does not exist.

**Output:**
```json
{
  "character_id": "凌欺霜",
  "entries": [
    { "ts": "2024-01-01T12:00:00Z", "state": { "hp": 60, "mp": 40, "location": "tavern" } },
    { "ts": "2024-01-01T11:55:00Z", "state": { "hp": 80, "mp": 45, "location": "road" } }
  ],
  "count": 2
}
```

---

## Resources

### Static Resources

| URI | Description |
|-----|-------------|
| `airp://characters` | JSON array of imported character IDs |
| `airp://presets` | JSON array of imported preset IDs |

### Resource Templates

| URI Template | Description |
|--------------|-------------|
| `airp://characters/{character_id}/card` | TavernV2 card JSON (`card/raw.json` or `card.json`) |
| `airp://characters/{character_id}/world/lorebook` | World book JSON (`world/lorebook.json`). Returns `{"entries":[]}` if absent. |
| `airp://characters/{character_id}/history` | Chat log JSON (`history/chat_log.jsonl` deserialized). Returns `{"messages":[]}` if absent. |
| `airp://characters/{character_id}/artifacts` | JSON array of Agent-written artifact relative paths (excludes system dirs/files). |
| `airp://characters/{character_id}/state/live` | Latest live state JSON written by `<state>{...}</state>` tag in LLM output. Returns `{}` if no state yet. |
| `airp://presets/{preset_id}/raw` | Raw `preset.json` content for Agent analysis. Supports `?offset=N&limit=M` pagination for large files (default limit: 100 000 chars). |
| `airp://presets/{preset_id}/artifacts` | JSON array of Agent-written preset artifact relative paths (excludes `preset.json` itself). |
| `airp://presets/{preset_id}/regex` | Same as `list_preset_regex_scripts` output — JSON array of regex script objects with `_filename`. |

---

## Prompts

### `build_system_prompt`
Assemble the full RP system prompt for a character.

**Arguments:**
| Name | Required | Description |
|------|----------|-------------|
| `character_id` | yes | Imported character ID |
| `preset_id` | no | Preset ID to merge into system prompt |
| `user_name` | no | Display name for `{{user}}` macro (default: `User`) |

**Returns:** Single `user` message containing the full system prompt string.

---

### `filter_text`
Static prompt for a text-filtering Agent.

**Arguments:** none

**Returns:** Instructions to strip `<think>/<thought>/<status>/[卷评估]/[OOC]` tags while preserving prose.

---

### `state_update_instruction`
Static prompt fragment instructing the LLM to emit `<state>{...}</state>` at end of response.

**Arguments:** none

**Returns:** Instruction text with example JSON fields (`hp`, `mp`, `time`, `location`, `npcs`, `quest`).

---

### `analyze_character_card` *(M_CA)*
Full Agent workflow prompt for character card analysis.

**Arguments:**
| Name | Required | Description |
|------|----------|-------------|
| `character_id` | yes | Character to analyze |

**Workflow the Agent follows:**
1. Read `airp://characters/{id}/card`
2. Read `airp://characters/{id}/world/lorebook`
3. Write artifacts via `write_character_artifact`:
   - `analysis/profile.md` — personality, background, speech style, rules summary
   - `analysis/tier.json` — complexity tier 1–4 + reasoning
   - `style/guide.md` — prose style extraction
   - `cot/strategy.md` — RP CoT strategy
4. Verify via `airp://characters/{id}/artifacts`

**Tier schema:**
```json
{
  "tier": 2,
  "label": "中等",
  "reasoning": "...",
  "lorebook_entries": 12,
  "has_custom_rules": true,
  "has_state_tracking": false
}
```

---

### `analyze_preset` *(M_CA)*
Full Agent workflow prompt for preset analysis.

**Arguments:**
| Name | Required | Description |
|------|----------|-------------|
| `preset_id` | yes | Preset to analyze |

**Workflow the Agent follows:**
1. Read `airp://presets/{id}/raw`
2. Write artifacts via `write_preset_artifact`:
   - `analysis/summary.md` — prompt list, order, purpose per segment
   - `analysis/regex_scripts.json` — regex filter scripts array
   - `style/instructions.md` — writing instructions extracted from preset
3. Verify via `airp://presets/{id}/artifacts`

---

## Agent Workflows

### Import + Analyze Character Card
```
import_card(character_id, card_json)
  → get_prompt(analyze_character_card, {character_id})
  → [Agent calls LLM with returned prompt]
  → write_character_artifact × N
  → read airp://characters/{id}/artifacts  (verify)
  → start_session(character_id, preset_id?, user_name)
```

### Import + Analyze Preset
```
import_preset(preset_id, preset_json)
  → get_prompt(analyze_preset, {preset_id})
  → [Agent calls LLM with returned prompt]
  → write_preset_artifact × N
  → read airp://presets/{id}/artifacts  (verify)
```

### MCP-Native RP Session (recommended for Claude Code)
```
start_session(character_id, preset_id, user_name)
  → [client calls LLM with system_prompt + greetings[0] as first assistant message]
  → loop:
      get_recent_context(character_id, n=20)  → build message array
      [client calls LLM]                      → get reply text
      append_message(character_id, "user", user_input)
      append_message(character_id, "assistant", llm_reply)
      update_state(character_id, state_json)  → persist HP/MP/location/etc.
      read airp://characters/{id}/state/live  → verify state
```

### SSE Gateway RP Session (legacy / high-volume path)
```
start_session(character_id, preset_id, user_name)
  → POST /v1/chat/completions (AIRP daemon with SSE streaming)
  → LLM emits <state>{...}</state> in response
  → AIRP finalizer: strips <state> from chat log, writes state/live.json
  → read airp://characters/{id}/state/live  (current state)
```

---

## Security

- All `character_id` / `preset_id` values are validated by `validate_id_segment` (alphanumeric + `_-` only, no `/` or `..`).
- All artifact paths use `safe_resolve_for_write`: only the base directory is canonicalized; component-by-component normalization rejects `..` path escape.
- HTTP transport `allowed_hosts` defaults to `["localhost","127.0.0.1","::1"]`.
