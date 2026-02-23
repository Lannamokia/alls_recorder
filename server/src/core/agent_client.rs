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
}

pub struct AgentClient {
    agent_addr: String,
}

impl AgentClient {
    pub fn new(agent_addr: String) -> Self {
        Self { agent_addr }
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
}
