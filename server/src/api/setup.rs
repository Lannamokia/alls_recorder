use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::fs::File;
use std::path::Path;

use crate::AppState;

#[derive(Serialize)]
pub struct StatusResponse {
    pub initialized: bool,
}

#[derive(Serialize)]
pub struct InfoResponse {
    pub initialized: bool,
    pub name: String,
}

#[derive(Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

#[derive(Deserialize)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(check_status))
        .route("/setup/status", get(check_status))
        .route("/info", get(get_info))
        .route("/setup/info", get(get_info))
        .route("/setup/db", post(setup_db))
        .route("/setup/admin", post(setup_admin))
}

async fn check_status(_state: State<Arc<AppState>>) -> impl IntoResponse {
    let initialized = Path::new("init.lock").exists();
    Json(StatusResponse { initialized })
}

async fn get_info(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let initialized = Path::new("init.lock").exists();
    let mut name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Alls Recorder".to_string());

    let db_guard = state.db.read().await;
    if let Some(pool) = db_guard.as_ref() {
        if let Ok(row) = sqlx::query_as::<_, (serde_json::Value,)>("SELECT value FROM system_config WHERE key = 'server_name'")
            .fetch_optional(pool)
            .await
        {
            if let Some((val,)) = row {
                if let Some(v) = val.as_str() {
                    if !v.trim().is_empty() {
                        name = v.to_string();
                    }
                }
            }
        }
    }

    Json(InfoResponse { initialized, name })
}

async fn setup_db(
    State(state): State<Arc<AppState>>,
    Json(config): Json<DbConfig>,
) -> impl IntoResponse {
    if Path::new("init.lock").exists() {
        return (StatusCode::BAD_REQUEST, "Already initialized").into_response();
    }

    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.user, config.password, config.host, config.port, config.dbname
    );

    // Test connection
    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to connect: {}", e)).into_response(),
    };

    // Run migrations
    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to run migrations: {}", e)).into_response();
    }

    // Store pool in state
    {
        let mut db_guard = state.db.write().await;
        *db_guard = Some(pool);
    }

    // Save DB config to .env temporarily (or permanently)
    let env_content = format!(
        "DATABASE_URL={}\nRUST_LOG=server=debug,tower_http=debug\nJWT_SECRET=please_change_this_secret\n",
        url
    );
    
    if let Err(e) = std::fs::write(".env", env_content) {
         return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write .env: {}", e)).into_response();
    }

    (StatusCode::OK, "Database connected and migrated").into_response()
}

async fn setup_admin(
    State(state): State<Arc<AppState>>,
    Json(config): Json<AdminConfig>,
) -> impl IntoResponse {
    if Path::new("init.lock").exists() {
        return (StatusCode::BAD_REQUEST, "Already initialized").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::BAD_REQUEST, "Database not configured yet").into_response(),
    };

    let password_hash = match bcrypt::hash(&config.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to hash password: {}", e)).into_response(),
    };

    // Insert admin using runtime query
    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, 'admin')"
    )
    .bind(&config.username)
    .bind(password_hash)
    .execute(pool)
    .await;

    if let Err(e) = result {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create admin: {}", e)).into_response();
    }

    // Create lock file
    if let Err(e) = File::create("init.lock") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create lock file: {}", e)).into_response();
    }

    (StatusCode::OK, "Initialization complete").into_response()
}
