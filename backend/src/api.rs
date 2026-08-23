use std::sync::Arc;

use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query as AxumQuery, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use tracing::{error, warn};
use urlencoding::encode as urlencode;
use uuid::Uuid;

use twitch_music_shared::SearchResult;

use crate::auth::{auth_state_injector, AuthUser, AuthState};
use crate::config::Settings;
use crate::database;
use crate::middleware::check_rate_limit;
use crate::music::MusicManager;
use crate::queue::detect_source;
use crate::queue::QueueManager;

pub const API_PREFIX: &str = "/api/v1";

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorBody>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { success: true, data: Some(data), error: None }
    }

    pub fn err(code: &str, message: &str) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiErrorBody { code: code.to_string(), message: message.to_string() }),
        }
    }
}

pub struct ApiState {
    pub pool: PgPool,
    #[allow(dead_code)]
    pub settings: Arc<Settings>,
    pub music_manager: Arc<MusicManager>,
    pub queue_manager: Arc<QueueManager>,
    pub http: reqwest::Client,
}

fn internal_error(context: &str, e: anyhow::Error) -> Response {
    error!("{context}: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiResponse::<()>::err("INTERNAL", "An internal error occurred")),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health_check(State(state): State<Arc<ApiState>>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => Json(json!({ "status": "ok" })).into_response(),
        Err(e) => {
            error!("health check DB ping failed: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "status": "degraded" }))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Queue endpoints (all streamer-scoped via JWT)
// ---------------------------------------------------------------------------

async fn get_queue(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
) -> Response {
    match state.queue_manager.get_queue(auth.streamer_id).await {
        Ok(queue) => Json(ApiResponse::ok(json!({ "items": queue, "count": queue.len() }))).into_response(),
        Err(e) => internal_error("get_queue failed", e),
    }
}

async fn get_current_song(auth: AuthUser, State(state): State<Arc<ApiState>>) -> Response {
    match state.queue_manager.get_current_song(auth.streamer_id).await {
        Some(song) => Json(ApiResponse::ok(song)).into_response(),
        None => Json(ApiResponse::ok(JsonValue::Null)).into_response(),
    }
}

#[derive(Deserialize)]
struct SongRequest {
    query: String,
    source_hint: Option<String>,
}

async fn add_request(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Json(body): Json<SongRequest>,
) -> Response {
    if body.query.trim().is_empty() || body.query.len() > 300 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::err("INVALID_QUERY", "query must be 1-300 characters")),
        )
            .into_response();
    }

    // Dashboard requests come from the streamer themselves.
    let streamer_row = match database::streamers::get(&state.pool, auth.streamer_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err("NOT_FOUND", "streamer missing"))).into_response()
        }
        Err(e) => return internal_error("add_request lookup", e),
    };

    let user = twitch_music_shared::TwitchUser {
        id: streamer_row.twitch_user_id.clone(),
        twitch_user_id: streamer_row.twitch_user_id,
        login: streamer_row.twitch_login.clone(),
        display_name: streamer_row.twitch_display_name.unwrap_or_else(|| streamer_row.twitch_login.clone()),
        is_mod: true,
        is_sub: true,
        is_vip: false,
    };

    match state.queue_manager.add_request(auth.streamer_id, &user, body.query.trim(), body.source_hint).await {
        Ok(queued) => Json(ApiResponse::ok(queued)).into_response(),
        Err(twitch_music_shared::BotError::QueueFull) => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<()>::err("QUEUE_FULL", "The queue is full")),
        )
            .into_response(),
        Err(twitch_music_shared::BotError::RateLimited) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<()>::err("RATE_LIMITED", "Cooldown active")),
        )
            .into_response(),
        Err(twitch_music_shared::BotError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::err("NO_MATCH", "No matching song found")),
        )
            .into_response(),
        Err(e) => internal_error("add_request failed", e.into()),
    }
}

async fn skip_song(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Response {
    let current = state.queue_manager.get_current_song(auth.streamer_id).await;
    let Some(current) = current.filter(|s| s.song.id == id || s.queue_item_id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::err("NOT_PLAYING", "That song is not currently playing")),
        )
            .into_response();
    };

    let _ = id;
    match state.queue_manager.skip_current(auth.streamer_id, "dashboard").await {
        Ok(true) => Json(ApiResponse::ok(json!({ "skipped": current.song.id }))).into_response(),
        _ => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<()>::err("SKIP_FAILED", "Could not skip right now")),
        )
            .into_response(),
    }
}

async fn remove_from_queue(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.queue_manager.remove_request(auth.streamer_id, id).await {
        Ok(true) => Json(ApiResponse::ok(json!({ "removed": id }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::err("NOT_FOUND", "Queue item not found or already playing")),
        )
            .into_response(),
        Err(e) => internal_error("remove_from_queue failed", e),
    }
}

async fn clear_queue(auth: AuthUser, State(state): State<Arc<ApiState>>) -> Response {
    match state.queue_manager.clear_queue(auth.streamer_id).await {
        Ok(n) => Json(ApiResponse::ok(json!({ "cleared": n }))).into_response(),
        Err(e) => internal_error("clear_queue failed", e),
    }
}

#[derive(Deserialize)]
struct ReorderRequest {
    order: Vec<Uuid>,
}

async fn reorder_queue(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Json(body): Json<ReorderRequest>,
) -> Response {
    if body.order.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::err("TOO_MANY_ITEMS", "order list too large")),
        )
            .into_response();
    }

    match state.queue_manager.reorder_queue(auth.streamer_id, &body.order).await {
        Ok(n) => Json(ApiResponse::ok(json!({ "reordered": n }))).into_response(),
        Err(e) => internal_error("reorder_queue failed", e),
    }
}

// ---------------------------------------------------------------------------
// History & search
// ---------------------------------------------------------------------------

async fn get_history(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    AxumQuery(params): AxumQuery<HistoryParams>,
) -> Response {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    match database::history::recent(&state.pool, auth.streamer_id, limit).await {
        Ok(rows) => Json(ApiResponse::ok(json!({ "items": rows, "count": rows.len() }))).into_response(),
        Err(e) => internal_error("get_history failed", e),
    }
}

#[derive(Deserialize)]
struct HistoryParams {
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
}

async fn search_songs(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    AxumQuery(params): AxumQuery<SearchParams>,
) -> Response {
    if let Err(resp) = check_rate_limit(addr, "search", 30, 60) {
        return resp;
    }
    if params.q.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::err("INVALID_QUERY", "q must not be empty")),
        )
            .into_response();
    }
    let limit = params.limit.unwrap_or(10).clamp(1, 25);

    let config = match database::configs::get(&state.pool, auth.streamer_id).await {
        Ok(c) => c,
        Err(e) => return internal_error("search_songs config", e),
    };

    let query = params.q.trim();

    if query.starts_with("http://") || query.starts_with("https://") {
        if let Some((source, id_or_url)) = detect_source(query) {
            let source_allowed = config.allowed_sources.iter().any(|s| s == source.as_str());
            if source_allowed {
                match state.music_manager.resolve(auth.streamer_id, source, &id_or_url).await {
                    Ok(song) => {
                        let results = vec![SearchResult {
                            song,
                            confidence: 1.0,
                            matched_query: query.to_string(),
                        }];
                        return Json(ApiResponse::ok(json!({
                            "results": results,
                            "count": 1
                        })))
                            .into_response();
                    }
                    Err(e) => warn!("link resolution failed for {}: {}", query, e),
                }
            } else {
                warn!("source {} not allowed for streamer {}", source, auth.streamer_id);
            }
        }
    }

    let results = state
        .music_manager
        .search(auth.streamer_id, query, limit, &config.allowed_sources)
        .await;

    Json(ApiResponse::ok(json!({ "results": results, "count": results.len() }))).into_response()
}

/// Resolves a playable URL only for songs that belong to this streamer's
/// playback context (current, queued, or recently played).
async fn get_stream_url(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Path(song_id): Path<Uuid>,
) -> Response {
    // Tenancy check: the song must appear in this streamer's queue/history.
    let allowed = match has_playback_context(&state.pool, auth.streamer_id, song_id).await {
        Ok(v) => v,
        Err(e) => return internal_error("get_stream_url tenancy check", e),
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, Json(ApiResponse::<()>::err("FORBIDDEN", "Song is not in your playback history"))).into_response();
    }

    let mut song = match database::songs::get_by_id(&state.pool, song_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err("NOT_FOUND", "Song not found"))).into_response()
        }
        Err(e) => return internal_error("get_stream_url fetch", e),
    };

    match state.music_manager.get_stream_url(auth.streamer_id, &mut song).await {
        Ok(url) => Json(ApiResponse::ok(json!({ "song": song, "url": url }))).into_response(),
        Err(e) => internal_error("stream url resolution failed", e),
    }
}

async fn has_playback_context(pool: &PgPool, streamer_id: Uuid, song_id: Uuid) -> anyhow::Result<bool> {
    // Currently queued or playing?
    let row: Option<(i64,)> = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM queue_items WHERE streamer_id = $1 AND song_id = $2 AND status IN ('pending', 'playing')",
    )
    .bind(streamer_id)
    .bind(song_id)
    .fetch_optional(pool)
    .await?;

    if row.map(|(n,)| n > 0).unwrap_or(false) {
        return Ok(true);
    }

    // Recently played?
    let row: Option<(i64,)> = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM play_history WHERE streamer_id = $1 AND song_id = $2",
    )
    .bind(streamer_id)
    .bind(song_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(n,)| n > 0).unwrap_or(false))
}

// ---------------------------------------------------------------------------
// Streamer configuration
// ---------------------------------------------------------------------------

async fn get_config(auth: AuthUser, State(state): State<Arc<ApiState>>) -> Response {
    match database::configs::get(&state.pool, auth.streamer_id).await {
        Ok(cfg) => Json(ApiResponse::ok(cfg)).into_response(),
        Err(e) => internal_error("get_config failed", e),
    }
}

async fn update_streamer_config(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Json(mut cfg): Json<database::StreamerConfig>,
) -> Response {
    cfg.streamer_id = auth.streamer_id;

    // Sanity bounds so a bad dashboard payload cannot brick playback.
    cfg.max_queue_size = cfg.max_queue_size.clamp(1, 500);
    cfg.max_requests_per_user = cfg.max_requests_per_user.clamp(1, 50);
    cfg.request_cooldown_seconds = cfg.request_cooldown_seconds.clamp(0, 3600);
    cfg.fuzzy_match_threshold = cfg.fuzzy_match_threshold.clamp(0.0, 1.0);
    cfg.vote_skip_threshold = cfg.vote_skip_threshold.clamp(0.05, 1.0);

    if let Err(e) = database::configs::update(&state.pool, &cfg).await {
        return internal_error("update_streamer_config failed", e);
    }
    state.queue_manager.invalidate_config(auth.streamer_id).await;

    if let Err(audit_err) = database::audit::log(
        &state.pool,
        auth.streamer_id,
        "dashboard",
        "dashboard",
        "config.update",
        Some("streamer_config"),
        None,
        None,
    )
    .await
    {
        error!("audit log failed: {audit_err:#}");
    }

    Json(ApiResponse::ok(cfg)).into_response()
}

// ---------------------------------------------------------------------------
// Blocked users management
// ---------------------------------------------------------------------------

async fn list_blocked_users(auth: AuthUser, State(state): State<Arc<ApiState>>) -> Response {
    match database::blocked_users::list(&state.pool, auth.streamer_id).await {
        Ok(users) => Json(ApiResponse::ok(users)).into_response(),
        Err(e) => internal_error("list_blocked_users failed", e),
    }
}

#[derive(Deserialize)]
struct BlockRequest {
    user_id: String,
    user_login: String,
    reason: Option<String>,
}

async fn block_user(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Json(body): Json<BlockRequest>,
) -> Response {
    if body.user_id.trim().is_empty() || body.user_login.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()>::err("VALIDATION", "user_id and user_login are required")),
        )
            .into_response();
    }

    match database::blocked_users::add(
        &state.pool,
        auth.streamer_id,
        body.user_id.trim(),
        body.user_login.trim(),
        body.reason.as_deref(),
        Some("dashboard"),
        Some("dashboard"),
    )
    .await
    {
        Ok(()) => Json(ApiResponse::ok(json!({ "blocked": body.user_login.trim() }))).into_response(),
        Err(e) => internal_error("block_user failed", e),
    }
}

async fn unblock_user(
    auth: AuthUser,
    State(state): State<Arc<ApiState>>,
    Path(user_id): Path<String>,
) -> Response {
    match database::blocked_users::remove(&state.pool, auth.streamer_id, &user_id).await {
        Ok(true) => Json(ApiResponse::ok(json!({ "unblocked": user_id }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::err("NOT_FOUND", "User was not blocked")),
        )
            .into_response(),
        Err(e) => internal_error("unblock_user failed", e),
    }
}

// ---------------------------------------------------------------------------
// Overlay endpoints (public: the streamer id acts as the bearer secret,
// exactly like an OBS browser-source URL)
// ---------------------------------------------------------------------------

async fn overlay_current(
    State(state): State<Arc<ApiState>>,
    Path(streamer_id): Path<Uuid>,
) -> Response {
    let exists = match database::streamers::get(&state.pool, streamer_id).await {
        Ok(Some(s)) => s.is_active,
        Ok(None) => false,
        Err(e) => return internal_error("overlay lookup", e),
    };
    if !exists {
        return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err("NOT_FOUND", "Unknown stream"))).into_response();
    }

    let Some(current) = state.queue_manager.get_current_song(streamer_id).await else {
        return Json(ApiResponse::ok(json!({ "song": null }))).into_response();
    };

    let mut song = current.song.clone();
    let url = match state.music_manager.get_stream_url(streamer_id, &mut song).await {
        Ok(u) => u,
        Err(e) => return internal_error("overlay stream url", e),
    };

    Json(ApiResponse::ok(json!({
        "song": song,
        "requested_by": current.requester_name,
        "url": url,
    })))
    .into_response()
}

// ---------------------------------------------------------------------------
// Public (unauthenticated) endpoints
// ---------------------------------------------------------------------------

/// View-only queue for chat viewers: sanitized now-playing + pending items.
async fn public_queue(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(streamer_id): Path<Uuid>,
) -> Response {
    if let Err(resp) = check_rate_limit(addr, "public_queue", 60, 60) {
        return resp;
    }

    let active = match database::streamers::get(&state.pool, streamer_id).await {
        Ok(Some(s)) => s.is_active,
        Ok(None) => false,
        Err(e) => return internal_error("public queue lookup", e),
    };
    if !active {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::err("NOT_FOUND", "Unknown stream")),
        )
            .into_response();
    }

    let current = state.queue_manager.get_current_song(streamer_id).await;
    let items = match state.queue_manager.get_queue(streamer_id).await {
        Ok(i) => i,
        Err(e) => return internal_error("public queue fetch", e),
    };

    let current_json = current.as_ref().map(|q| {
        json!({
            "title": q.song.title,
            "artist": q.song.artist,
            "duration_seconds": q.song.duration_seconds,
            "thumbnail_url": q.song.thumbnail_url,
            "requested_by": q.requester_name,
        })
    });

    let queue_json: Vec<_> = items
        .iter()
        .map(|q| {
            json!({
                "position": q.position,
                "title": q.song.title,
                "artist": q.song.artist,
                "duration_seconds": q.song.duration_seconds,
                "thumbnail_url": q.song.thumbnail_url,
                "requested_by": q.requester_name,
                "votes": q.votes,
            })
        })
        .collect();

    Json(ApiResponse::ok(json!({
        "current": current_json,
        "items": queue_json,
    })))
    .into_response()
}

/// Metadata for the /dl landing page.
async fn download_meta(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(code): Path<String>,
) -> Response {
    if let Err(resp) = check_rate_limit(addr, "download_meta", 30, 60) {
        return resp;
    }

    let link = match database::download_links::find_active(&state.pool, &code).await {
        Ok(Some(l)) => l,
        Ok(None) | Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::err("EXPIRED", "Download link not found or expired")),
            )
                .into_response()
        }
    };

    let song = match database::songs::get_by_id(&state.pool, link.song_id).await {
        Ok(Some(s)) => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::err("NOT_FOUND", "Song no longer exists")),
            )
                .into_response()
        }
    };

    Json(ApiResponse::ok(json!({
        "title": song.title,
        "artist": song.artist,
        "thumbnail_url": song.thumbnail_url,
        "expires_at": link.expires_at.to_rfc3339(),
    })))
    .into_response()
}

fn sanitize_filename_part(part: &str) -> String {
    let cleaned: String = part
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Streams the current media file for a valid download-link code as an
/// attachment. The upstream URL is resolved on demand (cobalt/mirrors).
async fn download_proxy(
    State(state): State<Arc<ApiState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(code): Path<String>,
) -> Response {
    if let Err(resp) = check_rate_limit(addr, "download_proxy", 10, 60) {
        return resp;
    }

    let link = match database::download_links::find_active(&state.pool, &code).await {
        Ok(Some(l)) => l,
        Ok(None) | Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::err("EXPIRED", "Download link not found or expired")),
            )
                .into_response()
        }
    };

    let mut song = match database::songs::get_by_id(&state.pool, link.song_id).await {
        Ok(Some(s)) => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::err("NOT_FOUND", "Song no longer exists")),
            )
                .into_response()
        }
    };

    let stream_url = match state
        .music_manager
        .get_stream_url(link.streamer_id, &mut song)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            return internal_error("download stream resolution", e);
        }
    };

    let upstream = match state.http.get(&stream_url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            return internal_error(
                "download upstream",
                anyhow::anyhow!("upstream returned {}", r.status()),
            );
        }
        Err(e) => return internal_error("download upstream", e.into()),
    };

    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_string();

    let ext = match content_type.as_str() {
        ct if ct.contains("mpeg") => "mp3",
        ct if ct.contains("mp4") || ct.contains("aac") || ct.contains("m4a") => "m4a",
        ct if ct.contains("ogg") => "ogg",
        ct if ct.contains("wav") => "wav",
        _ => "mp3",
    };

    let filename = format!(
        "{} - {}.{}",
        sanitize_filename_part(&song.artist),
        sanitize_filename_part(&song.title),
        ext
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        content_type.parse().unwrap_or(header::HeaderValue::from_static("audio/mpeg")),
    );
    if let Ok(v) = header::HeaderValue::from_str(&format!(
        "attachment; filename*=UTF-8''{}",
        urlencode(&filename)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }

    (
        StatusCode::OK,
        headers,
        Body::from_stream(upstream.bytes_stream()),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Router assembly
// ---------------------------------------------------------------------------

/// Injects Arc<AuthState> into request extensions so the [`AuthUser`]
/// extractor can authenticate JWTs on every protected route.
pub async fn api_auth_injector(
    State(state): State<Arc<AuthState>>,
    req: Request,
    next: Next,
) -> Response {
    auth_state_injector(State(state), req, next).await
}

pub fn create_api_router(
    api_state: Arc<ApiState>,
    auth_state: Arc<AuthState>,
) -> Router {
        let protected = Router::new()
        .route("/queue", get(get_queue))
        .route("/queue/current", get(get_current_song))
        .route("/queue/clear", post(clear_queue))
        .route("/queue/reorder", put(reorder_queue))
        .route("/queue/:id", delete(remove_from_queue))
        .route("/queue/:id/skip", post(skip_song))
        .route("/requests", post(add_request))
        .route("/history", get(get_history))
        .route("/search", get(search_songs))
        .route("/songs/:id/stream-url", get(get_stream_url))
        .route("/config", get(get_config).put(update_streamer_config))
        .route("/blocked-users", get(list_blocked_users).post(block_user))
        .route("/blocked-users/:user_id", delete(unblock_user))
        .layer(middleware::from_fn_with_state(auth_state.clone(), api_auth_injector));

    Router::new()
        .nest(API_PREFIX, protected)
        .nest(
            "/api/v1/public",
            Router::new()
                .route("/streamers/:streamer_id/queue", get(public_queue))
                .route("/download/:code", get(download_proxy))
                .route("/download/:code/meta", get(download_meta)),
        )
        .route("/health", get(health_check))
        .route("/api/v1/overlay/:streamer_id/current", get(overlay_current))
        .with_state(api_state)
}
