use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};

use crate::api::ApiResponse;

#[derive(Default)]
struct WindowStore {
    // (ip, bucket) -> (window_start, count)
    windows: HashMap<(SocketAddr, String), (DateTime<Utc>, u32)>,
}

static STORE: Mutex<Option<WindowStore>> = Mutex::new(None);

/// Fixed-window IP rate limiter shared by all buckets.
async fn limit(
    req: Request,
    next: Next,
    bucket: &'static str,
    max_requests: u32,
    window_seconds: i64,
) -> Response {
    let addr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    let mut guard = match STORE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let store = guard.get_or_insert_with(WindowStore::default);

    let now = Utc::now();
    let window = Duration::seconds(window_seconds);
    let key = (addr, bucket.to_string());

    let allowed = match store.windows.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut occupied) => {
            let (start, count) = occupied.get_mut();
            if now - *start > window {
                *start = now;
                *count = 1;
                true
            } else {
                *count += 1;
                *count <= max_requests
            }
        }
        std::collections::hash_map::Entry::Vacant(vacant) => {
            vacant.insert((now, 1));
            true
        }
    };

    if store.windows.len() > 10_000 {
        store.windows.retain(|_, (start, _)| now - *start <= window);
    }
    drop(guard);

    if !allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<()>::err("RATE_LIMITED", "Too many requests")),
        )
            .into_response();
    }

    next.run(req).await
}

/// 10 logins / minute per IP.
pub async fn login_rate_limit(State(_): State<()>, req: Request, next: Next) -> Response {
    limit(req, next, "login", 10, 60).await
}

/// 30 searches / minute per IP.
pub async fn search_rate_limit(State(_): State<()>, req: Request, next: Next) -> Response {
    limit(req, next, "search", 30, 60).await
}
