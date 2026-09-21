use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::process::{Command, Child};
use tokio::fs;
use uuid::Uuid;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use crate::core::agent_client::AgentClient;
#[cfg(windows)]
use crate::core::session_launch::{global_launcher, SessionLauncher};

pub enum RecorderMode {
    Direct,
    Service { agent_client: AgentClient },
}

pub struct RecorderManager {
    processes: RwLock<HashMap<Uuid, (Option<Child>, u32, String)>>, // Child (if direct), PID, Task type
    mode: RecorderMode,
    #[cfg(windows)]
    session_launcher: Option<&'static SessionLauncher>,
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
        let mode = if is_service_mode() {
            let agent_addr = std::env::var("AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
            RecorderMode::Service {
                agent_client: AgentClient::new(agent_addr),
            }
        } else {
            RecorderMode::Direct
        };

        Self {
            processes: RwLock::new(HashMap::new()),
            mode,
            #[cfg(windows)]
            session_launcher: if is_service_mode() {
                tracing::info!("session launcher enabled: capture processes will be spawned as the active console user");
                Some(global_launcher())
            } else {
                None
            },
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

        match &self.mode {
            RecorderMode::Direct => {
                let mut cmd = Command::new(&cli_path);
                cmd.args(args);
                // 自成进程组，停止时才能用 CTRL_BREAK 精确投递给它（不影响本进程）
                #[cfg(windows)]
                cmd.creation_flags(
                    windows::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP.0,
                );

                let child = cmd.spawn().map_err(|e| anyhow::anyhow!("Failed to spawn process '{}': {}", cli_path, e))?;
                let pid = child.id().unwrap_or(0);
                processes.insert(user_id, (Some(child), pid, task_type));
            }
            RecorderMode::Service { agent_client } => {
                // 优先：由系统服务通过 CreateProcessAsUser 以活动会话用户身份直接拉起
                #[cfg(windows)]
                if let Some(launcher) = &self.session_launcher {
                    let pid = launcher.spawn_capture(&cli_path, &args).await?;
                    processes.insert(user_id, (None, pid, task_type));
                    return Ok(());
                }
                // 回退：经 agent（用户态计划任务进程）拉起
                let pid = agent_client
                    .start_recording(cli_path, args)
                    .await
                    .map_err(|e| crate::core::agent_client::map_agent_connect_error(e, agent_client.addr()))?;
                processes.insert(user_id, (None, pid, task_type));
            }
        }
        
        Ok(())
    }

    pub async fn stop_recording(&self, user_id: Uuid) -> Result<()> {
        // 先摘掉记录并放锁：下面的优雅停止最长要等 20 秒，
        // 不能把 state 轮询堵在写锁上。
        let removed = self.processes.write().await.remove(&user_id);
        let Some((child_opt, pid, _)) = removed else {
            return Err(anyhow::anyhow!("No active process found"));
        };

        match &self.mode {
            RecorderMode::Direct => {
                if let Some(mut child) = child_opt {
                    // 硬杀会让 cli-capture 来不及写 MP4 尾部（moov），先走优雅停止
                    #[cfg(windows)]
                    if pid != 0 {
                        if let Err(e) = crate::core::session_launch::stop_capture_local(
                            pid,
                            std::time::Duration::from_secs(20),
                        )
                        .await
                        {
                            tracing::warn!("优雅停止失败（pid={}）: {}", pid, e);
                        }
                    }
                    // 兜底：还没退（优雅停止失败或拿不到 pid）就直接结束
                    if child.try_wait().ok().flatten().is_none() {
                        let _ = child.kill().await;
                    }
                    let _ = child.wait().await;
                }
            }
            RecorderMode::Service { .. } => {
                // 不能直接 taskkill /F：硬杀会让 OBS 来不及写 MP4 尾部（moov），
                // 录出来的文件播放器打不开。优先走会话内 CTRL_BREAK 优雅停止。
                #[cfg(windows)]
                {
                    if let Some(launcher) = &self.session_launcher {
                        launcher
                            .stop_capture(pid, std::time::Duration::from_secs(20))
                            .await?;
                    } else {
                        crate::core::session_launch::kill_process_tree(pid);
                    }
                }
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .output();
                }
            }
        }
        Ok(())
    }

    pub async fn get_task_type(&self, user_id: Uuid) -> Option<String> {
        let processes = self.processes.read().await;
        processes.get(&user_id).map(|(_, _, t)| t.clone())
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


fn is_service_mode() -> bool {
    std::env::args().any(|arg| arg == "--service")
        || std::env::var("RUN_AS_SERVICE").map(|v| v == "1").unwrap_or(false)
}
