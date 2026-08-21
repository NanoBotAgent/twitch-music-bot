use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};

use crate::api::ApiResponse;

/// Configuration for one rate-limit bucket.
#[derive(Debug, Clone)]
pub struct RateLimitBucket {
    /// Logical name embedded into the key (e.g. "login", "search").
    pub name: &'static str,
    pub max_requests: u32,
    pub window_seconds: i64,
}

#[derive(Default)]
struct WindowStore {
    // (ip, bucket) -> (window_start, count)
    windows: HashMap<(SocketAddr, String), (DateTime<Utc>, u32)>,
}

/// Simple fixed-window, in-process IP rate limiter. Good enough for a single
/// instance; swap for Redis-backed limiting when scaling horizontally.
///
/// The client address comes from the `ConnectInfo` extension inserted by
/// `into_make_service_with_connect_info`.
pub async fn ip_rate_limit(
    State(bucket): State<Arc<RateLimitBucket>>,
    req: Request,
    next: Next,
) -> Response {
    static STORE: Mutex<Option<WindowStore>> = Mutex::new(None);

    let addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    let mut guard = match STORE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let store = guard.get_or_insert_with(WindowStore::default);

    let now = Utc::now();
    let window = Duration::seconds(bucket.window_seconds);
    let key = (addr, bucket.name.to_string());

    let allowed = match store.windows.entry(key) {
        std::collections::hash_map::Entry::Occupied(mut occupied) => {
            let (start, count) = occupied.get_mut();
            if now - *start > window {
                *start = now;
                *count = 1;
                true
            } else {
                *count += 1;
                *count <= bucket.max_requests
            }
        }
        std::collections::hash_map::Entry::Vacant(vacant) => {
            vacant.insert((now, 1));
            true
        }
    };

    // Opportunistic cleanup of stale windows to bound memory use.
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
