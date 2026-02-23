use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::AppState;
use crate::core::auth::create_jwt;
use uuid::Uuid;
use sqlx::FromRow;

#[derive(Deserialize)]
pub struct AuthPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub username: String,
    pub role: String,
}

#[derive(FromRow)]
struct User {
    id: Uuid,
    password_hash: String,
    role: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AuthPayload>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    let user: Option<User> = match sqlx::query_as(
        "SELECT id, password_hash, role FROM users WHERE username = $1"
    )
    .bind(&payload.username)
    .fetch_optional(pool)
    .await
    {
        Ok(u) => u,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    };

    let user = match user {
        Some(u) => u,
        None => return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response(),
    };

    let valid = match bcrypt::verify(&payload.password, &user.password_hash) {
        Ok(v) => v,
        Err(_) => false,
    };

    if !valid {
        return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
    }

    match create_jwt(user.id, &payload.username, &user.role) {
        Ok(token) => Json(AuthResponse {
            token,
            username: payload.username,
            role: user.role,
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Token creation failed: {}", e)).into_response(),
    }
}

fn validate_password(password: &str) -> Result<(), &'static str> {
    // Minimum length
    if password.len() < 8 {
        return Err("Password must be at least 8 characters long");
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

async fn register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AuthPayload>,
) -> impl IntoResponse {
    let db_guard = state.db.read().await;
    let pool = match db_guard.as_ref() {
        Some(p) => p,
        None => return (StatusCode::SERVICE_UNAVAILABLE, "Database not connected").into_response(),
    };

    // Validate password strength
    if let Err(e) = validate_password(&payload.password) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // Check if user exists
    let exists = match sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(&payload.username)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)).into_response(),
    };

    if exists {
        return (StatusCode::CONFLICT, "Username already taken").into_response();
    }

    let password_hash = match bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Hashing failed: {}", e)).into_response(),
    };

    let role = "user";

    // Insert and get ID
    // Note: RETURNING clause works with query_scalar or query_as
    let user_id: Uuid = match sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3) RETURNING id"
    )
    .bind(&payload.username)
    .bind(password_hash)
    .bind(role)
    .fetch_one(pool)
    .await
    {
        Ok(id) => id,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create user: {}", e)).into_response(),
    };

    match create_jwt(user_id, &payload.username, role) {
        Ok(token) => Json(AuthResponse {
            token,
            username: payload.username,
            role: role.to_string(),
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Token creation failed: {}", e)).into_response(),
    }
}
