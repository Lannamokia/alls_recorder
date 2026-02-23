use axum::{
    extract::State,
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use std::sync::Arc;
use crate::AppState;
use crate::api::recorder::get_user_from_header;
use serde::Serialize;

#[derive(Serialize)]
pub struct InstallServiceResponse {
    pub success: bool,
    pub message: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/install", post(install_service))
        .route("/uninstall", post(uninstall_service))
}

async fn install_service(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    #[cfg(not(windows))]
    {
        return Json(InstallServiceResponse {
            success: false,
            message: "Service installation is only supported on Windows".to_string(),
        }).into_response();
    }

    #[cfg(windows)]
    {
        match perform_install().await {
            Ok(msg) => Json(InstallServiceResponse {
                success: true,
                message: msg,
            }).into_response(),
            Err(e) => Json(InstallServiceResponse {
                success: false,
                message: e.to_string(),
            }).into_response(),
        }
    }
}

async fn uninstall_service(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin access required").into_response();
    }

    #[cfg(not(windows))]
    {
        return Json(InstallServiceResponse {
            success: false,
            message: "Service uninstallation is only supported on Windows".to_string(),
        }).into_response();
    }

    #[cfg(windows)]
    {
        match perform_uninstall().await {
            Ok(msg) => Json(InstallServiceResponse {
                success: true,
                message: msg,
            }).into_response(),
            Err(e) => Json(InstallServiceResponse {
                success: false,
                message: e.to_string(),
            }).into_response(),
        }
    }
}

#[cfg(windows)]
async fn perform_install() -> Result<String, String> {
    use std::process::Command;
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    const SERVICE_NAME: &str = "AllsRecorder";

    // 检查是否有管理员权限
    if !is_elevated().map_err(|e| format!("Failed to check elevation: {}", e))? {
        // 尝试提升权限
        let exe_path = env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))?;
        
        return Err(format!(
            "Administrator privileges required. Please run the following command as administrator:\n\"{}\" --install-service",
            exe_path.display()
        ));
    }

    // 获取当前可执行文件的完整路径
    let exe_path = env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))?;
    let exe_path_str = exe_path.to_string_lossy();

    // 检查服务是否已存在
    let check_output = Command::new("sc")
        .args(["query", SERVICE_NAME])
        .output()
        .map_err(|e| format!("Failed to check service status: {}", e))?;

    if check_output.status.success() {
        return Err("Service already exists. Please uninstall it first using: sc delete AllsRecorder".to_string());
    }

    // 使用 sc 命令创建服务
    let output = Command::new("sc")
        .args([
            "create",
            SERVICE_NAME,
            "binPath=",
            &format!("\"{}\" --service", exe_path_str),
            "start=",
            "auto",
            "DisplayName=",
            "Alls Recorder Service",
        ])
        .output()
        .map_err(|e| format!("Failed to execute sc command: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create service: {}", error));
    }

    // 设置服务描述
    let _ = Command::new("sc")
        .args([
            "description",
            SERVICE_NAME,
            "Alls Recorder background service for screen recording",
        ])
        .output();

    // 创建 agent 配置文件
    let agent_dir = PathBuf::from(env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".to_string()))
        .join("AllsRecorder");
    
    fs::create_dir_all(&agent_dir).map_err(|e| format!("Failed to create agent directory: {}", e))?;
    
    let agent_config_path = agent_dir.join("agent.json");
    let agent_config = serde_json::json!({
        "auto_start": true,
        "service_url": "http://localhost:3000",
        "agent_port": 3001,
        "log_level": "info"
    });
    
    fs::write(&agent_config_path, serde_json::to_string_pretty(&agent_config).unwrap())
        .map_err(|e| format!("Failed to write agent config: {}", e))?;

    // 添加到用户启动项
    let output = Command::new("reg")
        .args([
            "add",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "AllsRecorderAgent",
            "/t",
            "REG_SZ",
            "/d",
            &format!("\"{}\" --agent", exe_path_str),
            "/f",
        ])
        .output()
        .map_err(|e| format!("Failed to add to startup: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to add agent to startup: {}", error));
    }

    // 启动服务
    let start_output = Command::new("sc")
        .args(["start", SERVICE_NAME])
        .output()
        .map_err(|e| format!("Failed to start service: {}", e))?;

    if !start_output.status.success() {
        let error = String::from_utf8_lossy(&start_output.stderr);
        return Ok(format!(
            "Service installed successfully but failed to start: {}. You can start it manually with: sc start {}",
            error, SERVICE_NAME
        ));
    }

    Ok(format!(
        "Service installed and started successfully. Agent will start automatically on user login."
    ))
}

#[cfg(windows)]
fn is_elevated() -> Result<bool, String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = HANDLE::default();
        
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| format!("Failed to open process token: {}", e))?;

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut return_length = 0u32;

        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        ).map_err(|e| format!("Failed to get token information: {}", e))?;

        Ok(elevation.TokenIsElevated != 0)
    }
}


#[cfg(windows)]
async fn perform_uninstall() -> Result<String, String> {
    use std::process::Command;
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    const SERVICE_NAME: &str = "AllsRecorder";

    // 检查是否有管理员权限
    if !is_elevated().map_err(|e| format!("Failed to check elevation: {}", e))? {
        let exe_path = env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))?;
        
        return Err(format!(
            "Administrator privileges required. Please run the following command as administrator:\n\"{}\" --uninstall-service",
            exe_path.display()
        ));
    }

    // 检查服务是否存在
    let check_output = Command::new("sc")
        .args(["query", SERVICE_NAME])
        .output()
        .map_err(|e| format!("Failed to check service status: {}", e))?;

    if !check_output.status.success() {
        return Err("Service is not installed".to_string());
    }

    // 尝试停止服务
    let stop_output = Command::new("sc")
        .args(["stop", SERVICE_NAME])
        .output()
        .map_err(|e| format!("Failed to stop service: {}", e))?;

    if stop_output.status.success() {
        // 等待服务完全停止
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // 删除服务
    let output = Command::new("sc")
        .args(["delete", SERVICE_NAME])
        .output()
        .map_err(|e| format!("Failed to execute sc command: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to delete service: {}", error));
    }

    // 删除 Agent 启动项
    let _ = Command::new("reg")
        .args([
            "delete",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "AllsRecorderAgent",
            "/f",
        ])
        .output();

    // 删除 Agent 配置文件
    let agent_dir = PathBuf::from(env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".to_string()))
        .join("AllsRecorder");
    
    if agent_dir.exists() {
        let _ = fs::remove_dir_all(&agent_dir);
    }

    Ok("Service uninstalled successfully. Agent configuration and startup entries have been removed.".to_string())
}
