use axum::{extract::{Path, State}, Json};
use serde::Serialize;
use server_manager::ServerStats;
use std::sync::Arc;
use uuid::Uuid;

use crate::{routes::servers::ServerError, state::AppState};

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub stats: Option<ServerStats>,
}

pub async fn get_server_stats(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<StatsResponse>, ServerError> {
    let servers = state.servers.read().await;
    let instance = servers.get(&id).ok_or(ServerError::NotFound)?;

    if let Some(pid) = instance.pid {
        let mut monitors = state.monitors.write().await;
        
        if let Some(monitor) = monitors.get_mut(&id) {
            let stats = monitor
                .get_stats(pid)
                .ok();

            Ok(Json(StatsResponse { stats }))
        } else {
            Ok(Json(StatsResponse { stats: None }))
        }
    } else {
        Ok(Json(StatsResponse { stats: None }))
    }
}

#[derive(Debug, Serialize)]
pub struct SystemStatsResponse {
    pub total_servers: usize,
    pub running_servers: usize,
    pub total_cpu_percent: f32,
    pub total_memory_mb: u64,
}

pub async fn get_system_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SystemStatsResponse>, ServerError> {
    let servers = state.servers.read().await;
    let mut monitors = state.monitors.write().await;

    let total_servers = servers.len();
    let running_servers = servers
        .values()
        .filter(|s| s.pid.is_some())
        .count();

    let mut total_cpu = 0.0f32;
    let mut total_memory = 0u64;

    for (id, instance) in servers.iter() {
        if let Some(pid) = instance.pid {
            if let Some(monitor) = monitors.get_mut(id) {
                if let Ok(stats) = monitor.get_stats(pid) {
                    total_cpu += stats.cpu_percent;
                    total_memory += stats.memory_mb;
                }
            }
        }
    }

    Ok(Json(SystemStatsResponse {
        total_servers,
        running_servers,
        total_cpu_percent: total_cpu,
        total_memory_mb: total_memory,
    }))
}
