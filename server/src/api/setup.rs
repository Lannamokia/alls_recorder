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
use uuid::Uuid;

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
    pub jwt_secret: String,
}

#[derive(Deserialize)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

/// 当前数据库类型，供前端初始化向导决定是否需要填写 PG 连接信息。
#[derive(Serialize)]
pub struct DbKindResponse {
    pub kind: &'static str, // "sqlite" | "postgres"
    pub configured: bool,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(check_status))
        .route("/setup/status", get(check_status))
        .route("/info", get(get_info))
        .route("/setup/info", get(get_info))
        .route("/setup/db_kind", get(get_db_kind))
        .route("/setup/db", post(setup_db))
        .route("/setup/admin", post(setup_admin))
}

/// 解析"生效中的数据库 URL"：环境变量优先，未设置时为默认 SQLite 路径。
fn effective_db_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
        format!("sqlite://{}", dir.join("data").join("alls_recorder.db").display()).replace('\\', "/")
    })
}

async fn get_db_kind(State(state): State<Arc<AppState>>) -> Json<DbKindResponse> {
    let db_guard = state.db.read().await;
    let kind = match db_guard.as_ref() {
        Some(crate::db::DbPool::Pg(_)) => "postgres",
        Some(crate::db::DbPool::Sqlite(_)) => "sqlite",
        None => {
            if crate::db::is_sqlite_url(&effective_db_url()) {
                "sqlite"
            } else {
                "postgres"
            }
        }
    };
    Json(DbKindResponse {
        kind,
        configured: db_guard.is_some(),
    })
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
        if let Ok(row) = crate::dbq_as!(
            pool, (serde_json::Value,), fetch_optional,
            "SELECT value FROM system_config WHERE key = 'server_name'",
            []
        )
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
    
    // Validate JWT secret strength
    let jwt_secret = config.jwt_secret.trim();
    if jwt_secret.is_empty() {
        return (StatusCode::BAD_REQUEST, "JWT_SECRET is required").into_response();
    }
    if jwt_secret.len() < 32 {
        return (StatusCode::BAD_REQUEST, "JWT_SECRET must be at least 32 characters for security").into_response();
    }
    // Check for weak patterns
    if jwt_secret.chars().all(|c| c.is_numeric()) {
        return (StatusCode::BAD_REQUEST, "JWT_SECRET cannot be all numbers").into_response();
    }
    if jwt_secret.to_lowercase() == jwt_secret || jwt_secret.to_uppercase() == jwt_secret {
        return (StatusCode::BAD_REQUEST, "JWT_SECRET should contain mixed case characters").into_response();
    }

    // SQLite 模式：数据库在启动时已自动创建并挂载，无需创建库/连接，
    // 本步骤仅保存 JWT_SECRET 配置。
    let db_guard = state.db.read().await;
    if matches!(db_guard.as_ref(), Some(crate::db::DbPool::Sqlite(_))) {
        drop(db_guard);
        let url = effective_db_url();
        let env_content = format!(
            "DATABASE_URL={}\nRUST_LOG=server=debug,tower_http=debug\nJWT_SECRET={}\n",
            url,
            jwt_secret
        );
        if let Err(e) = std::fs::write(".env", env_content) {
             return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write .env: {}", e)).into_response();
        }
        std::env::set_var("JWT_SECRET", jwt_secret);
        return (StatusCode::OK, "Configuration saved (SQLite mode)").into_response();
    }
    drop(db_guard);

    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.user, config.password, config.host, config.port, config.dbname
    );

    std::env::set_var("PGCLIENTENCODING", "UTF8");

    let admin_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.user, config.password, config.host, config.port
    );
    let admin_pool = match PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
    {
        Ok(p) => p,
        Err(e2) => {
            let msg = e2.to_string();
            if msg.contains("non-UTF-8") || msg.contains("lc_messages") {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to connect: authentication failed or database messages are not UTF-8 compatible. Please verify credentials or set lc_messages to C/UTF-8.".to_string()).into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to connect admin database: {}", msg)).into_response();
        }
    };
    let dbname = config.dbname.replace('"', "\"\"");
    let create_sql = format!("CREATE DATABASE \"{}\"", dbname);
    if let Err(create_err) = sqlx::query(&create_sql).execute(&admin_pool).await {
        let create_msg = create_err.to_string();
        if !create_msg.contains("already exists") {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create database: {}", create_msg)).into_response();
        }
    }

    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("non-UTF-8") || msg.contains("lc_messages") {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to connect: authentication failed or database messages are not UTF-8 compatible. Please verify credentials or set lc_messages to C/UTF-8.".to_string()).into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to connect: {}", msg)).into_response();
        }
    };

    // Run idempotent schema initialization
    if let Err(e) = crate::db::ensure_schema_pg(&pool).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to initialize schema: {}", e)).into_response();
    }

    // Store pool in state
    {
        let mut db_guard = state.db.write().await;
        *db_guard = Some(crate::db::DbPool::Pg(pool));
    }

    // Save DB config to .env temporarily (or permanently)
    let env_content = format!(
        "DATABASE_URL={}\nRUST_LOG=server=debug,tower_http=debug\nJWT_SECRET={}\n",
        url,
        jwt_secret
    );
    
    if let Err(e) = std::fs::write(".env", env_content) {
         return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write .env: {}", e)).into_response();
    }
    std::env::set_var("JWT_SECRET", jwt_secret);

    (StatusCode::OK, "Database connected and migrated").into_response()
}

fn validate_admin_password(password: &str) -> Result<(), &'static str> {
    // Minimum length
    if password.len() < 8 {
        return Err("Admin password must be at least 8 characters long");
    }
    
    // Maximum length to prevent DoS
    if password.len() > 128 {
        return Err("Password must not exceed 128 characters");
    }
    
    // Check for at least one letter (uppercase or lowercase)
    if !password.chars().any(|c| c.is_alphabetic()) {
        return Err("Password must contain at least one letter");
    }
    
    // Check for at least one digit
    if !password.chars().any(|c| c.is_numeric()) {
        return Err("Password must contain at least one number");
    }
    
    Ok(())
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

    // Validate admin password strength
    if let Err(e) = validate_admin_password(&config.password) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    let password_hash = match bcrypt::hash(&config.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to hash password: {}", e)).into_response(),
    };

    // Insert admin using runtime query（id 由应用生成，SQLite 无 uuid 默认值）
    let result = crate::dbq!(
        pool, execute,
        "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, 'admin')",
        [Uuid::new_v4(), &config.username, password_hash]
    );

    if let Err(e) = result {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create admin: {}", e)).into_response();
    }

    // Create lock file
    if let Err(e) = File::create("init.lock") {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create lock file: {}", e)).into_response();
    }

    (StatusCode::OK, "Initialization complete").into_response()
}
