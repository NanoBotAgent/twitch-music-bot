use std::sync::Arc;

use axum::{
    extract::{Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use tracing::{error, info, warn};
use urlencoding::encode as url_encode;
use uuid::Uuid;

use crate::api::ApiResponse;
use crate::config::Settings;
use crate::middleware::{ip_rate_limit, RateLimitBucket};
use crate::utils::crypto;

const STATE_TTL_SECONDS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
    pub token_type: String,
}

pub struct AuthState {
    pub pool: PgPool,
    pub settings: Arc<Settings>,
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
    pub aes_key: aes_gcm::Key<aes_gcm::Aes256Gcm>,
    pub http: reqwest::Client,
}

/// Extractor that authenticates a streamer via `Authorization: Bearer <jwt>`.
pub struct AuthUser {
    pub streamer_id: Uuid,
}

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = parts
            .extensions
            .get::<Arc<AuthState>>()
            .cloned()
            .ok_or_else(|| {
                error!("AuthState missing from request extensions");
                (StatusCode::INTERNAL_SERVER_ERROR, "server misconfigured").into_response()
            })?;

        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| unauthorized("MISSING_TOKEN", "Authorization header required"))?;

        let claims = decode::<Claims>(auth_header, &state.decoding_key, &Validation::new(jsonwebtoken::Algorithm::HS256))
            .map_err(|_| unauthorized("INVALID_TOKEN", "Invalid or expired token"))?;

        if claims.claims.token_type != "access" {
            return Err(unauthorized("WRONG_TOKEN_TYPE", "Access token required"));
        }

        let streamer_id = Uuid::parse_str(&claims.claims.sub)
            .map_err(|_| unauthorized("INVALID_TOKEN", "Invalid subject claim"))?;

        Ok(AuthUser { streamer_id })
    }
}

fn unauthorized(code: &str, message: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::err(code, message))).into_response()
}

fn issue_tokens(state: &AuthState, streamer_id: Uuid) -> anyhow::Result<(String, String)> {
    let now = i64::try_from(Utc::now().timestamp())?;
    let access_exp = now + Duration::hours(state.settings.security.jwt_expiry_hours).num_seconds();
    let refresh_exp = now + state.settings.security.refresh_token_days * 24 * 60 * 60;

    let access = encode(
        &Header::default(),
        &Claims { sub: streamer_id.to_string(), iat: now, exp: access_exp, token_type: "access".into() },
        &state.encoding_key,
    )?;
    let refresh = encode(
        &Header::default(),
        &Claims { sub: streamer_id.to_string(), iat: now, exp: refresh_exp, token_type: "refresh".into() },
        &state.encoding_key,
    )?;
    Ok((access, refresh))
}

fn frontend_url(settings: &Settings) -> String {
    settings
        .security
        .cors_allowed_origins
        .first()
        .cloned()
        .unwrap_or_else(|| "http://localhost:3000".to_string())
}

// ---------------------------------------------------------------------------
// Login state (CSRF-protected, single-use, provider-tagged)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct LoginStateData {
    provider: String,
    /// Streamer that initiated a provider *connect* flow. None for initial login.
    streamer_id: Option<Uuid>,
    pkce_verifier: Option<String>,
}

async fn begin_oauth_flow(
    state: &AuthState,
    provider: &str,
    streamer_id: Option<Uuid>,
    authorize_base: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
) -> anyhow::Result<String> {
    let state_token = crypto::generate_token(48);

    let data = LoginStateData {
        provider: provider.to_string(),
        streamer_id,
        pkce_verifier: None,
    };

    crate::database::oauth::store_state(&state.pool, &state_token, &serde_json::to_value(data)?, STATE_TTL_SECONDS)
        .await?;

    let scope_str = scopes.join(" ");
    Ok(format!(
        "{authorize_base}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        url_encode(client_id),
        url_encode(redirect_uri),
        url_encode(&scope_str),
        url_encode(&state_token),
    ))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TwitchCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error_description: Option<String>,
}

async fn twitch_login(State(state): State<Arc<AuthState>>) -> Response {
    match begin_oauth_flow(
        &state,
        "twitch",
        None,
        "https://id.twitch.tv/oauth2/authorize",
        state.settings.twitch_client_id(),
        &state.settings.twitch.redirect_uri,
        &state.settings.twitch.scopes.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await
    {
        Ok(url) => Json(ApiResponse::ok(json!({ "authorize_url": url }))).into_response(),
        Err(e) => {
            error!("Failed to start Twitch login: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("OAUTH_START_FAILED", "Could not start login"))).into_response()
        }
    }
}

async fn twitch_callback(
    State(state): State<Arc<AuthState>>,
    Query(q): Query<TwitchCallbackQuery>,
) -> Response {
    let Some(code) = q.code.as_deref() else {
        let reason = q.error_description.unwrap_or_else(|| "missing code".into());
        warn!("Twitch OAuth callback error: {reason}");
        return redirect_with_error(&state, "oauth_denied");
    };
    let Some(state_token) = q.state.as_deref() else {
        return redirect_with_error(&state, "missing_state");
    };

    // Single-use state validation (CSRF protection).
    let Some(data) = (match crate::database::oauth::take_state(&state.pool, state_token).await {
        Ok(d) => d,
        Err(e) => {
            error!("State lookup failed: {e:#}");
            return redirect_with_error(&state, "state_error");
        }
    }) else {
        return redirect_with_error(&state, "invalid_state");
    };

    let Ok(login_state) = serde_json::from_value::<LoginStateData>(data) else {
        return redirect_with_error(&state, "invalid_state");
    };
    if login_state.provider != "twitch" {
        return redirect_with_error(&state, "provider_mismatch");
    }

    // Exchange the authorization code for tokens.
    let token_resp = match state
        .http
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[
            ("client_id", state.settings.twitch_client_id()),
            ("client_secret", state.settings.twitch_client_secret()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", state.settings.twitch.redirect_uri.as_str()),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Twitch token exchange failed: {e}");
            return redirect_with_error(&state, "token_exchange_failed");
        }
    };

    #[derive(Deserialize)]
    struct TwitchToken {
        access_token: String,
    }

    let token: TwitchToken = match token_resp.json().await {
        Ok(t) => t,
        Err(_) => return redirect_with_error(&state, "token_exchange_failed"),
    };

    // Validate the token against Helix and fetch the user identity.
    #[derive(Deserialize)]
    struct HelixUser {
        id: String,
        login: String,
        display_name: String,
        #[serde(default)]
        profile_image_url: Option<String>,
        #[serde(default)]
        email: Option<String>,
    }

    #[derive(Deserialize)]
    struct HelixUsers {
        data: Vec<HelixUser>,
    }

    let helix = state
        .http
        .get("https://api.twitch.tv/helix/users")
        .header("Client-Id", state.settings.twitch_client_id())
        .bearer_auth(&token.access_token)
        .send()
        .await;

    let user: HelixUser = match helix {
        Ok(resp) => match resp.json::<HelixUsers>().await {
            Ok(mut users) if !users.data.is_empty() => users.data.remove(0),
            _ => return redirect_with_error(&state, "helix_validation_failed"),
        },
        Err(e) => {
            error!("Helix validation failed: {e}");
            return redirect_with_error(&state, "helix_validation_failed");
        }
    };

    // Create or refresh the streamer record.
    let streamer = match crate::database::streamers::create(
        &state.pool,
        &user.id,
        &user.login,
        Some(&user.display_name),
        user.profile_image_url.as_deref(),
        user.email.as_deref(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to persist streamer: {e:#}");
            return redirect_with_error(&state, "persist_failed");
        }
    };

    let (access, refresh) = match issue_tokens(&state, streamer.id) {
        Ok(t) => t,
        Err(e) => {
            error!("JWT issuance failed: {e:#}");
            return redirect_with_error(&state, "token_issue_failed");
        }
    };

    info!("Streamer logged in: {} ({})", user.login, streamer.id);

    let fe = frontend_url(&state.settings);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, format!("{fe}/auth/callback#access_token={access}&refresh_token={refresh}"))
        .body(axum::body::Body::empty())
        .unwrap()
}

fn redirect_with_error(state: &AuthState, code: &str) -> Response {
    let fe = frontend_url(&state.settings);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, format!("{fe}/auth/callback#error={code}"))
        .body(axum::body::Body::empty())
        .unwrap()
}

// --- Provider connect flows (authenticated) ---

async fn spotify_connect(auth: AuthUser, State(state): State<Arc<AuthState>>) -> Response {
    let scopes = ["user-read-email", "user-read-private", "playlist-read-private"];
    match begin_oauth_flow(
        &state,
        "spotify",
        Some(auth.streamer_id),
        "https://accounts.spotify.com/authorize",
        state.settings.spotify_client_id(),
        &state.settings.spotify.redirect_uri,
        &scopes,
    )
    .await
    {
        Ok(url) => Json(ApiResponse::ok(json!({ "authorize_url": url }))).into_response(),
        Err(e) => {
            error!("Failed to start Spotify connect: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("OAUTH_START_FAILED", "Could not start Spotify connect"))).into_response()
        }
    }
}

async fn soundcloud_connect() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ApiResponse::<()>::err("NOT_SUPPORTED", "SoundCloud OAuth connect is not available; configure a SoundCloud client_id in settings instead")),
    )
        .into_response()
}

#[derive(Deserialize)]
struct ProviderCallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

async fn spotify_callback(
    State(state): State<Arc<AuthState>>,
    Query(q): Query<ProviderCallbackQuery>,
) -> Response {
    provider_callback(&state, q, "spotify", "https://accounts.spotify.com/api/token").await
}

async fn soundcloud_callback(
    State(state): State<Arc<AuthState>>,
    Query(q): Query<ProviderCallbackQuery>,
) -> Response {
    let _ = (q.code, q.state);
    redirect_with_error(&state, "not_supported")
}

async fn provider_callback(
    state: &Arc<AuthState>,
    q: ProviderCallbackQuery,
    provider: &str,
    token_url: &str,
) -> Response {
    let (Some(code), Some(state_token)) = (q.code.as_deref(), q.state.as_deref()) else {
        return redirect_with_error(state, "missing_params");
    };

    let Some(data) = (match crate::database::oauth::take_state(&state.pool, state_token).await {
        Ok(d) => d,
        Err(e) => {
            error!("State lookup failed: {e:#}");
            return redirect_with_error(state, "state_error");
        }
    }) else {
        return redirect_with_error(state, "invalid_state");
    };

    let Ok(login_state) = serde_json::from_value::<LoginStateData>(data) else {
        return redirect_with_error(state, "invalid_state");
    };

    if login_state.provider != provider {
        return redirect_with_error(state, "provider_mismatch");
    }
    // Connect flows MUST carry the authenticated streamer id.
    let Some(streamer_id) = login_state.streamer_id else {
        return redirect_with_error(state, "unauthorized_connect");
    };

    let (client_id, client_secret) = if provider == "spotify" {
        (state.settings.spotify_client_id(), state.settings.spotify_client_secret())
    } else {
        (state.settings.soundcloud_client_id(), "")
    };

    let resp = state
        .http
        .post(token_url)
        .header(header::AUTHORIZATION, format!("Basic {}", base64_basic(client_id, client_secret)))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", state.settings.spotify.redirect_uri.as_str()),
        ])
        .send()
        .await;

    #[derive(Deserialize)]
    struct ProviderToken {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_in: Option<i64>,
        #[serde(default)]
        scope: Option<String>,
    }

    let token: ProviderToken = match resp {
        Ok(r) => match r.json().await {
            Ok(t) => t,
            Err(_) => return redirect_with_error(state, "token_exchange_failed"),
        },
        Err(e) => {
            error!("{provider} token exchange failed: {e}");
            return redirect_with_error(state, "token_exchange_failed");
        }
    };

    let aes_key = &state.aes_key;
    let enc_access = match crypto::encrypt(aes_key, &token.access_token) {
        Ok(v) => v,
        Err(e) => {
            error!("Token encryption failed: {e:#}");
            return redirect_with_error(state, "internal_error");
        }
    };
    let enc_refresh = match &token.refresh_token {
        Some(rt) => crypto::encrypt(aes_key, rt).ok(),
        None => None,
    };

    let expires_at = token
        .expires_in
        .map(|secs| Utc::now() + Duration::seconds(i64::from(secs)));
    let scope: Vec<String> = token
        .scope
        .map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();

    if let Err(e) = crate::database::oauth::upsert_provider(
        &state.pool,
        streamer_id,
        provider,
        Some(enc_access.as_bytes()),
        enc_refresh.as_deref().map(str::as_bytes),
        expires_at,
        &scope,
    )
    .await
    {
        error!("Failed to store {provider} tokens: {e:#}");
        return redirect_with_error(state, "persist_failed");
    }

    info!("Connected {provider} for streamer {streamer_id}");
    let fe = frontend_url(&state.settings);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, format!("{fe}/dashboard?connected={provider}"))
        .body(axum::body::Body::empty())
        .unwrap()
}

fn base64_basic(id: &str, secret: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(format!("{id}:{secret}"))
}

// --- Session endpoints ---

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh_token(State(state): State<Arc<AuthState>>, Json(body): Json<RefreshRequest>) -> Response {
    let claims = match decode::<Claims>(
        &body.refresh_token,
        &state.decoding_key,
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    ) {
        Ok(c) => c.claims,
        Err(_) => return unauthorized("INVALID_REFRESH", "Invalid refresh token"),
    };

    if claims.token_type != "refresh" {
        return unauthorized("WRONG_TOKEN_TYPE", "Refresh token required");
    }

    let Ok(streamer_id) = Uuid::parse_str(&claims.sub) else {
        return unauthorized("INVALID_REFRESH", "Invalid subject");
    };

    // The streamer must still exist and be active.
    match crate::database::streamers::get(&state.pool, streamer_id).await {
        Ok(Some(s)) if s.is_active => {}
        Ok(Some(_)) => return unauthorized("ACCOUNT_DISABLED", "Streamer account is disabled"),
        Ok(None) => return unauthorized("UNKNOWN_ACCOUNT", "Streamer not found"),
        Err(e) => {
            error!("Refresh DB lookup failed: {e:#}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("INTERNAL", "Database error"))).into_response();
        }
    }

    match issue_tokens(&state, streamer_id) {
        Ok((access, refresh)) => Json(ApiResponse::ok(json!({
            "access_token": access,
            "refresh_token": refresh,
            "token_type": "Bearer",
        })))
        .into_response(),
        Err(e) => {
            error!("Token refresh failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("INTERNAL", "Token issuance failed"))).into_response()
        }
    }
}

async fn me(auth: AuthUser, State(state): State<Arc<AuthState>>) -> Response {
    match crate::database::streamers::get(&state.pool, auth.streamer_id).await {
        Ok(Some(s)) => Json(ApiResponse::ok(json!({
            "id": s.id,
            "twitch_user_id": s.twitch_user_id,
            "login": s.twitch_login,
            "display_name": s.twitch_display_name,
            "avatar_url": s.avatar_url,
            "email": s.email,
        })))
        .into_response(),
        Ok(None) => unauthorized("UNKNOWN_ACCOUNT", "Streamer not found"),
        Err(e) => {
            error!("me() lookup failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("INTERNAL", "Database error"))).into_response()
        }
    }
}

async fn disconnect_provider(auth: AuthUser, State(state): State<Arc<AuthState>>, axum::extract::Path(provider): axum::extract::Path<String>) -> Response {
    if !matches!(provider.as_str(), "spotify" | "youtube" | "soundcloud") {
        return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err("NOT_FOUND", "Unknown provider"))).into_response();
    }

    let result = match provider.as_str() {
        "spotify" => crate::database::oauth::upsert_provider(&state.pool, auth.streamer_id, "spotify", None, None, None, &[]).await,
        "youtube" => crate::database::oauth::upsert_provider(&state.pool, auth.streamer_id, "youtube", None, None, None, &[]).await,
        _ => crate::database::oauth::upsert_provider(&state.pool, auth.streamer_id, "soundcloud", None, None, None, &[]).await,
    };

    match result {
        Ok(()) => Json(ApiResponse::ok(json!({ "disconnected": provider }))).into_response(),
        Err(e) => {
            error!("Disconnect failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err("INTERNAL", "Database error"))).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Middleware + router
// ---------------------------------------------------------------------------

/// Injects Arc<AuthState> into extensions for the [`AuthUser`] extractor.
pub async fn auth_state_injector(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Response {
    req.extensions_mut().insert(state);
    next.run(req).await
}

async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Response {
    req.extensions_mut().insert(state);
    // AuthUser extractor performs the actual verification.
    next.run(req).await
}

pub fn create_auth_router(state: Arc<AuthState>) -> Router {
    let login_bucket = Arc::new(RateLimitBucket { name: "login", max_requests: 10, window_seconds: 60 });

    // Public: the OAuth login flow itself must be reachable without a JWT.
    let public = Router::new()
        .route("/twitch", post(twitch_login))
        .route("/twitch/callback", get(twitch_callback))
        .route("/spotify/callback", get(spotify_callback))
        .route("/soundcloud/callback", get(soundcloud_callback))
        .route("/refresh", post(refresh_token))
        .layer(middleware::from_fn_with_state(state.clone(), auth_state_injector))
        .layer(middleware::from_fn_with_state(login_bucket.clone(), ip_rate_limit));

    // Protected: session info and provider connect management.
    let protected = Router::new()
        .route("/me", get(me))
        .route("/spotify/start", post(spotify_connect))
        .route("/soundcloud/start", post(soundcloud_connect))
        .route("/:provider", axum::routing::delete(disconnect_provider))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new().nest("/auth", public).nest("/auth", protected).with_state(state)
}
