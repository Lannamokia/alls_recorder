use axum::{
    extract::{State, Path},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, delete},
    Router,
    http::HeaderMap,
};
use std::sync::Arc;
use crate::AppState;
use crate::core::auth::decode_jwt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use sqlx::FromRow;

#[derive(Serialize, FromRow)]
pub struct Announcement {
    pub id: Uuid,
    pub content: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct CreateAnnouncementPayload {
    pub content: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_announcements).post(create_announcement))
        .route("/unread", get(get_unread_announcements))
        .route("/:id", delete(delete_announcement))
        .route("/:id/read", post(mark_read))
}

// ... helper ...

async fn list_announcements(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin only").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let announcements = sqlx::query_as::<_, Announcement>("SELECT id, content, created_at FROM announcements ORDER BY created_at DESC")
        .fetch_all(pool)
        .await;

    match announcements {
        Ok(a) => Json(a).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch: {}", e)).into_response(),
    }
}

async fn delete_announcement(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let (_, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin only").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    if let Err(e) = sqlx::query("DELETE FROM announcements WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to delete: {}", e)).into_response();
    }

    (StatusCode::OK, "Deleted").into_response()
}
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

async fn create_announcement(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateAnnouncementPayload>,
) -> impl IntoResponse {
    let (user_id, _, role) = match get_user_from_header(&headers) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    if role != "admin" {
        return (StatusCode::FORBIDDEN, "Admin only").into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO announcements (content, created_by) VALUES ($1, $2)"
    )
    .bind(&payload.content)
    .bind(user_id)
    .execute(pool)
    .await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response();
    }

    (StatusCode::OK, "Announcement created").into_response()
}

async fn get_unread_announcements(
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

    // Find announcements NOT in user_read_announcements for this user
    let announcements = sqlx::query_as::<_, Announcement>(
        r#"
        SELECT a.id, a.content, a.created_at 
        FROM announcements a
        WHERE NOT EXISTS (
            SELECT 1 FROM user_read_announcements ura 
            WHERE ura.announcement_id = a.id AND ura.user_id = $1
        )
        ORDER BY a.created_at DESC
        "#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await;

    match announcements {
        Ok(a) => Json(a).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    }
}

async fn mark_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
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

    if let Err(e) = sqlx::query(
        "INSERT INTO user_read_announcements (user_id, announcement_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(user_id)
    .bind(id)
    .execute(pool)
    .await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response();
    }

    (StatusCode::OK, "Marked as read").into_response()
}
