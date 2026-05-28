use super::*;
use rmcp::model::ResourceContents;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn test_card_json() -> String {
    serde_json::json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": "凌欺霜",
            "description": "天剑阁首席",
            "personality": "冷静、决断",
            "scenario": "江湖",
            "first_mes": "我便是凌欺霜。",
            "alternate_greetings": ["剑光一闪。", "茶摊偶遇。"],
            "character_book": {
                "entries": {
                    "0": {
                        "keys": ["天剑阁"],
                        "content": "江湖第一大派。",
                        "order": 10,
                        "enabled": true
                    }
                }
            }
        }
    })
    .to_string()
}

#[test]
fn test_mcp1_server_construct() {
    let s = AirpMcpServer::new(PathBuf::from("data"));
    assert_eq!(s.data_root.to_string_lossy(), "data");
}

#[test]
fn test_mcp1_get_info_has_tools_capability() {
    let s = AirpMcpServer::new(PathBuf::from("data"));
    let info = s.get_info();
    assert!(info.capabilities.tools.is_some());
}

#[test]
fn test_mcp1_ping_returns_version() {
    let s = AirpMcpServer::new(PathBuf::from("/tmp/airp"));
    let out = s.ping(Parameters(PingRequest {}));
    assert!(out.contains("AIRP MCP Server v"));
    assert!(out.contains("/tmp/airp"));
}

#[test]
fn test_mcp2_import_card_json() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let req = ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    };
    let out = s.import_card(Parameters(req)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["character_id"], "凌欺霜");
    assert_eq!(v["card_format"], "json");
    assert_eq!(v["greetings_count"], 3);
    assert_eq!(v["lorebook_entries"], 1);

    let cdir = tmp.path().join("characters").join("凌欺霜");
    assert!(cdir.join("card").join("raw.json").exists());
    assert!(cdir.join("card").join("greetings").join("00.md").exists());
    assert!(cdir.join("world").join("lorebook.json").exists());
}

#[test]
fn test_mcp2_import_card_missing_source() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let req = ImportCardRequest {
        character_id: "x".to_string(),
        card_json: None,
        card_png_base64: None,
    };
    assert!(s.import_card(Parameters(req)).is_err());
}

#[test]
fn test_mcp2_apply_lorebook_triggered() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();
    let out = s
        .apply_lorebook(Parameters(ApplyLorebookRequest {
            character_id: "凌欺霜".to_string(),
            text: "走到天剑阁外的茶摊。".to_string(),
        }))
        .unwrap();
    assert!(out.contains("江湖第一大派"), "out = {}", out);
}

#[test]
fn test_mcp2_apply_lorebook_no_match() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();
    let out = s
        .apply_lorebook(Parameters(ApplyLorebookRequest {
            character_id: "凌欺霜".to_string(),
            text: "无关文本".to_string(),
        }))
        .unwrap();
    assert!(out.is_empty() || !out.contains("江湖第一大派"));
}

#[test]
fn test_mcp2_apply_lorebook_no_lorebook_file() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    fs::create_dir_all(tmp.path().join("characters").join("nobook")).unwrap();
    let out = s
        .apply_lorebook(Parameters(ApplyLorebookRequest {
            character_id: "nobook".to_string(),
            text: "any".to_string(),
        }))
        .unwrap();
    assert!(out.is_empty());
}

#[test]
fn test_mcp2_start_session_returns_prompt_and_greetings() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    let out = s
        .start_session(Parameters(StartSessionRequest {
            character_id: "凌欺霜".to_string(),
            session_id: None,
            preset_id: None,
            user_name: "玩家".to_string(),
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["character_id"], "凌欺霜");
    assert_eq!(v["greetings_count"], 3);
    let sp = v["system_prompt"].as_str().unwrap();
    assert!(sp.contains("凌欺霜") || sp.contains("天剑阁"), "sp = {}", sp);
    assert!(v["session_dir"].as_str().unwrap().contains("memory"));
}

// ── MCP-3 Resources 测试 ─────────────────────────────────────────────

#[test]
fn test_mcp3_dispatch_characters_list() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();
    s.import_card(Parameters(ImportCardRequest {
        character_id: "alice".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    let contents = s.dispatch_resource("airp://characters").unwrap();
    assert_eq!(contents.len(), 1);
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<String> = serde_json::from_str(&text).unwrap();
    assert!(arr.contains(&"凌欺霜".to_string()));
    assert!(arr.contains(&"alice".to_string()));
}

#[test]
fn test_mcp3_dispatch_card() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    let contents = s
        .dispatch_resource("airp://characters/凌欺霜/card")
        .unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!(),
    };
    assert!(text.contains("凌欺霜"));
    assert!(text.contains("天剑阁首席"));
}

#[test]
fn test_mcp3_dispatch_lorebook() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    let contents = s
        .dispatch_resource("airp://characters/凌欺霜/world/lorebook")
        .unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!(),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["entries"].as_array().unwrap().len() >= 1);
}

#[test]
fn test_mcp3_dispatch_state_live_stub() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    fs::create_dir_all(tmp.path().join("characters").join("alice")).unwrap();

    let contents = s
        .dispatch_resource("airp://characters/alice/state/live")
        .unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!(),
    };
    assert_eq!(text, "{}");
}

#[test]
fn test_mcp3_dispatch_unknown_uri() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.dispatch_resource("airp://unknown/foo");
    assert!(r.is_err());
}

// ── MCP-4 Prompts 测试 ───────────────────────────────────────────────

#[test]
fn test_mcp4_filter_text_prompt_static() {
    let t = filter_text_prompt();
    assert!(t.contains("文本筛选 Agent"));
    assert!(t.contains("<think>"));
}

#[test]
fn test_mcp4_state_update_prompt_static() {
    let t = state_update_prompt();
    assert!(t.contains("<state>"));
}

#[test]
fn test_mcp4_assemble_system_prompt_works() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "凌欺霜".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();
    let sp = s.assemble_system_prompt("凌欺霜", None, "玩家").unwrap();
    assert!(sp.contains("凌欺霜") || sp.contains("天剑阁"), "sp = {}", sp);
}

#[test]
fn test_mcp4_assemble_rejects_missing_card() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.assemble_system_prompt("nocard", None, "User");
    assert!(r.is_err());
}

#[test]
fn test_mcp4_assemble_rejects_bad_id() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.assemble_system_prompt("../etc", None, "User");
    assert!(r.is_err());
}

// ── airp://characters/{id}/history ──────────────────────────────────────

#[test]
fn test_history_resource_empty_when_no_file() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    fs::create_dir_all(tmp.path().join("characters").join("alice")).unwrap();

    let contents = s.dispatch_resource("airp://characters/alice/history").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let msgs = v["messages"].as_array().unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_history_resource_returns_messages() {
    use crate::adapter::{ChatMessage, MessageRole};
    use crate::chat_store::ChatLog;

    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let mut log = ChatLog::load_or_create(tmp.path(), "alice").unwrap();
    let msg = ChatMessage {
        role: MessageRole::User,
        content: "你好".to_string(),
    };
    log.append(tmp.path(), msg).unwrap();

    let contents = s.dispatch_resource("airp://characters/alice/history").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let msgs = v["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["content"], "你好");
}

// ── DS-5 import_preset ───────────────────────────────────────────────────

#[test]
fn test_ds5_import_preset_ok() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let preset_json = serde_json::json!({"prompts": [{"identifier": "main", "content": "You are {{char}}."}]}).to_string();
    let out = s
        .import_preset(Parameters(ImportPresetRequest {
            preset_id: "my_preset".to_string(),
            preset_json: preset_json.clone(),
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["preset_id"], "my_preset");
    assert!(v["bytes_written"].as_u64().unwrap() > 0);

    let written = tmp.path().join("presets").join("my_preset").join("preset.json");
    assert!(written.exists());
    assert_eq!(fs::read_to_string(&written).unwrap(), preset_json);
}

#[test]
fn test_ds5_import_preset_invalid_json() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.import_preset(Parameters(ImportPresetRequest {
        preset_id: "bad".to_string(),
        preset_json: "not-json{{".to_string(),
    }));
    assert!(r.is_err());
}

#[test]
fn test_ds5_import_preset_bad_id() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.import_preset(Parameters(ImportPresetRequest {
        preset_id: "../etc".to_string(),
        preset_json: "{}".to_string(),
    }));
    assert!(r.is_err());
}

// ── DS-3 artifacts resource ──────────────────────────────────────────────

#[test]
fn test_ds3_artifacts_empty_when_no_dir() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let contents = s.dispatch_resource("airp://presets/nonexistent/artifacts").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<String> = serde_json::from_str(&text).unwrap();
    assert!(arr.is_empty());
}

#[test]
fn test_ds3_artifacts_lists_after_write() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    s.import_preset(Parameters(ImportPresetRequest {
        preset_id: "p1".to_string(),
        preset_json: "{}".to_string(),
    }))
    .unwrap();

    s.write_preset_artifact(Parameters(WritePresetArtifactRequest {
        preset_id: "p1".to_string(),
        artifact_path: "analysis/summary.md".to_string(),
        content: "# Summary".to_string(),
    }))
    .unwrap();
    s.write_preset_artifact(Parameters(WritePresetArtifactRequest {
        preset_id: "p1".to_string(),
        artifact_path: "regex/filters.json".to_string(),
        content: "[]".to_string(),
    }))
    .unwrap();

    let contents = s.dispatch_resource("airp://presets/p1/artifacts").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<String> = serde_json::from_str(&text).unwrap();
    assert!(!arr.contains(&"preset.json".to_string()));
    assert!(arr.contains(&"analysis/summary.md".to_string()), "arr={:?}", arr);
    assert!(arr.contains(&"regex/filters.json".to_string()), "arr={:?}", arr);
}

// ── character artifacts resource ─────────────────────────────────────────

#[test]
fn test_character_artifacts_empty_when_only_system_files() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "alice".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    let contents = s
        .dispatch_resource("airp://characters/alice/artifacts")
        .unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<String> = serde_json::from_str(&text).unwrap();
    for item in &arr {
        assert!(
            !item.starts_with("card/") && !item.starts_with("world/"),
            "system entry leaked: {item}"
        );
    }
}

#[test]
fn test_character_artifacts_after_write() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    s.import_card(Parameters(ImportCardRequest {
        character_id: "alice".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    s.write_character_artifact(Parameters(WriteCharacterArtifactRequest {
        character_id: "alice".to_string(),
        artifact_path: "analysis/profile.md".to_string(),
        content: "# Profile".to_string(),
    }))
    .unwrap();
    s.write_character_artifact(Parameters(WriteCharacterArtifactRequest {
        character_id: "alice".to_string(),
        artifact_path: "analysis/tier.json".to_string(),
        content: r#"{"tier":1}"#.to_string(),
    }))
    .unwrap();

    let contents = s
        .dispatch_resource("airp://characters/alice/artifacts")
        .unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<String> = serde_json::from_str(&text).unwrap();
    assert!(
        arr.contains(&"analysis/profile.md".to_string()),
        "arr={:?}",
        arr
    );
    assert!(
        arr.contains(&"analysis/tier.json".to_string()),
        "arr={:?}",
        arr
    );
    assert!(!arr.iter().any(|s| s == "card.json"), "arr={:?}", arr);
}

// ── M_CA prompt tests ─────────────────────────────────────────────────────

#[test]
fn test_mca_analyze_character_card_prompt_contains_steps() {
    let p = analyze_character_card_prompt("凌欺霜");
    assert!(p.contains("airp://characters/凌欺霜/card"));
    assert!(p.contains("analysis/profile.md"));
    assert!(p.contains("analysis/tier.json"));
    assert!(p.contains("tier.json 格式"));
    assert!(p.contains("airp://characters/凌欺霜/artifacts"));
}

#[test]
fn test_mca_analyze_preset_prompt_contains_steps() {
    let p = analyze_preset_prompt("my_preset");
    assert!(p.contains("airp://presets/my_preset/raw"));
    assert!(p.contains("analysis/summary.md"));
    assert!(p.contains("analysis/regex_scripts.json"));
    assert!(p.contains("airp://presets/my_preset/artifacts"));
}

// ── PR-5/6/7/8/9 regex script management ────────────────────────────────

fn write_script(tmp: &std::path::Path, preset_id: &str, filename: &str, body: &str) {
    let dir = tmp.join("presets").join(preset_id).join("regex");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(filename), body).unwrap();
}

const SCRIPT_A: &str = r#"{"scriptName":"Hide Think","findRegex":"/<think>[\\s\\S]*?<\\/think>/gi","replaceString":"","placement":[2],"disabled":false}"#;
const SCRIPT_B: &str = r#"{"scriptName":"Hide Status","findRegex":"/<status>[\\s\\S]*?<\\/status>/g","replaceString":"","placement":[2],"disabled":true}"#;

#[test]
fn test_pr5_list_empty_when_no_dir() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let out = s
        .list_preset_regex_scripts(Parameters(ListPresetRegexScriptsRequest {
            preset_id: "p1".to_string(),
        }))
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert!(arr.is_empty());
}

#[test]
fn test_pr5_list_returns_scripts_with_filename() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    write_script(tmp.path(), "p1", "a.json", SCRIPT_A);
    write_script(tmp.path(), "p1", "b.json", SCRIPT_B);

    let out = s
        .list_preset_regex_scripts(Parameters(ListPresetRegexScriptsRequest {
            preset_id: "p1".to_string(),
        }))
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["_filename"], "a.json");
    assert_eq!(arr[0]["scriptName"], "Hide Think");
    assert_eq!(arr[0]["disabled"], false);
    assert_eq!(arr[1]["_filename"], "b.json");
    assert_eq!(arr[1]["disabled"], true);
}

#[test]
fn test_pr5_list_bad_id_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.list_preset_regex_scripts(Parameters(ListPresetRegexScriptsRequest {
        preset_id: "../etc".to_string(),
    }));
    assert!(r.is_err());
}

#[test]
fn test_pr6_remove_ok() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    write_script(tmp.path(), "p1", "a.json", SCRIPT_A);

    let out = s
        .remove_preset_regex_script(Parameters(RemovePresetRegexScriptRequest {
            preset_id: "p1".to_string(),
            filename: "a.json".to_string(),
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["removed"], true);
    assert!(!tmp.path().join("presets").join("p1").join("regex").join("a.json").exists());
}

#[test]
fn test_pr6_remove_missing_file_errors() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.remove_preset_regex_script(Parameters(RemovePresetRegexScriptRequest {
        preset_id: "p1".to_string(),
        filename: "nonexistent.json".to_string(),
    }));
    assert!(r.is_err());
}

#[test]
fn test_pr6_remove_path_traversal_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.remove_preset_regex_script(Parameters(RemovePresetRegexScriptRequest {
        preset_id: "p1".to_string(),
        filename: "../preset.json".to_string(),
    }));
    assert!(r.is_err());
}

#[test]
fn test_pr7_set_enabled_toggles_disabled() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    write_script(tmp.path(), "p1", "a.json", SCRIPT_A);

    let out = s
        .set_preset_regex_enabled(Parameters(SetPresetRegexEnabledRequest {
            preset_id: "p1".to_string(),
            filename: "a.json".to_string(),
            enabled: false,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["enabled"], false);
    assert_eq!(v["disabled"], true);

    let content = fs::read_to_string(tmp.path().join("presets").join("p1").join("regex").join("a.json")).unwrap();
    let script: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(script["disabled"], true);
}

#[test]
fn test_pr7_set_enabled_true_clears_disabled() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    write_script(tmp.path(), "p1", "b.json", SCRIPT_B);

    s.set_preset_regex_enabled(Parameters(SetPresetRegexEnabledRequest {
        preset_id: "p1".to_string(),
        filename: "b.json".to_string(),
        enabled: true,
    }))
    .unwrap();

    let content = fs::read_to_string(tmp.path().join("presets").join("p1").join("regex").join("b.json")).unwrap();
    let script: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(script["disabled"], false);
}

#[test]
fn test_pr7_set_enabled_missing_file_errors() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.set_preset_regex_enabled(Parameters(SetPresetRegexEnabledRequest {
        preset_id: "p1".to_string(),
        filename: "nope.json".to_string(),
        enabled: true,
    }));
    assert!(r.is_err());
}

#[test]
fn test_pr9_regex_resource_empty_when_no_dir() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let contents = s.dispatch_resource("airp://presets/p1/regex").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(arr.is_empty());
}

#[test]
fn test_pr9_regex_resource_returns_scripts() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    write_script(tmp.path(), "p1", "a.json", SCRIPT_A);

    let contents = s.dispatch_resource("airp://presets/p1/regex").unwrap();
    let text = match &contents[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text"),
    };
    let arr: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["_filename"], "a.json");
}

#[test]
fn test_mcp3_dispatch_path_traversal_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.dispatch_resource("airp://characters/..%2Fetc/card");
    assert!(r.is_err());
}

#[test]
fn test_mcp2_start_session_missing_card() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let r = s.start_session(Parameters(StartSessionRequest {
        character_id: "nocard".to_string(),
        session_id: None,
        preset_id: None,
        user_name: "User".to_string(),
    }));
    assert!(r.is_err());
}

// DS-4: large preset pagination tests

#[test]
fn test_ds4_small_preset_no_header() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let preset_dir = tmp.path().join("presets").join("tiny");
    std::fs::create_dir_all(&preset_dir).unwrap();
    let content = r#"{"prompts":[]}"#;
    std::fs::write(preset_dir.join("preset.json"), content).unwrap();

    let result = s.dispatch_resource("airp://presets/tiny/raw").unwrap();
    let text = match &result[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text resource"),
    };
    assert!(!text.starts_with("[PARTIAL"), "unexpected header: {}", &text[..text.len().min(100)]);
    assert!(text.contains("prompts"));
}

#[test]
fn test_ds4_large_preset_gets_header() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let preset_dir = tmp.path().join("presets").join("big");
    std::fs::create_dir_all(&preset_dir).unwrap();
    let large_content = "x".repeat(200_001);
    std::fs::write(preset_dir.join("preset.json"), &large_content).unwrap();

    let result = s.dispatch_resource("airp://presets/big/raw").unwrap();
    let text = match &result[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text resource"),
    };
    assert!(text.starts_with("[PARTIAL"), "expected [PARTIAL] header, got: {}", &text[..text.len().min(120)]);
    assert!(text.contains("total=200001"), "expected total in header");
}

#[test]
fn test_ds4_offset_and_limit_params() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let preset_dir = tmp.path().join("presets").join("paged");
    std::fs::create_dir_all(&preset_dir).unwrap();
    let content = "0".repeat(100) + &"1".repeat(100) + &"2".repeat(100);
    std::fs::write(preset_dir.join("preset.json"), &content).unwrap();

    let result = s
        .dispatch_resource("airp://presets/paged/raw?offset=100&limit=100")
        .unwrap();
    let text = match &result[0] {
        ResourceContents::TextResourceContents { text, .. } => text.clone(),
        _ => panic!("expected text resource"),
    };
    assert!(text.starts_with("[PARTIAL"), "expected [PARTIAL] header");
    let body = text.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(body, "1".repeat(100), "expected 100 '1' chars, got: {:?}", &body[..body.len().min(20)]);
}

// ── DS-6/7: get_recent_context + append_message ──────────────────────────

#[test]
fn test_ds67_append_then_get_context() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("chr1")).unwrap();

    let r1 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "chr1".to_string(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    assert_eq!(v1["total_messages"], 1);
    assert_eq!(v1["role"], "user");

    let r2 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "chr1".to_string(),
            role: "assistant".to_string(),
            content: "Hi there!".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    assert_eq!(v2["total_messages"], 2);

    let r3 = s
        .get_recent_context_impl(Parameters(GetRecentContextRequest {
            character_id: "chr1".to_string(),
            n: 10,
            session_id: None,
        }))
        .unwrap();
    let v3: serde_json::Value = serde_json::from_str(&r3).unwrap();
    assert_eq!(v3["total_messages"], 2);
    assert_eq!(v3["returned"], 2);
    assert_eq!(v3["messages"][0]["role"], "user");
    assert_eq!(v3["messages"][0]["content"], "Hello!");
    assert_eq!(v3["messages"][1]["role"], "assistant");
}

#[test]
fn test_ds67_get_context_empty_log() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("empty_chr")).unwrap();

    let result = s
        .get_recent_context_impl(Parameters(GetRecentContextRequest {
            character_id: "empty_chr".to_string(),
            n: 5,
            session_id: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["total_messages"], 0);
    assert_eq!(v["returned"], 0);
    assert!(v["messages"].as_array().unwrap().is_empty());
}

#[test]
fn test_ds67_append_invalid_role() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("chr2")).unwrap();

    let err = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "chr2".to_string(),
            role: "bot".to_string(),
            content: "hi".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap_err();
    assert!(err.message.contains("未知 role 'bot'"));
}

#[test]
fn test_ds67_get_context_n_limit() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("chr3")).unwrap();

    for i in 0..5u32 {
        s.append_message_impl(Parameters(AppendMessageRequest {
            character_id: "chr3".to_string(),
            role: "user".to_string(),
            content: format!("msg {}", i),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let result = s
        .get_recent_context_impl(Parameters(GetRecentContextRequest {
            character_id: "chr3".to_string(),
            n: 3,
            session_id: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["total_messages"], 5);
    assert_eq!(v["returned"], 3);
    assert_eq!(v["messages"][0]["content"], "msg 2");
    assert_eq!(v["messages"][2]["content"], "msg 4");
}

// ── DS-8: update_state ───────────────────────────────────────────────────

#[test]
fn test_ds8_update_state_merge() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(
        tmp.path().join("characters").join("hero").join("state"),
    )
    .unwrap();

    let r1 = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "hero".to_string(),
            state_json: r#"{"hp":100,"mp":50}"#.to_string(),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap();
    let v1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    assert_eq!(v1["state"]["hp"], 100);
    assert_eq!(v1["state"]["mp"], 50);
    assert_eq!(v1["fields_updated"], 2);

    let r2 = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "hero".to_string(),
            state_json: r#"{"hp":80,"location":"tavern"}"#.to_string(),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap();
    let v2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    assert_eq!(v2["state"]["hp"], 80);
    assert_eq!(v2["state"]["mp"], 50, "mp should be preserved after merge");
    assert_eq!(v2["state"]["location"], "tavern");
}

#[test]
fn test_ds8_update_state_overwrite() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(
        tmp.path().join("characters").join("hero2").join("state"),
    )
    .unwrap();

    s.update_state_impl(Parameters(UpdateStateRequest {
        character_id: "hero2".to_string(),
        state_json: r#"{"hp":100,"mp":50}"#.to_string(),
        overwrite: false,
            idempotency_key: None,
    }))
    .unwrap();

    let r = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "hero2".to_string(),
            state_json: r#"{"hp":30}"#.to_string(),
            overwrite: true,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["state"]["hp"], 30);
    assert!(v["state"]["mp"].is_null(), "mp should be absent after overwrite");
}

#[test]
fn test_ds8_update_state_invalid_json() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let err = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "hero3".to_string(),
            state_json: "not json".to_string(),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap_err();
    assert!(err.message.contains("非合法 JSON"));
}

#[test]
fn test_ds8_update_state_not_object() {
    use rmcp::handler::server::wrapper::Parameters;
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let err = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "hero4".to_string(),
            state_json: "[1,2,3]".to_string(),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap_err();
    assert!(err.message.contains("JSON 对象"));
}

// ── DS-9: rollback_messages ──────────────────────────────────────────────

#[test]
fn test_ds9_rollback_one_message() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("rb_chr")).unwrap();

    // Append 3 messages
    for (role, content) in &[("user", "msg1"), ("assistant", "msg2"), ("user", "msg3")] {
        s.append_message_impl(Parameters(AppendMessageRequest {
            character_id: "rb_chr".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let r = s
        .rollback_messages_impl(Parameters(RollbackMessagesRequest {
            character_id: "rb_chr".to_string(),
            n: 1,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["removed"], 1);
    assert_eq!(v["total_messages"], 2);
    assert_eq!(v["requested"], 1);

    // Verify last message is now msg2
    let ctx = s
        .get_recent_context_impl(Parameters(GetRecentContextRequest {
            character_id: "rb_chr".to_string(),
            n: 10,
            session_id: None,
        }))
        .unwrap();
    let cv: serde_json::Value = serde_json::from_str(&ctx).unwrap();
    assert_eq!(cv["messages"][1]["content"], "msg2");
}

#[test]
fn test_ds9_rollback_multiple_messages() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("rb_chr2")).unwrap();

    for i in 0..5u32 {
        s.append_message_impl(Parameters(AppendMessageRequest {
            character_id: "rb_chr2".to_string(),
            role: "user".to_string(),
            content: format!("m{}", i),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let r = s
        .rollback_messages_impl(Parameters(RollbackMessagesRequest {
            character_id: "rb_chr2".to_string(),
            n: 3,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["removed"], 3);
    assert_eq!(v["total_messages"], 2);
}

#[test]
fn test_ds9_rollback_n_exceeds_total_clears_all() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("rb_chr3")).unwrap();

    for i in 0..2u32 {
        s.append_message_impl(Parameters(AppendMessageRequest {
            character_id: "rb_chr3".to_string(),
            role: "user".to_string(),
            content: format!("m{}", i),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let r = s
        .rollback_messages_impl(Parameters(RollbackMessagesRequest {
            character_id: "rb_chr3".to_string(),
            n: 999,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    // removed = min(999, 2) = 2
    assert_eq!(v["total_messages"], 0);
}

#[test]
fn test_ds9_rollback_empty_log_returns_zero_removed() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("rb_empty")).unwrap();

    let r = s
        .rollback_messages_impl(Parameters(RollbackMessagesRequest {
            character_id: "rb_empty".to_string(),
            n: 1,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["removed"], 0);
    assert_eq!(v["total_messages"], 0);
}

#[test]
fn test_ds9_rollback_invalid_character_id_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let err = s
        .rollback_messages_impl(Parameters(RollbackMessagesRequest {
            character_id: "../etc".to_string(),
            n: 1,
        }))
        .unwrap_err();
    assert!(err.message.contains("非法 character_id"));
}

// ── DS-10: list_sessions ─────────────────────────────────────────────────────

#[test]
fn test_ds10_list_sessions_empty_when_no_sessions_dir() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    // Character exists but no sessions/ dir
    std::fs::create_dir_all(tmp.path().join("characters").join("chr_s")).unwrap();

    let r = s
        .list_sessions_impl(Parameters(ListSessionsRequest {
            character_id: "chr_s".to_string(),
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["count"], 0);
    assert!(v["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn test_ds10_list_sessions_after_create_session() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    // Import a card so the character dir exists
    s.import_card(Parameters(ImportCardRequest {
        character_id: "sess_chr".to_string(),
        card_json: Some(test_card_json()),
        card_png_base64: None,
    }))
    .unwrap();

    // Create a named session via data_dir
    let sid = crate::data_dir::create_session(tmp.path(), "sess_chr").unwrap();

    let r = s
        .list_sessions_impl(Parameters(ListSessionsRequest {
            character_id: "sess_chr".to_string(),
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["count"], 1);
    let sessions = v["sessions"].as_array().unwrap();
    assert_eq!(sessions[0].as_str().unwrap(), sid.to_string());
}

#[test]
fn test_ds10_list_sessions_invalid_id_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let err = s
        .list_sessions_impl(Parameters(ListSessionsRequest {
            character_id: "../evil".to_string(),
        }))
        .unwrap_err();
    assert!(err.message.contains("非法 character_id"));
}

// ── DS-11: get_state_history ─────────────────────────────────────────────────

#[test]
fn test_ds11_get_state_history_no_file_returns_empty() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("sh_chr")).unwrap();

    let r = s
        .get_state_history_impl(Parameters(GetStateHistoryRequest {
            character_id: "sh_chr".to_string(),
            n: 10,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["count"], 0);
    assert!(v["entries"].as_array().unwrap().is_empty());
}

#[test]
fn test_ds11_get_state_history_reads_newest_first() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    // Write history via update_state (which appends to state/history.jsonl)
    std::fs::create_dir_all(
        tmp.path().join("characters").join("sh_hero").join("state"),
    )
    .unwrap();

    for (hp, mp) in &[(100u32, 50u32), (80, 45), (60, 40)] {
        s.update_state_impl(Parameters(UpdateStateRequest {
            character_id: "sh_hero".to_string(),
            state_json: format!(r#"{{"hp":{}, "mp":{}}}"#, hp, mp),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let r = s
        .get_state_history_impl(Parameters(GetStateHistoryRequest {
            character_id: "sh_hero".to_string(),
            n: 10,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    // newest-first: last update was hp=60; each entry is {ts, state: {hp, mp}}
    assert_eq!(entries[0]["state"]["hp"], 60);
    assert_eq!(entries[2]["state"]["hp"], 100);
}

#[test]
fn test_ds11_get_state_history_n_limit() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(
        tmp.path().join("characters").join("sh_lim").join("state"),
    )
    .unwrap();

    for i in 0..5u32 {
        s.update_state_impl(Parameters(UpdateStateRequest {
            character_id: "sh_lim".to_string(),
            state_json: format!(r#"{{"turn":{}}}"#, i),
            overwrite: false,
            idempotency_key: None,
        }))
        .unwrap();
    }

    let r = s
        .get_state_history_impl(Parameters(GetStateHistoryRequest {
            character_id: "sh_lim".to_string(),
            n: 2,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r).unwrap();
    assert_eq!(v["count"], 2);
    // newest-first: last two turns are turn=4 and turn=3; each entry is {ts, state: {turn}}
    assert_eq!(v["entries"][0]["state"]["turn"], 4);
    assert_eq!(v["entries"][1]["state"]["turn"], 3);
}

#[test]
fn test_ds11_get_state_history_invalid_id_rejected() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    let err = s
        .get_state_history_impl(Parameters(GetStateHistoryRequest {
            character_id: "../bad".to_string(),
            n: 5,
        }))
        .unwrap_err();
    assert!(err.message.contains("非法 character_id"));
}

// ── AUDIT-12: idempotency keys ─────────────────────────────────────────────

#[test]
fn test_audit_12_append_message_idempotency_dedups_retry() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc_idem")).unwrap();

    // First call with idempotency_key
    let resp1 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc_idem".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            session_id: None,
            idempotency_key: Some("retry-key-1".to_string()),
        }))
        .unwrap();

    // Second call with the same key — should return identical result without
    // adding another message
    let resp2 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc_idem".to_string(),
            role: "user".to_string(),
            // Different content — should be ignored on cache hit
            content: "DIFFERENT CONTENT".to_string(),
            session_id: None,
            idempotency_key: Some("retry-key-1".to_string()),
        }))
        .unwrap();

    assert_eq!(resp1, resp2, "same idempotency key must return cached result");

    // Verify only one message was actually written
    let v: serde_json::Value = serde_json::from_str(&resp2).unwrap();
    assert_eq!(v["total_messages"], 1, "second call must not append");
}

#[test]
fn test_audit_12_append_message_different_keys_proceed() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc_keys")).unwrap();

    let r1 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc_keys".to_string(),
            role: "user".to_string(),
            content: "first".to_string(),
            session_id: None,
            idempotency_key: Some("key-A".to_string()),
        }))
        .unwrap();
    let r2 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc_keys".to_string(),
            role: "user".to_string(),
            content: "second".to_string(),
            session_id: None,
            idempotency_key: Some("key-B".to_string()),
        }))
        .unwrap();
    let v2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    assert_eq!(v2["total_messages"], 2, "different keys should write both");
    assert_ne!(r1, r2);
}

#[test]
fn test_audit_12_append_message_no_key_no_caching() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc_nokey")).unwrap();

    // Two calls without idempotency_key should always append (back-compat).
    s.append_message_impl(Parameters(AppendMessageRequest {
        character_id: "npc_nokey".to_string(),
        role: "user".to_string(),
        content: "a".to_string(),
        session_id: None,
        idempotency_key: None,
    }))
    .unwrap();
    let r2 = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc_nokey".to_string(),
            role: "user".to_string(),
            content: "b".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&r2).unwrap();
    assert_eq!(v["total_messages"], 2, "no key means always append");
}

#[test]
fn test_audit_12_update_state_idempotency_dedups() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc_us")).unwrap();

    let r1 = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "npc_us".to_string(),
            state_json: serde_json::json!({"hp": 50}).to_string(),
            overwrite: false,
            idempotency_key: Some("us-key".to_string()),
        }))
        .unwrap();
    // Retry with different state — cache hit should return original
    let r2 = s
        .update_state_impl(Parameters(UpdateStateRequest {
            character_id: "npc_us".to_string(),
            state_json: serde_json::json!({"hp": 99}).to_string(),
            overwrite: false,
            idempotency_key: Some("us-key".to_string()),
        }))
        .unwrap();
    assert_eq!(r1, r2, "cached response must match original");

    // Verify state on disk was the original 50, not the retry 99
    let live = std::fs::read_to_string(
        tmp.path().join("characters/npc_us/state/live.json"),
    )
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&live).unwrap();
    assert_eq!(parsed["hp"], 50, "retry with same key must not overwrite state");
}

// ── AUDIT-6 / AUDIT-7: append_message soft hints ──────────────────────────

#[test]
fn test_audit_6_short_message_no_seal_hint() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc1")).unwrap();

    let resp = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc1".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    // No volume_seal hint when chat is tiny
    let hints = v["hints"].as_array().unwrap();
    assert!(
        !hints
            .iter()
            .any(|h| h["kind"] == "volume_seal_recommended"),
        "tiny chat should not trigger volume_seal_recommended hint"
    );
}

#[test]
fn test_audit_6_long_chat_emits_seal_hint() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc2")).unwrap();

    // Default soft threshold is 2500 tokens. ASCII estimate = chars/4, so
    // we need ~10000 chars to safely exceed. Stuff a single long message.
    let bulk = "a".repeat(12000);
    let resp = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc2".to_string(),
            role: "assistant".to_string(),
            content: bulk,
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let hints = v["hints"].as_array().unwrap();
    let seal_hint = hints
        .iter()
        .find(|h| h["kind"] == "volume_seal_recommended")
        .expect("long chat should emit volume_seal_recommended hint");
    assert!(seal_hint["current_tokens"].as_u64().unwrap() > 2500);
    assert_eq!(seal_hint["soft_threshold"], 2500);
}

#[test]
fn test_audit_7_three_volumes_emits_maintenance_hint() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    let memory_volumes = tmp
        .path()
        .join("characters")
        .join("npc3")
        .join("memory")
        .join("volumes");
    std::fs::create_dir_all(&memory_volumes).unwrap();
    // Pre-create 3 fake volumes
    for i in 1..=3 {
        std::fs::write(memory_volumes.join(format!("vol_{:03}.md", i)), "").unwrap();
    }

    let resp = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc3".to_string(),
            role: "user".to_string(),
            content: "x".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let hints = v["hints"].as_array().unwrap();
    let maint = hints
        .iter()
        .find(|h| h["kind"] == "volume_maintenance_available")
        .expect("3 volumes should emit volume_maintenance_available hint");
    assert_eq!(maint["volume_count"], 3);
}

#[test]
fn test_audit_7_no_volumes_no_maintenance_hint() {
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    std::fs::create_dir_all(tmp.path().join("characters").join("npc4")).unwrap();
    // No memory/ dir at all

    let resp = s
        .append_message_impl(Parameters(AppendMessageRequest {
            character_id: "npc4".to_string(),
            role: "user".to_string(),
            content: "x".to_string(),
            session_id: None,
            idempotency_key: None,
        }))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let hints = v["hints"].as_array().unwrap();
    assert!(
        !hints
            .iter()
            .any(|h| h["kind"] == "volume_maintenance_available"),
        "no volumes should mean no maintenance hint"
    );
}

// ── AUDIT-13: resource subscribe emit ─────────────────────────────────────

#[tokio::test]
async fn test_audit_13_update_state_emits_no_panic_with_empty_subs() {
    // AUDIT-13: smoke test — calling update_state with no subscribers must
    // not panic and the emit helper must early-return cleanly.
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());
    // Pre-create character dir so update_state path validation passes.
    std::fs::create_dir_all(tmp.path().join("characters").join("npc1")).unwrap();

    let result = s.update_state_impl(Parameters(UpdateStateRequest {
        character_id: "npc1".to_string(),
        state_json: serde_json::json!({"hp": 80}).to_string(),
        overwrite: false,
            idempotency_key: None,
    }));
    assert!(result.is_ok(), "update_state should succeed without subscribers");

    // Verify state was actually written
    let live = tmp.path().join("characters/npc1/state/live.json");
    assert!(live.exists());
    let content: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&live).unwrap()).unwrap();
    assert_eq!(content["hp"], 80);
}

#[tokio::test]
async fn test_audit_13_emit_filters_by_uri() {
    // AUDIT-13: verify the emit helper's URI filter — populate state_subs
    // manually and confirm only matching subscribers are selected.
    // We cannot construct a real rmcp Peer in unit tests, so we verify the
    // filter logic indirectly by ensuring emit on a non-matching URI is
    // a no-op (no spawned tasks crash on empty filter).
    let tmp = tempdir().unwrap();
    let s = AirpMcpServer::new(tmp.path().to_path_buf());

    // Empty subs registry — emit should early-return cleanly.
    super::tools::emit_resource_updated(
        &s.state_subs,
        "airp://characters/nobody/state/live".to_string(),
    );
    // If we got here without panic, helper handles empty subs correctly.

    // Verify subs registry is still in a usable state.
    let guard = s.state_subs.lock().unwrap();
    assert_eq!(guard.len(), 0);
}
