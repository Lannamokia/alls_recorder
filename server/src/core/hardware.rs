use serde::{Deserialize, Serialize};
use tokio::fs;
use crate::core::agent_client::AgentClient;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareInfo {
    pub screens: Vec<Device>,
    #[serde(default)]
    pub desktop_audio: Vec<Device>,
    #[serde(default)]
    pub microphone: Vec<Device>,
    pub encoders: Vec<Device>,
    #[serde(default)]
    pub windows: Vec<WindowItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub index: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WindowItem {
    pub title: String,
    pub exe: String,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct WindowsInfo {
    #[serde(default)]
    pub windows: Vec<WindowItem>,
}

pub async fn probe_hardware(cli_path: String) -> anyhow::Result<HardwareInfo> {
    validate_cli_path(&cli_path).await?;

    let stdout = run_scan(&cli_path, vec!["--scan".to_string()]).await?;
    // Attempt to find JSON in output if there's other noise, or assume pure JSON
    // For now assume pure JSON or JSON is the last part
    let mut info: HardwareInfo = serde_json::from_str(&stdout)
        .map_err(|e| anyhow::anyhow!("Failed to parse scan output: {} (Output: {})", e, stdout))?;

    if let Ok(win_stdout) = run_scan(&cli_path, vec!["--scan-windows".to_string()]).await {
        if let Ok(win_info) = serde_json::from_str::<WindowsInfo>(&win_stdout) {
            info.windows = win_info.windows;
        }
    }

    Ok(info)
}

async fn run_scan(cli_path: &str, args: Vec<String>) -> anyhow::Result<String> {
    if is_service_mode() {
        let agent_addr = std::env::var("AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
        let agent_client = AgentClient::new(agent_addr);
        return agent_client.scan_hardware_with_args(cli_path.to_string(), args).await;
    }
    let output = tokio::process::Command::new(cli_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to execute CLI '{}': {}", cli_path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("CLI scan failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn is_service_mode() -> bool {
    std::env::args().any(|arg| arg == "--service")
        || std::env::var("RUN_AS_SERVICE").map(|v| v == "1").unwrap_or(false)
}

async fn validate_cli_path(cli_path: &str) -> anyhow::Result<()> {
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
