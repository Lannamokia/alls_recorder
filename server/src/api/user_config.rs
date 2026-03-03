use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use crate::AppState;
use crate::api::recorder::{
    get_user_from_header,
    validate_capture_method,
    validate_capture_mode,
    validate_device_id,
    validate_window_id,
    validate_max_bitrate,
    validate_max_fps,
    validate_resolution_limit,
    validate_resolution_value,
    validate_rtmp_key,
    validate_rtmp_url,
};

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct UserConfig {
    pub max_bitrate: Option<i32>,
    pub max_fps: Option<i32>,
    pub resolution: Option<String>,
    pub monitor_id: Option<String>,
    pub desktop_audio: Option<String>,
    pub mic_audio: Option<String>,
    pub rtmp_url: Option<String>,
    pub rtmp_key: Option<String>,
    pub capture_mode: Option<String>,
    pub capture_method: Option<String>,
    pub window_id: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/config", get(get_config).post(update_config))
}

async fn get_sys_val(pool: &sqlx::PgPool, key: &str) -> Option<serde_json::Value> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    row.map(|r| r.0)
}

async fn get_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let config = sqlx::query_as::<_, UserConfig>("SELECT max_bitrate, max_fps, resolution, monitor_id, desktop_audio, mic_audio, rtmp_url, rtmp_key, capture_mode, capture_method, window_id FROM user_configs WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    match config {
        Some(c) => Json(c).into_response(),
        None => Json(UserConfig {
            max_bitrate: None,
            max_fps: None,
            resolution: None,
            monitor_id: None,
            desktop_audio: None,
            mic_audio: None,
            rtmp_url: None,
            rtmp_key: None,
            capture_mode: None,
            capture_method: None,
            window_id: None,
        }).into_response(),
    }
}

async fn update_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UserConfig>,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let sys_max_fps = get_sys_val(pool, "max_fps").await.and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let sys_max_bitrate = get_sys_val(pool, "max_bitrate").await.and_then(|v| v.as_i64()).unwrap_or(4000) as i32;
    let sys_max_res = get_sys_val(pool, "max_res").await.and_then(|v| v.as_str().map(String::from)).unwrap_or("1920x1080".to_string());

    if let Err(e) = validate_max_fps(sys_max_fps) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    if let Err(e) = validate_max_bitrate(sys_max_bitrate) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    if let Err(e) = validate_resolution_value(&sys_max_res, false) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    if let Some(max_fps) = payload.max_fps {
        if let Err(e) = validate_max_fps(max_fps) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
        if max_fps > sys_max_fps {
            return (StatusCode::BAD_REQUEST, format!("max_fps exceeds system limit {}", sys_max_fps)).into_response();
        }
    }
    if let Some(max_bitrate) = payload.max_bitrate {
        if let Err(e) = validate_max_bitrate(max_bitrate) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
        if max_bitrate > sys_max_bitrate {
            return (StatusCode::BAD_REQUEST, format!("max_bitrate exceeds system limit {}", sys_max_bitrate)).into_response();
        }
    }
    if let Some(resolution) = payload.resolution.as_ref() {
        if let Err(e) = validate_resolution_value(resolution, true) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
        if let Err(e) = validate_resolution_limit(resolution, &sys_max_res) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(monitor_id) = payload.monitor_id.as_ref() {
        if let Err(e) = validate_device_id(monitor_id) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(desktop_audio) = payload.desktop_audio.as_ref() {
        if let Err(e) = validate_device_id(desktop_audio) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(mic_audio) = payload.mic_audio.as_ref() {
        if let Err(e) = validate_device_id(mic_audio) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(rtmp_url) = payload.rtmp_url.as_ref() {
        if let Err(e) = validate_rtmp_url(rtmp_url) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(rtmp_key) = payload.rtmp_key.as_ref() {
        if let Err(e) = validate_rtmp_key(rtmp_key) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(capture_mode) = payload.capture_mode.as_ref() {
        if let Err(e) = validate_capture_mode(capture_mode) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(capture_method) = payload.capture_method.as_ref() {
        if let Err(e) = validate_capture_method(capture_method) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }
    if let Some(window_id) = payload.window_id.as_ref() {
        if let Err(e) = validate_window_id(window_id) {
            return (StatusCode::BAD_REQUEST, e).into_response();
        }
    }

    let result = sqlx::query(
        r#"
        INSERT INTO user_configs (user_id, max_bitrate, max_fps, resolution, monitor_id, desktop_audio, mic_audio, rtmp_url, rtmp_key, capture_mode, capture_method, window_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (user_id)
        DO UPDATE SET
            max_bitrate = EXCLUDED.max_bitrate,
            max_fps = EXCLUDED.max_fps,
            resolution = EXCLUDED.resolution,
            monitor_id = EXCLUDED.monitor_id,
            desktop_audio = EXCLUDED.desktop_audio,
            mic_audio = EXCLUDED.mic_audio,
            rtmp_url = EXCLUDED.rtmp_url,
            rtmp_key = EXCLUDED.rtmp_key,
            capture_mode = EXCLUDED.capture_mode,
            capture_method = EXCLUDED.capture_method,
            window_id = EXCLUDED.window_id
        "#
    )
    .bind(user_id)
    .bind(payload.max_bitrate)
    .bind(payload.max_fps)
    .bind(payload.resolution)
    .bind(payload.monitor_id)
    .bind(payload.desktop_audio)
    .bind(payload.mic_audio)
    .bind(payload.rtmp_url)
    .bind(payload.rtmp_key)
    .bind(payload.capture_mode)
    .bind(payload.capture_method)
    .bind(payload.window_id)
    .execute(pool)
    .await;

    match result {
        Ok(_) => (StatusCode::OK, "Config updated").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update config: {}", e)).into_response(),
    }
}
