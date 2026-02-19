use axum::{
    body::Body,
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Json, Response},
    routing::{get, delete, post},
    Router,
};
use std::sync::Arc;
use crate::AppState;
use crate::DownloadToken;
use crate::core::auth::decode_jwt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use sqlx::FromRow;
use serde_json::Value;
use std::path::{Path as FsPath, PathBuf};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use chrono::Utc;

#[derive(Serialize, FromRow)]
pub struct RecordingFile {
    pub id: Uuid,
    pub filename: String,
    pub status: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct RenamePayload {
    pub new_filename: String,
}

#[derive(Serialize)]
struct DownloadTokenResponse {
    token: String,
    expires_at: i64,
}

#[derive(Deserialize)]
struct DownloadTokenQuery {
    token: String,
}

#[derive(FromRow)]
struct FileOwnership {
    user_id: Option<Uuid>,
    filepath: String,
    filename: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_files))
        .route("/:id", delete(delete_file))
        .route("/:id/rename", post(rename_file))
        .route("/:id/download-token", post(create_download_token))
        .route("/download", get(download_with_token))
}

// Helper to extract user info from token
fn get_user_from_header(headers: &HeaderMap) -> Result<(Uuid, String, String), (StatusCode, &'static str)> {
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

async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let files = if role == "admin" {
         sqlx::query_as::<_, RecordingFile>(
            "SELECT id, filename, status, created_at FROM recordings ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, RecordingFile>(
            "SELECT id, filename, status, created_at FROM recordings WHERE user_id = $1 ORDER BY created_at DESC"
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    };

    match files {
        Ok(f) => Json(f).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    }
}

async fn resolve_file_path(pool: &sqlx::PgPool, filepath: &str) -> String {
    let path = FsPath::new(filepath);
    if path.is_absolute() {
        return filepath.to_string();
    }

    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or(filepath);

    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'global_recording_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let base = row.and_then(|v| v.0.as_str().map(String::from)).unwrap_or_default();
    if base.is_empty() {
        filename.to_string()
    } else {
        FsPath::new(&base).join(filename).to_string_lossy().to_string()
    }
}

async fn get_recording_base(pool: &sqlx::PgPool) -> Result<PathBuf, Response> {
    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'global_recording_path'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let base = row.and_then(|v| v.0.as_str().map(String::from)).unwrap_or_default();
    if base.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Global recording path not configured").into_response());
    }

    let base_path = PathBuf::from(base);
    let base_canon = match tokio::fs::canonicalize(&base_path).await {
        Ok(p) => p,
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid recording path: {}", e)).into_response()),
    };
    Ok(base_canon)
}

async fn get_download_token_ttl_minutes(pool: &sqlx::PgPool) -> i64 {
    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM system_config WHERE key = 'download_token_ttl_minutes'")
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    let minutes = row.and_then(|v| v.0.as_i64()).unwrap_or(60);
    if minutes < 1 { 1 } else { minutes }
}

async fn resolve_download_path(base: &PathBuf, relative: &str) -> Result<PathBuf, Response> {
    if relative.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename").into_response());
    }

    let candidate = base.join(relative);
    let candidate_canon = match tokio::fs::canonicalize(&candidate).await {
        Ok(p) => p,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err((StatusCode::NOT_FOUND, "File not found").into_response());
            }
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("File Resolve Error: {}", e)).into_response());
        }
    };

    if !candidate_canon.starts_with(base) {
        return Err((StatusCode::FORBIDDEN, "Access denied").into_response());
    }

    Ok(candidate_canon)
}

async fn delete_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let file: Option<FileOwnership> = sqlx::query_as(
        "SELECT user_id, filepath, filename FROM recordings WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let file = match file {
        Some(f) => f,
        None => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    if role != "admin" && file.user_id != Some(user_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let resolved_path = resolve_file_path(pool, &file.filepath).await;
    if let Err(e) = tokio::fs::remove_file(&resolved_path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("File Delete Error: {}", e)).into_response();
        }
    }

    if let Err(e) = sqlx::query("DELETE FROM recordings WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await 
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Delete Error: {}", e)).into_response();
    }

    (StatusCode::OK, "File deleted").into_response()
}

async fn create_download_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let file: Option<FileOwnership> = sqlx::query_as(
        "SELECT user_id, filepath, filename FROM recordings WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let file = match file {
        Some(f) => f,
        None => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    if role != "admin" && file.user_id != Some(user_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let base = match get_recording_base(pool).await {
        Ok(b) => b,
        Err(r) => return r,
    };

    let resolved = match resolve_download_path(&base, &file.filename).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    let token = Uuid::new_v4().to_string();
    let ttl_minutes = get_download_token_ttl_minutes(pool).await;
    let expires_at = Utc::now().timestamp() + ttl_minutes * 60;
    let mut tokens = state.download_tokens.write().await;
    tokens.insert(token.clone(), DownloadToken {
        user_id,
        file_id: id,
        expires_at,
    });

    let _ = resolved;
    Json(DownloadTokenResponse { token, expires_at }).into_response()
}

async fn download_with_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<DownloadTokenQuery>,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let token_value = params.token.trim().to_string();
    if token_value.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing token").into_response();
    }

    let token_info = {
        let mut tokens = state.download_tokens.write().await;
        tokens.remove(&token_value)
    };

    let token_info = match token_info {
        Some(t) => t,
        None => return (StatusCode::UNAUTHORIZED, "Invalid token").into_response(),
    };

    if token_info.expires_at < Utc::now().timestamp() {
        return (StatusCode::UNAUTHORIZED, "Token expired").into_response();
    }

    if token_info.user_id != user_id {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let file: Option<FileOwnership> = sqlx::query_as(
        "SELECT user_id, filepath, filename FROM recordings WHERE id = $1"
    )
    .bind(token_info.file_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let file = match file {
        Some(f) => f,
        None => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    if role != "admin" && file.user_id != Some(user_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let base = match get_recording_base(pool).await {
        Ok(b) => b,
        Err(r) => return r,
    };

    let resolved = match resolve_download_path(&base, &file.filename).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    let data = match tokio::fs::read(&resolved).await {
        Ok(d) => d,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("File Read Error: {}", e)).into_response();
        }
    };

    let mut response = Response::new(Body::from(data));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, "application/octet-stream".parse().unwrap());
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", file.filename).parse().unwrap(),
    );
    response
}

async fn rename_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(payload): Json<RenamePayload>,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let file: Option<FileOwnership> = sqlx::query_as(
        "SELECT user_id, filepath, filename FROM recordings WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let file = match file {
        Some(f) => f,
        None => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    if role != "admin" && file.user_id != Some(user_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let new_name = payload.new_filename.trim();
    if new_name.is_empty() {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }
    if std::path::Path::new(new_name).components().count() != 1 {
        return (StatusCode::BAD_REQUEST, "Invalid filename").into_response();
    }

    let base = match get_recording_base(pool).await {
        Ok(b) => b,
        Err(r) => return r,
    };

    let old_path = match resolve_download_path(&base, &file.filename).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    let new_path = base.join(new_name);
    if let Ok(_) = tokio::fs::metadata(&new_path).await {
        return (StatusCode::CONFLICT, "File already exists").into_response();
    }

    if let Err(e) = tokio::fs::rename(&old_path, &new_path).await {
        if e.kind() == std::io::ErrorKind::NotFound {
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("File Rename Error: {}", e)).into_response();
    }

    let new_filepath = format!("/recordings/{}", new_name);

    // Update DB
    if let Err(e) = sqlx::query(
        "UPDATE recordings SET filename = $1, filepath = $2 WHERE id = $3"
    )
    .bind(new_name)
    .bind(new_filepath)
    .bind(id)
    .execute(pool)
    .await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Update Error: {}", e)).into_response();
    }

    (StatusCode::OK, "File renamed").into_response()
}
