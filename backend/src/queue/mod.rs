#![allow(clippy::type_complexity)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::Settings;
use crate::database;
use crate::metrics;
use crate::music::MusicManager;
use twitch_music_shared::*;

/// Events emitted to internal consumers (API layer) about queue changes.
#[derive(Debug, Clone)]
pub enum QueueNotification {
    SongStarted(QueuedSong),
    SongEnded(Uuid),
    QueueCleared,
}

pub struct QueueManager {
    pool: PgPool,
    #[allow(dead_code)]
    settings: Arc<Settings>,
    music_manager: Arc<MusicManager>,
    /// Shared with the overlay hub; every message is tagged with a streamer id.
    pub overlay_tx: broadcast::Sender<OverlayMessage>,
    notification_tx: mpsc::Sender<QueueNotification>,
    /// Serializes all playback state transitions to prevent double-play races
    /// between the ticker, API skips, and song-end timers.
    playback_lock: Arc<Mutex<()>>,
    current_song: RwLock<Option<QueuedSong>>,
    current_history_id: RwLock<Option<Uuid>>,
    config_cache: RwLock<HashMap<Uuid, database::StreamerConfig>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl QueueManager {
    pub fn new(
        pool: PgPool,
        settings: Arc<Settings>,
        music_manager: Arc<MusicManager>,
        overlay_tx: broadcast::Sender<OverlayMessage>,
        notification_tx: mpsc::Sender<QueueNotification>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(4);

        Self {
            pool,
            settings,
            music_manager,
            overlay_tx,
            notification_tx,
            playback_lock: Arc::new(Mutex::new(())),
            current_song: RwLock::new(None),
            current_history_id: RwLock::new(None),
            config_cache: RwLock::new(HashMap::new()),
            shutdown_tx,
        }
    }

    // -- accessors ---------------------------------------------------------

    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    pub async fn get_current_song(&self, streamer_id: Uuid) -> Option<QueuedSong> {
        self.current_song
            .read()
            .await
            .as_ref()
            .filter(|s| s.streamer_id == streamer_id)
            .cloned()
    }

    // -- configuration ------------------------------------------------------

    async fn config_for(&self, streamer_id: Uuid) -> anyhow::Result<database::StreamerConfig> {
        if let Some(cfg) = self.config_cache.read().await.get(&streamer_id) {
            return Ok(cfg.clone());
        }
        let cfg = database::configs::get(&self.pool, streamer_id).await?;
        self.config_cache.write().await.insert(streamer_id, cfg.clone());
        Ok(cfg)
    }

    pub async fn invalidate_config(&self, streamer_id: Uuid) {
        self.config_cache.write().await.remove(&streamer_id);
    }

    // -- lifecycle ----------------------------------------------------------

    /// Runs the playback loop until shutdown or a fatal DB error.
    pub async fn run(&self) {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));

        info!("Queue manager started");

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Queue manager shutting down");
                    break;
                }
                _ = tick.tick() => {
                    if let Err(e) = self.process_playback().await {
                        error!("Playback processing failed: {e:#}");
                    }
                }
            }
        }

        // Mark any interrupted playing item so it does not stick around.
        if let Some(current) = self.current_song.write().await.take() {
            if let Err(e) =
                database::queue::set_status(&self.pool, current.queue_item_id, QueueStatus::Pending, None).await
            {
                warn!("failed to reset playing item on shutdown: {e:#}");
            }
        }
    }

    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { self.run().await })
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    // -- request intake -----------------------------------------------------

    /// Adds a chat or web request to the queue after applying all configured
    /// restrictions (blocklist, cooldowns, per-user caps, duplicates).
    pub async fn add_request(
        &self,
        streamer_id: Uuid,
        user: &TwitchUser,
        query: &str,
        source_hint: Option<String>,
    ) -> Result<QueuedSong, BotError> {
        let config = self.config_for(streamer_id).await.map_err(BotError::Music)?;

        if database::blocked_users::is_blocked(&self.pool, streamer_id, &user.twitch_user_id)
            .await
            .map_err(BotError::Music)?
        {
            metrics::record_queue_operation("add_blocked_user", false);
            return Err(BotError::UserBlocked);
        }

        // Per-user pending cap.
        let pending_for_user = database::queue::user_pending_count(&self.pool, streamer_id, &user.twitch_user_id)
            .await
            .map_err(BotError::Music)?;
        if pending_for_user >= i64::from(config.max_requests_per_user) && !user.is_mod {
            metrics::record_queue_operation("add_limit", false);
            return Err(BotError::RateLimited);
        }

        // Cooldown between requests.
        if config.request_cooldown_seconds > 0 && !user.is_mod {
            if let Some(last) =
                database::queue::last_request_time(&self.pool, streamer_id, &user.twitch_user_id)
                    .await
                    .map_err(BotError::Music)?
            {
                let elapsed = Utc::now() - last;
                if elapsed < Duration::seconds(i64::from(config.request_cooldown_seconds)) {
                    metrics::record_queue_operation("add_cooldown", false);
                    return Err(BotError::RateLimited);
                }
            }
        }

        // Global per-user rate limit window.
        if !database::rate_limits::check_and_increment(
            &self.pool,
            streamer_id,
            &user.twitch_user_id,
            &user.login,
            "song_request",
            60,
            config.max_requests_per_user * 2,
        )
        .await
        .map_err(BotError::Music)?
        {
            metrics::record_queue_operation("add_ratelimit", false);
            return Err(BotError::RateLimited);
        }

        // Queue capacity.
        let pending_total = database::queue::count_pending(&self.pool, streamer_id)
            .await
            .map_err(BotError::Music)?;
        if pending_total >= i64::from(config.max_queue_size) {
            metrics::record_queue_operation("add_full", false);
            return Err(BotError::QueueFull);
        }

        // Search across enabled sources.
        let results = self
            .music_manager
            .search(streamer_id, query, 5, &config.allowed_sources)
            .await;
        let mut best = results.into_iter().next().ok_or(BotError::NotFound)?;

        // Explicit filter.
        if best.song.explicit && config.explicit_filter == "block" {
            debug!("explicit track filtered for streamer {streamer_id}: {}", best.song.title);
            return Err(BotError::NotFound);
        }

        // Duplicate guard.
        if self.is_duplicate(streamer_id, &best.song.source_id).await.map_err(BotError::Music)? {
            metrics::record_queue_operation("add_duplicate", false);
            return Err(BotError::NotFound);
        }

        // Persist catalog entry and enqueue.
        best.song.id = self.music_manager.persist_song(&best.song).await.map_err(BotError::Music)?;

        let priority = if user.is_mod { 10 } else { 0 };
        let (item_id, _position) =
            database::queue::add(&self.pool, streamer_id, best.song.id, user, priority)
                .await
                .map_err(BotError::Music)?;

        metrics::record_queue_operation("add", true);
        let _ = source_hint;

        let queued = QueuedSong {
            queue_item_id: item_id,
            streamer_id,
            requested_by_user_id: user.twitch_user_id.clone(),
            requester_name: user.display_name.clone(),
            song: best.song,
            status: QueueStatus::Pending,
            position: _position,
            votes: 0,
            required_votes: self.required_votes(streamer_id, &config).await,
            requested_at: Utc::now(),
        };

        self.broadcast_queue(streamer_id).await;
        Ok(queued)
    }

    async fn is_duplicate(&self, streamer_id: Uuid, source_id: &str) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM queue_items q JOIN songs s ON s.id = q.song_id \
             WHERE q.streamer_id = $1 AND s.source_id = $2 AND q.status IN ('pending', 'playing')",
        )
        .bind(streamer_id)
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(n,)| n > 0).unwrap_or(false))
    }

    async fn required_votes(&self, streamer_id: Uuid, config: &database::StreamerConfig) -> i32 {
        let pending = database::queue::count_pending(&self.pool, streamer_id)
            .await
            .unwrap_or(0) as f32;
        let votes = ((pending + 1.0) * config.vote_skip_threshold).ceil() as i32;
        votes.max(1)
    }

    // -- queue inspection ----------------------------------------------------

    pub async fn get_queue(&self, streamer_id: Uuid) -> anyhow::Result<Vec<QueuedSong>> {
        let config = self.config_for(streamer_id).await?;
        let rows = database::queue::get_queue(&self.pool, streamer_id).await?;
        let required = self.required_votes(streamer_id, &config).await;
        Ok(rows.into_iter().map(|r| r.into_queued_song(required)).collect())
    }

    // -- playback -------------------------------------------------------------

    async fn process_playback(&self) -> anyhow::Result<()> {
        let streamers = database::streamers::list_active_with_channels(&self.pool).await?;

        for (streamer_id, login) in streamers {
            // Serialize per-streamer transitions; skip if busy.
            let Ok(_guard) = self.playback_lock.try_lock() else {
                continue;
            };

            let already_playing = self.get_current_song(streamer_id).await.is_some();

            if !already_playing {
                match self.start_next_song(streamer_id).await {
                    Ok(Some(song)) => {
                        let _ = self.notification_tx.send(QueueNotification::SongStarted(song)).await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Failed to start next song for {login}: {e:#}");
                        metrics::record_queue_operation("start_next", false);
                    }
                }
            } else {
                self.check_auto_advance(streamer_id).await?;
            }
        }

        Ok(())
    }

    /// Advances when the current song has played past its duration.
    async fn check_auto_advance(&self, streamer_id: Uuid) -> anyhow::Result<()> {
        let Some(current) = self.get_current_song(streamer_id).await else {
            return Ok(());
        };

        // started_at is tracked through play_history.started_at.
        let history_id = *self.current_history_id.read().await;
        let started_at: Option<(DateTime<Utc>,)> = sqlx::query_as::<_, (DateTime<Utc>,)>(
            "SELECT started_at FROM play_history WHERE id = $1",
        )
        .bind(history_id.unwrap_or_else(Uuid::nil))
        .fetch_optional(&self.pool)
        .await?;

        let Some((started_at,)) = started_at else {
            return Ok(());
        };

        if let Some(dur) = current.song.duration_seconds {
            let elapsed = (Utc::now() - started_at).num_seconds();
            if elapsed > i64::from(dur) + 5 {
                self.finalize_current(streamer_id, false, Some("ended")).await?;
                let _ = self.notification_tx.send(QueueNotification::SongEnded(current.song.id)).await;
            }
        }

        Ok(())
    }

    /// Pops the next pending item and begins playing it.
    async fn start_next_song(&self, streamer_id: Uuid) -> anyhow::Result<Option<QueuedSong>> {
        let Some(row) = database::queue::next_pending(&self.pool, streamer_id).await? else {
            return Ok(None);
        };

        let config = self.config_for(streamer_id).await?;
        let mut song = row.clone().into_queued_song(self.required_votes(streamer_id, &config).await);

        // Resolve the stream URL BEFORE flipping state so failures mark the
        // item failed instead of leaving a ghost "playing" entry.
        let url = match self.music_manager.get_stream_url(streamer_id, &mut song.song).await {
            Ok(url) => url,
            Err(e) => {
                warn!("stream url resolution failed for {}: {e:#}", song.song.title);
                metrics::record_queue_operation("resolve_failed", false);
                database::queue::set_status(&self.pool, row.queue_item_id, QueueStatus::Failed, Some(&format!("{e:#}")))
                    .await?;
                self.broadcast_queue(streamer_id).await;
                return Ok(None);
            }
        };

        let _ = url;
        database::queue::set_status(&self.pool, row.queue_item_id, QueueStatus::Playing, None).await?;

        let history_id = database::history::start(
            &self.pool,
            streamer_id,
            song.song.id,
            Some(row.queue_item_id),
            None,
        )
        .await?;

        *self.current_history_id.write().await = Some(history_id);
        *self.current_song.write().await = Some(song.clone());

        metrics::record_queue_operation("start", true);
        metrics::record_current_song_duration(
            &streamer_id.to_string(),
            f64::from(song.song.duration_seconds.unwrap_or(0)),
        );

        // Notify overlays.
        let _ = self.overlay_tx.send(OverlayMessage::new(
            streamer_id,
            OverlayEvent::NowPlaying {
                song: Box::new(song.song.clone()),
                requested_by: song.requester_name.clone(),
            },
        ));
        self.broadcast_queue(streamer_id).await;

        Ok(Some(song))
    }

    /// Marks the currently playing song finished (or skipped).
    async fn finalize_current(&self, streamer_id: Uuid, skipped: bool, reason: Option<&str>) -> anyhow::Result<()> {
        let Some(current) = self.current_song.write().await.take() else {
            return Ok(());
        };
        let history_id = self.current_history_id.write().await.take();

        let status = if skipped { QueueStatus::Skipped } else { QueueStatus::Played };
        database::queue::set_status(&self.pool, current.queue_item_id, status, None).await?;

        if let Some(history_id) = history_id {
            let duration = (Utc::now() - current.requested_at).num_seconds().clamp(0, 24 * 3600) as i32;
            let _ = database::history::end(&self.pool, history_id, skipped, reason, Some(duration)).await;
        }

        let _ = database::songs::record_played(&self.pool, current.song.id).await;
        let _ = database::votes::clear_for_item(&self.pool, current.queue_item_id).await;

        let _ = self.overlay_tx.send(OverlayMessage::new(
            streamer_id,
            OverlayEvent::SongEnded { song_id: current.song.id },
        ));

        Ok(())
    }

    // -- moderation actions ---------------------------------------------------

    pub async fn skip_current(&self, streamer_id: Uuid, by_name: &str) -> anyhow::Result<bool> {
        let _guard = self.playback_lock.lock().await;

        let Some(current) = self.current_song.read().await.as_ref().filter(|s| s.streamer_id == streamer_id).cloned() else {
            return Ok(false);
        };

        self.finalize_current(streamer_id, true, Some("skipped")).await?;

        let _ = self.overlay_tx.send(OverlayMessage::new(
            streamer_id,
            OverlayEvent::SongSkipped {
                song_id: current.song.id,
                skipped_by: by_name.to_string(),
            },
        ));

        info!("Song '{}' skipped by {}", current.song.title, by_name);
        Ok(true)
    }

    /// Registers a vote-skip. Returns the vote progress, and skips when the
    /// threshold is reached.
    pub async fn vote_skip(
        &self,
        streamer_id: Uuid,
        voter: &TwitchUser,
    ) -> anyhow::Result<Option<(i64, i32)>> {
        let config = self.config_for(streamer_id).await?;

        let Some(current) = self.current_song.read().await.as_ref().filter(|s| s.streamer_id == streamer_id).cloned() else {
            return Ok(None);
        };

        // Mods skip directly.
        if voter.is_mod {
            self.skip_current(streamer_id, &voter.display_name).await?;
            return Ok(None);
        }

        let inserted = database::votes::add(&self.pool, streamer_id, current.queue_item_id, voter).await?;
        let count = database::votes::count(&self.pool, current.queue_item_id).await?;
        let required = self.required_votes(streamer_id, &config).await;

        let _ = self.overlay_tx.send(OverlayMessage::new(
            streamer_id,
            OverlayEvent::VoteProgress {
                queue_item_id: current.queue_item_id,
                current_votes: count,
                required_votes: required,
            },
        ));

        if !inserted {
            return Ok(Some((count, required)));
        }

        info!("vote skip {}/{} for {}", count, required, current.song.title);
        if count >= i64::from(required) {
            self.skip_current(streamer_id, "community vote").await?;
            return Ok(None);
        }

        Ok(Some((count, required)))
    }

    // -- direct queue management (web dashboard) ------------------------------

    pub async fn remove_request(&self, streamer_id: Uuid, queue_item_id: Uuid) -> anyhow::Result<bool> {
        // Cannot remove the currently playing item this way; use skip instead.
        let removed = database::queue::remove_item(&self.pool, queue_item_id).await?;
        if removed {
            metrics::record_queue_operation("remove", true);
            self.broadcast_queue(streamer_id).await;
        }
        Ok(removed)
    }

    pub async fn clear_queue(&self, streamer_id: Uuid) -> anyhow::Result<u64> {
        let cleared = database::queue::clear_pending(&self.pool, streamer_id).await?;
        metrics::record_queue_operation("clear", true);
        let _ = self.notification_tx.send(QueueNotification::QueueCleared).await;
        self.broadcast_queue(streamer_id).await;
        Ok(cleared)
    }

    pub async fn reorder_queue(&self, streamer_id: Uuid, ordered_ids: &[Uuid]) -> anyhow::Result<u64> {
        let moved = database::queue::reorder(&self.pool, streamer_id, ordered_ids).await?;
        metrics::record_queue_operation("reorder", true);
        self.broadcast_queue(streamer_id).await;
        Ok(moved)
    }

    // -- broadcasting -----------------------------------------------------------

    async fn broadcast_queue(&self, streamer_id: Uuid) {
        match self.get_queue_summary(streamer_id).await {
            Ok(queue) => {
                metrics::record_queue_size(&streamer_id.to_string(), queue.len());
                let _ = self.overlay_tx.send(OverlayMessage::new(
                    streamer_id,
                    OverlayEvent::QueueUpdated { queue },
                ));
            }
            Err(e) => warn!("failed to build queue summary for {streamer_id}: {e:#}"),
        }
    }

    async fn get_queue_summary(&self, streamer_id: Uuid) -> anyhow::Result<Vec<QueuedSongSummary>> {
        let config = self.config_for(streamer_id).await?;
        let required = self.required_votes(streamer_id, &config).await;

        Ok(database::queue::get_queue(&self.pool, streamer_id)
            .await?
            .into_iter()
            .map(|row| {
                let q = row.into_queued_song(required);
                QueuedSongSummary {
                    position: q.position,
                    title: q.song.title.clone(),
                    artist: q.song.artist.clone(),
                    thumbnail_url: q.song.thumbnail_url.clone(),
                    source: q.song.source,
                    requested_by: q.requester_name,
                    duration_seconds: q.song.duration_seconds,
                    explicit: q.song.explicit,
                    votes: q.votes,
                    required_votes: q.required_votes,
                    queue_item_id: q.queue_item_id,
                }
            })
            .collect())
    }
}
