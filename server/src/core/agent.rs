use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentCommand {
    pub command: String,
    pub cli_path: String,
    pub args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub success: bool,
    pub message: String,
    pub pid: Option<u32>,
    pub output: Option<String>,
}

pub struct AgentServer {
    port: u16,
}

impl AgentServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(&self) -> Result<()> {
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Agent server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 8192];
                        match socket.read(&mut buf).await {
                            Ok(n) if n > 0 => {
                                if let Ok(cmd) = serde_json::from_slice::<AgentCommand>(&buf[..n]) {
                                    let response = handle_command(cmd).await;
                                    if let Ok(json) = serde_json::to_vec(&response) {
                                        let _ = socket.write_all(&json).await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

async fn handle_command(cmd: AgentCommand) -> AgentResponse {
    match cmd.command.as_str() {
        "start" => start_cli_process(cmd.cli_path, cmd.args).await,
        "scan" => run_cli_scan(cmd.cli_path, cmd.args).await,
        "stop" => AgentResponse {
            success: false,
            message: "Stop command not implemented yet".to_string(),
            pid: None,
            output: None,
        },
        _ => AgentResponse {
            success: false,
            message: format!("Unknown command: {}", cmd.command),
            pid: None,
            output: None,
        },
    }
}

async fn start_cli_process(cli_path: String, args: Vec<String>) -> AgentResponse {
    use tokio::process::Command;

    match Command::new(&cli_path)
        .args(&args)
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            AgentResponse {
                success: true,
                message: "Process started successfully".to_string(),
                pid,
                output: None,
            }
        }
        Err(e) => AgentResponse {
            success: false,
            message: format!("Failed to start process: {}", e),
            pid: None,
            output: None,
        },
    }
}

async fn run_cli_scan(cli_path: String, args: Vec<String>) -> AgentResponse {
    use tokio::process::Command;

    match Command::new(&cli_path).args(&args).output().await {
        Ok(output) => {
            if output.status.success() {
                AgentResponse {
                    success: true,
                    message: "Scan completed".to_string(),
                    pid: None,
                    output: Some(String::from_utf8_lossy(&output.stdout).to_string()),
                }
            } else {
                AgentResponse {
                    success: false,
                    message: format!(
                        "Scan failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    pid: None,
                    output: None,
                }
            }
        }
        Err(e) => AgentResponse {
            success: false,
            message: format!("Failed to execute CLI: {}", e),
            pid: None,
            output: None,
        },
    }
}
