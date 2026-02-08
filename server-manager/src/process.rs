use crate::types::ServerConfig;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

pub struct ServerProcess {
    config: ServerConfig,
    base_dir: PathBuf,
    child: Arc<RwLock<Option<Child>>>,
    stdin_tx: Option<mpsc::UnboundedSender<String>>,
    output_tx: broadcast::Sender<String>,
}

impl ServerProcess {
    pub fn new(config: ServerConfig, base_dir: PathBuf) -> Self {
        let (output_tx, _) = broadcast::channel(1000);
        Self {
            config,
            base_dir,
            child: Arc::new(RwLock::new(None)),
            stdin_tx: None,
            output_tx,
        }
    }

    /// Create a ServerProcess from an existing PID (recovery scenario)
    pub fn from_pid(config: ServerConfig, base_dir: PathBuf, _pid: u32) -> Self {
        let (output_tx, _) = broadcast::channel(1000);
        
        // Note: For recovered processes, we currenty don't have access to stdin/stdout
        // as they were owned by the previous parent process.
        // Future improvement: Use named pipes or tmux/screen for persistent I/O.
        
        Self {
            config,
            base_dir,
            child: Arc::new(RwLock::new(None)), // We don't have the Child object for recovered processes
            stdin_tx: None,
            output_tx,
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<u32> {
        let server_dir = self.config.server_dir(&self.base_dir);
        let jar_path = server_dir.join("server.jar");

        if !jar_path.exists() {
            anyhow::bail!("Server JAR not found at {:?}", jar_path);
        }

        // Accept EULA
        let eula_path = server_dir.join("eula.txt");
        tokio::fs::write(&eula_path, "eula=true\n")
            .await
            .context("Failed to write eula.txt")?;

        // Build JVM arguments
        let memory_arg = format!("-Xmx{}M", self.config.memory_mb);
        let min_memory_arg = format!("-Xms{}M", self.config.memory_mb / 2);

        let mut child = Command::new("java")
            .arg(&min_memory_arg)
            .arg(&memory_arg)
            .arg("-XX:+UseG1GC")
            .arg("-XX:+ParallelRefProcEnabled")
            .arg("-XX:MaxGCPauseMillis=200")
            .arg("-XX:+UnlockExperimentalVMOptions")
            .arg("-XX:+DisableExplicitGC")
            .arg("-XX:+AlwaysPreTouch")
            .arg("-XX:G1NewSizePercent=30")
            .arg("-XX:G1MaxNewSizePercent=40")
            .arg("-XX:G1HeapRegionSize=8M")
            .arg("-XX:G1ReservePercent=20")
            .arg("-XX:G1HeapWastePercent=5")
            .arg("-XX:G1MixedGCCountTarget=4")
            .arg("-XX:InitiatingHeapOccupancyPercent=15")
            .arg("-XX:G1MixedGCLiveThresholdPercent=90")
            .arg("-XX:G1RSetUpdatingPauseTimePercent=5")
            .arg("-XX:SurvivorRatio=32")
            .arg("-XX:+PerfDisableSharedMem")
            .arg("-XX:MaxTenuringThreshold=1")
            .arg("-XX:+ExitOnOutOfMemoryError")
            .arg("-Dusing.aikars.flags=https://mcflags.emc.gs")
            .arg("-Daikars.new.flags=true")
            .arg("-jar")
            .arg("server.jar")
            .arg("--nogui")
            .current_dir(&server_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn server process")?;

        let pid = child.id().context("Failed to get process ID")?;

        // Write PID file
        let pid_path = server_dir.join("server.pid");
        tokio::fs::write(&pid_path, pid.to_string())
            .await
            .context("Failed to write PID file")?;

        // Set up stdin channel
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
        let mut stdin = child.stdin.take().context("Failed to get stdin")?;

        tokio::spawn(async move {
            while let Some(command) = stdin_rx.recv().await {
                if let Err(e) = stdin.write_all(command.as_bytes()).await {
                    tracing::error!("Failed to write to server stdin: {}", e);
                    break;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    tracing::error!("Failed to write newline to server stdin: {}", e);
                    break;
                }
            }
        });

        // Set up stdout/stderr streaming
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stderr = child.stderr.take().context("Failed to get stderr")?;

        let output_tx = self.output_tx.clone();
        let output_tx_err = output_tx.clone();
        
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = LinesStream::new(reader.lines());
            while let Some(Ok(line)) = lines.next().await {
                let _ = output_tx.send(line);
            }
        });

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = LinesStream::new(reader.lines());
            while let Some(Ok(line)) = lines.next().await {
                let _ = output_tx_err.send(format!("[ERROR] {}", line));
            }
        });

        *self.child.write().await = Some(child);
        self.stdin_tx = Some(stdin_tx);

        Ok(pid)
    }

    pub async fn send_command(&self, command: String) -> Result<()> {
        if let Some(tx) = &self.stdin_tx {
            tx.send(command)
                .context("Failed to send command to server")?;
            Ok(())
        } else {
            // For recovered processes, we can't send commands via stdin easily
            anyhow::bail!("Server is running but I/O is not attached (recovered process)")
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.send_command("stop".to_string()).await?;
        Ok(())
    }

    pub async fn force_stop(&self) -> Result<()> {
        let mut child_guard = self.child.write().await;
        if let Some(child) = child_guard.as_mut() {
            child.kill().await.context("Failed to kill server process")?;
            *child_guard = None;
        } else {
            // Try killing by PID if we have it in recovery or if child is lost
            // We'd need to store PID in ServerProcess or read it from file
            let server_dir = self.config.server_dir(&self.base_dir);
            let pid_path = server_dir.join("server.pid");
            if pid_path.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
            }
        }
        
        // Clean up PID file
        let server_dir = self.config.server_dir(&self.base_dir);
        let _ = tokio::fs::remove_file(server_dir.join("server.pid")).await;
        
        Ok(())
    }

    pub async fn wait(&self) -> Result<()> {
        let mut child_guard = self.child.write().await;
        if let Some(child) = child_guard.as_mut() {
            child.wait().await.context("Failed to wait for server")?;
            *child_guard = None;
            
            // Cleanup PID file
            let server_dir = self.config.server_dir(&self.base_dir);
            let _ = tokio::fs::remove_file(server_dir.join("server.pid")).await;
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.output_tx.subscribe()
    }

    pub fn get_child(&self) -> Arc<RwLock<Option<Child>>> {
        self.child.clone()
    }

    pub async fn is_running(&self) -> bool {
        // Check if child exists and is running
        if self.child.read().await.is_some() {
            return true;
        }
        
        // If no child (recovery), check if PID is alive
        let server_dir = self.config.server_dir(&self.base_dir);
        let pid_path = server_dir.join("server.pid");
        if pid_path.exists() {
            if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    // Check if process exists using kill -0
                    return unsafe { libc::kill(pid, 0) == 0 };
                }
            }
        }
        
        false
    }
}

