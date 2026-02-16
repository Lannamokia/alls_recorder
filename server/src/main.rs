mod api;
mod core;
mod db;

use axum::{routing::get, Router};
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Try to connect to DB if DATABASE_URL exists
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

    let state = Arc::new(AppState {
        db: RwLock::new(db_pool),
        recorder_manager: Arc::new(RecorderManager::new()),
        stop_requests: RwLock::new(HashMap::new()),
        download_tokens: RwLock::new(HashMap::new()),
    });

    // build our application with a route
    let app = Router::new()
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
        .layer(CorsLayer::permissive()) // Allow CORS for development
        .with_state(state);

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> &'static str {
    "Alls Recorder API"
}
