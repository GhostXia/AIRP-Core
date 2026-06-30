# AIRP-Core → 独立 Agent 后端 · 计划书

> **一句话定位**：AIRP-Core 从「单回合流式 RP 后端」演进为**独立、开源、乐高式的 Agent 后端**——长出它过去明确拒绝的那个 loop，成为 AIRP 生态里一直空着、却谁也没去填的「推理大脑」。
>
> **范围澄清**：本计划只针对**本仓（AIRP-Core）**。Core 就是这个 Agent 后端，不另起新仓。"把四个 AIRP 项目并进一个新仓"目前**只是想法、不排期**——见 §9 地平线，本计划不为它落任何 milestone。

状态：草案 v1（转向已确认 · 提交 PR 待审计 bot · 后续 M_AGENT 开发交由其他 Agent）· 日期：2026-06-30

---

## 1. 为什么转向（诚实记录）

### 1.0 依据：`AGENT_CLIENT_ASSESSMENT.md`

本转向的直接依据是仓根 **[`AGENT_CLIENT_ASSESSMENT.md`](AGENT_CLIENT_ASSESSMENT.md)**（曾于 commit `f054ae9` 当"过时文档"删除，2026-06-30 已从 git 历史取回入库——它是本转向的根，不该躺在历史里）。该报告原是评估"下游团队拟自建 Rust Agent 客户端"是否必要，按源码审阅后得出三条对本计划至关重要的结论：

1. **Core 的 daemon 面已经是一个完整、带测试、自调 LLM 的流式 RP 后端。** 任何"接入 LLM 的 Agent 客户端"想要的 ~80% 功能（OpenAI/Anthropic 双 provider 流式、SSE 跨包行缓冲、上下文装配、FSM 过滤+XML 拆包、客户端断连取消后仍 finalize、配额计量、封卷）**Core 已实现**。重写 = 重造 Core。
2. **唯一合理的"自建"理由是 §5.4：Core 内没有 server-side agent loop。** 若 RP 需要 agentic 多步循环（模型调工具 → 观察 → 再调，N 轮），按当时设计该 loop 在 Core 内**无处安放**。
3. **即便要补这个 loop，范围也必须严格限定为"loop 调度器"**——把 `adapter` / `chat_pipeline` **当库复用**，**绝不重写** SSE / provider / 拆包。

报告还指出一个定位矛盾：旧 README 宣称"Core 不调 LLM"，但 `daemon + chat_pipeline + adapter` 这条路径**确实调 LLM**——Core 当时"一个 crate 两张脸"（MCP 工具面不调 LLM / daemon 面调 LLM）。**剥离（commit `24aff97`）删掉了 MCP/Hub/diagnose/sync/UI 面，正是为消除这个矛盾**：Core 收缩成单一的 daemon 面（流式 RP 后端）。

**于是本计划的逻辑闭环成立：** 剥离把 Core 收成一张干净的脸（自调 LLM 的流式后端）；现在按报告 §5.4，在这张干净的脸上**只补那一件真正缺的东西——loop**，且按报告纪律"复用不重写"。

> **战略背景（用户 2026-06-30）**：UI（State-Protocol）、网关（Gateway）、MCP 数据底座（MCP-Server）生态里**都已有、且发展迅速**。唯独缺一个**原生的 Agent 后端来驱动 LLM**。Core 既然已是那条自调 LLM 的路径，就该转型补位成这个原生 Agent 后端，而不是再起一个下游客户端。

### 1.1 生态的空框

AIRP 生态四块，各自把"推理 / loop"这个框**一致地留空**，向上委托给"外部 host"：

| 项目 | 对 loop 的态度 | 原文 |
|---|---|---|
| AIRP-Core | 拒绝 | 戒律#2「不跑 server-side agent loop —— agentic 多步循环由外部 host 调度」 |
| AIRP-MCP-Server | 拒绝 | 「不自造 Agent runtime，不做推理」 |
| AIRP-Gateway | 不碰 | 「纯协议桥，不做推理、不拼 prompt」 |
| AIRP-State-Protocol | 只渲染 | 「UI 只渲染，不生成」；生态图底部 `Agent Runtime (推理)` 是个**空框** |

**那个框从来没人填。** 四块都把它推给"上游 Agent"（Claude Code / Cursor / pi / Codex）。这在"借别人的 Agent 跑"时成立，但生态本身**没有一个自带的、可独立部署的参考大脑**。

本计划就是去填它：**Core 转身，成为这个参考大脑。**

这不是推翻乐高，是**补上乐高缺的那一块**。底座的纯度不丢——它移交给 Lean 版（AIRP-MCP-Server）永久守护；Core 升入 runtime 层。

### 1.2 为什么必须"原生"——提示词纯净度（决定性理由）

为什么 loop 必须由 Core **原生拥有**，而不是"托管一个第三方 Agent 客户端来跑"？因为 **RP 对提示词纯净度的要求极高**：

- 角色上下文里**每一个 token 都影响角色保真度与文风**。RP 不是问答，是长程演绎——一句外来的"You are a helpful assistant / 请一步步思考 / 安全前导词"就能把角色拉出戏（社区所谓"死人化"）。
- 第三方 Agent 客户端（Claude Code / Cursor / Codex / pi）**必然在 RP 内容外裹自己的脚手架**：自带 system prompt、工具使用说明、思考 harness、安全前导。这些是它们的产品本体，关不掉 → **提示词污染**。
- **即便用 subagent 隔离上下文也不能根除**：subagent 换的是上下文窗口，但 subagent runtime 仍在 RP 内容外包它自己的 prompt 脚手架。隔离 ≠ 纯净。
- **结论**：要 pristine prompt control，loop 必须 Core 原生拥有——**进模型上下文的每个 token 由 Core 全权决定**，不经任何第三方 runtime 的手。这就是"Core 就是这个后端"而非"§7 托管外部 runtime"的根本原因。

> **诚实约束（反噬自检）**：原生拥有 loop ≠ 自动纯净。Core 自己的 agent 脚手架（工具 schema / ReAct 指令 / 工具结果回灌）若塞进角色 system prompt，**Core 就成了新的污染源**。所以这条理由直接派生一条新戒律（§2.1 第 6 条）：**loop 脚手架必须与 RP 上下文物理可分离**。

---

## 2. 定位边界（最重要的一节）

转向会立刻撞上 Core 现有的 README 戒律与"数据底座"历史定位。必须先把边界划清，否则后面每个 milestone 都会被"这不是违反戒律吗"反复质询。

### 2.1 戒律分层，不是被推翻

过去的"四戒律"（拒 server 侧 loop / 拒自动副作用 / 拒业务决策 / 开放接入）是**底座戒律**。它继续 100% 约束 **AIRP-MCP-Server（Lean）**——纯数据底座由 Lean 版守到底。

Core 升入 runtime 层后，改受**「有界 Agent 戒律」**约束。过去一句"禁止 loop"，拆成五条"如何安全地跑 loop"：

1. **有界**：loop 必须有 step 上限 + token/成本预算 + 墙钟超时，任一触顶即停。无限循环 = bug，不是特性。
2. **可取消**：任何在跑的 agent run 必须能被客户端单次请求中止，已派生子任务随之收敛（复用现有 `JoinSet` 收束模型）。
3. **可观测**：每一步（规划 / 工具调用 / 工具结果 / 生成）都流式可见，不做"黑箱跑完才吐结果"。
4. **工具最小授权**：工具调用走 allowlist；破坏性工具（删除 / 覆盖）**默认 dry-run**，需显式确认才真执行。呼应旧 AUDIT-6/7「软提示」——server 给数据与建议，是否落副作用由 loop 在边界内决策，而非自动卷封存式的隐式副作用。
5. **幂等与隔离**：带幂等键的工具重试不重复副作用；同一角色 / quota root 的并发写串行化（顺手偿还现有"并发写无锁"已知限制）。
6. **上下文纯净**（RP 的命门，见 §1.2）：agent 脚手架（工具定义 / 规划指令 / 观测回灌）走**结构化通道**，**不混入角色 system prompt**。进角色上下文的 token 由 RP 数据（卡 / 世界书 / 预设 / 卷 / state）决定，不由 loop 机制决定。Core 既然为"纯净"而存在，自己更不能当污染源。

> 一句话：**底座戒律守"server 永不自醒"；Agent 戒律守"自醒也得在笼子里、且不弄脏角色上下文"。** 两者不矛盾，分属两层。

### 2.2 护城河不变

转向**不动**[克制即护城河]的三项：

- **License**：仍 MIT OR Apache-2.0。Agent 后端照样商用 / fork / 集成无限制。
- **协议标准**：Core 现在是 **MCP 客户端**（消费上游工具），仍只走标准协议——不自造 VCP 式闭源文本协议。对外仍 OpenAI 兼容。
- **零架构绑定 + 单二进制**：standalone 可跑，不硬依赖任何兄弟仓。

唯一变化：Core **从"水平数据底座阵营"挪到"Agent runtime 阵营"**。这是主动选择——生态需要一个 MIT 干净、协议标准、架构自由的**现成开源 Agent 后端**，市面上重型框架（VCPToolBox / TavernHeadless）三项约束全失，正是错位空位。

### 2.3 乐高独立性保证（硬约束）

Agent 后端化**不得**让 Core 长出对兄弟仓的硬依赖：

- **默认 batteries-included**：loop 的工具默认用 Core **进程内自带**的数据操作（角色卡 / 世界书 / 状态 / 卷 / 会话 / 场景 / 预设 —— 这些机能 Core 已有，见 §4）。零外部进程即可跑一个完整 agent run。
- **外部能力全可选**：接 AIRP-MCP-Server（更全的数据工具面）、接 AIRP-Gateway（鉴权 / 限流 / 路由）、接 AIRP-State-Protocol（声明式 UI）——**都是可选适配，不是运行前置**。一个兄弟仓都不接，Core 也是完整 Agent 后端。

---

## 3. 能力增量（Core 缺什么才算 Agent 后端）

### 3.0 loop 到底为 RP 做什么（动机：先想清楚再加能力）

loop 不是为"agentic 而 agentic"。它只为**单次请求-响应装不下的回合**存在——生成途中需要"去取数据 / 算个确定结果 / 协调多个角色"的场景。具体 RP 用例（全部复用 Core/生态已有资产，不内置业务模块）：

| # | RP 用例 | 为什么单回合不够 | 复用资产 |
|---|---|---|---|
| 1 | **多角色场景纯净轮转**（最强，绑定纯净度） | N 个 NPC 各自需要**只属于自己**的纯净上下文 + 轮流发言。单回合无法在一次调用里给每个 NPC 独立装配纯净上下文 | `scene.rs` 多角色；loop 每 NPC 一次独立生成 |
| 2 | **世界书按需检索回灌**（RAG-in-loop） | 生成途中才发现缺某设定（某地规则/某 NPC 背景）。开场一次性灌全世界书 = 烧 token + 稀释；按需取只灌触发条 | `apply_lorebook` + aho-corasick；未来 AIRP-RAG |
| 3 | **长程记忆语义检索** | 超长 RP，相关旧情节在已封存的卷里。线性塞历史装不下；需检索回灌 | volume system（先用简单检索；**RAG 暂不考虑**，§9） |
| 4 | **工具化确定性结算**（dice/combat/economy） | 叙事需要确定数值（掷骰/伤害/经济），但模型自己编不可信。戒律禁内置 dice 模块 → loop 调**外部 MCP 工具**算，结果回灌叙事 | MCP upstream 工具；Agent 决定何时调，工具只算 |
| 5 | **状态/gating 驱动剧情推进** | 到检查点需判定是否解锁下一阶段再决定后续走向。把全部 checkpoint 规则塞 prompt = 污染 + 失控 | 现有 `gating/checkpoints` |

**反面（loop 不该做的，守戒律）**：不替用户发言/行动；不做 server 侧自动剧情决策（"该进第二幕了"这种由 Agent/用户拍板，不是 server 自驱）；不内置 dice/combat/economy 逻辑（只调外部工具算）。

> 一句话：**默认单回合（现有管线）就够；只有当这一回合"生成到一半得去拿点什么/算点什么/协调谁先说"，才升级成 loop。** 见 §4.3 触发判定。

### 3.1 能力缺口表

| 维度 | 现状（单回合 RP 后端） | Agent 后端需要 |
|---|---|---|
| 控制流 | `prepare → stream → finalize` 单回合，一进一出 | **多步 loop**：规划 → 工具调用/生成 → 观测 → 续/停 |
| 工具 | 无工具调用执行（`<action>` 只做拆包分流） | **工具执行面** + 工具注册表（built-in + MCP upstream） |
| 角色 | 上游 LLM 的**调用方**（adapter） | 既调 LLM，又当 **MCP 客户端**消费上游工具 |
| 流式 | 流式单回合输出（immersive/`<action>`/`<state>`） | **流式多步**：中间步、tool_calls、部分输出皆可见 |
| 记忆 | 卷 / 状态 / 世界书（被动装配进 prompt） | 同一套机能升为 agent 的**长期 + 工作记忆**，loop 内主动读写 |
| 安全 | 限流 + 可选鉴权 | 上述 + **step/预算/超时闸 + 工具 allowlist + dry-run + 幂等** |

**好消息**：generation step 的执行器（`chat_pipeline` 三相流 + `adapter` 双 provider + `orchestrator` 上下文装配 + `fsm`/`xml_unpacker` 流过滤 + 持久化）**已经存在且成熟**。Agent 后端 = 在它**外面**套一个 loop controller，把"生成一步"当成 loop 的一个动作。不重写管线，是包住它。

---

## 4. 架构（演进后的数据流）

### 4.0 loop 到底有什么用？= 纯净 subagent 的编排器（直接回答）

你的直觉对：RP 大概率要 subagent 辅助。**loop 不是别的东西——loop 就是"调度这些 subagent"的那一层。** 二者不矛盾，是同一件事的两面：

- **你要的 subagent 辅助** = 把真正的 RP 书写交给一个**上下文纯净的隔离 subagent**：只装 RP 数据（卡 / 世界书 / state），没有工具说明、编程噪声、规划指令。文笔不被主上下文压扁（社区说的"死人化"）。
- **但"派一个 subagent 写一段"只是一次调用。** 真实一个回合常需要**多次纯净 subagent 调用 + 中间夹工具**：
  - 多角色：NPC A 一个纯净 subagent（只看 A 的卡），NPC B 另一个（只看 B 的卡）。
  - 写之前：取该触发的世界书条 / 掷个骰子 / 查 state。
  - 写之后：落 state、判 gating。
- **把"纯净 subagent 调用 + 中间工具"按顺序串起来 = loop。** 没 loop，一个请求只能做一次 subagent 调用，做不了"先取数据 → 派 A 写 → 派 B 写 → 落 state"这种多步。**这就是 loop 的用处。**

**唯一正确的用法（也是 RP 的命门）：loop 不是把所有东西堆进一个越滚越大的上下文**——那恰恰污染。loop 是**外层协调器**：

- 协调器自己的多步状态（调了哪些工具、轮到谁）活在**它自己的上下文**里。
- 每次派生的 RP 书写 subagent 都是**全新纯净上下文**，只看自己那份 RP 数据，看不到协调器的噪声。
- **两层物理隔离** → 比"单一 ReAct 上下文累积工具调用"更纯。这才是戒律#6 的真正落地（§4.2 把它做成可验证）。

**为什么必须 Core 原生派生 subagent**（你说"即使 subagent 也不能完全解决污染"的答案）：第三方的 subagent（如 Claude Code 的 Task）仍跑在它的 runtime 里、裹它的 system prompt / 脚手架 → subagent 上下文从一开始就不纯。**只有 Core 原生派生，subagent 上下文才 100% 由 Core 装配 = 真纯净。** 这也是编排器必须在 Core 里、不能外包的根本原因。

> 一句话：**loop = 纯净 subagent 的编排器。** 单回合 = 派一个 subagent；loop = 按需派多个 + 中间夹工具，每个 subagent 上下文都是 Core 亲手装配的纯净 RP 数据。

### 4.1 数据流

```
客户端 (HTTP/SSE) → POST /v1/agent/run   (新入口；/v1/chat/completions 保留为"单步"快捷方式)
  → AgentLoop::run  ★新增★
      │
      ├─[每步] 协调器决定：派生 RP 书写 subagent / 调工具 / 收敛结束
      │        （结构化 tool-calling 驱动；脚手架不进角色上下文，见 §4.2）
      │
      ├─[分支 A · 派生纯净 subagent] → 现有 chat_pipeline (prepare→stream→finalize)
      │                    装配 card/lorebook/preset/卷/state（全新纯净上下文，只 RP 数据）→ adapter 调 LLM
      │                    → fsm 过滤 + xml_unpacker 拆 immersive/<action>/<state>
      │                    → 持久化 + 落 state + 软提示封卷
      │
      ├─[分支 B · 工具] → ToolExecutor  ★新增★
      │                    ├─ built-in 工具：进程内调 Core 已有数据操作
      │                    │   (chat_store / volume_* / orchestrator / scene / preset_regex / png_parser)
      │                    └─ MCP upstream 工具：McpClient 调 AIRP-MCP-Server / 任意 MCP server
      │                       (stdio / HTTP；可经 Gateway 也可直连)
      │                    → 工具结果回灌入下一步上下文（observe）
      │
      └─[闸] 每步检查：step 数 / token 预算 / 成本 / 墙钟 / 取消信号 → 触顶即 finalize 退出
  → 全程 SSE 流式：规划事件 / tool_call / tool_result / 生成增量 / 终止原因
```

> **铁律（来自 `AGENT_CLIENT_ASSESSMENT.md` §5.4 / §6）：复用，不重写。** loop 调度器是唯一的新代码。它把以下现成资产**当库调用**，一行 SSE / provider / 拆包都不重写：
> `adapter::call_streaming_api_auto`（引擎分发）· `chat_pipeline::prepare_pipeline`（上下文装配）· `build_sse_stream`（流式过滤+拆包+下发）· `run_finalize`（落库+状态+封卷）。任何"自研增量 JSON 解析器 / 状态机 Actor / 正则越权 guardrail"在报告 §4 已被逐条证伪——不做。

**关键设计点：**

- **loop controller 是新的薄层**，不进 `chat_pipeline`（保持三相流纯净，复用现有 `pub(crate)` 边界）。报告反复强调：新建范围**严格限定为 loop 调度器**。
- **canonical 主线已确定**：剥离删掉 MCP 工具面后，"生成 / 状态写入"不再有 daemon-vs-MCP 两条并行路径（报告 §3 的待澄清项）——**daemon 面是唯一主线**。loop 直接建在它上面，无歧义。
- **`<action>` 通道升级**：现有 xml_unpacker 已把 `<action>` 从 immersive 流里分出来——它是工具调用的**原生种子**。首版优先用结构化 OpenAI tool_calls；`<action>` 作为不支持 structured tool-calling 的模型的回退格式。
- **Core 当 MCP 客户端**：这是相对历史的反转。Core 曾要做 MCP *server*（已随剥离移除 `/mcp/v1`）；现在做 MCP *client*，在 loop 里消费上游工具。底座那一侧由 AIRP-MCP-Server 当 server。
- **engine 复用**：`adapter.rs` 现有 `BackendEngine`（Direct / AnthropicMessages / ClaudeCodeSdk stub）就是"生成一步"的可插拔后端。`ClaudeCodeSdk` stub 可演化成"把外部 agent runtime 当一个 generation engine"，但 **loop 的所有权在 Core**，不外包。

### 4.2 上下文纯净的实现机制（把戒律#6 变成可验证的东西）

戒律#6（§1.2）不能停在口号。具体机制 = **两个物理隔离的平面**：

- **角色平面（character plane）** = 真正喂进模型、影响演绎的 RP 上下文。**只由现有 `orchestrator` 装配 RP 数据**（card / lorebook / preset / 卷 / state / 对话历史），**零 agent 脚手架**。这一面就是今天 Core 已经在产的东西，不动。
- **控制平面（control plane）** = loop 的工具定义、工具调用、工具结果。**走模型 API 的原生结构化字段**，不拼进角色平面的自然语言：
  - OpenAI：`tools` 参数 + assistant 的 `tool_calls` + `tool` role 消息。
  - Anthropic：`tools` 参数 + `tool_use` block + `tool_result` block。
  - 这些字段在协议层就**独立于 system / character prompt**——工具说明永不进角色 system prompt，工具结果永不混进角色叙事历史。

**这解释了 §7 的选型**：为什么选原生结构化 tool-calling、排斥"prompt 里塞 ReAct 指令"——后者把工具说明写进 prompt **文本**，等于把控制平面灌进角色平面 = 自我污染。结构化 tool API 天然两平面隔离，是守#6 的唯一干净路径。

**可验证（不是口号的证据）**：加一条不变式 + 测试——断言送进 `adapter` 的 character-plane prompt 字符串里**不含**任何 agent 脚手架标记（工具名/规划指令/observe 包装）。CI 跑。违反即红。

> 代价（诚实）：#6 把"靠 in-context ReAct 脚手架驱动工具"的纯文本模型挡在外面。换来的是 RP 上下文 100% 由 RP 数据决定。对一个为"纯净"而存在的后端，这个取舍值得——但要写进文档让用户知情（§7 选型约束）。

### 4.3 单回合 vs loop：触发判定（loop 是单回合的严格超集）

不是每个请求都进 loop。默认仍是今天的单回合，**loop 是退化情形 = 单回合**，向后兼容彻底：

| 条件 | 走哪条 |
|---|---|
| 请求无 `tools` 授权 且 `max_steps` 缺省/=1 | **单回合**（= 现有 `/v1/chat/completions`，纯 RP 生成，零工具，零 loop 开销） |
| 请求带 `max_steps>1` 或带 `tools` 授权 | **进 loop**（多步） |
| loop 中模型某步吐出 `tool_call` | 执行工具 → 回灌 → 续步 |
| loop 中模型某步直接出叙事且无 tool_call | 视为收敛信号，该步 finalize 后结束 |

即：`/v1/chat/completions` ≡ `max_steps=1` 的 `/v1/agent/run`。老客户端零改动继续用单回合；要 agentic 的显式开 `max_steps`/`tools`。

### 4.4 生态拓扑（大概流程，非最终链条）

用户给的 roleplay 大致走向（**只是大概流程，不是定死的最终链条**，会随骨架落地收敛）：

```
AIRP-State-Protocol  ←  AIRP-Gateway  ←  AIRP-MCP-Server  ←  AIRP-Core
     (UI·渲染)            (协议桥)          (数据底座)        (大脑·本仓·跑 loop)
```

**唯一稳定的一条**：**Core 在最底当大脑**——唯一调 LLM、跑 loop、派生纯净 subagent。其余各层顺序、以及 Core 的具体接入方式，**现在都不定**，等骨架跑起来、需求清楚了自然收敛。

> 备忘（不拍板，仅记录将来可能性）：Core 接进链大致有几种接法——**A** 当前门（客户端直打 Core）／**B** 汇合在 MCP-Server（Core 当其客户端、产物写回、复用订阅推送）／**C** 挂 Gateway 背后。取舍主要在"流式顺不顺 vs 解耦干不干净"。真要联调时再选，不前置。

---

## 5. 里程碑（M_AGENT 系列）

沿用仓库 `M_XXX` 命名。排序成**增量可交付**——每个 milestone 自身可跑、可测、可验收。

### M_AGENT-0 · 定位落档 + 戒律改写
- 本计划书入库；README / AGENTS.md 同步：戒律#2 从「不跑 loop」→「**有界** loop」，补 §2.1 六条 Agent 戒律。
- 生态定位表更新：Core 标注为 "Agent runtime 层"，底座纯度指向 AIRP-MCP-Server。
- **验收**：文档自洽，无"既说不跑 loop 又要跑 loop"的内部矛盾。

### M_AGENT-1 · Loop 骨架（最小编排器）
- 新 `agent/` 模块 + `AgentLoop::run`：协调器循环——每步在"派生纯净 subagent 生成 / 调一个工具 / 收敛结束"间选择；有界步数、可取消、带预算。
- "派生纯净 subagent" = 用现有 `chat_pipeline` 装配一份全新纯净上下文跑一次生成（§4.0）；协调器自己的多步状态不进 subagent 上下文。
- 新入口 `POST /v1/agent/run`（SSE）。`/v1/chat/completions` 保留 = "loop 上限 = 1 步"的快捷方式（向后兼容）。
- **验收**：能跑通"协调器调一个 mock 工具 → 拿结果 → 派生纯净 subagent 续写 → 停"的最小闭环；step cap 触顶能停；中途取消能收敛；断言 subagent 上下文不含协调器噪声。

### M_AGENT-2 · 工具执行面 + built-in 工具
- `ToolExecutor` + 工具注册表（trait + 元数据：readonly/mutate/destructive/append）。
- 把 Core 已有进程内数据操作包成 built-in 工具（角色卡 / 状态 / 卷 / 世界书 / 会话 / 场景读写）。
- `<action>` → 工具调用协议的正式映射（结构化 tool_calls 优先，`<action>` 回退）。
- **报告 §5.2「防越权」取舍**：若需确定性"不替用户发言/行动"，**不采用**设计稿的"正则匹配第二人称即 abort+重生"（误杀正常台词、热路径成本翻倍）；优先利用 `<action>` 的结构分离做约束，或明确划归 preset/prompt 层并写进文档。本计划取后者 + `<action>` 约束，不在 Core 做生成级 auto-retry。
- **验收**：零外部进程，loop 能调 built-in 工具读写真实 `data/`；破坏性工具默认 dry-run。

### M_AGENT-3 · MCP 客户端 + 上游工具
- `McpClient`（stdio + HTTP）：Core 作为 MCP client 消费上游（AIRP-MCP-Server 或任意 MCP server；可直连或经 Gateway）。
- 工具注册表合并 built-in + MCP upstream；命名空间隔离避免撞名。
- **验收**：配一个 MCP upstream，loop 能在同一轮里混用 built-in 与 upstream 工具；upstream 崩溃/超时被隔离成错误不拖垮 loop。

### M_AGENT-4 · 多步流式
- SSE 事件协议：`plan` / `tool_call` / `tool_result` / `delta`（生成增量）/ `done`（含 stop_reason）。
- OpenAI 兼容面：在 `/v1/chat/completions` 暴露标准 `tool_calls` 流，让既有 OpenAI 客户端无改动消费。
- **验收**：客户端能实时看到每一步；OpenAI SDK 直连不报错。

### M_AGENT-5 · 安全闸（偿还 server-side loop 的风险）
- step cap / token 预算 / 成本预算 / 墙钟超时（复用 `quota.rs` 计量基建）。
- 工具 allowlist；破坏性工具确认流；幂等键去重；**同角色/quota root 并发写串行化**（顺手修现有"无文件锁"已知限制）。
- **验收**：构造"失控 loop"用例，每道闸都能独立兜停；并发写同角色不再漂移/互相覆盖。

### M_AGENT-6 · Loop 记忆接线
- 卷 / 状态 / 世界书升为 agent 长期+工作记忆：loop 内主动 `apply_lorebook` 注入、`update_state`、按软提示阈值决策封卷（**不自动**——server 给阈值信号，loop 拍板，守 AUDIT-6/7）。
- **顺手补报告 §5.1 真实缺口**：`state/schema.json` 的 min/max 目前**只用于 prompt 渲染与前端进度条，写入路径不钳制**——模型可写出 `affection: 999`。在 `persist_live_state`（落 `<state>`）与状态写入路径落盘前按 schema clamp 数值。小改动、高价值，天然属 Core 职责（**待确认数值强制是否为预期目标**）。
- **验收**：长 run 中状态/卷正确演进；封卷是 loop 的显式决策步而非隐式副作用；越界数值被 clamp。

### M_AGENT-7 · 多 agent / scene 编排（预留）
- 复用现有 `scene.rs` 多角色场景：每角色一个 agent，共享/隔离记忆与世界书。
- **验收**：待 M_AGENT-1~6 稳定后再排。

---

## 6. 与现有工程不变式的衔接

转向**不破坏**仓库现有不变式，反而复用：

- **`JoinSet` 收束** → loop 子任务（工具调用 / finalize）的取消与收敛模型现成。
- **热路径无 `Arc<Mutex>`、`RwLock<MutableConfig>`** → loop controller 沿用，不引锁竞争。
- **JSONL append O(1)** → agent run 的步骤轨迹（trace）同样行式追加。
- **newtype ID 反序列化即校验** → 工具入参里的 `character_id` 等照旧免重复校验。
- **`pub(crate)` 内部模块** → `agent/` 复用内部能力但不破坏对外 API 面。
- **`aho-corasick` / `quota.rs`** → 世界书扫描加速、预算计量直接复用。

---

## 7. 推荐选型（减少开放问题）

给出推荐，避免每条都变成待拍板项：

| 决策 | 推荐 | 理由 |
|---|---|---|
| loop 协议 | **复用 OpenAI tool-calling wire format** 为主，`<action>` XML 为回退 | Core 已 OpenAI 兼容；客户端零学习成本；守"协议标准"护城河 |
| 规划策略 | **首版 ReAct（有界工具循环）**，plan-and-execute 留后 | 最简、最稳、最易验收；先证明闭环再上复杂规划 |
| 上游工具来源 | **built-in 默认 + MCP upstream 可选** | 守乐高独立（standalone 可跑）；upstream 是增强不是前置 |
| State-Protocol 输出 | **不进 Core**；留给 Gateway 的 `agentbus` feature 适配 | 守戒律「Core 不做 UI」；Core 只吐 OpenAI 兼容 + 结构化步骤事件，谁要声明式 UI 由 Gateway 翻译 |
| 外部 agent runtime | 当**可选 generation engine**（沿 ClaudeCodeSdk stub），**不**当 loop owner | 第三方 runtime 自带 system prompt / 脚手架 / 思考 harness 关不掉 → **污染 RP 提示词**，subagent 隔离也不根除（§1.2）。loop 所有权必须在 Core，纯净度才可控 |
| agent 脚手架位置 | 工具 schema / 规划指令 / 观测回灌走**结构化通道**，不进角色 system prompt | 戒律#6 上下文纯净：Core 自己不能当污染源（§1.2 反噬自检） |
| 模型选型约束 | 偏向有**原生结构化工具调用**的模型（OpenAI / Anthropic tool API）；不支持的纯文本模型只能走单回合或 `<action>` 回退，**不享 loop 工具的纯净度** | §4.2：in-prompt ReAct 脚手架会污染角色平面；结构化 tool API 是守#6 唯一干净路径。这是纯净度的代价，需写进文档让用户知情 |
| 入口形态 | `/v1/agent/run`（多步）为主；`/v1/chat/completions` ≡ `max_steps=1` 退化情形 | §4.3：loop 是单回合严格超集，老客户端零改动 |

---

## 8. 风险与诚实缺口

- **server-side loop 重新引入它当初被禁的风险**：token 烧穿 / 失控 / 隐式副作用。→ 用 M_AGENT-5 五道闸偿还；闸不齐不算 done。
- **`estimate_tokens` ±30% 偏差**影响 token 预算精度。→ 预算阈值留安全边际；或在 loop 计量处接真实 tokenizer（属 M_AGENT-5 子决策）。
- **并发写无文件锁**（现有已知限制）：单 agent 顺序写不触发，多 agent（M_AGENT-7）会。→ M_AGENT-5 的 per-character 串行化是前置。
- **ClaudeCodeSdk engine 仍是 stub**（报告 §5.3，`adapter.rs` 的 `"ClaudeCodeSdk engine not yet implemented"`）：若以 Claude Code SDK 当后端是路线之一，这是**唯一需新写的接入点**；作为可选 engine 落地前不承诺。
- **定位转向需同步多处**：README / AGENTS / 兄弟仓交叉引用 / 记忆库 doctrine。→ 记忆库**已更新**（pivot 记忆 + 标注旧两条 + 索引）；README / AGENTS 戒律#2 待 M_AGENT-0 改（推荐等 M_AGENT-1 骨架可跑，§10）；兄弟仓三处 README 的"Core = 自调 LLM 流式 RP 后端"表述将来需同步成"Agent 后端"（未排期）。

---

## 9. 地平线（不排期，仅记录）

- **四项目收敛成发行版**：把 Core / MCP-Server / Gateway / State-Protocol 在一个新仓拼成一键全栈。**当前只是想法**（用户 2026-06-30 明确"现阶段不考虑"）。若将来启动，倾向"bundle 发行版"（submodule/workspace 收纳 + 各自仓仍独立上游开发），而非物理 monorepo 吞并——以守每块的乐高独立。本计划不为它落任何 milestone。
- **AIRP-RAG（语义检索）**：§3.0 用例 2/3 天然指向检索。但用户 2026-06-30 明确"RAG 只是想法，除非真的需要否则暂不考虑"。**暂缓**——长程记忆先用 volume + 简单检索顶着；真撞到瓶颈再起。本地 `D:\AIRP-RAG` 仍空目录、未起仓。

---

## 10. 开放问题（待用户拍板）

**本轮已决（不再开放）：**
- ✅ canonical 主线 = daemon 面（MCP 面已删，无歧义）；daemon 面 = 一等生产路径。
- ✅ `AGENT_CLIENT_ASSESSMENT.md` 已从 git 历史取回入库（仓根，已暂存）。
- ✅ 入口形态 = `/v1/agent/run` 为主，`/v1/chat/completions` ≡ `max_steps=1`（§4.3 / §7）。
- ✅ `<state>` schema clamp（M_AGENT-6）、防越权划归 preset/prompt + `<action>`（M_AGENT-2）、ClaudeCodeSdk 列可选 engine（§8）。
- ✅ 记忆库 doctrine 已更新（新增 [[project_core_agent_backend_pivot]] + 标注旧两条 + 索引）。
- 🔻 README/AGENTS 戒律#2 改写：**推荐等 M_AGENT-1 骨架可跑后再一并改**（避免文档先于代码"承诺"loop）。除非你要现在就改。

**仍真正开放（需你拍板）：**

1. **纯净度代价是否接受**：戒律#6 实际把"纯文本 in-prompt-ReAct"模型挡在 loop 工具之外（§4.2 / §7）。你接受这个取舍（纯净优先），还是要留一条"污染模式"开关兼容那类模型？
2. **首批 built-in 工具范围**：M_AGENT-2 先做哪几个？建议从 §3.0 用例 1（多角色纯净轮转）+ 5（gating 推进）切入——纯复用现有 `scene` / `gating`，零外部依赖，且用例 1 直接展示纯净度卖点。
3. **本文件命名 / 位置**：现 `AGENT_BACKEND_PLAN.md`（仓根，未入库）。保持仓根 / 移 `docs/` / 仿 `REFACTOR_PLAN.md` 设本地 gitignore 工作文档？

> 拓扑链（§4.4）是大概流程、非最终，Core 接入方式不前置决定——故不列开放项。
