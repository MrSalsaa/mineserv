use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use server_manager::{
    download_server_jar, get_available_versions, initialize_server_properties, ServerConfig,
    ServerInstance, ServerMonitor, ServerProcess, ServerState, ServerType,
};
use std::sync::Arc;
use tokio::fs;
use uuid::Uuid;

use crate::{db, state::AppState, routes::plugins::InstallPluginRequest};

#[derive(Debug, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub server_type: ServerType,
    pub minecraft_version: String,
    pub port: Option<u16>,
    pub max_players: Option<u32>,
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ServerResponse {
    pub id: Uuid,
    pub name: String,
    pub server_type: ServerType,
    pub minecraft_version: String,
    pub port: u16,
    pub state: ServerState,
    pub players_online: u32,
}

#[derive(Debug, Serialize)]
pub struct VersionsResponse {
    pub versions: Vec<String>,
}

pub async fn get_versions(
    State(_state): State<Arc<AppState>>,
    Path(server_type): Path<String>,
) -> Result<Json<VersionsResponse>, ServerError> {
    let server_type = match server_type.as_str() {
        "paper" => ServerType::Paper,
        "spigot" => ServerType::Spigot,
        _ => return Err(ServerError::InvalidServerType),
    };

    let versions = get_available_versions(server_type)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(Json(VersionsResponse { versions }))
}

pub async fn create_server(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateServerRequest>,
) -> Result<Json<ServerResponse>, ServerError> {
    let mut config = ServerConfig::new(
        payload.name,
        payload.server_type,
        payload.minecraft_version,
    );

    if let Some(port) = payload.port {
        config.port = port;
    }
    if let Some(max_players) = payload.max_players {
        config.max_players = max_players;
    }
    if let Some(memory_mb) = payload.memory_mb {
        config.memory_mb = memory_mb;
    }

    // Create server directory
    let server_dir = config.server_dir(&state.servers_dir);
    fs::create_dir_all(&server_dir)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Download server JAR
    let jar_path = server_dir.join("server.jar");
    download_server_jar(config.server_type, &config.minecraft_version, &jar_path)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Initialize server.properties
    initialize_server_properties(&server_dir, config.port, config.max_players)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Save to database
    db::create_server(&state.db, &config)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Create instance
    let instance = ServerInstance::new(config.clone());
    state.servers.write().await.insert(config.id, instance.clone());

    Ok(Json(ServerResponse {
        id: instance.config.id,
        name: instance.config.name,
        server_type: instance.config.server_type,
        minecraft_version: instance.config.minecraft_version,
        port: instance.config.port,
        state: instance.state,
        players_online: instance.players_online,
    }))
}

pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ServerResponse>>, ServerError> {
    let configs = db::list_servers(&state.db)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    let servers = state.servers.read().await;
    let mut response = Vec::new();

    for config in configs {
        let instance = servers.get(&config.id);
        
        response.push(ServerResponse {
            id: config.id,
            name: config.name.clone(),
            server_type: config.server_type,
            minecraft_version: config.minecraft_version.clone(),
            port: config.port,
            state: instance.map(|i| i.state).unwrap_or(ServerState::Stopped),
            players_online: instance.map(|i| i.players_online).unwrap_or(0),
        });
    }

    Ok(Json(response))
}

pub async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<ServerResponse>, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let servers = state.servers.read().await;
    let instance = servers.get(&id);

    Ok(Json(ServerResponse {
        id: config.id,
        name: config.name,
        server_type: config.server_type,
        minecraft_version: config.minecraft_version,
        port: config.port,
        state: instance.map(|i| i.state).unwrap_or(ServerState::Stopped),
        players_online: instance.map(|i| i.players_online).unwrap_or(0),
    }))
}

pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ServerError> {
    // Stop server if running
    let processes = state.processes.read().await;
    if let Some(process) = processes.get(&id) {
        if process.is_running().await {
            return Err(ServerError::ServerRunning);
        }
    }
    drop(processes);

    // Get config for directory path
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    // Delete from database
    db::delete_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Remove from memory
    state.servers.write().await.remove(&id);
    state.processes.write().await.remove(&id);
    state.monitors.write().await.remove(&id);

    // Delete server directory
    let server_dir = config.server_dir(&state.servers_dir);
    if server_dir.exists() {
        fs::remove_dir_all(&server_dir)
            .await
            .map_err(|e| ServerError::Internal(e.to_string()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn start_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ServerError> {
    let config = db::get_server(&state.db, id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?
        .ok_or(ServerError::NotFound)?;

    let mut processes = state.processes.write().await;
    let mut servers = state.servers.write().await;
    let mut monitors = state.monitors.write().await;

    // Check if already running
    if let Some(process) = processes.get(&id) {
        if process.is_running().await {
            return Err(ServerError::AlreadyRunning);
        }
    }

    // Create process
    let mut process = ServerProcess::new(config.clone(), state.servers_dir.clone());
    let pid = process
        .start()
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    // Update instance
    if let Some(instance) = servers.get_mut(&id) {
        instance.state = ServerState::Running;
        instance.pid = Some(pid);
    } else {
        let mut instance = ServerInstance::new(config);
        instance.state = ServerState::Running;
        instance.pid = Some(pid);
        servers.insert(id, instance);
    }

    // Create monitor
    let mut monitor = ServerMonitor::new();
    monitor.reset_uptime();
    monitors.insert(id, monitor);

    // Spawn supervisor task
    let state_clone = state.clone();
    let id_clone = id;
    let child_arc = process.get_child();
    
    tokio::spawn(async move {
        let mut child_guard = child_arc.write().await;
        if let Some(mut child) = child_guard.take() {
            drop(child_guard); // Release lock while waiting
            let _ = child.wait().await;
            tracing::info!("Server {} process exited", id_clone);
            
            // Update state to Stopped
            let mut servers = state_clone.servers.write().await;
            if let Some(instance) = servers.get_mut(&id_clone) {
                instance.state = ServerState::Stopped;
                instance.pid = None;
            }
            
            // Cleanup process and monitor
            state_clone.processes.write().await.remove(&id_clone);
            state_clone.monitors.write().await.remove(&id_clone);
        }
    });

    processes.insert(id, Arc::new(process));

    Ok(StatusCode::OK)
}

pub async fn stop_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ServerError> {
    let processes = state.processes.read().await;
    let process = processes.get(&id).ok_or(ServerError::NotRunning)?;

    process
        .stop()
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    drop(processes);

    // Update instance state
    let mut servers = state.servers.write().await;
    if let Some(instance) = servers.get_mut(&id) {
        instance.state = ServerState::Stopping;
    }

    Ok(StatusCode::OK)
}

pub async fn force_stop_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ServerError> {
    let processes = state.processes.read().await;
    let process = processes.get(&id).ok_or(ServerError::NotRunning)?;

    process
        .force_stop()
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    drop(processes);

    // Update instance state
    let mut servers = state.servers.write().await;
    if let Some(instance) = servers.get_mut(&id) {
        instance.state = ServerState::Stopped;
        instance.pid = None;
    }

    Ok(StatusCode::OK)
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
    
    // Pass config.server_type to install_plugin
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

pub async fn restart_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ServerError> {
    // 1. Get the process
    let processes = state.processes.read().await;
    let process = if let Some(p) = processes.get(&id) {
        p.clone()
    } else {
        // If not in processes map, maybe it's stopped already. 
        // Just try starting it.
        drop(processes);
        return start_server(State(state), Path(id)).await;
    };
    drop(processes);

    // 2. Stop it gracefully
    process.stop().await.map_err(|e| ServerError::Internal(e.to_string()))?;

    // 3. Wait for it to actually exit
    // Set a timeout so we don't wait forever if it hangs
    let wait_result = tokio::time::timeout(tokio::time::Duration::from_secs(30), process.wait()).await;
    
    if let Err(_) = wait_result {
        tracing::warn!("Server {} stop timed out, force stopping", id);
        let _ = process.force_stop().await;
    }

    // 4. Update instance state locally
    {
        let mut servers = state.servers.write().await;
        if let Some(instance) = servers.get_mut(&id) {
            instance.state = ServerState::Stopped;
            instance.pid = None;
        }
    }

    // 5. Start again
    start_server(State(state), Path(id)).await?;

    Ok(StatusCode::OK)
}


#[derive(Debug)]
pub enum ServerError {
    NotFound,
    AlreadyRunning,
    NotRunning,
    ServerRunning,
    InvalidServerType,
    Internal(String),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ServerError::NotFound => (StatusCode::NOT_FOUND, "Server not found"),
            ServerError::AlreadyRunning => (StatusCode::CONFLICT, "Server already running"),
            ServerError::NotRunning => (StatusCode::CONFLICT, "Server not running"),
            ServerError::ServerRunning => (StatusCode::CONFLICT, "Cannot delete running server"),
            ServerError::InvalidServerType => (StatusCode::BAD_REQUEST, "Invalid server type"),
            ServerError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        (status, message).into_response()
    }
}
