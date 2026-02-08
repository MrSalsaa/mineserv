use server_manager::{ServerInstance, ServerProcess, ServerMonitor};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct AppState {
    pub db: SqlitePool,
    pub servers_dir: PathBuf,
    pub admin_password: String,
    pub jwt_secret: String,
    pub servers: Arc<RwLock<HashMap<Uuid, ServerInstance>>>,
    pub processes: Arc<RwLock<HashMap<Uuid, Arc<ServerProcess>>>>,
    pub monitors: Arc<RwLock<HashMap<Uuid, ServerMonitor>>>,
}

impl AppState {
    pub fn new(
        db: SqlitePool,
        servers_dir: PathBuf,
        admin_password: String,
        jwt_secret: String,
    ) -> Self {
        Self {
            db,
            servers_dir,
            admin_password,
            jwt_secret,
            servers: Arc::new(RwLock::new(HashMap::new())),
            processes: Arc::new(RwLock::new(HashMap::new())),
            monitors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn recover_processes(self: Arc<Self>) -> anyhow::Result<()> {
        use crate::db;
        use server_manager::{ServerState, ServerInstance, ServerProcess, ServerMonitor};

        let servers = db::list_servers(&self.db).await?;
        
        for config in servers {
            let server_dir = config.server_dir(&self.servers_dir);
            let pid_path = server_dir.join("server.pid");
            
            if pid_path.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        // Check if process is still alive using kill -0
                        if unsafe { libc::kill(pid as i32, 0) == 0 } {
                            tracing::info!("Recovering server '{}' (PID {})", config.name, pid);
                            
                            let process = Arc::new(ServerProcess::from_pid(config.clone(), self.servers_dir.clone(), pid));
                            let mut instance = ServerInstance::new(config.clone());
                            instance.state = ServerState::Running;
                            instance.pid = Some(pid);
                            
                            self.servers.write().await.insert(config.id, instance);
                            self.processes.write().await.insert(config.id, process);
                            
                            // Re-start monitoring
                            let mut monitor = ServerMonitor::new();
                            monitor.reset_uptime();
                            self.monitors.write().await.insert(config.id, monitor);
                            
                            // Spawn a supervisor to handle cleanup if it exits
                            let state_clone = self.clone();
                            let config_id = config.id;
                            let pid_path_clone = pid_path.clone();
                            tokio::spawn(async move {
                                // Polling wait logic for recovered process (no child handle)
                                loop {
                                    if unsafe { libc::kill(pid as i32, 0) != 0 } {
                                        break;
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                }
                                
                                tracing::info!("Recovered server {} exited", config_id);
                                if let Some(instance) = state_clone.servers.write().await.get_mut(&config_id) {
                                    instance.state = ServerState::Stopped;
                                    instance.pid = None;
                                }
                                state_clone.processes.write().await.remove(&config_id);
                                state_clone.monitors.write().await.remove(&config_id);
                                let _ = tokio::fs::remove_file(pid_path_clone).await;
                            });
                        } else {
                            // PID file exists but process is dead, clean it up
                            let _ = tokio::fs::remove_file(pid_path).await;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

