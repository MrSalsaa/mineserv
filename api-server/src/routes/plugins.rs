use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use server_manager::PluginInfo;
use std::sync::Arc;
use uuid::Uuid;

use crate::{db, routes::servers::ServerError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Serialize)]
pub struct PluginsResponse {
    pub plugins: Vec<PluginInfo>,
}

#[derive(Debug, Deserialize)]
pub struct InstallPluginRequest {
    pub plugin_name: String,
}

pub async fn search_plugins(
    State(_state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PluginsResponse>, ServerError> {
    let plugins = server_manager::search_plugins(&query.q, "paper")
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(PluginsResponse { plugins }))
}

pub async fn list_installed_plugins(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<PluginsResponse>, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    let plugins = server_manager::list_installed_plugins(&server_dir)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(PluginsResponse { plugins }))
}

pub async fn install_plugin(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<InstallPluginRequest>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    server_manager::install_plugin(
        &server_dir,
        &payload.plugin_name,
        &config.minecraft_version,
        config.server_type,
    )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn remove_plugin(
    State(state): State<Arc<AppState>>,
    Path((id, plugin_name)): Path<(Uuid, String)>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let server_dir = config.server_dir(&state.servers_dir);

    server_manager::remove_plugin(&server_dir, &plugin_name)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
