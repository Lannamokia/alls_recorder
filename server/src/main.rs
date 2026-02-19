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
fn run_service() -> Result<(), windows_service::Error> {
    use std::sync::{Arc, Mutex};
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

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
