# Core 项目评估报告：关于下游"自建 Agent 客户端"的决策建议

**致：** Core 项目维护者
**主题：** 下游团队拟自建 Rust Agent 客户端以"接入 LLM 驱动 RP"——经源码审阅，该客户端的主体功能已存在于 Core，建议复用而非重写；同时发现需 Core 侧澄清的定位矛盾与 3 个真实缺口。
**审阅范围：** 本地 `D:\AIRPCLI`（Core 完整版），重点 `adapter.rs` / `chat_pipeline.rs` / `mcp/` / `hub/mod.rs`。
**背景：** 下游持有一份外部 AI 生成的架构设计稿，主张新写一个"Rust 数据流转网关客户端"，内含增量 JSON 流式解析器、状态机 Actor、越权 guardrail 等。本报告对照 Core 实际代码评估其必要性。

---

## 1. 核心结论

**Core 的 daemon 面已经是一个完整、可用、带测试的流式 RP 后端，自身会调用 LLM。** 下游想自建的"接入 LLM 的 Agent 客户端"，其 80% 功能已在 Core 内实现。不建议另起炉灶。

证据（源码，非 README）：

| 能力 | 实现位置 |
|---|---|
| OpenAI 兼容 `/v1/chat/completions` 流式调用 | `adapter.rs:96` `call_streaming_api` |
| Anthropic 原生 `/v1/messages` 流式调用 | `adapter.rs:186` `call_streaming_api_anthropic` |
| 引擎分发（Direct / AnthropicMessages / ClaudeCodeSdk） | `adapter.rs:292` `call_streaming_api_auto` |
| 上下文装配（card / lorebook / preset / volume / system prompt） | `chat_pipeline.rs:296` `prepare_pipeline` |
| 流式处理（FSM 过滤 + XML 拆包 + SSE 下发） | `chat_pipeline.rs:596` `build_sse_stream`，调用点 `:611` |
| 落库 + 状态持久化 + 封卷 | `chat_pipeline.rs:694` `run_finalize` |
| 向 UI 推送状态更新（MCP resource-updated） | `chat_pipeline.rs:1024` |

SSE 跨包行缓冲、双 provider 格式、客户端断连取消（mpsc 接收端 drop，`chat_pipeline.rs:641`）、取消后仍 finalize、配额计量、封卷/维护子任务——这些"自建必然踩"的细节，Core 均已处理。

---

## 2. 需要 Core 澄清的定位矛盾（下游误判的根源）

README / 设计军规声明 **"Core 不调用 LLM，所有推理由 client/Agent 完成"**。但源码中 `daemon + chat_pipeline + adapter` 这条路径**确实调用 LLM**。

实际情况是 **Core 一个 crate 有两张脸**：

| 面 | 模块 | 是否调 LLM | "不调 LLM"军规适用 |
|---|---|---|---|
| MCP 工具面 | `mcp/mod.rs`、`mcp/tools.rs` | 否（纯数据工具） | 是 |
| daemon 面 | `daemon/`、`chat_pipeline`、`adapter` | **是**（流式 RP 后端） | 否（README 未区分） |

**影响：** 下游严格按 README 理解，得出"Core 不驱动 LLM、必须自建客户端来接入"的结论——这是整个自建动议的起点，而它建立在一个与源码不符的表述上。

**请求 1：** 在 README / 设计文档中显式区分这两张脸，并说明 daemon 面的支持级别（一等公民 / 兼容层 / 计划弃用）。这一条直接决定下游是否需要自建。

---

## 3. 存在两套并行机制，需指定 canonical 主线

Core 对"生成"和"状态更新"各有两条并行路径：

**生成：**
- (a) daemon 自调 LLM：`build_sse_stream` → `adapter`
- (b) MCP host 自调：MCP 工作流 `get_recent_context → LLM → append_message → update_state` 循环（见 `mcp/mod.rs:1050` 注释）

**状态写入：**
- (a) daemon 路径：LLM 输出 `<state>{...}</state>`，由 `extract_state_content`（`chat_pipeline.rs:928`）解析、`persist_live_state`（`:972`）落盘
- (b) MCP 工具路径：客户端显式调 `update_state`（`mcp/tools.rs:1142`）

下游"建客户端"必须二选一对接，否则会出现两套状态来源互相覆盖。

**请求 2：** 指明哪条是 canonical 主线。若 daemon 面是主线 → 下游基本无需自建；若 MCP 面是主线 → 下游需要的是一个"按工作流调度 MCP 工具 + 自调 LLM"的 host，且仍应复用 `adapter.rs` 作为库，而非重写流式层。

---

## 4. 下游设计稿三大"技术攻关"对照 Core 现状

下游设计稿把以下三项列为核心攻关。对照 Core 源码，均为伪命题或已有更简方案：

| 设计稿主张 | Core 实际 | 评估 |
|---|---|---|
| §3.1 自研增量 JSON 流式解析器（解决未闭合 JSON） | 输出为纯文本 + XML 标签（`<think>`/`<action>`/`<state>`）；流式过滤 `fsm.process_chunk` + 拆包 `unpacker.process_chunk`（`chat_pipeline.rs:638-640`）；`<state>` 仅在 finalize 对**闭合块**做一次 `serde_json::from_str` | 不需要。JSON 大对象方案是自造难题；Core 的标签方案天然可流式、未闭合优雅降级 |
| §3.2 越权 guardrail（匹配用户名/第二人称即 abort+重生，最多 2 次） | **未实现**。FSM 仅做输出净化（剥 `<state>`/`<卷评估>`/preset 正则）；全仓无生成级 auto-retry（grep "retry" 仅命中 MCP 幂等去重 `mcp/idempotency.rs`） | "别替用户行动"目前仍靠 prompt/preset。详见缺口 §5.2 |
| §3.3 状态机 Actor（mpsc 信箱防死锁） | **未使用**。finalize 在流结束后串行写 `live.json`（覆盖）+ `history.jsonl`（追加）；配置走 `RwLock` 快照读一次即释放 | 不需要。串行 finalize 已保证时序；Actor 防死锁是伪命题 |

结论：设计稿的"工程量"绝大部分要么是 Core 已用更简方案解决，要么是不该做的事。

---

## 5. 真实缺口（建议在 Core 内补强，而非新建客户端）

### 5.1 state schema 存在但不强制（文档承诺的"数值锁死"未兑现）

- `state/schema.json` 定义了 `{key, type, min, max, label}`，有读取端点 `get_character_state_schema`（`daemon/handlers.rs:526`）。
- 但 min/max 目前**仅用于**：prompt 注入的 `[Current State]` 标签渲染（`orchestrator/mod.rs:289-302`）、前端进度条（`index.html`）。
- **未见在写入路径按 schema 钳制数值**：`persist_live_state`（`chat_pipeline.rs:972`）把 LLM 吐的 `<state>` JSON 原样落盘；`update_state_impl`（`mcp/tools.rs:1142`）做 merge/overwrite/合法性校验，但不按 schema clamp 数值。
- 设计稿宣称的 `clamp(min,max)` "从根本杜绝超界" 因此**不成立**——模型可写出 `affection: 999`。

**建议：** 在 `persist_live_state` 与/或 `update_state_impl` 落盘前，按 `schema.json` 的 min/max 钳制数值字段。小改动、高价值，且天然属于 Core 职责。**请确认数值强制是否为预期目标。**

### 5.2 无"防越权"guardrail、无生成级重试

若需要确定性的"不替用户发言/行动"，当前完全没有。建议**不要**采用设计稿的"正则匹配第二人称即 abort"方案（误杀正常 RP 台词、热路径成本翻倍）；优先利用已有的 `<action>` 结构分离做约束，或明确将此交给 preset/prompt 层并在文档中说清。

### 5.3 `ClaudeCodeSdk` 引擎未实现

`adapter.rs:315` 为 stub（`"ClaudeCodeSdk engine not yet implemented"`）。若以 Claude Code SDK 作为后端是路线之一，这是唯一需要新写的接入点。

### 5.4 设计红线：无 server-side agent loop

`hub/mod.rs:11` 明确「Hub 不自调度、无 server-side Agent loop」。**这是下游唯一合理的自建理由**：若 RP 需要 agentic 多步循环（模型调工具→观察→再调，N 轮），该 loop 在 Core 内按设计无处安放。

**但即便如此，新建范围应严格限定为"loop 调度器"**——把 `adapter` / `chat_pipeline` 当库复用（或经 daemon HTTP 调用），绝不重写 SSE / provider / 拆包。

---

## 6. 给下游的最终建议

- **单轮 RP（请求-响应）：** Core daemon 面已完全够用。下游前端（State-Protocol）直接消费 Core 的 `UnpackedChunk` SSE + 订阅 `airp://characters/{id}/state/live`。**无需自建客户端。**
- **agentic 多步 RP：** 仅需写一个薄 loop 调度器（§5.4），复用 Core 现有 Rust 资产。
- 无论哪种，"自建一个含流式 JSON 解析器 + 状态 Actor + 正则 guardrail 的 Rust 网关" = 重造 Core daemon + 实现已被证伪的需求，不建议。

---

## 7. 待 Core 维护者答复的问题清单

1. daemon 面（自调 LLM）的支持级别？是否为推荐生产路径，还是仅 SillyTavern 兼容层？
2. 生成与状态写入，canonical 主线是 daemon 面还是 MCP 工具面？（§3）
3. state schema 的 min/max 是否**应当**在写入时强制 clamp？若是，是否接受在 `persist_live_state` / `update_state_impl` 内补强？（§5.1）
4. "防越权"是否在 Core 的职责范围内，还是明确划归 preset/prompt 层？（§5.2）
5. `ClaudeCodeSdk` 引擎是否在路线图内？（§5.3）

---

## 附：关键源码索引

```
adapter.rs:18           BackendEngine（Direct/AnthropicMessages/ClaudeCodeSdk）
adapter.rs:96           call_streaming_api（OpenAI 兼容 SSE）
adapter.rs:186          call_streaming_api_anthropic（Anthropic SSE）
adapter.rs:292          call_streaming_api_auto（引擎分发）
adapter.rs:315          ClaudeCodeSdk 未实现（stub）
chat_pipeline.rs:296    prepare_pipeline（上下文装配）
chat_pipeline.rs:596    build_sse_stream（流式处理 + 下发）
chat_pipeline.rs:638    fsm.process_chunk + unpacker.process_chunk（流式过滤/拆包）
chat_pipeline.rs:694    run_finalize（落库 + 状态 + 封卷）
chat_pipeline.rs:928    extract_state_content（<state> 一次性解析）
chat_pipeline.rs:972    persist_live_state（落盘，无 clamp）
chat_pipeline.rs:1024   MCP notify_resource_updated（状态推 UI）
orchestrator/mod.rs:289 schema min/max 用于 prompt 渲染（非强制）
daemon/handlers.rs:526  GET /v1/characters/:id/state/schema
mcp/tools.rs:1142       update_state_impl（MCP 状态写入）
mcp/mod.rs:1050         MCP RP 工作流 loop（客户端驱动）
hub/mod.rs:11           设计红线：无 server-side agent loop
```
