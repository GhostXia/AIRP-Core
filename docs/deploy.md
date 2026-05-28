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

设置 `AIRP_ACCESS_KEY` 后，所有 `/v1/*` 请求需携带：

```
Authorization: Bearer <your-key>
```

MCP HTTP 端点（`/mcp/v1`）和 Web UI（`/`）不受鉴权影响。

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

1. **公网部署必须设置 `AIRP_ACCESS_KEY`** — 否则任何人可调用 LLM（计费风险）
2. MCP `/mcp/v1` 端点目前无独立鉴权 — 建议公网部署时在 Caddy 层加 IP 白名单或 Basic Auth
3. `data/settings.json` 含 API Key 明文 — 确保数据目录访问权限（chmod 700）
4. 建议定期轮换 `AIRP_ACCESS_KEY`（通过 `POST /v1/settings` 热更新，无需重启）
