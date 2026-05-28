//! `airp-core diagnose` — one-shot health + state report.
//!
//! Walks the data directory and produces a single structured JSON report.
//! Designed for the "user hits a bug, pastes diagnose output" workflow:
//! a maintainer can read the report and instantly see the state of every
//! character, preset, scene, settings field, volume count, etc., without
//! asking the user to manually dump individual files.
//!
//! Deliberately read-only and non-failing: any file that can't be read or
//! parsed is reported with an `error` field rather than aborting the run.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Produce a full diagnostic report.
///
/// `focus_character` and `focus_scene` narrow the report to one entity when
/// set; otherwise lists all characters / scenes with summary fields each.
pub fn run_diagnose(
    data_root: &Path,
    focus_character: Option<&str>,
    focus_scene: Option<&str>,
) -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "data_root": data_root.display().to_string(),
        "data_root_exists": data_root.exists(),
        "settings": describe_settings(data_root),
        "characters": describe_characters(data_root, focus_character),
        "users": describe_users(data_root),
        "presets": describe_presets(data_root),
        "scenes": describe_scenes(data_root, focus_scene),
    })
}

/// M_UP debug: enumerate user personas under data/users/.
fn describe_users(data_root: &Path) -> Value {
    let dir = data_root.join("users");
    if !dir.exists() {
        return json!([]);
    }
    let mut out: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = match entry.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let user_dir = entry.path();
            let persona_path = user_dir.join("persona.json");
            let lock_path = user_dir.join("persona.lock");
            let state_live = user_dir.join("state").join("live.json");
            let state_history = user_dir.join("state").join("history.jsonl");

            let persona_name: Option<String> = std::fs::read_to_string(&persona_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(crate::data_dir::strip_utf8_bom(&s)).ok())
                .and_then(|v| {
                    v.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                });

            // drift_key_count: keys in current_state not in persona
            let drift_key_count: usize =
                match (persona_path.exists(), state_live.exists()) {
                    (true, true) => {
                        let p: Option<Value> = std::fs::read_to_string(&persona_path)
                            .ok()
                            .and_then(|s| {
                                serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok()
                            });
                        let s: Option<Value> = std::fs::read_to_string(&state_live)
                            .ok()
                            .and_then(|s| {
                                serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok()
                            });
                        match (p, s) {
                            (Some(pv), Some(sv)) => match (pv.as_object(), sv.as_object()) {
                                (Some(pm), Some(sm)) => {
                                    sm.keys().filter(|k| !pm.contains_key(k.as_str())).count()
                                }
                                _ => 0,
                            },
                            _ => 0,
                        }
                    }
                    _ => 0,
                };

            out.push(json!({
                "id": id,
                "persona_present": persona_path.exists(),
                "persona_name": persona_name,
                "locked": lock_path.exists(),
                "state_live_present": state_live.exists(),
                "state_history_lines": count_jsonl_lines(&state_history),
                "drift_key_count": drift_key_count,
            }));
        }
    }
    out.sort_by(|a, b| {
        a.get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .cmp(b.get("id").and_then(|x| x.as_str()).unwrap_or(""))
    });
    Value::Array(out)
}

fn describe_settings(data_root: &Path) -> Value {
    let path = data_root.join("settings.json");
    if !path.exists() {
        return json!({"present": false});
    }
    match std::fs::read_to_string(&path) {
        Err(e) => json!({"present": true, "error": format!("read failed: {}", e)}),
        Ok(raw) => match serde_json::from_str::<Value>(crate::data_dir::strip_utf8_bom(&raw)) {
            Err(e) => json!({"present": true, "error": format!("parse failed: {}", e)}),
            Ok(v) => json!({
                "present": true,
                "endpoint_set": is_nonempty_string(&v, "endpoint"),
                "model_set": is_nonempty_string(&v, "model"),
                "provider": v.get("provider").cloned().unwrap_or(Value::Null),
                "api_key_set": is_nonempty_string(&v, "api_key"),
                "access_api_key_set": is_nonempty_string(&v, "access_api_key"),
                "daemon_port": v.get("daemon_port").cloned().unwrap_or(Value::Null),
            }),
        },
    }
}

fn is_nonempty_string(v: &Value, key: &str) -> bool {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

fn describe_characters(data_root: &Path, focus: Option<&str>) -> Value {
    let dir = data_root.join("characters");
    if !dir.exists() {
        return json!([]);
    }
    let mut out: Vec<Value> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(e) => return json!({"error": format!("read characters/ failed: {}", e)}),
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if let Some(f) = focus {
            if f != id {
                continue;
            }
        }
        out.push(describe_one_character(data_root, &id));
    }
    out.sort_by(|a, b| {
        a.get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .cmp(b.get("id").and_then(|x| x.as_str()).unwrap_or(""))
    });
    Value::Array(out)
}

fn describe_one_character(data_root: &Path, id: &str) -> Value {
    let char_dir = data_root.join("characters").join(id);

    // Card detection — CF-1 folder form preferred, legacy fallback.
    let card_folder = char_dir.join("card").join("card.json");
    let card_legacy = char_dir.join("card.json");
    let (card_path, card_format) = if card_folder.exists() {
        (Some(card_folder), "v2_folder")
    } else if card_legacy.exists() {
        (Some(card_legacy), "v2_legacy")
    } else {
        (None, "missing")
    };
    let lorebook_entries = count_lorebook_entries(&char_dir);

    // State files
    let state_live = char_dir.join("state").join("live.json").exists();
    let state_history_lines = count_jsonl_lines(&char_dir.join("state").join("history.jsonl"));

    // Chat log (CF-2: history/chat_log.jsonl)
    let chat_log = char_dir.join("history").join("chat_log.jsonl");
    let chat_log_messages = count_jsonl_lines(&chat_log);

    // Volume system
    let memory_dir = char_dir.join("memory");
    let volume_count = crate::volume_store::list_volume_numbers(&memory_dir).len();
    let current_md = memory_dir.join("current.md");
    let current_md_tokens = if current_md.exists() {
        std::fs::read_to_string(&current_md)
            .map(|s| crate::volume_store::estimate_tokens(&s))
            .unwrap_or(0)
    } else {
        0
    };

    // Named sessions
    let sessions_dir = char_dir.join("sessions");
    let sessions_count = if sessions_dir.exists() {
        std::fs::read_dir(&sessions_dir)
            .map(|it| it.flatten().filter(|e| e.path().is_dir()).count())
            .unwrap_or(0)
    } else {
        0
    };

    // Gating checkpoint
    let checkpoint = crate::orchestrator::gating::get_current_checkpoint(data_root, id);

    // Chat log tail — last 3 messages (role + content truncated to 200 chars)
    let chat_tail = read_chat_tail(&chat_log, 3);

    // State live snapshot (current values)
    let state_live_value: Option<Value> = if state_live {
        std::fs::read_to_string(char_dir.join("state").join("live.json"))
            .ok()
            .and_then(|s| serde_json::from_str(crate::data_dir::strip_utf8_bom(&s)).ok())
    } else {
        None
    };

    json!({
        "id": id,
        "card_present": card_path.is_some(),
        "card_format": card_format,
        "lorebook_entries": lorebook_entries,
        "state_live_present": state_live,
        "state_live": state_live_value,
        "state_history_lines": state_history_lines,
        "chat_log_messages": chat_log_messages,
        "chat_tail": chat_tail,
        "volume_count": volume_count,
        "current_md_tokens_estimate": current_md_tokens,
        "sessions_count": sessions_count,
        "current_checkpoint": checkpoint,
    })
}

/// Read last N messages from chat_log.jsonl (newest-last), truncating
/// each `content` to 200 chars for compact debug output.
fn read_chat_tail(path: &Path, n: usize) -> Vec<Value> {
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(n);
    lines[start..]
        .iter()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|mut v| {
            // Truncate content
            if let Some(c) = v.get("content").and_then(|x| x.as_str()) {
                let truncated: String = c.chars().take(200).collect();
                let trailing = if c.chars().count() > 200 { "…" } else { "" };
                if let Some(obj) = v.as_object_mut() {
                    obj.insert(
                        "content".to_string(),
                        Value::String(format!("{}{}", truncated, trailing)),
                    );
                }
            }
            v
        })
        .collect()
}

fn count_lorebook_entries(char_dir: &Path) -> usize {
    let path = char_dir.join("world").join("lorebook.json");
    if !path.exists() {
        return 0;
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let v: Value = match serde_json::from_str(crate::data_dir::strip_utf8_bom(&raw)) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    // entries field can be array or object map (V2 spec uses object).
    v.get("entries")
        .map(|e| match e {
            Value::Array(a) => a.len(),
            Value::Object(o) => o.len(),
            _ => 0,
        })
        .unwrap_or(0)
}

fn count_jsonl_lines(path: &PathBuf) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_to_string(path)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

fn describe_presets(data_root: &Path) -> Value {
    let dir = data_root.join("presets");
    if !dir.exists() {
        return json!([]);
    }
    let mut out: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = match entry.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let preset_json = entry.path().join("preset.json");
            let regex_dir = entry.path().join("regex");
            let regex_scripts = if regex_dir.exists() {
                std::fs::read_dir(&regex_dir)
                    .map(|it| {
                        it.flatten()
                            .filter(|e| {
                                e.path().extension().and_then(|s| s.to_str()) == Some("json")
                            })
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            out.push(json!({
                "id": id,
                "preset_json_present": preset_json.exists(),
                "regex_scripts": regex_scripts,
            }));
        }
    }
    out.sort_by(|a, b| {
        a.get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .cmp(b.get("id").and_then(|x| x.as_str()).unwrap_or(""))
    });
    Value::Array(out)
}

fn describe_scenes(data_root: &Path, focus: Option<&str>) -> Value {
    let dir = data_root.join("scenes");
    if !dir.exists() {
        return json!([]);
    }
    let mut out: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = match entry.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            if let Some(f) = focus {
                if f != id {
                    continue;
                }
            }
            let scene_json = entry.path().join("scene.json");
            let characters_count = if scene_json.exists() {
                std::fs::read_to_string(&scene_json)
                    .ok()
                    .and_then(|s| {
                        serde_json::from_str::<Value>(crate::data_dir::strip_utf8_bom(&s)).ok()
                    })
                    .and_then(|v| {
                        v.get("characters")
                            .and_then(|c| c.as_array())
                            .map(|a| a.len())
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            out.push(json!({
                "id": id,
                "scene_json_present": scene_json.exists(),
                "characters_count": characters_count,
            }));
        }
    }
    out.sort_by(|a, b| {
        a.get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .cmp(b.get("id").and_then(|x| x.as_str()).unwrap_or(""))
    });
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn diagnose_empty_data_root() {
        let tmp = tempdir().unwrap();
        let r = run_diagnose(tmp.path(), None, None);
        assert_eq!(r["data_root_exists"], true);
        assert_eq!(r["characters"], json!([]));
        assert_eq!(r["presets"], json!([]));
        assert_eq!(r["scenes"], json!([]));
        assert_eq!(r["settings"]["present"], false);
    }

    #[test]
    fn diagnose_nonexistent_root() {
        let r = run_diagnose(Path::new("D:/__nonexistent_airp__"), None, None);
        assert_eq!(r["data_root_exists"], false);
    }

    #[test]
    fn diagnose_picks_up_settings_fields() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{"endpoint":"https://x","model":"m","api_key":"secret","access_api_key":"","provider":"openai","daemon_port":8000}"#,
        )
        .unwrap();
        let r = run_diagnose(tmp.path(), None, None);
        assert_eq!(r["settings"]["present"], true);
        assert_eq!(r["settings"]["endpoint_set"], true);
        assert_eq!(r["settings"]["model_set"], true);
        assert_eq!(r["settings"]["api_key_set"], true);
        assert_eq!(r["settings"]["access_api_key_set"], false);
        assert_eq!(r["settings"]["daemon_port"], 8000);
        // Crucially, the secret value must not leak — only the _set boolean.
        let serialized = r["settings"].to_string();
        assert!(!serialized.contains("secret"), "secret leaked: {}", serialized);
    }

    #[test]
    fn diagnose_reports_one_character() {
        let tmp = tempdir().unwrap();
        let cdir = tmp.path().join("characters").join("alice");
        std::fs::create_dir_all(&cdir.join("card")).unwrap();
        std::fs::write(cdir.join("card").join("card.json"), r#"{"spec":"chara_card_v2","data":{"name":"alice"}}"#).unwrap();
        std::fs::create_dir_all(cdir.join("world")).unwrap();
        std::fs::write(
            cdir.join("world").join("lorebook.json"),
            r#"{"entries":{"0":{"keys":["x"],"content":"y"},"1":{"keys":["a"],"content":"b"}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(cdir.join("history")).unwrap();
        std::fs::write(
            cdir.join("history").join("chat_log.jsonl"),
            "{\"role\":\"user\",\"content\":\"hi\"}\n{\"role\":\"assistant\",\"content\":\"hello\"}\n",
        )
        .unwrap();

        let r = run_diagnose(tmp.path(), None, None);
        let chars = r["characters"].as_array().unwrap();
        assert_eq!(chars.len(), 1);
        let c = &chars[0];
        assert_eq!(c["id"], "alice");
        assert_eq!(c["card_present"], true);
        assert_eq!(c["card_format"], "v2_folder");
        assert_eq!(c["lorebook_entries"], 2);
        assert_eq!(c["chat_log_messages"], 2);
    }

    #[test]
    fn diagnose_focus_filters_to_one_character() {
        let tmp = tempdir().unwrap();
        for id in &["alice", "bob"] {
            std::fs::create_dir_all(tmp.path().join("characters").join(id)).unwrap();
        }
        let r = run_diagnose(tmp.path(), Some("bob"), None);
        let chars = r["characters"].as_array().unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0]["id"], "bob");
    }

    #[test]
    fn diagnose_reports_presets_and_scenes() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("presets").join("LENI");
        std::fs::create_dir_all(p.join("regex")).unwrap();
        std::fs::write(p.join("preset.json"), "{}").unwrap();
        std::fs::write(p.join("regex").join("a.json"), "{}").unwrap();
        std::fs::write(p.join("regex").join("b.json"), "{}").unwrap();

        let s = tmp.path().join("scenes").join("tavern");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(
            s.join("scene.json"),
            r#"{"scene_id":"tavern","characters":[{"character_id":"a"},{"character_id":"b"}]}"#,
        )
        .unwrap();

        let r = run_diagnose(tmp.path(), None, None);
        let presets = r["presets"].as_array().unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0]["id"], "LENI");
        assert_eq!(presets[0]["regex_scripts"], 2);
        let scenes = r["scenes"].as_array().unwrap();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0]["id"], "tavern");
        assert_eq!(scenes[0]["characters_count"], 2);
    }
}
