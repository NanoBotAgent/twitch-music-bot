mod api;
mod auth;
mod config;
mod database;
mod metrics;
mod middleware;
mod music;
mod overlay;
mod queue;
mod twitch;
mod utils;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::{HeaderValue, Method},
    routing::get,
    Router,
};
use secrecy::ExposeSecret;
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::AllowOrigin;
use tracing::{error, info};

use crate::api::ApiState;
use crate::auth::AuthState;
use crate::config::Settings;
use crate::music::MusicManager;
use crate::overlay::{cleanup_task, overlay_socket, OverlayHub};
use crate::queue::{QueueManager, QueueNotification};
use crate::twitch::bot::TwitchBot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
        let settings = Arc::new(Settings::new()?);

    // Refuse to run with insecure placeholder secrets.
    if let Err(reason) = settings.security.validate() {
        anyhow::bail!("insecure configuration: {reason}");
    }

    init_tracing(&settings)?;

    info!("twitch-music-bot backend starting");

    // -- Database ------------------------------------------------------------
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(settings.database.url.expose_secret())
        .await?;

    sqlx::query("SELECT 1").execute(&pool).await?;
    info!("Connected to PostgreSQL");

    database::run_migrations(&pool).await?;
    info!("Migrations applied");

    // -- Crypto ---------------------------------------------------------------
    let aes_key = utils::crypto::derive_key_from_secret(settings.security.encryption_key.expose_secret())?;

    // -- Channels ---------------------------------------------------------------
    let (overlay_tx, _) = broadcast::channel::<twitch_music_shared::OverlayMessage>(1024);
    let (notification_tx, mut notification_rx) = mpsc::channel::<QueueNotification>(256);

    // -- Music & queue -----------------------------------------------------------
    let music_manager = Arc::new(MusicManager::new(pool.clone(), settings.clone(), aes_key)?);

    let queue_manager = Arc::new(QueueManager::new(
        pool.clone(),
        settings.clone(),
        music_manager.clone(),
        overlay_tx.clone(),
        notification_tx.clone(),
    ));

    let qm_for_task = queue_manager.clone();
    let queue_task = tokio::spawn(async move { qm_for_task.spawn().await });

    // -- Auth -----------------------------------------------------------------
    let auth_state = Arc::new(AuthState {
        pool: pool.clone(),
        settings: settings.clone(),
        encoding_key: jsonwebtoken::EncodingKey::from_secret(settings.security.jwt_secret.expose_secret().as_bytes()),
        decoding_key: jsonwebtoken::DecodingKey::from_secret(settings.security.jwt_secret.expose_secret().as_bytes()),
        aes_key,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?,
    });

    // -- API state --------------------------------------------------------------
    let api_state = Arc::new(ApiState {
        pool: pool.clone(),
        settings: settings.clone(),
        music_manager,
        queue_manager: queue_manager.clone(),
    });

    let overlay_hub = Arc::new(OverlayHub { pool: pool.clone(), tx: overlay_tx });

    tokio::spawn(cleanup_task(overlay_hub.clone()));

    // -- Router -----------------------------------------------------------------
    let cors = build_cors(&settings);

    let overlay_router = Router::new()
        .route(
            &format!("{}/overlay/:streamer_id/ws", api::API_PREFIX),
            get(overlay_socket),
        )
        .with_state(overlay_hub.clone());

    let app = Router::new()
        .merge(api::create_api_router(api_state.clone(), auth_state.clone()))
        .merge(auth::create_auth_router(auth_state.clone()))
        .merge(overlay_router)
        .layer(cors);

    // -- Background bots ----------------------------------------------------------
    let bot_handles = start_bots(&pool, queue_manager.clone(), notification_tx.clone()).await;

    // Metrics collection loop
    if settings.metrics.enabled {
        metrics::setup_metrics(&settings)?;
        metrics::start_metrics_collection();
    }

    // Notification consumer (currently just logs; extend as needed)
    tokio::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            info!("queue notification: {notification:?}");
        }
    });

    // -- Serve ---------------------------------------------------------------------
    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on http://{addr}");

    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        let _ = ctrl_c.await;
        info!("shutdown signal received");
    };

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await?;

    // -- Teardown --------------------------------------------------------------------
    queue_manager.shutdown();
    let _ = queue_task.await;
    for handle in bot_handles {
        handle.abort();
    }
    info!("shutdown complete");

    Ok(())
}

fn init_tracing(settings: &Settings) -> anyhow::Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(settings.logging.level.clone()));

    let registry = tracing_subscriber::registry().with(filter);

    if settings.logging.json_output {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().pretty()).init();
    }

    Ok(())
}

fn build_cors(settings: &Settings) -> tower_http::cors::CorsLayer {
    let allowed = settings.security.cors_allowed_origins.clone();

    tower_http::cors::CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _| {
            let origin = origin.to_str().unwrap_or("");
            allowed.iter().any(|pattern| {
                if pattern == "*" {
                    true
                } else if let Some(prefix) = pattern.strip_suffix('*') {
                    origin.starts_with(prefix)
                } else {
                    origin == pattern
                }
            })
        }))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ])
        .allow_credentials(false)
        .max_age(Duration::from_secs(3600))
}

/// Spawns chat bots for every active streamer.
async fn start_bots(
    pool: &sqlx::PgPool,
    queue_manager: Arc<QueueManager>,
    notification_tx: mpsc::Sender<QueueNotification>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    match database::streamers::list_active_with_channels(pool).await {
        Ok(streamers) => {
            for (streamer_id, login) in streamers {
                match TwitchBot::spawn_for_channel(
                    streamer_id,
                    login.clone(),
                    queue_manager.clone(),
                    notification_tx.clone(),
                ) {
                    Ok((_, handle)) => handles.push(handle),
                    Err(e) => error!("failed to start bot for #{login}: {e:#}"),
                }
            }
        }
        Err(e) => error!("failed to list streamers for bots: {e:#}"),
    }

    handles
}

