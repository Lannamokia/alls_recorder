use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::AppState;

#[derive(Serialize, Deserialize)]
pub struct CliPathConfig {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct SystemRecordConfig {
    pub max_bitrate: i32,
    pub max_fps: i32,
    pub max_res: String,
    pub video_encoder: String,
}

#[derive(Serialize, Deserialize)]
pub struct GlobalPathConfig {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct DownloadTokenTtlConfig {
    pub minutes: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ServerNameConfig {
    pub name: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/cli-path", get(get_cli_path).post(set_cli_path))
        .route("/record-config", get(get_record_config).post(set_record_config))
        .route("/global-path", get(get_global_path).post(set_global_path))
        .route("/download-token-ttl", get(get_download_token_ttl).post(set_download_token_ttl))
        .route("/server-name", get(get_server_name).post(set_server_name))
}

async fn get_global_path(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'global_recording_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let path = match row {
        Some((val,)) => val.as_str().unwrap_or("").to_string(),
        None => "".to_string(),
    };

    Json(GlobalPathConfig { path }).into_response()
}

async fn set_global_path(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GlobalPathConfig>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let val = serde_json::Value::String(payload.path.clone());

    if let Err(e) = sqlx::query("INSERT INTO system_config (key, value) VALUES ('global_recording_path', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(val)
        .execute(pool)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update config: {}", e)).into_response();
    }

    (StatusCode::OK, "Updated").into_response()
}

async fn get_download_token_ttl(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'download_token_ttl_minutes'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let minutes = row.and_then(|v| v.0.as_i64()).unwrap_or(60);
    Json(DownloadTokenTtlConfig { minutes }).into_response()
}

async fn set_download_token_ttl(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DownloadTokenTtlConfig>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let minutes = if payload.minutes < 1 { 1 } else { payload.minutes };
    let val = serde_json::Value::Number(serde_json::Number::from(minutes));

    if let Err(e) = sqlx::query("INSERT INTO system_config (key, value) VALUES ('download_token_ttl_minutes', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(val)
        .execute(pool)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update config: {}", e)).into_response();
    }

    (StatusCode::OK, "Updated").into_response()
}

async fn get_server_name(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'server_name'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let name = match row.and_then(|v| v.0.as_str().map(|s| s.to_string())) {
        Some(v) if !v.trim().is_empty() => v,
        _ => std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Alls Recorder".to_string()),
    };

    Json(ServerNameConfig { name }).into_response()
}

async fn set_server_name(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ServerNameConfig>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let val = serde_json::Value::String(payload.name.trim().to_string());

    if let Err(e) = sqlx::query("INSERT INTO system_config (key, value) VALUES ('server_name', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(val)
        .execute(pool)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update config: {}", e)).into_response();
    }

    (StatusCode::OK, "Updated").into_response()
}

async fn get_cli_path(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'cli_capture_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let path = match row {
        Some((val,)) => val.as_str().unwrap_or("").to_string(),
        None => "".to_string(),
    };

    Json(CliPathConfig { path }).into_response()
}

async fn set_cli_path(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CliPathConfig>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let val = serde_json::Value::String(payload.path.clone());

    if let Err(e) = sqlx::query("INSERT INTO system_config (key, value) VALUES ('cli_capture_path', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(val)
        .execute(pool)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update config: {}", e)).into_response();
    }

    (StatusCode::OK, "Updated").into_response()
}

async fn get_record_config(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    // Helper to get value
    async fn get_val(pool: &sqlx::PgPool, key: &str) -> Option<serde_json::Value> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
        row.map(|r| r.0)
    }

    let max_bitrate = get_val(pool, "max_bitrate").await.and_then(|v| v.as_i64()).unwrap_or(4000) as i32;
    let max_fps = get_val(pool, "max_fps").await.and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let max_res = get_val(pool, "max_res").await.and_then(|v| v.as_str().map(String::from)).unwrap_or("1920x1080".to_string());
    let video_encoder = get_val(pool, "video_encoder").await.and_then(|v| v.as_str().map(String::from)).unwrap_or("x264".to_string());

    Json(SystemRecordConfig {
        max_bitrate,
        max_fps,
        max_res,
        video_encoder,
    }).into_response()
}

async fn set_record_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SystemRecordConfig>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start transaction: {}", e)).into_response(),
    };

    async fn upsert(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, key: &str, val: serde_json::Value) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO system_config (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2")
            .bind(key)
            .bind(val)
            .execute(&mut **tx)
            .await
            .map(|_| ())
    }

    if let Err(e) = upsert(&mut tx, "max_bitrate", serde_json::json!(payload.max_bitrate)).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set max_bitrate: {}", e)).into_response();
    }
    if let Err(e) = upsert(&mut tx, "max_fps", serde_json::json!(payload.max_fps)).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set max_fps: {}", e)).into_response();
    }
    if let Err(e) = upsert(&mut tx, "max_res", serde_json::json!(payload.max_res)).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set max_res: {}", e)).into_response();
    }
    if let Err(e) = upsert(&mut tx, "video_encoder", serde_json::json!(payload.video_encoder)).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set video_encoder: {}", e)).into_response();
    }

    if let Err(e) = tx.commit().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)).into_response();
    }

    (StatusCode::OK, "Updated").into_response()
}
