# AIRP-Core 公网部署指南 (DX-9)

## 快速启动（Docker Compose）

```bash
# 1. 克隆仓库
git clone <repo-url> airp && cd airp

# 2. 配置环境变量
cp .env.example .env
# 编辑 .env 填写 AIRP_API_KEY 等

# 3. 一键启动
docker compose up -d

# 4. 验证
curl http://localhost:8000/v1/characters
```

## 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `AIRP_ENDPOINT` | OpenAI `/v1/chat/completions` | 上游 LLM 端点 |
| `AIRP_API_KEY` | — | 上游 LLM API Key |
| `AIRP_MODEL` | `gpt-4o` | 默认模型 |
| `AIRP_ACCESS_KEY` | 空（不鉴权）| daemon 访问 Key；设置后 `/v1/*` 要求 `Authorization: Bearer <key>` |
| `AIRP_LOG` | `info` | 日志级别（error/warn/info/debug/trace） |
| `AIRP_DATA_DIR` | `/app/data` | 数据根目录（Docker 内路径） |

## 数据持久化

`./data/` 目录挂载到容器 `/app/data/`，包含：

```
data/
├── characters/      # 角色卡目录
├── presets/         # 预设目录
├── settings.json    # 运行时可热重载配置
└── sessions/        # 会话数据（若使用 session ID）
```

## Caddy 反向代理 + HTTPS

`/etc/caddy/Caddyfile`:

```caddy
your-domain.example.com {
    reverse_proxy localhost:8000

    # 可选：CORS 头（若前端在其他域）
    header Access-Control-Allow-Origin *
    header Access-Control-Allow-Methods "GET, POST, OPTIONS"
    header Access-Control-Allow-Headers "Authorization, Content-Type"
}
```

启动 Caddy：

```bash
caddy run --config /etc/caddy/Caddyfile
```

Caddy 自动申请并续期 Let's Encrypt 证书。

## API Key 鉴权（公网推荐）

设置 `AIRP_ACCESS_KEY` 后，所有 `/v1/*` **和 `/mcp/v1`** 请求需携带：

```
Authorization: Bearer <your-key>
```

（AUDIT-1 起 `/mcp/v1` 也纳入鉴权中间件。）仅 Web UI（`/`）、`/version`、`/health` 三个公开端点不要求鉴权 —— 它们只返回静态/构建信息，不触碰用户数据。

鉴权 key 比较使用常量时间算法（A2-5），不泄露逐字节计时旁路。

## MCP 客户端连接（stdio 模式）

Claude Code / Cursor 本地连接，无需 daemon 或 Docker：

```json
{
  "mcpServers": {
    "airp": {
      "command": "airp-core",
      "args": ["mcp"],
      "env": {
        "AIRP_DATA_DIR": "/path/to/your/data"
      }
    }
  }
}
```

## MCP HTTP 连接（公网模式）

在 Claude Desktop / 支持 HTTP MCP 的客户端中：

```json
{
  "mcpServers": {
    "airp-remote": {
      "url": "https://your-domain.example.com/mcp/v1",
      "transport": "http"
    }
  }
}
```

## 安全建议

1. **公网部署必须设置 `AIRP_ACCESS_KEY`** — 否则任何人可调用 LLM（计费风险）。`/v1/*` 与 `/mcp/v1` 均受其保护。
2. `data/settings.json` 含 API Key 明文 — 确保数据目录访问权限（chmod 700）
3. 建议定期轮换 `AIRP_ACCESS_KEY`（通过 `POST /v1/settings` 热更新，无需重启）
4. 全端点（除 chat 外的 import/sync/scene/mcp）均挂限流：10 req/s + burst 20 per-IP（A2-7）

### A2-3：默认本地 CORS 风险（必读）

daemon 默认 **不鉴权**（`AIRP_ACCESS_KEY` 为空）且 CORS `Access-Control-Allow-Origin: *`。监听地址硬编码 `127.0.0.1`，但这**不足以**防御浏览器侧攻击：

- **本地 CSRF / DNS-rebind**：用户浏览器访问任意恶意网页时，该页面的 JS 可向 `http://127.0.0.1:8000` 发跨域请求，驱动本地 daemon（导入角色卡、跑对话烧 LLM 额度、读 `/v1/characters`）。`Allow-Origin: *` 放行了这些跨域读取。

**缓解（按强度）：**
1. 本机自用、无浏览器接触 daemon → 现状可接受。
2. 任何浏览器可能访问该机器 → **设 `AIRP_ACCESS_KEY`**。带凭据的跨域请求被 CORS 预检挡下，且无 key 直接 401。
3. 多用户 / 团队 → 反代层（Caddy）加 IP 白名单，并收紧 `Allow-Origin` 到可信前端域。

> 默认值优先开箱即用（本地单用户场景）。生产 / 共享环境必须显式加固。未来可考虑默认收紧 CORS（需用户拍板，属设计决策）。

### A2-4：并发写约束（已知限制）

AIRP **假设单写者**（一个用户、串行请求）。同一角色 / 同一 quota root 的**并发写**目前无文件锁保护：

- 同角色多并发 `append_message` / chat 完成 → 内存 ChatLog 副本可能漂移、`chat_log_meta.json` 互相覆盖、滚动截断丢追加。
- quota `load → check → increment → save` 是 TOCTOU 窗口，高并发下计数可能少算。

**现状定位**：本地单用户 RP 场景下不触发。多 Agent 并行写同一角色属于未支持用法。需要并发安全时，由上层（Agent 编排器）串行化对同一角色的写，或等未来引入 per-character 文件锁（属设计决策，未排期）。
