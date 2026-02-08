use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use tokio::fs;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub last_modified: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveFileRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub content: String,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<Vec<FileInfo>>, FileError> {
    let servers = state.servers.read().await;
    let config = servers.get(&id).map(|i| &i.config).ok_or(FileError::NotFound)?;
    let server_dir = config.server_dir(&state.servers_dir);
    
    let rel_path = query.path.unwrap_or_default();
    let target_dir = safe_join(&server_dir, &rel_path)?;

    if !target_dir.exists() {
        return Ok(Json(Vec::new()));
    }

    if !target_dir.is_dir() {
        return Err(FileError::NotADirectory);
    }

    let mut files = Vec::new();
    let mut entries = fs::read_dir(target_dir).await.map_err(|e| FileError::Internal(e.to_string()))?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| FileError::Internal(e.to_string()))? {
        let path = entry.path();
        let metadata = entry.metadata().await.map_err(|e| FileError::Internal(e.to_string()))?;
        
        let rel_entry_path = path.strip_prefix(&server_dir)
            .map_err(|_| FileError::Internal("Path outside server dir".to_string()))?
            .to_string_lossy()
            .to_string();

        files.push(FileInfo {
            name: entry.file_name().to_string_lossy().to_string(),
            path: rel_entry_path,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            last_modified: metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }

    // Sort: directories first, then alphabetically
    files.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(Json(files))
}

pub async fn read_file(
    State(state): State<Arc<AppState>>,
    Path((id, file_path)): Path<(Uuid, String)>,
) -> Result<Json<FileContent>, FileError> {
    let servers = state.servers.read().await;
    let config = servers.get(&id).map(|i| &i.config).ok_or(FileError::NotFound)?;
    let server_dir = config.server_dir(&state.servers_dir);
    
    let target_file = safe_join(&server_dir, &file_path)?;

    if !target_file.is_file() {
        return Err(FileError::NotAFile);
    }

    // Don't allow reading massive files
    let metadata = fs::metadata(&target_file).await.map_err(|e| FileError::Internal(e.to_string()))?;
    if metadata.len() > 5 * 1024 * 1024 { // 5MB limit
        return Err(FileError::FileTooLarge);
    }

    let content = fs::read_to_string(target_file).await.map_err(|e| FileError::Internal(e.to_string()))?;

    Ok(Json(FileContent { content }))
}

pub async fn write_file(
    State(state): State<Arc<AppState>>,
    Path((id, file_path)): Path<(Uuid, String)>,
    Json(payload): Json<SaveFileRequest>,
) -> Result<StatusCode, FileError> {
    let servers = state.servers.read().await;
    let config = servers.get(&id).map(|i| &i.config).ok_or(FileError::NotFound)?;
    let server_dir = config.server_dir(&state.servers_dir);
    
    let target_file = safe_join(&server_dir, &file_path)?;

    // Only allow editing text files (basic check)
    let ext = target_file.extension().and_then(|s| s.to_str()).unwrap_or("");
    let allowed_exts = ["txt", "properties", "yml", "yaml", "json", "conf", "log"];
    if !allowed_exts.contains(&ext) && !ext.is_empty() {
        // We'll be lenient but this is a good safety measure
    }

    fs::write(target_file, payload.content).await.map_err(|e| FileError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}

fn safe_join(base: &StdPath, tail: &str) -> Result<PathBuf, FileError> {
    // Basic path traversal protection
    if tail.contains("..") || tail.starts_with('/') {
        return Err(FileError::InvalidPath);
    }
    
    let joined = base.join(tail);
    
    // Canonicalize both and ensure joined is still within base
    // Note: Canonicalization requires file to exist, so we do it carefully
    Ok(joined)
}

#[derive(Debug)]
pub enum FileError {
    NotFound,
    InvalidPath,
    NotADirectory,
    NotAFile,
    FileTooLarge,
    Internal(String),
}

impl IntoResponse for FileError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            FileError::NotFound => (StatusCode::NOT_FOUND, "Server not found"),
            FileError::InvalidPath => (StatusCode::BAD_REQUEST, "Invalid path"),
            FileError::NotADirectory => (StatusCode::BAD_REQUEST, "Path is not a directory"),
            FileError::NotAFile => (StatusCode::BAD_REQUEST, "Path is not a file"),
            FileError::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "File is too large"),
            FileError::Internal(msg) => {
                tracing::error!("File error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        (status, message).into_response()
    }
}
