//! 登录/注册用的简易算术验证码。
//!
//! 内存存储、一次性使用、5 分钟过期。用于防爆破，不追求图形验证码强度。

use axum::{extract::State, response::Json};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::AppState;

const CAPTCHA_TTL: Duration = Duration::from_secs(300);
const MAX_ENTRIES: usize = 5000;

struct CaptchaEntry {
    answer: String,
    expires_at: Instant,
}

#[derive(Default)]
pub struct CaptchaStore {
    entries: RwLock<HashMap<String, CaptchaEntry>>,
}

#[derive(Serialize)]
pub struct CaptchaResponse {
    pub id: String,
    pub question: String,
}

impl CaptchaStore {
    pub async fn generate(&self) -> CaptchaResponse {
        self.cleanup().await;
        let (question, answer) = generate_question();
        let id = Uuid::new_v4().to_string();
        self.entries.write().await.insert(
            id.clone(),
            CaptchaEntry {
                answer,
                expires_at: Instant::now() + CAPTCHA_TTL,
            },
        );
        CaptchaResponse { id, question }
    }

    /// 一次性校验：无论对错，条目都会被移除。
    pub async fn verify(&self, id: &str, answer: &str) -> bool {
        let entry = self.entries.write().await.remove(id);
        match entry {
            Some(e) => {
                e.expires_at > Instant::now() && e.answer == normalize_answer(answer)
            }
            None => false,
        }
    }

    async fn cleanup(&self) {
        let mut map = self.entries.write().await;
        if map.len() > MAX_ENTRIES {
            let now = Instant::now();
            map.retain(|_, e| e.expires_at > now);
        }
    }
}

fn normalize_answer(s: &str) -> String {
    s.trim().to_lowercase()
}

// 无 rand 依赖，用时钟熵做种子的 xorshift64*，对验证码足够。
struct SimpleRng(u64);

impl SimpleRng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn make_rng() -> SimpleRng {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() << 32) ^ u64::from(d.subsec_nanos()))
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    SimpleRng(nanos | 1)
}

fn generate_question() -> (String, String) {
    let mut rng = make_rng();
    let a = (rng.next_u64() % 50 + 1) as u32;
    let b = (rng.next_u64() % 50 + 1) as u32;
    if rng.next_u64() % 2 == 0 {
        (format!("{} + {} = ?", a, b), (a + b).to_string())
    } else if a >= b {
        (format!("{} - {} = ?", a, b), (a - b).to_string())
    } else {
        (format!("{} - {} = ?", b, a), (b - a).to_string())
    }
}

pub async fn captcha_handler(State(state): State<Arc<AppState>>) -> Json<CaptchaResponse> {
    Json(state.captcha_store.generate().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_format() {
        for _ in 0..100 {
            let (q, ans) = generate_question();
            let ans_val: i64 = ans.parse().unwrap();
            assert!(q.ends_with("= ?"));
            assert!(ans_val >= 0 && ans_val <= 99, "q={} ans={}", q, ans);
        }
    }
}
