//! MCP-6: Streamable HTTP transport。
//!
//! 将 [`StreamableHttpService`] 挂载到 axum 的 `/mcp/v1` 路径。
//! 单端点处理 POST（JSON-RPC 请求/流）与 GET（SSE 监听），
//! 符合 MCP 2025-03-26 Streamable HTTP 规范。
//!
//! 默认 `allowed_hosts` = `["localhost", "127.0.0.1", "::1"]`（rmcp 内置，防 DNS rebinding）。
//! daemon 仅绑定 127.0.0.1，无需额外配置。
//!
//! MCP-7：`state_subs` 与 `DaemonState` 共享，使 chat-pipeline finalizer 可推送
//! `notifications/resources/updated` 到订阅了 `airp://characters/{id}/state/live` 的客户端。

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::path::PathBuf;
use std::sync::Arc;

use super::{AirpMcpServer, StateSubs};

/// 构造 MCP HTTP transport 路由器，挂载于 `/mcp/v1`。
///
/// `state_subs` 须与 `DaemonState::state_subs` 共享同一 [`Arc`]，
/// 使 chat-pipeline finalizer 与 MCP 订阅注册表保持同步。
///
/// 泛型参数 `S`：允许与任何 axum 主路由器状态合并（本路由不提取 axum State）。
pub fn mcp_http_router<S>(data_root: PathBuf, state_subs: StateSubs) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let service = StreamableHttpService::new(
        move || {
            Ok(AirpMcpServer::new_with_subs(
                data_root.clone(),
                state_subs.clone(),
            ))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    axum::Router::new().nest_service("/mcp/v1", service)
}
