use axum::{
    extract::{State, Query},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use crate::AppState;
use crate::core::auth::decode_jwt;
use crate::core::recorder::{StopRequest, RequestStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use sqlx::FromRow;

#[derive(Deserialize)]
pub struct StartRecordingPayload {
    pub filename: Option<String>,
    pub mode: Option<String>, // "record" or "stream"
}

#[derive(Deserialize)]
pub struct RequestStopPayload {
    pub target_user_id: Uuid,
}

#[derive(Deserialize)]
pub struct RespondStopPayload {
    pub accept: bool,
    pub requester_id: Uuid,
}

#[derive(Deserialize)]
pub struct RequestStatusParams {
    pub target_user_id: Uuid,
}

#[derive(Serialize, FromRow)]
pub struct ActiveUser {
    pub user_id: Uuid,
    pub username: String,
}

async fn get_sys_val(pool: &sqlx::PgPool, key: &str) -> Option<serde_json::Value> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    row.map(|r| r.0)
}

fn resolution_rank_from_label(value: &str) -> i32 {
    let v = value.trim().to_lowercase();
    if v == "4k" || v == "2160p" {
        return 3;
    }
    if v == "1080p" {
        return 2;
    }
    if v == "720p" {
        return 1;
    }
    if v == "480p" {
        return 0;
    }
    -1
}

fn resolution_rank_from_dims(w: i32, h: i32) -> i32 {
    let max_side = w.max(h);
    if max_side >= 3000 {
        3
    } else if max_side >= 1900 {
        2
    } else if max_side >= 1200 {
        1
    } else {
        0
    }
}

fn dims_for_rank(rank: i32, portrait: bool) -> (i32, i32) {
    let (w, h) = match rank {
        3 => (3840, 2160),
        2 => (1920, 1080),
        1 => (1280, 720),
        _ => (854, 480),
    };
    if portrait { (h, w) } else { (w, h) }
}

fn parse_resolution_dims(value: &str) -> Option<(i32, i32)> {
    let v = value.trim();
    if let Some((w, h)) = v.split_once('x') {
        let wv = w.parse::<i32>().ok()?;
        let hv = h.parse::<i32>().ok()?;
        if wv > 0 && hv > 0 {
            return Some((wv, hv));
        }
    }
    let rank = resolution_rank_from_label(v);
    if rank >= 0 {
        return Some(dims_for_rank(rank, false));
    }
    None
}

fn clamp_resolution(requested: &str, max_value: &str) -> (i32, i32) {
    let max_rank = {
        let from_label = resolution_rank_from_label(max_value);
        if from_label >= 0 {
            from_label
        } else if let Some((w, h)) = parse_resolution_dims(max_value) {
            resolution_rank_from_dims(w, h)
        } else {
            2
        }
    };

    let (req_w, req_h, portrait, req_rank) = if let Some((w, h)) = parse_resolution_dims(requested) {
        (w, h, h > w, resolution_rank_from_dims(w, h))
    } else {
        (0, 0, false, max_rank)
    };

    if req_rank > max_rank {
        return dims_for_rank(max_rank, portrait);
    }
    if req_w > 0 && req_h > 0 {
        return (req_w, req_h);
    }
    dims_for_rank(max_rank, false)
}

async fn build_start_params(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    username: &str,
    mode: String,
    filename_override: Option<String>,
) -> Result<(Vec<String>, Option<String>, String), Response> {
    let sys_max_bitrate = get_sys_val(pool, "max_bitrate").await.and_then(|v| v.as_i64()).unwrap_or(4000) as i32;
    let sys_max_fps = get_sys_val(pool, "max_fps").await.and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let sys_max_res = get_sys_val(pool, "max_res").await.and_then(|v| v.as_str().map(String::from)).unwrap_or("1920x1080".to_string());
    let sys_encoder = get_sys_val(pool, "video_encoder").await.and_then(|v| v.as_str().map(String::from)).unwrap_or("x264".to_string());

    let user_config = sqlx::query_as::<_, crate::api::user_config::UserConfig>("SELECT max_bitrate, max_fps, resolution, desktop_audio, mic_audio, rtmp_url, rtmp_key FROM user_configs WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    
    let bitrate = user_config.as_ref().and_then(|c| c.max_bitrate).unwrap_or(sys_max_bitrate);
    let fps = user_config.as_ref().and_then(|c| c.max_fps).unwrap_or(sys_max_fps).min(sys_max_fps);
    let resolution = user_config.as_ref().and_then(|c| c.resolution.clone()).unwrap_or(sys_max_res.clone());
    let rtmp_url = user_config.as_ref().and_then(|c| c.rtmp_url.clone()).unwrap_or_default();
    let rtmp_key = user_config.as_ref().and_then(|c| c.rtmp_key.clone()).unwrap_or_default();
    let desktop_audio = user_config.as_ref().and_then(|c| c.desktop_audio.clone()).unwrap_or_default();
    let mic_audio = user_config.as_ref().and_then(|c| c.mic_audio.clone()).unwrap_or_default();

    let mut args = Vec::new();
    args.push("--bitrate".to_string());
    args.push(bitrate.to_string());
    
    args.push("--fps".to_string());
    args.push(fps.to_string());

    let (w, h) = clamp_resolution(&resolution, &sys_max_res);
    args.push("--width".to_string());
    args.push(w.to_string());
    args.push("--height".to_string());
    args.push(h.to_string());

    args.push("--encoder".to_string());
    args.push(sys_encoder);
    
    if mode == "stream" {
        if !rtmp_url.is_empty() {
            args.push("--rtmp".to_string());
            args.push(rtmp_url);
        } else {
            return Err((StatusCode::BAD_REQUEST, "RTMP URL is required for streaming").into_response());
        }
        if !rtmp_key.is_empty() {
            args.push("--key".to_string());
            args.push(rtmp_key);
        }
    } else if mode != "record" {
        return Err((StatusCode::BAD_REQUEST, "Invalid mode").into_response());
    }

    if !desktop_audio.is_empty() {
        args.push("--desktop-audio".to_string());
        args.push(desktop_audio);
    }
    if !mic_audio.is_empty() {
        args.push("--mic-audio".to_string());
        args.push(mic_audio);
    }

    let mut filename = None;
    if mode == "record" {
        let name = filename_override.unwrap_or_else(|| format!("{}_{}.mp4", username, chrono::Utc::now().timestamp()));

        let global_path_row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'global_recording_path'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
        
        let full_path = if let Some((val,)) = global_path_row {
            let base = val.as_str().unwrap_or("");
            if !base.is_empty() {
                std::path::Path::new(base).join(&name).to_string_lossy().to_string()
            } else {
                name.clone()
            }
        } else {
            name.clone()
        };

        args.push("--output".to_string());
        args.push(full_path);
        filename = Some(name);
    }

    let row: Option<(serde_json::Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'cli_capture_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    
    let cli_path = match row {
        Some((val,)) => val.as_str().unwrap_or("").to_string(),
        None => "".to_string(),
    };

    Ok((args, filename, cli_path))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/start", post(start_recording))
        .route("/stop", post(stop_recording))
        .route("/status", get(get_status))
        .route("/active", get(get_active_users))
        .route("/request-stop", post(request_stop))
        .route("/request-status", get(get_request_status))
        .route("/notifications", get(get_notifications))
        .route("/respond-stop", post(respond_stop))
}

// Helper to extract user info from token
pub fn get_user_from_header(headers: &HeaderMap) -> Result<(Uuid, String, String), (StatusCode, &'static str)> {
    let token = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "Missing token"))?;

    let claims = decode_jwt(token)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token"))?;

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID"))?;

    Ok((user_id, claims.username, claims.role))
}

async fn start_recording(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<StartRecordingPayload>,
) -> impl IntoResponse {
    let (user_id, username, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    // Check if recording
    if state.recorder_manager.is_recording(user_id).await {
         return (StatusCode::BAD_REQUEST, "Process already in progress").into_response();
    }
    if state.recorder_manager.has_any_recording().await {
        return (StatusCode::CONFLICT, "Another user is recording").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let mode = payload.mode.unwrap_or_else(|| "record".to_string());
    let (args, filename, cli_path) = match build_start_params(pool, user_id, &username, mode.clone(), payload.filename.clone()).await {
        Ok(v) => v,
        Err(r) => return r,
    };

    match state.recorder_manager.start_recording(user_id, cli_path, args, mode.clone()).await {
        Ok(_) => {
            if let Some(name) = filename {
                 let _ = sqlx::query(
                    "INSERT INTO recordings (user_id, filename, filepath, status) VALUES ($1, $2, $3, 'recording')"
                )
                .bind(user_id)
                .bind(&name)
                .bind(format!("/recordings/{}", name))
                .execute(pool)
                .await;
            }
            
            (StatusCode::OK, format!("{} started", if mode == "record" { "Recording" } else { "Streaming" })).into_response()
        },
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Another recording in progress") {
                return (StatusCode::CONFLICT, "Another user is recording").into_response();
            }
            if is_cli_config_error(&msg) {
                return (StatusCode::BAD_REQUEST, msg).into_response();
            }
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start: {}", msg)).into_response()
        },
    }
}

fn is_cli_config_error(msg: &str) -> bool {
    msg.contains("CLI path")
        || msg.contains("CLI is not executable")
        || msg.contains("Failed to execute CLI")
}

async fn stop_recording(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    perform_stop(&state, user_id).await
}

async fn perform_stop(state: &Arc<AppState>, user_id: Uuid) -> Response {
    match state.recorder_manager.stop_recording(user_id).await {
        Ok(_) => {
             // Update DB status
            let db_guard = state.db.read().await;
            if let Some(pool) = db_guard.as_ref() {
                let _ = sqlx::query(
                    "UPDATE recordings SET status = 'stopped' WHERE user_id = $1 AND status = 'recording'"
                )
                .bind(user_id)
                .execute(pool)
                .await;
            }

            (StatusCode::OK, "Process stopped").into_response()
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to stop: {}", e)).into_response(),
    }
}

async fn get_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let is_recording = state.recorder_manager.is_recording(user_id).await;
    let task_type = state.recorder_manager.get_task_type(user_id).await.unwrap_or_else(|| "idle".to_string());
    Json(serde_json::json!({ 
        "recording": is_recording,
        "task_type": if is_recording { task_type } else { "idle".to_string() }
    })).into_response()
}

async fn get_active_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let active_ids = state.recorder_manager.get_active_users().await;
    
    if active_ids.is_empty() {
        return Json(Vec::<ActiveUser>::new()).into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let users = sqlx::query_as::<_, ActiveUser>(
        "SELECT id as user_id, username FROM users WHERE id = ANY($1)"
    )
    .bind(&active_ids)
    .fetch_all(pool)
    .await;

    match users {
        Ok(u) => Json(u).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    }
}

async fn request_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RequestStopPayload>,
) -> impl IntoResponse {
    let (user_id, username, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if user_id == payload.target_user_id {
         return (StatusCode::BAD_REQUEST, "Cannot request stop for yourself").into_response();
    }
    if !state.recorder_manager.is_recording(payload.target_user_id).await {
        return (StatusCode::BAD_REQUEST, "Target user is not recording").into_response();
    }
    if state.recorder_manager.is_recording(user_id).await {
        return (StatusCode::BAD_REQUEST, "You are already recording").into_response();
    }

    let mut requests = state.stop_requests.write().await;
    if requests.contains_key(&payload.target_user_id) {
        return (StatusCode::CONFLICT, "Request already pending").into_response();
    }
    requests.insert(payload.target_user_id, StopRequest {
        requester_id: user_id,
        requester_name: username.clone(),
        status: RequestStatus::Pending,
    });

    (StatusCode::OK, "Stop request sent").into_response()
}

async fn get_request_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<RequestStatusParams>,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let mut requests = state.stop_requests.write().await;
    if let Some(req) = requests.get(&params.target_user_id) {
        if req.requester_id == user_id {
            let result = req.clone();
            if !matches!(result.status, RequestStatus::Pending) {
                requests.remove(&params.target_user_id);
            }
            return Json(result).into_response();
        }
    }
    
    (StatusCode::NOT_FOUND, "Request not found").into_response()
}

async fn get_notifications(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let requests = state.stop_requests.read().await;
    let request = requests.get(&user_id).filter(|r| matches!(r.status, RequestStatus::Pending));
    Json(request.cloned()).into_response()
}

async fn respond_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RespondStopPayload>,
) -> impl IntoResponse {
    let (user_id, _, _) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let request = {
        let requests = state.stop_requests.read().await;
        requests.get(&user_id).cloned()
    };
    let request = match request {
        Some(r) => r,
        None => return (StatusCode::NOT_FOUND, "No active request found").into_response(),
    };
    if request.requester_id != payload.requester_id {
        return (StatusCode::BAD_REQUEST, "Requester mismatch").into_response();
    }

    if payload.accept {
        if !state.recorder_manager.is_recording(user_id).await {
            return (StatusCode::BAD_REQUEST, "Target user is not recording").into_response();
        }
        let response = perform_stop(&state, user_id).await;
        if response.status() != StatusCode::OK {
            return response;
        }

        let db_guard = state.db.read().await;
        let pool = match db_guard.as_ref() {
            Some(p) => p,
            None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
        };

        let (args, filename, cli_path) = match build_start_params(pool, request.requester_id, &request.requester_name, "record".to_string(), None).await {
            Ok(v) => v,
            Err(r) => return r,
        };

        if let Err(e) = state.recorder_manager.start_recording(request.requester_id, cli_path, args, "record".to_string()).await {
            let msg = e.to_string();
            if msg.contains("Another recording in progress") {
                return (StatusCode::CONFLICT, "Another user is recording").into_response();
            }
            if is_cli_config_error(&msg) {
                return (StatusCode::BAD_REQUEST, msg).into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start: {}", msg)).into_response();
        }

        if let Some(name) = filename {
            let _ = sqlx::query(
                "INSERT INTO recordings (user_id, filename, filepath, status) VALUES ($1, $2, $3, 'recording')"
            )
            .bind(request.requester_id)
            .bind(&name)
            .bind(format!("/recordings/{}", name))
            .execute(pool)
            .await;
        }

        let mut requests = state.stop_requests.write().await;
        if let Some(req) = requests.get_mut(&user_id) {
            req.status = RequestStatus::Accepted;
        }

        (StatusCode::OK, "Request accepted. Switched users.").into_response()
    } else {
        let mut requests = state.stop_requests.write().await;
        if let Some(req) = requests.get_mut(&user_id) {
            req.status = RequestStatus::Denied;
        }
        (StatusCode::OK, "Request denied").into_response()
    }
}
