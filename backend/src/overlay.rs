use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{database, metrics};
use twitch_music_shared::OverlayMessage;

/// Shared hub that fans queue events out to all overlay connections.
pub struct OverlayHub {
    pub pool: PgPool,
    /// Every producer (queue managers, bots) sends tagged messages here.
    pub tx: broadcast::Sender<OverlayMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClientMessage {
    Ping,
}

pub async fn overlay_socket(
    ws: WebSocketUpgrade,
    Path(streamer_id): Path<Uuid>,
    State(hub): State<Arc<OverlayHub>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_overlay_socket(socket, streamer_id, hub))
}

async fn handle_overlay_socket(socket: WebSocket, streamer_id: Uuid, hub: Arc<OverlayHub>) {
    let connection_id = Uuid::new_v4().to_string();

    if let Err(e) =
        database::overlay::register(&hub.pool, streamer_id, &connection_id, None).await
    {
        debug!("failed to register overlay connection: {e:#}");
    }
    metrics::record_active_connections(active_count(&hub.pool).await);

    let (mut sender, mut receiver) = socket.split();

    // Fan-out task: only forward messages belonging to THIS streamer.
    let mut rx = hub.tx.subscribe();
    let forward_handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if msg.streamer_id != streamer_id {
                        continue;
                    }
                    let Ok(json) = serde_json::to_string(&msg) else {
                        continue;
                    };
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("overlay client lagged by {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Read loop: client pings keep the DB row fresh.
    while let Some(Ok(msg)) = receiver.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Ping(_) | Message::Pong(_) => {
                let _ = database::overlay::ping(&hub.pool, &connection_id).await;
                continue;
            }
            Message::Close(_) => break,
            Message::Binary(_) => continue,
        };

        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Ping) => {
                let _ = database::overlay::ping(&hub.pool, &connection_id).await;
                let _ = sender.send(Message::Text(r#"{"type":"pong"}"#.to_string())).await;
            }
            Err(e) => debug!("unparsable overlay message: {e}"),
        }
    }

    forward_handle.abort();

    let _ = database::overlay::disconnect(&hub.pool, &connection_id).await;
    metrics::record_active_connections(active_count(&hub.pool).await);
    debug!("overlay disconnected: {connection_id}");
}

async fn active_count(pool: &PgPool) -> usize {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM overlay_connections WHERE disconnected_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .ok();
    row.map(|(n,)| n.max(0) as usize).unwrap_or(0)
}

/// Periodic maintenance: mark stale connections offline and purge old rows.
pub async fn cleanup_task(hub: Arc<OverlayHub>) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        if let Err(e) = database::overlay::cleanup_stale(&hub.pool, 120).await {
            info!("overlay stale cleanup failed: {e:#}");
        }
        if let Err(e) = database::overlay::purge_old(&hub.pool, 7).await {
            info!("overlay purge failed: {e:#}");
        }
    }
}

/// Utility used by tests to ensure unique connection ids.
#[allow(dead_code)]
fn dedupe(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    ids.iter().filter(|id| seen.insert(**id)).copied().collect()
}
