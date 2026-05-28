use crate::error::AirpError;
use std::fs;
use std::path::{Path, PathBuf};

pub fn safe_resolve_under_data_root(
    data_root: &Path,
    user_path: &str,
) -> Result<PathBuf, AirpError> {
    let trimmed = user_path.trim();
    if trimmed.is_empty() {
        return Err(AirpError::BadRequest("路径为空".to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    let looks_absolute = trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || (lower.len() >= 2 && lower.as_bytes()[1] == b':');
    if looks_absolute {
        return Err(AirpError::BadRequest(format!(
            "拒绝绝对路径: {}",
            user_path
        )));
    }
    if trimmed.contains('\0') {
        return Err(AirpError::BadRequest("路径包含空字节".to_string()));
    }

    let candidate = data_root.join(trimmed);
    let canon_root = fs::canonicalize(data_root)?;
    let canon_candidate = fs::canonicalize(&candidate)?;
    if !canon_candidate.starts_with(&canon_root) {
        return Err(AirpError::PathEscape(canon_candidate));
    }
    Ok(canon_candidate)
}

/// 写路径安全解析：允许目标文件不存在。
///
/// 与 [`safe_resolve_under_data_root`] 的区别：
/// - `data_root`（基目录）必须存在（做 canonicalize 锚点）。
/// - 目标文件/目录**可以不存在**（通过组件级展开替代 canonicalize）。
/// - 仍拒绝绝对路径、`..` 穿越、空字节。
///
/// 用于 `write_preset_artifact` / `write_character_artifact` 等写新文件场景。
pub fn safe_resolve_for_write(
    base_dir: &Path,
    user_path: &str,
) -> Result<PathBuf, AirpError> {
    let trimmed = user_path.trim();
    if trimmed.is_empty() {
        return Err(AirpError::BadRequest("路径为空".to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    let looks_absolute = trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || (lower.len() >= 2 && lower.as_bytes()[1] == b':');
    if looks_absolute {
        return Err(AirpError::BadRequest(format!("拒绝绝对路径: {}", user_path)));
    }
    if trimmed.contains('\0') {
        return Err(AirpError::BadRequest("路径包含空字节".to_string()));
    }

    // 仅对基目录做 canonicalize（基目录必须存在）
    let canon_base = fs::canonicalize(base_dir)?;

    // 组件级展开：逐段处理 user_path，`..` 弹栈；超出根则拒绝
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if stack.pop().is_none() {
                    return Err(AirpError::PathEscape(canon_base.join(trimmed)));
                }
            }
            std::path::Component::Normal(s) => stack.push(s.to_owned()),
            _ => {
                return Err(AirpError::BadRequest(format!("非法路径组件: {}", user_path)))
            }
        }
    }
    if stack.is_empty() {
        return Err(AirpError::BadRequest("路径解析为空".to_string()));
    }
    let resolved = stack.iter().fold(canon_base.clone(), |acc, c| acc.join(c));
    // 双重保险：即使组件展开有漏洞，starts_with 仍阻挡穿越
    if !resolved.starts_with(&canon_base) {
        return Err(AirpError::PathEscape(resolved));
    }
    Ok(resolved)
}

pub fn validate_id_segment(id: &str) -> Result<(), AirpError> {
    if id.is_empty() {
        return Err(AirpError::BadRequest("ID 为空".to_string()));
    }
    if id == "." || id == ".." {
        return Err(AirpError::BadRequest(format!("非法 ID: {}", id)));
    }
    if id.starts_with('.') {
        return Err(AirpError::BadRequest(format!("ID 不允许以点开头: {}", id)));
    }
    for c in id.chars() {
        match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => {
                return Err(AirpError::BadRequest(format!(
                    "ID 含非法字符 {:?}: {}",
                    c, id
                )));
            }
            _ => {}
        }
    }
    if id.contains("..") {
        return Err(AirpError::BadRequest(format!("ID 含 ..: {}", id)));
    }
    Ok(())
}
