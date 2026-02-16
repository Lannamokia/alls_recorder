use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::process::{Command, Child};
use tokio::fs;
use uuid::Uuid;
use anyhow::Result;
use serde::{Serialize, Deserialize};

pub struct RecorderManager {
    processes: RwLock<HashMap<Uuid, (Child, String)>>, // Child process, Task type (recording/streaming)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RequestStatus {
    Pending,
    Accepted,
    Denied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StopRequest {
    pub requester_id: Uuid,
    pub requester_name: String,
    pub status: RequestStatus,
}

impl StopRequest {
    pub fn new(requester_id: Uuid, requester_name: String) -> Self {
        Self {
            requester_id,
            requester_name,
            status: RequestStatus::Pending,
        }
    }
}

impl RecorderManager {
    pub fn new() -> Self {
        Self {
            processes: RwLock::new(HashMap::new()),
        }
    }

    pub async fn start_recording(&self, user_id: Uuid, cli_path: String, args: Vec<String>, task_type: String) -> Result<()> {
        let mut processes = self.processes.write().await;
        
        if processes.contains_key(&user_id) {
            return Err(anyhow::anyhow!("Process already in progress"));
        }
        if !processes.is_empty() {
            return Err(anyhow::anyhow!("Another recording in progress"));
        }

        validate_cli_path(&cli_path).await?;

        let mut cmd = Command::new(&cli_path);
        cmd.args(args);

        let child = cmd.spawn().map_err(|e| anyhow::anyhow!("Failed to spawn process '{}': {}", cli_path, e))?;
        processes.insert(user_id, (child, task_type));
        
        Ok(())
    }

    pub async fn stop_recording(&self, user_id: Uuid) -> Result<()> {
        let mut processes = self.processes.write().await;
        
        if let Some((mut child, _)) = processes.remove(&user_id) {
            let _ = child.kill().await; // Ignore if already dead
            let _ = child.wait().await;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No active process found"))
        }
    }

    pub async fn get_task_type(&self, user_id: Uuid) -> Option<String> {
        let processes = self.processes.read().await;
        processes.get(&user_id).map(|(_, t)| t.clone())
    }

    pub async fn is_recording(&self, user_id: Uuid) -> bool {
        let processes = self.processes.read().await;
        processes.contains_key(&user_id)
    }

    pub async fn get_active_users(&self) -> Vec<Uuid> {
        let processes = self.processes.read().await;
        processes.keys().cloned().collect()
    }

    pub async fn has_any_recording(&self) -> bool {
        let processes = self.processes.read().await;
        !processes.is_empty()
    }
}

async fn validate_cli_path(cli_path: &str) -> Result<()> {
    let path = cli_path.trim();
    if path.is_empty() {
        return Err(anyhow::anyhow!("CLI path not configured"));
    }
    let meta = fs::metadata(path)
        .await
        .map_err(|e| anyhow::anyhow!("CLI path invalid: {}", e))?;
    if !meta.is_file() {
        return Err(anyhow::anyhow!("CLI path is not a file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(anyhow::anyhow!("CLI is not executable"));
        }
    }
    Ok(())
}
