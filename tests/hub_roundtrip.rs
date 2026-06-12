//! M_FEDERATION FED-3 集成测试：进程边界插件 Hub 端到端往返。
//!
//! 用 `airp-core hub-echo` 作参考插件（process boundary），验证：
//! - echo / initialize 方法往返
//! - 崩溃隔离：坏 command 不挂 Core，返回非零退出 + 错误信息
//! - method-not-found 透传为非零退出
//! - hub list 发现可执行插件、跳过纯数据插件

use std::fs;
use std::path::Path;
use std::process::Command;

/// 被测二进制路径（cargo 在集成测试里注入）。
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_airp-core")
}

/// 在 root/plugins/{name}/hub.json 写一份启动清单。
fn write_manifest(root: &Path, name: &str, manifest: &serde_json::Value) {
    let dir = root.join("plugins").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("hub.json"),
        serde_json::to_string(manifest).unwrap(),
    )
    .unwrap();
}

/// 清单：启动 airp-core 自带的参考 echo 插件。
fn echo_manifest() -> serde_json::Value {
    serde_json::json!({
        "command": bin(),
        "args": ["hub-echo"],
        "description": "reference echo plugin"
    })
}

#[test]
fn hub_echo_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(root, "echo", &echo_manifest());

    let out = Command::new(bin())
        .args([
            "hub",
            "call",
            "echo",
            "echo",
            "--json",
            r#"{"hello":"world","n":42}"#,
            "--data-dir",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "hub call 应成功，stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(v["hello"], "world");
    assert_eq!(v["n"], 42);
}

#[test]
fn hub_initialize_returns_protocol() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(root, "echo", &echo_manifest());

    let out = Command::new(bin())
        .args([
            "hub",
            "call",
            "echo",
            "initialize",
            "--data-dir",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(v["name"], "echo");
    assert_eq!(v["protocol"], "fed-1");
}

#[test]
fn hub_method_not_found_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(root, "echo", &echo_manifest());

    let out = Command::new(bin())
        .args([
            "hub",
            "call",
            "echo",
            "no_such_method",
            "--data-dir",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!out.status.success(), "未知方法应返回非零退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("-32601"), "stderr 应含 JSON-RPC 错误码: {}", stderr);
}

#[test]
fn hub_crash_isolation_bad_command() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // 指向一个绝不存在的命令：spawn 必败，但 Core 不应 panic / 挂起。
    write_manifest(
        root,
        "broken",
        &serde_json::json!({ "command": "this_command_does_not_exist_xyz_12345" }),
    );

    let out = Command::new(bin())
        .args([
            "hub",
            "call",
            "broken",
            "echo",
            "--data-dir",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!out.status.success(), "spawn 失败应返回非零退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("spawn") && stderr.contains("broken"),
        "stderr 应说明 spawn 失败: {}",
        stderr
    );
}

#[test]
fn hub_call_rejects_incompatible_protocol() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // 插件声明一个此 Hub 不支持的协议版本；command 指向参考 echo 插件，
    // 但协商在 spawn 之前生效，故根本不会启动子进程。
    let mut manifest = echo_manifest();
    manifest["protocol"] = serde_json::json!("fed-99");
    write_manifest(root, "future", &manifest);

    let out = Command::new(bin())
        .args([
            "hub",
            "call",
            "future",
            "echo",
            "--data-dir",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!out.status.success(), "不兼容协议应返回非零退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fed-99") && stderr.contains("不被此 Hub 支持"),
        "stderr 应说明协议不兼容: {}",
        stderr
    );
}

#[test]
fn hub_list_shows_protocol_compat() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(root, "echo", &echo_manifest()); // 未声明 protocol → legacy OK
    let mut bad = echo_manifest();
    bad["protocol"] = serde_json::json!("fed-99");
    write_manifest(root, "future", &bad);

    let out = Command::new(bin())
        .args(["hub", "list", "--data-dir", root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("protocol=fed-1 OK"), "legacy 插件应显示兼容: {}", stdout);
    assert!(stdout.contains("INCOMPAT"), "未来协议插件应显示不兼容: {}", stdout);
}

#[test]
fn hub_list_finds_executable_skips_dataonly() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(root, "echo", &echo_manifest());
    // 纯数据插件：仅目录无 hub.json，不应出现在 list
    fs::create_dir_all(root.join("plugins").join("dataonly")).unwrap();

    let out = Command::new(bin())
        .args(["hub", "list", "--data-dir", root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("echo"), "应列出 echo 插件: {}", stdout);
    assert!(!stdout.contains("dataonly"), "不应列出纯数据插件: {}", stdout);
    assert!(stdout.contains("1 total"), "应只有 1 个可执行插件: {}", stdout);
}
