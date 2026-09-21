mod api;
mod core;
mod db;

use axum::{routing::get, Router};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::core::recorder::{RecorderManager, StopRequest};
use crate::db::DbPool;
use tower_http::trace::TraceLayer;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
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
    pub db: RwLock<Option<DbPool>>,
    pub recorder_manager: Arc<RecorderManager>,
    pub stop_requests: RwLock<HashMap<Uuid, StopRequest>>,
    pub download_tokens: RwLock<HashMap<String, DownloadToken>>,
    pub captcha_store: api::captcha::CaptchaStore,
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

    // 处理可选的 agent 自启计划任务安装（默认不再随服务安装创建，
    // 采集进程改由服务通过 CreateProcessAsUser 以用户身份直接拉起）
    if std::env::args().any(|arg| arg == "--install-agent") {
        #[cfg(windows)]
        {
            if !is_elevated()? {
                println!("Requesting administrator privileges...");
                return elevate_and_run("--install-agent");
            }
            return install_agent_task();
        }
        #[cfg(not(windows))]
        {
            eprintln!("Agent installation is only supported on Windows");
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
        let env_filter = tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
        );

        // 服务模式（或显式 ALLS_LOG_FILE=1，供 NSSM 等托管场景）写滚动日志文件；
        // 服务进程没有控制台，stdout 会被丢弃，文件日志是唯一的错误现场记录。
        let log_to_file = is_service_mode()
            || std::env::var("ALLS_LOG_FILE").map(|v| v == "1").unwrap_or(false);
        if log_to_file {
            // 服务模式没有控制台，stdout 会被丢弃：改写滚动日志文件，
            // 并启动时清理 7 天前的旧日志。
            let log_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("logs")))
                .unwrap_or_else(|| std::path::PathBuf::from("logs"));
            if let Err(e) = std::fs::create_dir_all(&log_dir) {
                eprintln!("Failed to create log directory {}: {}", log_dir.display(), e);
            }
            cleanup_old_logs(&log_dir, 7);

            let file_appender = tracing_appender::rolling::daily(&log_dir, "server.log");
            let (writer, guard) = tracing_appender::non_blocking(file_appender);
            // WorkerGuard 必须在整个进程生命周期存活（drop 时才 flush 残余），挂到静态变量上
            let _ = LOG_GUARD.set(std::sync::Mutex::new(guard));

            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().with_writer(writer).with_ansi(false))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        }

        // panic 也进日志（服务模式下这是唯一的错误现场记录手段）
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("panic occurred: {}", info);
            default_hook(info);
        }));
    });
}

static LOG_GUARD: std::sync::OnceLock<std::sync::Mutex<tracing_appender::non_blocking::WorkerGuard>> =
    std::sync::OnceLock::new();

/// 删除日志目录中修改时间超过 `keep_days` 天的文件。
fn cleanup_old_logs(log_dir: &std::path::Path, keep_days: u64) {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(keep_days * 86400))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let too_old = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| t < cutoff)
                .unwrap_or(false);
            if too_old {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

async fn build_state() -> Arc<AppState> {
    init_tracing();
    // 数据库选择：
    //   * DATABASE_URL=postgres://... → PostgreSQL（既有部署）
    //   * DATABASE_URL=sqlite://path  → 指定 SQLite 文件
    //   * 未设置                      → 默认可执行文件旁 data/alls_recorder.db（单机零配置）
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| default_sqlite_url());
    let db_pool = match crate::db::connect(&db_url).await {
        Ok(pool) => {
            tracing::info!("Connected to database ({})", if pool.is_pg() { "postgres" } else { "sqlite" });
            Some(pool)
        }
        Err(e) => {
            tracing::warn!("Failed to connect to database: {}", e);
            None
        }
    };

    Arc::new(AppState {
        db: RwLock::new(db_pool),
        recorder_manager: Arc::new(RecorderManager::new()),
        stop_requests: RwLock::new(HashMap::new()),
        download_tokens: RwLock::new(HashMap::new()),
        captcha_store: api::captcha::CaptchaStore::default(),
    })
}

/// 默认 SQLite 位置：<exe_dir>/data/alls_recorder.db，便于随软件分发、免安装。
fn default_sqlite_url() -> String {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let data_dir = dir.join("data");
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        tracing::warn!("Failed to create data directory: {}", e);
    }
    format!("sqlite://{}", data_dir.join("alls_recorder.db").display())
        .replace('\\', "/")
}

fn build_app(state: Arc<AppState>) -> Router {
    let api_router = Router::new()
        .nest("/api", api::setup::router())
        .nest("/api/auth", api::auth::router())
        .nest("/api/discovery", api::discovery::router())
        .nest("/api/hardware", api::hardware::router())
        .nest("/api/recorder", api::recorder::router())
        .nest("/api/files", api::files::router())
        .nest("/api/announcements", api::announcements::router())
        .nest("/api/settings", api::settings::router())
        .nest("/api/user", api::user_config::router())
        .nest("/api/users", api::users::router())
        .nest("/api/service", api::service::router());

    let app = api_router
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // 内置 web-ui：优先在可执行文件旁的 web-ui 目录查找，
    // 其次当前工作目录（cargo run 场景）。存在则作为 SPA 托管，
    // 所有未匹配路径回退到 index.html；不存在则保留纯 API 模式。
    if let Some(webui_dir) = find_webui_dir() {
        let index_path = webui_dir.join("index.html");
        tracing::info!("serving embedded web-ui from {}", webui_dir.display());
        let serve_dir = tower_http::services::ServeDir::new(&webui_dir)
            // 用 fallback（而非 not_found_service，后者会强制 404 状态码），
            // 让未匹配的 SPA 路由以 200 返回 index.html
            .fallback(tower_http::services::ServeFile::new(index_path));
        app.fallback_service(axum::routing::any({
            let serve_dir = serve_dir;
            move |req: axum::extract::Request| {
                let serve_dir = serve_dir.clone();
                async move {
                    use axum::response::IntoResponse;
                    if req.uri().path().starts_with("/api/") {
                        return axum::http::StatusCode::NOT_FOUND.into_response();
                    }
                    use tower::ServiceExt;
                    match serve_dir.oneshot(req).await {
                        Ok(resp) => resp.into_response(),
                        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                    }
                }
            }
        }))
    } else {
        tracing::warn!("web-ui directory not found next to executable; running in API-only mode");
        app.route("/", get(root))
    }
}

fn find_webui_dir() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("web-ui"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("web-ui"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("web-ui"));
    }
    candidates
        .into_iter()
        .find(|p| p.join("index.html").is_file())
}

async fn run_server<F>(shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let state = build_state().await;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown.await;
        let _ = shutdown_tx.send(true);
    });
    // 断线重连看门狗：仅 PostgreSQL 需要（网络连接可能断开）；
    // SQLite 是本地文件，连接池创建即长期有效，无需重连。
    if is_service_mode() {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| default_sqlite_url());
        if !crate::db::is_sqlite_url(&url) {
            let state_clone = state.clone();
            let mut shutdown_rx = shutdown_rx.clone();
            tokio::spawn(async move {
                let retry_interval = std::time::Duration::from_secs(3);
                loop {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                    let pool_opt = {
                        let db_guard = state_clone.db.read().await;
                        db_guard.clone()
                    };
                    let mut needs_connect = pool_opt.is_none();
                    if let Some(pool) = pool_opt {
                        if let Err(e) = crate::db::ping(&pool).await {
                            tracing::warn!("Database connection lost, retrying in 3s: {}", e);
                            needs_connect = true;
                            let mut db_guard = state_clone.db.write().await;
                            *db_guard = None;
                        }
                    }
                    if needs_connect {
                        match crate::db::connect(&url).await {
                            Ok(pool) => {
                                let mut db_guard = state_clone.db.write().await;
                                *db_guard = Some(pool);
                                tracing::info!("Connected to database");
                            }
                            Err(e) => {
                                tracing::warn!("Failed to connect to database, retrying in 3s: {}", e);
                            }
                        }
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(retry_interval) => {}
                        _ = async {
                            let _ = shutdown_rx.changed().await;
                        } => break,
                    }
                }
            });
        }
    }
    let app = build_app(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut shutdown_rx = shutdown_rx.clone();
            let _ = shutdown_rx.changed().await;
        })
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
            "delayed-auto",
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

    println!("\nService installation completed!");
    println!("The service now spawns capture processes directly as the active console user");
    println!("(no ONLOGON scheduled task is created).");
    println!("To start the service, run: sc start {}", SERVICE_NAME);
    println!("To stop the service, run: sc stop {}", SERVICE_NAME);
    println!("To uninstall the service, run: server.exe --uninstall-service");
    println!("(Optional) To install the legacy user-mode agent autostart task, run: server.exe --install-agent");

    Ok(())
}

#[cfg(windows)]
fn install_agent_task() -> anyhow::Result<()> {
    use std::process::Command;
    use std::env;

    println!("Installing legacy agent autostart scheduled task...");

    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/SC",
            "ONLOGON",
            "/TN",
            "AllsRecorderAgent",
            "/TR",
            &format!("\"{}\" --agent", exe_path_str),
            "/RL",
            "LIMITED",
            "/IT",
            "/F",
        ])
        .output()?;

    if output.status.success() {
        println!("✓ Agent scheduled task created");
    } else {
        eprintln!("Failed to create agent scheduled task: {}", String::from_utf8_lossy(&output.stderr));
        return Err(anyhow::anyhow!("Agent scheduled task creation failed"));
    }

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

    let output = Command::new("schtasks")
        .args([
            "/Delete",
            "/TN",
            "AllsRecorderAgent",
            "/F",
        ])
        .output()?;

    if output.status.success() {
        println!("✓ Agent scheduled task removed");
    } else {
        println!("Warning: Failed to remove agent scheduled task (may not exist)");
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
