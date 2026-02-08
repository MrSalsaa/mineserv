use crate::types::ServerStats;
use anyhow::Result;
use std::time::Instant;
use sysinfo::{Pid, System};

pub struct ServerMonitor {
    system: System,
    start_time: Instant,
}

impl ServerMonitor {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            start_time: Instant::now(),
        }
    }

    pub fn get_stats(&mut self, pid: u32) -> Result<ServerStats> {
        self.system.refresh_all();

        let pid = Pid::from_u32(pid);
        
        let process = self.system.process(pid)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;

        let cpu_percent = process.cpu_usage();
        let memory_mb = process.memory() / 1024 / 1024;
        
        // For disk usage, we'd need to track the server directory
        // This is a simplified version
        let disk_mb = 0;

        let uptime_seconds = self.start_time.elapsed().as_secs();

        Ok(ServerStats {
            cpu_percent,
            memory_mb,
            disk_mb,
            uptime_seconds,
        })
    }

    pub fn reset_uptime(&mut self) {
        self.start_time = Instant::now();
    }
}

impl Default for ServerMonitor {
    fn default() -> Self {
        Self::new()
    }
}
