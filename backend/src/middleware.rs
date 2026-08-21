use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
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

/// Client IP from the ConnectInfo extension.
pub fn client_ip(req: &Request) -> SocketAddr {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)))
}

/// Fixed-window per-IP limiter shared by all buckets. Returns Err(response)
/// with 429 when exhausted; handlers call this before doing work.
pub fn check_rate_limit(
    ip: SocketAddr,
    bucket: &'static str,
    max_requests: u32,
    window_seconds: i64,
) -> Result<(), Response> {
    let mut guard = match STORE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let store = guard.get_or_insert_with(WindowStore::default);

    let now = Utc::now();
    let window = Duration::seconds(window_seconds);
    let key = (ip, bucket.to_string());

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

    // Opportunistic cleanup to bound memory use.
    if store.windows.len() > 10_000 {
        store.windows.retain(|_, (start, _)| now - *start <= window);
    }
    drop(guard);

    if allowed {
        Ok(())
    } else {
        Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<()>::err("RATE_LIMITED", "Too many requests")),
        )
            .into_response())
    }
}

/// Convenience wrapper returning Arc-free ip for tests.
pub fn unknown_ip() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 0))
}
