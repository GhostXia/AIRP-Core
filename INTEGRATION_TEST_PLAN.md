# AIRP-Core 集成测试执行计划

> 创建日期：2026-05-22  
> 完成日期：2026-05-22  
> 目标：模拟用户完整工作流 — 导入角色卡 → 导入预设 → 五轮连续对话  
> 状态图例：⬜ 待执行 / 🟨 进行中 / ✅ 完成 / ❌ 失败 / 🔧 需修复

---

## 测试资产

| 文件 | 类型 | 说明 |
|---|---|---|
| `test/大乾风华录 Ver1.73.png` | 角色卡 PNG | Tavern V2 格式，嵌入角色 JSON 于 tEXt 块 |
| `test/【LENI】1.5.json` | 预设 JSON | LENI 调酒师预设，包含身份锚定 + 思维加强 prompt |

---

## 阶段一：环境准备

| # | 步骤 | 预期结果 | 状态 |
|---|---|---|---|
| 1.1 | 验证/编译 binary | `target/debug/airp-core.exe` 最新 | ✅ |
| 1.2 | 导入角色卡 — 复制 PNG 到 `data/characters/大乾风华录/card.png` | 文件就位 | ✅ |
| 1.3 | 导入预设 — 复制 JSON 到 `data/presets/LENI.json` | 文件就位 | ✅ |
| 1.4 | 配置 API 凭据写入 `data/settings.json` | endpoint + api_key 已写入 | ✅ |
| 1.5 | **Bug 修复：TavernPrompt serde 字段缺失** — 见 P-01/P-02 | card.rs 已修复 | ✅ |
| 1.6 | 重新编译 binary | 含修复的新 binary（8.61s） | ✅ |

---

## 阶段二：Daemon 启动

| # | 步骤 | 预期结果 | 状态 |
|---|---|---|---|
| 2.1 | 启动 daemon（端口 8000） | 正常启动 | ✅ |
| 2.2 | 验证 `GET /v1/characters` | 返回 `["大乾风华录"]` | ✅ |
| 2.3 | 验证 `GET /v1/presets` | 返回 `["Izumi_0407_optimized","LENI"]` | ✅ |

---

## 阶段三：五轮连续对话

角色：`大乾风华录`（含角色卡）；预设：`LENI`；用户名：`旅人`；模型：`zai-org/glm-5`

| 轮次 | 用户消息 | 状态 | AI 输出摘要 |
|---|---|---|---|
| R1 | 你好，Leni。能给我调一杯……能让人忘掉过去的酒吗？ | ✅ | 调制"忘川之沫"，苦涩艾草入口，蜂蜜入喉，烛光氛围营造细腻 |
| R2 | 这家酒馆是什么地方？你怎么来到这里的？ | ✅ | 揭示酒馆"世界尽头的驿站"身份，Leni 自述误入成主的来历 |
| R3 | 蒙面黑衣人持刃闯入，Leni 小心！ | ✅ | Leni 镇定应对，取出"赤焰之息"，保护旅人，展示江湖过往 |
| R4 | 他走了，那人是谁？为什么来找你？ | ✅ | Leni 坦承旧债，说明酒馆的保护规则，情感深化 |
| R5 | 再调一杯——能让人有勇气面对未来的酒 | ✅ | 调制"晨曦之誓"（昨夜灰烬+此刻烈火+未知星光），与 R1"忘川之沫"形成完整呼应 |

**剧情连贯性**：✅ 每轮引用上文（杯子、闯入者、过往债务），角色性格一致，五轮构成完整弧光。

---

## 阶段四：验收

| # | 检查项 | 实际结果 | 状态 |
|---|---|---|---|
| 4.1 | `POST /v1/chat/history` | 18 条消息（含调试轮次）；最后 10 条为正式五轮 user+assistant 对 | ✅ |
| 4.2 | `data/characters/大乾风华录/session/` | current.md 8377 字节，内容正确；turn_counter = 12 | ✅ |
| 4.3 | 系统 prompt 含预设 | P-01/P-02 修复后预设正常解析（LENI 调酒师人格已注入） | ✅ |

---

## 已知问题与解决方案

### P-01 — TavernPrompt.enabled 非 Optional ✅ 已修复

**发现**：`orchestrator/card.rs` 中 `TavernPrompt.enabled: bool` 为必填字段。  
LENI 预设的所有 prompts 均未携带 `"enabled"` 键 → serde 反序列化失败 →  
`assemble_preset_prompts` 静默返回空字符串 → 预设 system prompt 全部丢失。

**SillyTavern 规范**：缺少 `enabled` 字段意味着该 prompt **默认启用**（`true`）。

**修复**（`src/orchestrator/card.rs`）：
```rust
/// SillyTavern 规范：缺少 `enabled` 字段时视为 `true`（默认启用）。
#[serde(default = "default_true")]
pub enabled: bool,

fn default_true() -> bool { true }
```

---

### P-02 — TavernPrompt.role 非 Optional ✅ 已修复

**发现**：`TavernPrompt.role: String` 为必填字段。  
LENI 预设中 marker 类型 prompt（`dialogueExamples`、`chatHistory` 等）不含 `"role"` 键 → 整个 prompts 数组解析失败。

**修复**（`src/orchestrator/card.rs`）：
```rust
/// marker 类型 prompt 可能不含 role 字段。
#[serde(default)]
pub role: String,
```

---

### P-03 — API Endpoint 格式 ✅ 已解决

**发现**：代码将 `endpoint` 字段作为完整 URL 直接 POST，不追加路径。  
用户提供 `https://nano-gpt.com/api/v1`，需手动追加 `/chat/completions`。

**解决**：`data/settings.json` 写入完整路径 `https://nano-gpt.com/api/v1/chat/completions`。

---

### P-04 — data/settings.json API 凭据未加载 🔧 待查

**发现**：启动 daemon 后，第一轮请求返回 401（无 API Key）。  
settings.json 内容正确，但 daemon 使用了 config.json 默认值（endpoint = api.openai.com，api_key = null）。

**影响**：必须在每次 HTTP 请求体中显式携带 `endpoint` / `api_key` 字段，绕过 settings.json 加载。  
（此行为等同于用户通过 Web UI "GATEWAY CONFIG" 面板填写凭据，功能上等价。）

**疑似原因**：daemon 进程工作目录确认为 `D:\AIRPCLI`，settings.json 文件存在且内容正确。可能是 `Start-Process` 不支持 `-WorkingDirectory` 正确继承，或存在 VolumeConfig 字段引起的解析异常。  
**待查**：在 `merge_data_settings` 后添加 `tracing::info!` 打印实际加载值。

---

### P-05 — API 模型订阅限制 ✅ 已解决

**发现**：设置中默认模型 `gpt-4o` 以及 `glm-4-plus` 返回 403 `model_not_included`。  
nano-gpt.com 账户订阅不含这些模型。

**解决**：通过 `GET /v1/models` 探测，确认 `zai-org/glm-5`（GLM 5）可用，切换为此模型。

---

### P-06 — PowerShell 空数组序列化错误 ✅ 已解决

**发现**：PowerShell `@{}` hashtable 中 `messages_history = @()` 经 `ConvertTo-Json` 后序列化为 map `{}`，  
而非空数组 `[]`，导致 serde 报错 `invalid type: map, expected a sequence`。

**解决**：当历史为空时不传 `messages_history` 字段（服务端 `Option<Vec<_>>` 默认为空列表）；  
后续轮次使用 `[System.Collections.ArrayList]` + `.ToArray()` 确保正确数组序列化。

---

### P-07 — auto_converter 预设转换失败（轻微警告）📝 记录

**发现**：daemon 启动时出现警告：  
```
WARN Failed to convert preset JSON path="data\presets\LENI.json" err=Failed to parse Preset JSON: missing field `value` at line 39 column 9
```  
`auto_converter` 尝试将预设 JSON 转换为 Markdown，但 LENI 预设格式不符合 `auto_converter` 的 legacy JSON 格式要求。

**影响**：不影响功能。预设 JSON 原样使用，转换被跳过。

---

### P-08 — 预设顶层参数未应用（设计局限）📝 记录

**发现**：LENI 预设顶层含 `temperature: 1.19`、`openai_max_tokens: 50000` 等参数。  
当前 `TavernPreset` 结构体只解析 `prompts` 字段，顶层参数被忽略。  
实际使用 `data/settings.json` 的模型默认参数。

**状态**：设计局限，需扩展 `TavernPreset` + `prepare_pipeline` 以支持预设级参数覆盖。

---

## 执行日志

### 1.1 Binary 编译
- **时间**：2026-05-22 16:30  
- **结果**：✅ `cargo build` 成功，耗时 8.61s（增量编译，仅 card.rs 变更）

### 1.2 导入角色卡
- **操作**：`Copy-Item "test\大乾风华录 Ver1.73.png" "data\characters\大乾风华录\card.png"`  
- **结果**：✅ 文件就位，`data/characters/大乾风华录/` 目录自动创建

### 1.3 导入预设
- **操作**：`Copy-Item "test\【LENI】1.5.json" "data\presets\LENI.json"`  
- **结果**：✅ 文件就位

### 1.4 配置 API
- **操作**：写入 `data/settings.json`：endpoint = nano-gpt /chat/completions，api_key = sk-nano-xxx，model = glm-4-flash  
- **结果**：✅ 文件写入正确，但 daemon 启动后未能自动加载（见 P-04）

### 1.5 修复 P-01/P-02
- **修改文件**：`src/orchestrator/card.rs`  
- **改动**：`enabled: bool` 加 `#[serde(default = "default_true")]`；`role: String` 加 `#[serde(default)]`  
- **结果**：✅ LENI 预设所有 prompts 正常反序列化，身份锚定 prompt 注入 system prompt

### 1.6 重新编译
- **结果**：✅ `cargo build` 增量编译成功，0 warning

### 2.1–2.3 Daemon 启动与验证
- **启动命令**：`Start-Process ... -WorkingDirectory "D:\AIRPCLI" -WindowStyle Hidden`  
- **characters 列表**：`["大乾风华录"]` ✅  
- **presets 列表**：`["Izumi_0407_optimized","LENI"]` ✅  
- **P-04 发现**：API key 未从 settings.json 加载，首轮 401。改为请求体携带凭据

### 模型探测
- 通过 `GET https://nano-gpt.com/api/v1/models` 列出可用模型
- gpt-4o：403 model_not_included
- glm-4-plus：403 model_not_included  
- **`zai-org/glm-5`：✅ 可用**（GLM-5 正式模型）

### 3.x 五轮对话（`zai-org/glm-5`，完成时间 2026-05-22 16:53）

**R1**：旅人初入酒馆，Leni 调制"忘川之沫"（苦艾草→蜂蜜，忘忧而不失记忆的哲思）  
**R2**：Leni 揭示酒馆真身"世界尽头的驿站"，自述误入成主的来历  
**R3**：黑衣人持刃闯入，Leni 镇定取出"赤焰之息"保护旅人，展示江湖过往  
**R4**：Leni 坦承"旧债"身份，说明酒馆烛火庇护规则，情感层次加深  
**R5**：旅人请求"面对未来的酒"，Leni 调制"晨曦之誓"，与 R1 忘川之沫形成对仗，弧光完整  

**剧情一致性**：五轮均引用前文细节（杯子、气味、烛火、赤焰之息瓶），角色性格稳定，LENI 预设身份锚定生效。

### 4.x 验收
- **chat history**：18 条消息，最后 10 条为正式五轮对话 ✅  
- **current.md**：8377 字节，内容为完整对话记录 ✅  
- **turn_counter**：12（含调试轮次） ✅  
- **index.md**：已创建，初始状态 ✅

---

## 总结

| 分类 | 结论 |
|---|---|
| 核心流程（导入→对话→持久化） | **全部正常** |
| 预设系统 | **修复后正常**（P-01/P-02 为实际 Bug，已在 `card.rs` 修复） |
| 卷系统 | **正常**（current.md 追加写入，turn_counter 递增） |
| settings.json 加载 | **待查**（P-04，功能上可用请求体绕过） |
| 预设顶层参数 | **设计局限**（P-08，temperature 等未读取） |

---

## 测试后反思：主要缺口

> 2026-05-22 测试完成后逐条复盘

### R-01 — 无角色卡导入 API 🔴 功能缺失

**现状**：不存在 `POST /v1/characters/import` 端点。  
测试中只能手动执行 `Copy-Item` 将 PNG 放入 `data/characters/{id}/` 目录。  
真实用户无法通过任何 API 或 Web UI 完成"导入"操作。

**期望行为**：上传 PNG / JSON → 自动解析 → 创建角色文件夹 + 存储卡文件。

**影响**：整个"导入角色卡"步骤是伪工作流，实际由开发者手动模拟。

---

### R-02 — 角色文件夹结构不透明 🟡 UX 问题

**现状**：
```
data/characters/{id}/
├── card.png          ← 角色卡散放根层，无专用子目录
├── chat.json         ← 聊天记录散放根层，无 chat_logs/ 子目录
├── checkpoints.md
├── timeline.md
├── worldbooks/       ← 世界书 ✅
├── memory/           ← 记忆 ✅
└── session/          ← 卷系统 ✅
```
- 角色卡文件（`card.png`/`card.json`）无专用子目录
- 聊天记录（`chat.json`）无专用子目录
- 无 `personas/` 目录，人设依赖卡内字段，扩展性差

**影响**：文件夹语义对用户不透明，手动管理时易混淆。

---

### R-03 — 分卷系统封卷链路未实际触发 🟠 测试覆盖漏洞

**现状**：
- `current.md` 追加写入正常（8377 字节）✅
- `turn_counter.txt` 递增正常（12）✅
- **封卷从未触发**：估算 token 数 ≈ 8377 / 4 = 2094，低于 `soft_threshold = 2500`

**未验证的链路**：
- soft 压力提示注入 system prompt
- hard 阈值强制封卷
- 封卷 API 调用（seal prompt → LLM → vol_001.md）
- `index.md` 更新
- 跨卷关键词匹配注入

**建议**：专项测试需降低阈值（如 `soft=100, hard=200`）或构造足够长的对话触发封卷。

---

### R-04 — 聊天历史不自动注入上下文 🔴 设计缺陷

**现状**：`prepare_pipeline` 只使用客户端显式传入的 `messages_history`：

```rust
let mut list = payload.messages_history.clone().unwrap_or_default();
// 若客户端不传 → 空列表 → 上下文断裂
```

daemon 不会从已持久化的 `chat.json` 中自动读取历史消息并注入。

本次测试中上下文连贯依赖手工在 PowerShell 脚本里累积 `$hist` 并逐轮传递——这是测试脚本代劳，而非系统能力。

**影响**：
- 真实前端若不传 history，每轮对话零记忆
- 持久化的聊天记录形同虚设（只能查看，不能驱动上下文）
- 与用户预期（"daemon 记住对话"）严重偏差

**期望行为**：`prepare_pipeline` 应在 `messages_history` 为 None 时，从 `ChatLog` 加载最近 N 条消息作为默认上下文。

---

### R-05 — 预设/角色卡即时切换 ✅ 正常

- 预设修改后下一请求即生效（每次从磁盘读取，无缓存）
- `preset_id` / `character_id` 均为请求级参数，无需重启 daemon
- 多角色/多预设并发使用无障碍

---

## 缺口优先级汇总

| ID | 问题 | 严重度 | 类型 |
|---|---|---|---|
| R-04 | chat history 不自动注入上下文 | ✅ 已修复 | 设计缺陷 |
| R-01 | 无角色卡导入 API | ✅ 已修复 | 功能缺失 |
| R-03 | 封卷链路未经实际验证 | 🟠 中 | 测试覆盖 |
| R-02 | 角色文件夹结构不透明 | 🟡 低 | UX 问题 |
