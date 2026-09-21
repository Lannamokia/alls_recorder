use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;
use serde::{Deserialize, Serialize};

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

pub struct AgentClient {
    agent_addr: String,
}

impl AgentClient {
    pub fn new(agent_addr: String) -> Self {
        Self { agent_addr }
    }

    pub fn addr(&self) -> &str {
        &self.agent_addr
    }

    pub async fn send_command(&self, cmd: AgentCommand) -> Result<AgentResponse> {
        let mut stream = TcpStream::connect(&self.agent_addr).await?;
        
        let json = serde_json::to_vec(&cmd)?;
        stream.write_all(&json).await?;
        
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await?;
        
        let response: AgentResponse = serde_json::from_slice(&buf[..n])?;
        Ok(response)
    }

    pub async fn start_recording(&self, cli_path: String, args: Vec<String>) -> Result<u32> {
        let cmd = AgentCommand {
            command: "start".to_string(),
            cli_path,
            args,
        };
        
        let response = self.send_command(cmd).await?;
        
        if response.success {
            response.pid.ok_or_else(|| anyhow::anyhow!("No PID returned"))
        } else {
            Err(anyhow::anyhow!("Agent error: {}", response.message))
        }
    }

    #[allow(dead_code)]
    pub async fn scan_hardware(&self, cli_path: String) -> Result<String> {
        self.scan_hardware_with_args(cli_path, vec!["--scan".to_string()]).await
    }

    pub async fn scan_hardware_with_args(&self, cli_path: String, args: Vec<String>) -> Result<String> {
        let cmd = AgentCommand {
            command: "scan".to_string(),
            cli_path,
            args,
        };

        let response = self.send_command(cmd).await?;

        if response.success {
            response
                .output
                .ok_or_else(|| anyhow::anyhow!("No scan output returned"))
        } else {
            Err(anyhow::anyhow!("Agent error: {}", response.message))
        }
    }
}

/// 将代理连接错误翻译为可操作的提示。连接拒绝（os 10061）说明代理进程没跑，
/// 这是部署问题而非程序错误，直接告诉用户怎么修。
pub fn map_agent_connect_error(e: anyhow::Error, agent_addr: &str) -> anyhow::Error {
    let refused = e.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .map(|io| io.kind() == std::io::ErrorKind::ConnectionRefused)
            .unwrap_or(false)
    });
    if refused {
        anyhow::anyhow!(
            "Agent not running: 无法连接采集代理 {}（连接被拒绝）。\
             请以管理员身份运行 server.exe --install-agent 安装代理计划任务，\
             并确认用户登录后代理进程（server.exe --agent）正在运行。",
            agent_addr
        )
    } else {
        e
    }
}
