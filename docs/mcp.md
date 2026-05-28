# AIRP MCP Server Reference

AIRP exposes a full MCP (Model Context Protocol) server over two transports:

| Transport | Endpoint | Use case |
|-----------|----------|----------|
| **stdio** | `airp-core mcp` | Claude Desktop / Claude Code (recommended) |
| **HTTP Streamable** | `POST /mcp/v1` + `GET /mcp/v1` SSE | Remote agents, HTTP-based MCP clients |

**Tool count**: 33 (see `airp-core list-tools --format summary` for the live list — that command reads `rmcp::ToolRouter::list_all()` so it never falls out of sync with the code).

## Universal conventions

Every tool ships with MCP standard `annotations` (AUDIT-11) so harnesses can auto-classify safety without per-host mapping tables:

- `readOnlyHint: true` — pure read, safe to call silently
- `destructiveHint: true` — may delete or remove data; clients should confirm
- `idempotentHint: true` — repeated calls converge to the same state
- `openWorldHint: false` — every AIRP tool only touches the local `data/` dir

Every write tool (12 of them) accepts an optional `idempotency_key` (AUDIT-12). Pass the same key on retry; the cache (FIFO 1000, TTL 24 h) returns the original response and skips the side effect.

## Quick Start (Claude Code)

1. Build release binary:
   ```powershell
   cargo build --release
   ```
2. Copy `mcp_config.example.json` to your Claude config directory and adjust paths.
3. Verify with `airp-core mcp` — first JSON-RPC message should respond with server info.

For shell automation without a long-running server, use the one-shot dispatcher:
```bash
airp-core tool ping
airp-core tool list_characters
airp-core tool append_message --json '{"character_id":"alice","role":"user","content":"hi"}'
```

For full-state debug reports (user-pasted bug triage), use:
```bash
airp-core diagnose                       # full JSON
airp-core diagnose --format summary      # human-readable table
```

Sensitive fields (`api_key`, `access_api_key`) never leak in plaintext — only `*_set` booleans surface.

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

### `list_characters`
List every imported character ID.

**Input:** `{}`

**Output:** `{ "count": 2, "characters": ["alice", "bob"] }`

---

### `list_users`
List every imported user persona ID (see M_UP section below).

**Input:** `{}`

**Output:** `{ "count": 1, "users": ["player_alice"] }`

---

### `get_character`
Fetch a character's card JSON + folder presence metadata. Friendlier than the `airp://characters/{id}/card` resource for harnesses that prefer tool calls.

**Input:** `{ "character_id": "alice" }`

**Output:**
```json
{
  "character_id": "alice",
  "card_present": true,
  "card_format": "v2_folder",
  "card": { "spec": "chara_card_v2", "data": { "name": "Alice", ... } }
}
```

`card_format` values: `v2_folder` (CF-1 `card/raw.json`), `v2_legacy` (root `card.json`), `missing`.

---

### `get_live_state`
Read the current `state/live.json` snapshot without history. Equivalent to the `airp://characters/{id}/state/live` resource as a tool.

**Input:** `{ "character_id": "alice" }`

**Output:** `{ "character_id": "alice", "present": true, "state": { "hp": 80, ... } }`

When the file doesn't exist: `{ "present": false, "state": {} }`.

---

### `delete_character`
Recursively remove `data/characters/{id}/`. Safety-latched.

**Input:** `{ "character_id": "alice", "confirm": false }`

**Default (dry-run):**
```json
{
  "character_id": "alice",
  "deleted": false,
  "dry_run": true,
  "would_remove_top_entries": ["card", "history", "memory", "state", ...],
  "hint": "pass {\"confirm\":true} to actually delete"
}
```

**Confirmed (`confirm: true`):** removes directory; returns `{ "deleted": true, "removed_top_entries": [...] }`.

`destructiveHint: true`. No undo.

---

## M_UP — User Persona tools

User personas mirror character cards but with an explicit **base / drift** split:

- `users/{user_id}/persona.json` — **元设定** (immutable base, optionally sealed by `persona.lock`)
- `users/{user_id}/state/live.json` — **变量设定** (mutable drift overlay)
- `users/{user_id}/state/history.jsonl` — snapshot timeline

Server does **not** judge semantic conflicts (戒律 1). It returns base + drift + a `drift_keys` diff; the Agent decides whether e.g. "learned_basketball" in drift contradicts "skills: []" in the base.

### `import_user_persona`
Write the immutable base persona.

**Input:**
```json
{
  "user_id": "player_alice",
  "persona_json": "{\"name\":\"Alice\",\"description\":\"cannot play basketball\"}",
  "lock": false,
  "idempotency_key": null
}
```

`persona_json` must be a JSON object with a non-empty `name` field. Setting `lock: true` creates `persona.lock` immediately. If `persona.lock` already exists, this tool **rejects** the import.

**Output:** `{ "user_id": ..., "name": "Alice", "locked": false, "persona_path": "..." }`

---

### `lock_user_persona`
Seal the persona (create `persona.lock`). Idempotent.

**Input:** `{ "user_id": "player_alice" }`

**Output:** `{ "user_id": ..., "locked": true, "was_already_locked": false }`

Errors if `persona.json` doesn't exist.

---

### `get_user_persona`
Return full base + current drift + drift_keys diff.

**Input:** `{ "user_id": "player_alice" }`

**Output:**
```json
{
  "user_id": "player_alice",
  "persona": { "name": "Alice", "description": "cannot play basketball" },
  "locked": true,
  "current_state": { "learned_basketball": true, "mood": "excited" },
  "drift_keys": ["learned_basketball", "mood"]
}
```

`drift_keys` are top-level keys in `current_state` that are NOT in `persona`. Agent reads both halves to reason about contradictions.

---

### `update_user_state`
Update the drift overlay. Merge or overwrite. **Never** modifies `persona.json` (the immutable base stays stable across an entire campaign).

**Input:**
```json
{
  "user_id": "player_alice",
  "state_json": "{\"learned_basketball\": true}",
  "overwrite": false,
  "idempotency_key": null
}
```

**Output:** `{ "user_id": ..., "overwrite": false, "fields_updated": 1, "state": { ... } }`

Side effect: appends a snapshot to `state/history.jsonl` and emits `notifications/resources/updated` for `airp://users/{user_id}/state/live`.

---

### `get_user_state_history`
Read recent state snapshots (newest-first).

**Input:** `{ "user_id": "player_alice", "n": 10 }`

**Output:**
```json
{
  "user_id": "player_alice",
  "entries": [
    { "ts": "2026-05-28T07:00:00Z", "state": { ... } },
    ...
  ],
  "count": 3
}
```

---

## Scene CRUD tools (M_MS)

Multi-character scenes. Server-side multi-character orchestration is opt-in: a scene defines `characters: [{character_id, role, intro}]` plus narrator style + lorebook merge mode. Used by `chat_pipeline` when a `scene_id` is passed.

### `list_scenes`
List every scene ID. **Input:** `{}` **Output:** `{ "count": 1, "scenes": ["tavern"] }`

### `get_scene`
Return full SceneConfig. **Input:** `{ "scene_id": "tavern" }`

### `create_scene`
Create or replace a scene from a full SceneConfig JSON string.

**Input:**
```json
{
  "scene_json": "{\"scene_id\":\"tavern\",\"characters\":[],\"description\":\"Dawn tavern\"}",
  "idempotency_key": null
}
```

`scene_id` is determined by the JSON. Invalid scene_ids are rejected by SceneId's serde validation.

**Output:** `{ "scene_id": ..., "characters_count": 0, "created": true }`

### `add_scene_character`
Append a character to a scene's `characters` array.

**Input:**
```json
{
  "scene_id": "tavern",
  "character_id": "alice",
  "role": "primary",
  "intro": "the hero",
  "idempotency_key": null
}
```

`role` is `primary` or `npc` (default `npc`).

---

## Volume management tools

Pure file operations. **AIRP never calls the LLM from these tools** (戒律 1) — Agent supplies pre-summarized digests when it wants summarization.

### `list_volumes`
**Input:** `{ "character_id": "alice" }` **Output:** `{ "character_id": ..., "count": 2, "volumes": [1, 2] }`

### `read_volume`
**Input:** `{ "character_id": "alice", "number": 1 }` **Output:** `{ "character_id": ..., "number": 1, "content": "..." }`

### `seal_volume`
Archive `current.md` as the next `vol_NNN.md` and clear `current.md`. Optional `content` parameter overrides the raw current.md (Agent-computed digest).

**Input:**
```json
{
  "character_id": "alice",
  "content": null,
  "idempotency_key": null
}
```

**Output:** `{ "character_id": ..., "sealed_number": 1, "bytes": 1234, "used_override_content": false }`

Empty current.md (and no override) returns an error — there's nothing to archive.

---

## Resources

### Static Resources

| URI | Description |
|-----|-------------|
| `airp://characters` | JSON array of imported character IDs |
| `airp://presets` | JSON array of imported preset IDs |
| `airp://users` | JSON array of imported user persona IDs (M_UP) |

### Resource Templates

| URI Template | Description |
|--------------|-------------|
| `airp://characters/{character_id}/card` | TavernV2 card JSON (`card/raw.json` or `card.json`) |
| `airp://characters/{character_id}/world/lorebook` | World book JSON (`world/lorebook.json`). Returns `{"entries":[]}` if absent. |
| `airp://characters/{character_id}/history` | Chat log JSON (`history/chat_log.jsonl` deserialized). Returns `{"messages":[]}` if absent. |
| `airp://characters/{character_id}/artifacts` | JSON array of Agent-written artifact relative paths (excludes system dirs/files). |
| `airp://characters/{character_id}/state/live` | Latest live state JSON written by `<state>{...}</state>` tag in LLM output. Returns `{}` if no state yet. Subscribable — `update_state` emits `notifications/resources/updated`. |
| `airp://presets/{preset_id}/raw` | Raw `preset.json` content for Agent analysis. Supports `?offset=N&limit=M` pagination for large files (default limit: 100 000 chars). |
| `airp://presets/{preset_id}/artifacts` | JSON array of Agent-written preset artifact relative paths (excludes `preset.json` itself). |
| `airp://presets/{preset_id}/regex` | Same as `list_preset_regex_scripts` output — JSON array of regex script objects with `_filename`. |
| `airp://users/{user_id}/persona` | M_UP base persona (元设定). Returns `null` if no persona imported. |
| `airp://users/{user_id}/state/live` | M_UP drift overlay (变量设定). Returns `{}` if no state yet. Subscribable — `update_user_state` emits `notifications/resources/updated`. |

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
