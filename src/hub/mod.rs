//! # M_FEDERATION FED-3：进程边界插件 Hub
//!
//! Hub spawn 外部插件**子进程**，通过其 stdin/stdout 收发换行分隔的
//! JSON-RPC 2.0。Core **绝不**把插件代码载入自身地址空间：插件崩溃 / 卡死
//! 被隔离在子进程内，不污染 Core。插件**只依赖线协议**（[`WireRequest`] /
//! [`WireResponse`]），永不 link Hub 的实现语言 / ABI —— 故 Hub 将来可整体
//! 换语言移植，第三方插件零改动继续兼容。
//!
//! ## 红线（见 `REFACTOR_PLAN.md` M_FEDERATION）
//! - ⛔ 不在 Hub 进程内执行第三方代码（仅 process boundary）。
//! - ⛔ Hub 不自调度、无 server-side Agent loop（[`invoke`] 是单次往返）。
//! - ⛔ 插件不得依赖 Hub 实现语言 / ABI，只许依赖稳定线协议。
//!
//! ## 启动元数据 vs 零 schema
//! 可执行插件在 `data/plugins/{name}/hub.json` 声明 `command` / `args`。
//! 这是**启动元数据**（Hub 需知道 spawn 什么），与 M_PLUGIN_DATA 的「零
//! schema 数据语义」不矛盾 —— Hub 仍不解析插件交换的业务数据语义。
//! 无 `hub.json` 的插件目录 = 纯数据插件（M_PLUGIN_DATA），Hub 跳过。

use crate::error::{AirpError, AirpResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// 线协议版本。Hub 与插件用它协商兼容性（FED-4 占位：当前仅声明）。
pub const HUB_PROTOCOL_VERSION: &str = "fed-1";

/// 插件启动清单文件名。
pub const HUB_MANIFEST: &str = "hub.json";

/// [`invoke`] 默认超时：插件未在此时限内返回则强制终止（卡死隔离）。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// 发往插件的请求（JSON-RPC 2.0 子集，换行分隔）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireRequest {
    /// 固定 `"2.0"`。
    pub jsonrpc: String,
    /// 请求 id；单次 [`invoke`] 用 `1`。
    pub id: u64,
    /// 方法名（如 `initialize` / `echo`）。语义由插件定义，Hub 透传。
    pub method: String,
    /// 方法参数（任意 JSON）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// 插件返回的响应（JSON-RPC 2.0 子集）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireResponse {
    /// 固定 `"2.0"`。
    pub jsonrpc: String,
    /// 对应请求 id。
    pub id: u64,
    /// 成功结果（与 `error` 二选一）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 失败信息（与 `result` 二选一）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WireError>,
}

impl WireResponse {
    /// 构造成功响应。
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 构造错误响应（code 沿用 JSON-RPC 约定：-32601 method not found 等）。
    pub fn err(id: u64, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(WireError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC 错误对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    /// 错误码（JSON-RPC 约定）。
    pub code: i64,
    /// 人可读错误信息。
    pub message: String,
    /// 可选附加数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// 插件启动清单：Hub spawn 子进程所需的最小元数据。
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    /// 可执行命令（任意语言的解释器 / 二进制，如 `python` / `node` / 绝对路径）。
    pub command: String,
    /// 命令参数（如 `["plugin.py"]`）。
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录，相对 `data/plugins/{name}/`，越界拒绝；缺省即插件目录本身。
    #[serde(default)]
    pub cwd: Option<String>,
    /// 可选自述（仅展示，Hub 不解析）。
    #[serde(default)]
    pub description: Option<String>,
}

/// 一个已发现的可执行插件。
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// 插件名（= `data/plugins/` 下目录名）。
    pub name: String,
    /// 其启动清单。
    pub manifest: PluginManifest,
}

/// 扫描 `data/plugins/*/hub.json`，返回所有**可执行**插件（按名排序）。
///
/// 无 `hub.json` 的目录是纯数据插件（M_PLUGIN_DATA），跳过；坏 `hub.json`
/// 记 warn 跳过，不致命 —— 一个插件清单损坏不影响其他插件被发现。
pub fn discover(data_root: &Path) -> Vec<DiscoveredPlugin> {
    let plugins_dir = data_root.join("plugins");
    let mut out = Vec::new();
    let entries = match fs::read_dir(&plugins_dir) {
        Ok(e) => e,
        Err(_) => return out, // plugins/ 不存在 = 无插件，非错误
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if crate::data_dir::validate_id_segment(&name).is_err() {
            continue;
        }
        let manifest_path = entry.path().join(HUB_MANIFEST);
        if !manifest_path.exists() {
            continue; // 纯数据插件，无可执行清单
        }
        match load_manifest_at(&manifest_path) {
            Ok(manifest) => out.push(DiscoveredPlugin { name, manifest }),
            Err(e) => tracing::warn!(plugin = %name, err = %e, "跳过插件：hub.json 无效"),
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 加载指定插件的启动清单。无 `hub.json` → [`AirpError::NotFound`]。
pub fn load_manifest(data_root: &Path, name: &str) -> AirpResult<PluginManifest> {
    crate::data_dir::validate_id_segment(name)?;
    let path = data_root.join("plugins").join(name).join(HUB_MANIFEST);
    if !path.exists() {
        return Err(AirpError::NotFound(format!(
            "插件 `{}` 无 {}（非可执行插件，或插件不存在）",
            name, HUB_MANIFEST
        )));
    }
    load_manifest_at(&path)
}

fn load_manifest_at(path: &Path) -> AirpResult<PluginManifest> {
    let raw = fs::read_to_string(path)?;
    let manifest: PluginManifest = serde_json::from_str(&raw)?;
    if manifest.command.trim().is_empty() {
        return Err(AirpError::Config(format!(
            "{:?} 的 command 为空",
            path
        )));
    }
    Ok(manifest)
}

/// 调用插件单个方法：spawn 子进程 → 写一行请求 → 读一行响应 → 终止子进程。
///
/// **崩溃 / 卡死隔离**：
/// - spawn 失败（命令不存在等）→ 返回 [`AirpError::Internal`]，Core 不受影响。
/// - 插件超时未响应 → kill 子进程，返回超时错误。
/// - 插件返回 JSON-RPC error → 转 [`AirpError::Internal`]。
///
/// 单次往返后即关闭插件 stdin（插件读循环见 EOF 可自行退出）。
pub fn invoke(
    data_root: &Path,
    name: &str,
    method: &str,
    params: Option<Value>,
    timeout: Duration,
) -> AirpResult<Value> {
    let manifest = load_manifest(data_root, name)?;
    let plugin_dir = data_root.join("plugins").join(name);
    // cwd 限定在插件目录子树内（safe_resolve_for_write 拒绝 .. / 绝对路径）。
    let cwd = match &manifest.cwd {
        Some(rel) => crate::data_dir::safe_resolve_for_write(&plugin_dir, rel)?,
        None => plugin_dir.clone(),
    };

    let mut child = Command::new(&manifest.command)
        .args(&manifest.args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit()) // 插件 stderr 直透 Core stderr，便于排错
        .spawn()
        .map_err(|e| {
            AirpError::Internal(format!(
                "spawn 插件 `{}` 失败（command={:?}）: {}",
                name, manifest.command, e
            ))
        })?;

    let req = WireRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: method.to_string(),
        params,
    };
    let line = serde_json::to_string(&req)?;

    // 写请求 + 关 stdin（让插件读循环见 EOF）
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| AirpError::Internal("无法获取插件 stdin".to_string()))?;
        if let Err(e) = writeln!(stdin, "{}", line) {
            let _ = child.kill();
            return Err(AirpError::Internal(format!("写插件 `{}` stdin 失败: {}", name, e)));
        }
        let _ = stdin.flush();
    }
    drop(child.stdin.take());

    // 读一行响应，带超时（卡死隔离）：reader 线程 + channel recv_timeout。
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AirpError::Internal("无法获取插件 stdout".to_string()))?;
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let res = BufReader::new(stdout).read_line(&mut buf);
        let _ = tx.send(res.map(|_| buf));
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(buf)) => {
            let _ = child.wait();
            let _ = reader.join();
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                return Err(AirpError::Internal(format!(
                    "插件 `{}` 未返回响应（stdout 空，可能已崩溃）",
                    name
                )));
            }
            let resp: WireResponse = serde_json::from_str(trimmed).map_err(|e| {
                AirpError::Internal(format!(
                    "插件 `{}` 响应非合法 JSON-RPC: {} | raw={}",
                    name, e, trimmed
                ))
            })?;
            if let Some(err) = resp.error {
                return Err(AirpError::Internal(format!(
                    "插件 `{}` 返回错误 [{}]: {}",
                    name, err.code, err.message
                )));
            }
            Ok(resp.result.unwrap_or(Value::Null))
        }
        Ok(Err(e)) => {
            let _ = child.kill();
            let _ = reader.join();
            Err(AirpError::Internal(format!(
                "读插件 `{}` stdout 失败: {}",
                name, e
            )))
        }
        Err(_) => {
            // 超时 → 杀子进程；stdout 关闭后 reader 线程自行解除阻塞。
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            Err(AirpError::Internal(format!(
                "插件 `{}` 超时（>{:?}），已强制终止",
                name, timeout
            )))
        }
    }
}

/// FED-3 参考插件实现（Rust）：从 stdin 逐行读 [`WireRequest`]，回 [`WireResponse`]。
///
/// 作为 polyglot 进程边界插件的样例 + 集成测试目标。支持方法：
/// - `initialize` → `{name, version, protocol}`
/// - `echo` → 原样返回 `params`
/// - 其他 → JSON-RPC `-32601 method not found`
///
/// 由隐藏子命令 `airp-core hub-echo` 驱动。任何语言只要实现同样的 stdin/stdout
/// JSON-RPC 行为即可作为插件接入 —— 这是「插件依赖协议不依赖 Hub」的活证。
pub fn run_echo_plugin() -> AirpResult<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<WireRequest>(&line) {
            Ok(req) => handle_echo(req),
            Err(e) => WireResponse::err(0, -32700, format!("parse error: {}", e)),
        };
        writeln!(out, "{}", serde_json::to_string(&resp)?)?;
        out.flush()?;
    }
    Ok(())
}

fn handle_echo(req: WireRequest) -> WireResponse {
    let WireRequest {
        id,
        method,
        params,
        ..
    } = req;
    match method.as_str() {
        "initialize" => WireResponse::ok(
            id,
            json!({
                "name": "echo",
                "version": env!("CARGO_PKG_VERSION"),
                "protocol": HUB_PROTOCOL_VERSION,
            }),
        ),
        "echo" => WireResponse::ok(id, params.unwrap_or(Value::Null)),
        other => WireResponse::err(id, -32601, format!("method not found: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_manifest(root: &Path, name: &str, contents: &str) {
        let dir = root.join("plugins").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(HUB_MANIFEST), contents).unwrap();
    }

    #[test]
    fn manifest_parse_ok_and_defaults() {
        let tmp = tempdir().unwrap();
        write_manifest(tmp.path(), "p", r#"{"command":"python","args":["a.py"]}"#);
        let m = load_manifest(tmp.path(), "p").unwrap();
        assert_eq!(m.command, "python");
        assert_eq!(m.args, vec!["a.py"]);
        assert!(m.cwd.is_none());
    }

    #[test]
    fn manifest_empty_command_rejected() {
        let tmp = tempdir().unwrap();
        write_manifest(tmp.path(), "p", r#"{"command":"   "}"#);
        assert!(matches!(
            load_manifest(tmp.path(), "p"),
            Err(AirpError::Config(_))
        ));
    }

    #[test]
    fn load_manifest_missing_is_notfound() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("plugins").join("dataonly")).unwrap();
        assert!(matches!(
            load_manifest(tmp.path(), "dataonly"),
            Err(AirpError::NotFound(_))
        ));
    }

    #[test]
    fn load_manifest_rejects_bad_id() {
        let tmp = tempdir().unwrap();
        assert!(load_manifest(tmp.path(), "../escape").is_err());
    }

    #[test]
    fn discover_skips_dataonly_and_bad_json_returns_sorted() {
        let tmp = tempdir().unwrap();
        // 可执行插件 b、a（验证排序）
        write_manifest(tmp.path(), "b", r#"{"command":"node"}"#);
        write_manifest(tmp.path(), "a", r#"{"command":"python"}"#);
        // 纯数据插件（无 hub.json）应被跳过
        fs::create_dir_all(tmp.path().join("plugins").join("dataonly")).unwrap();
        // 坏 hub.json 应被跳过，不致命
        write_manifest(tmp.path(), "broken", "{ not json");

        let found = discover(tmp.path());
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn discover_missing_plugins_dir_empty() {
        let tmp = tempdir().unwrap();
        assert!(discover(tmp.path()).is_empty());
    }

    #[test]
    fn echo_handler_methods() {
        let init = handle_echo(WireRequest {
            jsonrpc: "2.0".into(),
            id: 7,
            method: "initialize".into(),
            params: None,
        });
        assert_eq!(init.id, 7);
        assert_eq!(init.result.unwrap()["protocol"], HUB_PROTOCOL_VERSION);

        let echo = handle_echo(WireRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "echo".into(),
            params: Some(json!({"x": 42})),
        });
        assert_eq!(echo.result.unwrap()["x"], 42);

        let bad = handle_echo(WireRequest {
            jsonrpc: "2.0".into(),
            id: 2,
            method: "nope".into(),
            params: None,
        });
        assert_eq!(bad.error.unwrap().code, -32601);
    }
}
