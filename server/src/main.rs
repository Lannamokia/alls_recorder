mod api;
mod core;
mod db;

use axum::{routing::get, Router};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use crate::core::recorder::{RecorderManager, StopRequest};
use std::collections::HashMap;
use uuid::Uuid;
use std::sync::Once;

#[cfg(windows)]
use windows_service::define_windows_service;

pub struct DownloadToken {
    pub user_id: Uuid,
    pub file_id: Uuid,
    pub expires_at: i64,
}

pub struct AppState {
    pub db: RwLock<Option<PgPool>>,
    pub recorder_manager: Arc<RecorderManager>,
    pub stop_requests: RwLock<HashMap<Uuid, StopRequest>>,
    pub download_tokens: RwLock<HashMap<String, DownloadToken>>,
}

#[cfg(windows)]
const SERVICE_NAME: &str = "AllsRecorder";

#[cfg(windows)]
define_windows_service!(ffi_service_main, service_main);

fn main() -> anyhow::Result<()> {
    // 处理 Agent 模式
    if std::env::args().any(|arg| arg == "--agent") {
        #[cfg(windows)]
        return run_agent();
        #[cfg(not(windows))]
        {
            eprintln!("Agent mode is only supported on Windows");
            return Err(anyhow::anyhow!("Unsupported platform"));
        }
    }

    // 处理服务卸载命令
    if std::env::args().any(|arg| arg == "--uninstall-service") {
        #[cfg(windows)]
        {
            if !is_elevated()? {
                println!("Requesting administrator privileges...");
                return elevate_and_run("--uninstall-service");
            }
            return uninstall_service();
        }
        #[cfg(not(windows))]
        {
            eprintln!("Service uninstallation is only supported on Windows");
            return Err(anyhow::anyhow!("Unsupported platform"));
        }
    }

    // 处理服务安装命令
    if std::env::args().any(|arg| arg == "--install-service") {
        #[cfg(windows)]
        {
            if !is_elevated()? {
                println!("Requesting administrator privileges...");
                return elevate_and_run("--install-service");
            }
            return install_service();
        }
        #[cfg(not(windows))]
        {
            eprintln!("Service installation is only supported on Windows");
            return Err(anyhow::anyhow!("Unsupported platform"));
        }
    }

    if cfg!(windows) && is_service_mode() {
        #[cfg(windows)]
        return run_as_service();
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        run_server(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    })?;

    Ok(())
}

fn is_service_mode() -> bool {
    std::env::args().any(|arg| arg == "--service")
        || std::env::var("RUN_AS_SERVICE").map(|v| v == "1").unwrap_or(false)
}

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        dotenvy::dotenv().ok();
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .init();
    });
}

async fn build_state() -> Arc<AppState> {
    init_tracing();
    let db_pool = if let Ok(url) = std::env::var("DATABASE_URL") {
        match sqlx::postgres::PgPoolOptions::new().connect(&url).await {
            Ok(pool) => {
                tracing::info!("Connected to database");
                Some(pool)
            }
            Err(e) => {
                tracing::warn!("Failed to connect to database: {}", e);
                None
            }
        }
    } else {
        None
    };

    Arc::new(AppState {
        db: RwLock::new(db_pool),
        recorder_manager: Arc::new(RecorderManager::new()),
        stop_requests: RwLock::new(HashMap::new()),
        download_tokens: RwLock::new(HashMap::new()),
    })
}

fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(root))
        .nest("/api", api::setup::router())
        .nest("/api/auth", api::auth::router())
        .nest("/api/hardware", api::hardware::router())
        .nest("/api/recorder", api::recorder::router())
        .nest("/api/files", api::files::router())
        .nest("/api/announcements", api::announcements::router())
        .nest("/api/settings", api::settings::router())
        .nest("/api/user", api::user_config::router())
        .nest("/api/users", api::users::router())
        .nest("/api/service", api::service::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn run_server<F>(shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let state = build_state().await;
    let app = build_app(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[cfg(windows)]
fn run_as_service() -> anyhow::Result<()> {
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

#[cfg(windows)]
fn service_main(_args: Vec<std::ffi::OsString>) {
    let _ = run_service();
}

#[cfg(windows)]
fn is_elevated() -> anyhow::Result<bool> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = HANDLE::default();
        
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| anyhow::anyhow!("Failed to open process token: {}", e))?;

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut return_length = 0u32;

        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        ).map_err(|e| anyhow::anyhow!("Failed to get token information: {}", e))?;

        Ok(elevation.TokenIsElevated != 0)
    }
}

#[cfg(windows)]
fn elevate_and_run(arg: &str) -> anyhow::Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use std::env;
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;

    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy().to_string();

    // Convert to wide string
    let operation: Vec<u16> = OsStr::new("runas").encode_wide().chain(Some(0)).collect();
    let file: Vec<u16> = OsStr::new(&exe_path_str).encode_wide().chain(Some(0)).collect();
    let parameters: Vec<u16> = OsStr::new(arg).encode_wide().chain(Some(0)).collect();

    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(parameters.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );

        let result_code = result.0 as isize;
        if result_code <= 32 {
            return Err(anyhow::anyhow!("Failed to elevate privileges. Error code: {}", result_code));
        }
    }

    println!("Elevated process started. Please check the new window.");
    Ok(())
}

#[cfg(windows)]
fn run_agent() -> anyhow::Result<()> {
    use crate::core::agent::AgentServer;

    // 设置工作目录为可执行文件所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            std::env::set_current_dir(exe_dir)?;
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        init_tracing();
        
        let port = std::env::var("AGENT_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(3001);

        let agent = AgentServer::new(port);
        
        tracing::info!("Starting agent server on port {}", port);
        
        if let Err(e) = agent.start().await {
            tracing::error!("Agent server error: {}", e);
            return Err(e);
        }
        
        Ok(())
    })?;

    Ok(())
}

#[cfg(windows)]
fn install_service() -> anyhow::Result<()> {
    use std::process::Command;
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    println!("Installing {} as Windows Service...", SERVICE_NAME);

    // 获取当前可执行文件的完整路径
    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

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
        .output()?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        eprintln!("Failed to create service: {}", error);
        return Err(anyhow::anyhow!("Service creation failed: {}", error));
    }

    println!("✓ Service created successfully");

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
    
    fs::create_dir_all(&agent_dir)?;
    
    let agent_config_path = agent_dir.join("agent.json");
    let agent_config = serde_json::json!({
        "auto_start": true,
        "service_url": "http://localhost:3000",
        "log_level": "info"
    });
    
    fs::write(&agent_config_path, serde_json::to_string_pretty(&agent_config)?)?;
    println!("✓ Agent configuration created at: {}", agent_config_path.display());

    // 添加到用户启动项（HKCU\Software\Microsoft\Windows\CurrentVersion\Run）
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
        .output()?;

    if output.status.success() {
        println!("✓ Agent added to user startup");
    } else {
        eprintln!("Warning: Failed to add agent to startup: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\nService installation completed!");
    println!("To start the service, run: sc start {}", SERVICE_NAME);
    println!("To stop the service, run: sc stop {}", SERVICE_NAME);
    println!("To uninstall the service, run: server.exe --uninstall-service");

    Ok(())
}

#[cfg(windows)]
fn uninstall_service() -> anyhow::Result<()> {
    use std::process::Command;
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    println!("Uninstalling {} Windows Service...", SERVICE_NAME);

    // 检查服务是否存在
    let check_output = Command::new("sc")
        .args(["query", SERVICE_NAME])
        .output()?;

    if !check_output.status.success() {
        println!("Service {} is not installed.", SERVICE_NAME);
        return Ok(());
    }

    // 尝试停止服务
    println!("Stopping service...");
    let stop_output = Command::new("sc")
        .args(["stop", SERVICE_NAME])
        .output()?;

    if stop_output.status.success() {
        println!("✓ Service stopped");
        // 等待服务完全停止
        std::thread::sleep(std::time::Duration::from_secs(2));
    } else {
        println!("Service may already be stopped or not running");
    }

    // 删除服务
    let output = Command::new("sc")
        .args(["delete", SERVICE_NAME])
        .output()?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        eprintln!("Failed to delete service: {}", error);
        return Err(anyhow::anyhow!("Service deletion failed: {}", error));
    }

    println!("✓ Service deleted successfully");

    // 删除 Agent 启动项
    let output = Command::new("reg")
        .args([
            "delete",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "AllsRecorderAgent",
            "/f",
        ])
        .output()?;

    if output.status.success() {
        println!("✓ Agent removed from user startup");
    } else {
        println!("Warning: Failed to remove agent from startup (may not exist)");
    }

    // 可选：删除 Agent 配置文件
    let agent_dir = PathBuf::from(env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".to_string()))
        .join("AllsRecorder");
    
    if agent_dir.exists() {
        match fs::remove_dir_all(&agent_dir) {
            Ok(_) => println!("✓ Agent configuration directory removed"),
            Err(e) => println!("Warning: Failed to remove agent config directory: {}", e),
        }
    }

    println!("\nService uninstallation completed!");
    println!("To reinstall the service, run: server.exe --install-service");

    Ok(())
}

#[cfg(windows)]
fn run_service() -> Result<(), windows_service::Error> {
    use std::sync::{Arc, Mutex};
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    // 设置工作目录为可执行文件所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = std::env::set_current_dir(exe_dir);
        }
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = Arc::new(Mutex::new(Some(tx)));
    let shutdown_handle = shutdown.clone();

    let status_handle = service_control_handler::register(SERVICE_NAME, move |control| {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Ok(mut guard) = shutdown_handle.lock() {
                    if let Some(sender) = guard.take() {
                        let _ = sender.send(());
                    }
                }
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    })?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(30),
        process_id: None,
    })?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(windows_service::Error::Winapi)?;

    let result = rt.block_on(async {
        run_server(async {
            let _ = rx.await;
        })
        .await
    });

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::Win32(1)
        },
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(30),
        process_id: None,
    })?;

    Ok(())
}

async fn root() -> &'static str {
    "Alls Recorder API"
}
