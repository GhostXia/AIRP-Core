/// DX-5: Cross-device sync helpers.
///
/// `GET /v1/sync/manifest` — lists all syncable files with their SHA-256 and size.
/// `GET /v1/sync/blob/:hash`  — returns raw file content located by SHA-256.
///
/// Syncable scope: `data/{characters,presets,scenes}/` (recursive).
/// Clients can implement rsync-style delta sync by comparing manifests and
/// fetching only blobs whose hash differs from their local copy.
use crate::error::AirpError;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path as FsPath;
use std::sync::Arc;

use crate::daemon::DaemonState;

/// One entry in the manifest: relative path, SHA-256 hex, and byte size.
#[derive(Debug, Serialize)]
pub struct SyncEntry {
    /// Path relative to `data_root`, using forward slashes (OS-independent).
    pub path: String,
    /// Lowercase hex-encoded SHA-256 of the file contents.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
}

/// Compute SHA-256 hex of a file.
fn sha256_file(path: &FsPath) -> Result<String, std::io::Error> {
    let bytes = std::fs::read(path)?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("{:x}", digest))
}

/// Walk `root` recursively, returning `SyncEntry` for every regular file.
/// `rel_base` is the prefix to strip to produce the relative path in the entry.
fn walk_dir(root: &FsPath, rel_base: &FsPath, entries: &mut Vec<SyncEntry>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk_dir(&path, rel_base, entries);
        } else if ft.is_file() {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let Ok(hash) = sha256_file(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(rel_base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(SyncEntry {
                path: rel,
                sha256: hash,
                size: meta.len(),
            });
        }
    }
}

/// Build manifest for `data/{characters,presets,scenes}/` under `data_root`.
pub fn build_manifest(data_root: &FsPath) -> Vec<SyncEntry> {
    let mut entries = Vec::new();
    for subdir in &["characters", "presets", "scenes"] {
        let dir = data_root.join(subdir);
        if dir.exists() {
            walk_dir(&dir, data_root, &mut entries);
        }
    }
    // Stable order for diffing
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

/// Find the first file whose SHA-256 matches `hash` and return its contents.
fn find_blob(data_root: &FsPath, hash: &str) -> Option<Vec<u8>> {
    let mut found = None;
    for subdir in &["characters", "presets", "scenes"] {
        let dir = data_root.join(subdir);
        if dir.exists() {
            find_blob_in(&dir, hash, &mut found);
        }
        if found.is_some() {
            break;
        }
    }
    found
}

fn find_blob_in(root: &FsPath, hash: &str, out: &mut Option<Vec<u8>>) {
    if out.is_some() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        if out.is_some() {
            return;
        }
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            find_blob_in(&path, hash, out);
        } else if ft.is_file() {
            if let Ok(h) = sha256_file(&path) {
                if h == hash {
                    *out = std::fs::read(&path).ok();
                }
            }
        }
    }
}

// ─── axum handlers ──────────────────────────────────────────────────────────

/// `GET /v1/sync/manifest`
pub async fn sync_manifest_handler(
    State(state): State<Arc<DaemonState>>,
) -> Result<Json<Vec<SyncEntry>>, AirpError> {
    let entries = build_manifest(&state.data_root);
    Ok(Json(entries))
}

/// `GET /v1/sync/blob/:hash`
pub async fn sync_blob_handler(
    State(state): State<Arc<DaemonState>>,
    Path(hash): Path<String>,
) -> impl IntoResponse {
    // Validate hash: must be 64 hex chars (SHA-256)
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return (StatusCode::BAD_REQUEST, "invalid hash").into_response();
    }
    match find_blob(&state.data_root, &hash) {
        Some(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            Bytes::from(bytes),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "blob not found").into_response(),
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_dx5 {
    use super::*;
    use std::fs;

    fn tmp_data_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_build_manifest_empty() {
        let dir = tmp_data_root();
        // create subdirs so walk doesn't bail out
        fs::create_dir_all(dir.path().join("characters")).unwrap();
        fs::create_dir_all(dir.path().join("presets")).unwrap();
        fs::create_dir_all(dir.path().join("scenes")).unwrap();
        let entries = build_manifest(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_build_manifest_detects_files() {
        let dir = tmp_data_root();
        let chars_dir = dir.path().join("characters").join("alice");
        fs::create_dir_all(&chars_dir).unwrap();
        fs::write(chars_dir.join("card.json"), br#"{"name":"Alice"}"#).unwrap();

        let entries = build_manifest(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.contains("alice/card.json") || entries[0].path.contains("alice\\card.json") || entries[0].path.ends_with("card.json"));
        assert_eq!(entries[0].sha256.len(), 64);
        assert_eq!(entries[0].size, 16);
    }

    #[test]
    fn test_sha256_deterministic() {
        let dir = tmp_data_root();
        let chars_dir = dir.path().join("characters").join("bob");
        fs::create_dir_all(&chars_dir).unwrap();
        let content = b"hello world";
        fs::write(chars_dir.join("card.json"), content).unwrap();

        let entries = build_manifest(dir.path());
        use sha2::{Digest, Sha256};
        let expected = format!("{:x}", Sha256::digest(content));
        assert_eq!(entries[0].sha256, expected);
    }

    #[test]
    fn test_find_blob_returns_content() {
        let dir = tmp_data_root();
        let chars_dir = dir.path().join("characters").join("carol");
        fs::create_dir_all(&chars_dir).unwrap();
        let content = b"test blob content";
        fs::write(chars_dir.join("data.json"), content).unwrap();

        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(content));
        let result = find_blob(dir.path(), &hash);
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_find_blob_missing_returns_none() {
        let dir = tmp_data_root();
        fs::create_dir_all(dir.path().join("characters")).unwrap();
        let result = find_blob(dir.path(), &"a".repeat(64));
        assert!(result.is_none());
    }

    #[test]
    fn test_manifest_sorted() {
        let dir = tmp_data_root();
        let chars_dir = dir.path().join("characters");
        fs::create_dir_all(chars_dir.join("zebra")).unwrap();
        fs::create_dir_all(chars_dir.join("alice")).unwrap();
        fs::write(chars_dir.join("zebra").join("card.json"), b"z").unwrap();
        fs::write(chars_dir.join("alice").join("card.json"), b"a").unwrap();

        let entries = build_manifest(dir.path());
        assert_eq!(entries.len(), 2);
        assert!(entries[0].path < entries[1].path, "manifest must be sorted");
    }
}
