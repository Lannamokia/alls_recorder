use axum::{
    extract::State,
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use crate::AppState;
use crate::core::hardware::probe_hardware;
use crate::core::auth::decode_jwt;
use sqlx::FromRow;
use serde::Serialize;

#[derive(Serialize, FromRow)]
struct SystemConfigRow {
    value: serde_json::Value,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/scan", post(scan_hardware))
        .route("/info", get(get_hardware_info))
}

async fn scan_hardware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Auth check: Admin only
    let token = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "Missing token"));

    let token = match token {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    let claims = match decode_jwt(token) {
        Ok(c) => c,
        Err(_) => return (StatusCode::UNAUTHORIZED, "Invalid token").into_response(),
    };

    if claims.role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin only").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    // Fetch CLI path
    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'cli_capture_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    
    let cli_path = match row {
        Some((val,)) => val.as_str().unwrap_or("").to_string(),
        None => "".to_string(),
    };

    // Run probe
    let info = match probe_hardware(cli_path).await {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            if is_cli_config_error(&msg) {
                return (StatusCode::BAD_REQUEST, msg).into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Probe failed: {}", msg)).into_response();
        }
    };

    // Save to DB
    let json_value = serde_json::to_value(&info).unwrap();

    let result = sqlx::query(
        "INSERT INTO system_config (key, value) VALUES ('hardware_info', $1) 
         ON CONFLICT (key) DO UPDATE SET value = $1"
    )
    .bind(json_value)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Json(info).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Write Failed: {}", e)).into_response(),
    }
}

async fn get_hardware_info(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let rec: Option<SystemConfigRow> = sqlx::query_as(
        "SELECT value FROM system_config WHERE key = 'hardware_info'"
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None); // Simplified error handling for fetch

    match rec {
        Some(row) => Json(row.value).into_response(),
        None => (StatusCode::NOT_FOUND, "No hardware info found. Please run scan.").into_response(),
    }
}

fn is_cli_config_error(msg: &str) -> bool {
    msg.contains("CLI path")
        || msg.contains("CLI is not executable")
        || msg.contains("Failed to execute CLI")
}
