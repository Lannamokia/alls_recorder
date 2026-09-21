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
        // 优先：直接以活动会话用户身份运行扫描（无需 agent，同录制拉起的新路径）
        #[cfg(windows)]
        {
            let launcher = crate::core::session_launch::global_launcher();
            match launcher
                // cli-capture --scan 的耗时波动极大（本机实测同一参数连续 4 次为
                // 91s / 4s / 14s / 2s，全部成功），60 秒会把偶发的长尾误判成拉起失败。
                .run_in_session(cli_path, &args, std::time::Duration::from_secs(180))
                .await
            {
                Ok(out) => return Ok(out),
                Err(e) => {
                    tracing::warn!(
                        "会话内扫描失败（{}），回退到采集代理: {}",
                        cli_path,
                        e
                    );
                }
            }
        }

        // 回退：经 agent（用户态计划任务进程）执行
        let agent_addr = std::env::var("AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
        let agent_client = AgentClient::new(agent_addr.clone());
        return agent_client
            .scan_hardware_with_args(cli_path.to_string(), args)
            .await
            .map_err(|e| crate::core::agent_client::map_agent_connect_error(e, &agent_addr));
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

/// 将代理连接错误翻译为可操作的提示（实现见 agent_client::map_agent_connect_error）。
#[cfg(test)]
mod tests {
    use crate::core::agent_client::map_agent_connect_error;

    #[test]
    fn connection_refused_gets_actionable_hint() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "os error 10061");
        let e = map_agent_connect_error(anyhow::anyhow!(io_err), "127.0.0.1:3001");
        let msg = e.to_string();
        assert!(msg.contains("Agent not running"), "msg: {}", msg);
        assert!(msg.contains("--install-agent"), "msg: {}", msg);
    }

    #[test]
    fn other_agent_errors_pass_through() {
        let e = map_agent_connect_error(anyhow::anyhow!("Agent error: CLI scan failed"), "127.0.0.1:3001");
        assert_eq!(e.to_string(), "Agent error: CLI scan failed");
    }
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
