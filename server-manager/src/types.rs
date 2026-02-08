use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerType {
    Paper,
    Spigot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: Uuid,
    pub name: String,
    pub server_type: ServerType,
    pub minecraft_version: String,
    pub port: u16,
    pub max_players: u32,
    pub memory_mb: u32,
    pub auto_start: bool,
    pub properties: HashMap<String, String>,
}

impl ServerConfig {
    pub fn new(name: String, server_type: ServerType, minecraft_version: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            server_type,
            minecraft_version,
            port: 25565,
            max_players: 20,
            memory_mb: 2048,
            auto_start: false,
            properties: HashMap::new(),
        }
    }

    pub fn server_dir(&self, base_dir: &PathBuf) -> PathBuf {
        base_dir.join(self.id.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInstance {
    pub config: ServerConfig,
    pub state: ServerState,
    pub pid: Option<u32>,
    pub players_online: u32,
}

impl ServerInstance {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            state: ServerState::Stopped,
            pid: None,
            players_online: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStats {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldInfo {
    pub name: String,
    pub size_mb: u64,
    pub last_modified: u64,
}
