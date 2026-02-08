use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use server_manager::{read_server_properties, write_server_properties, WorldInfo};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::{db, routes::servers::ServerError, state::AppState};

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct WorldsResponse {
    pub worlds: Vec<WorldInfo>,
}

#[derive(Debug, Deserialize)]
pub struct BackupWorldRequest {
    pub world_name: String,
}

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub backup_name: String,
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<ConfigResponse>, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);
    let properties_path = server_dir.join("server.properties");

    let properties = read_server_properties(&properties_path)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(ConfigResponse { properties }))
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateConfigRequest>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);
    let properties_path = server_dir.join("server.properties");

    write_server_properties(&properties_path, &payload.properties)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn list_worlds(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorldsResponse>, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    let worlds = server_manager::list_worlds(&server_dir)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(WorldsResponse { worlds }))
}

pub async fn backup_world(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<BackupWorldRequest>,
) -> Result<Json<BackupResponse>, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    let backup_name = server_manager::backup_world(&server_dir, &payload.world_name)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(BackupResponse { backup_name }))
}

pub async fn delete_world(
    State(state): State<Arc<AppState>>,
    Path((id, world_name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    server_manager::delete_world(&server_dir, &world_name)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn upload_world(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);
    let mut world_name = String::new();
    let mut zip_data = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to get next field: {}", e);
        ServerError::Internal(format!("Multipart error: {}", e))
    })? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "name" {
            world_name = field.text().await.map_err(|e| {
                tracing::error!("Failed to get world name text: {}", e);
                ServerError::Internal(e.to_string())
            })?;
        } else if name == "file" {
            zip_data = field.bytes().await.map_err(|e| {
                tracing::error!("Failed to get zip data bytes: {}", e);
                ServerError::Internal(e.to_string())
            })?.to_vec();
        }
    }

    if world_name.is_empty() || zip_data.is_empty() {
        tracing::error!("Missing world name or file (name: {}, data size: {})", world_name, zip_data.len());
        return Err(ServerError::Internal("Missing world name or file".to_string()));
    }

    tokio::task::spawn_blocking(move || {
        server_manager::upload_world(&server_dir, &world_name, zip_data)
    })
    .await
    .map_err(|e| ServerError::Internal(e.to_string()))?
    .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

pub async fn set_default_world(
    State(state): State<Arc<AppState>>,
    Path((id, world_name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);
    let properties_path = server_dir.join("server.properties");

    let mut properties = server_manager::read_server_properties(&properties_path)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    properties.insert("level-name".to_string(), world_name);

    server_manager::write_server_properties(&properties_path, &properties)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}
