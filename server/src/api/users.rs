use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, delete, post},
    Router,
};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;
use crate::AppState;
use crate::api::recorder::get_user_from_header;

#[derive(Serialize, FromRow)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct ResetPasswordPayload {
    pub new_password: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_users))
        .route("/:id", delete(delete_user))
        .route("/:id/reset-password", post(reset_password))
}

// Middleware check for admin would be better, but we'll check manually
async fn ensure_admin(headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let (_, _, role) = get_user_from_header(headers)?;
    if role != "admin" {
        return Err((StatusCode::FORBIDDEN, "Admin access required"));
    }
    Ok(())
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = ensure_admin(&headers).await {
        return e.into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let users = sqlx::query_as::<_, UserInfo>("SELECT id, username, role, created_at FROM users ORDER BY created_at DESC")
        .fetch_all(pool)
        .await;

    match users {
        Ok(u) => Json(u).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch users: {}", e)).into_response(),
    }
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = ensure_admin(&headers).await {
        return e.into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    // Cannot delete self? Maybe check that later.
    
    match sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await 
    {
        Ok(_) => (StatusCode::OK, "User deleted").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to delete user: {}", e)).into_response(),
    }
}

async fn reset_password(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<ResetPasswordPayload>,
) -> impl IntoResponse {
    if let Err(e) = ensure_admin(&headers).await {
        return e.into_response();
    }

    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let hash = match bcrypt::hash(payload.new_password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response(),
    };

    match sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(hash)
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => (StatusCode::OK, "Password reset").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to reset password: {}", e)).into_response(),
    }
}
