use chrono::{DateTime, Duration, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use twitch_music_shared::{MusicSource, QueuedSong, QueueStatus, Song};

/// Runs embedded migrations.
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Streamers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Streamer {
    pub id: Uuid,
    pub twitch_user_id: String,
    pub twitch_login: String,
    pub twitch_display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub is_active: bool,
}

impl Streamer {
    fn row(
        id: Uuid,
        twitch_user_id: String,
        twitch_login: String,
        twitch_display_name: Option<String>,
        avatar_url: Option<String>,
        email: Option<String>,
        is_active: bool,
    ) -> Self {
        Self { id, twitch_user_id, twitch_login, twitch_display_name, avatar_url, email, is_active }
    }
}

const STREAMER_COLUMNS: &str =
    "id, twitch_user_id, twitch_login, twitch_display_name, avatar_url, email, is_active";

pub mod streamers {
    use super::*;

    pub async fn create(
        pool: &PgPool,
        twitch_user_id: &str,
        login: &str,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
        email: Option<&str>,
    ) -> anyhow::Result<Streamer> {
        let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>, Option<String>, bool)>(
            &format!(
                "INSERT INTO streamers (twitch_user_id, twitch_login, twitch_display_name, avatar_url, email) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (twitch_user_id) DO UPDATE SET \
                   twitch_login = EXCLUDED.twitch_login, \
                   twitch_display_name = EXCLUDED.twitch_display_name, \
                   avatar_url = EXCLUDED.avatar_url, \
                   email = COALESCE(EXCLUDED.email, streamers.email) \
                 RETURNING {STREAMER_COLUMNS}"
            ),
        )
        .bind(twitch_user_id)
        .bind(login)
        .bind(display_name)
        .bind(avatar_url)
        .bind(email)
        .fetch_one(pool)
        .await?;

        // Ensure every streamer has a config row and position counter.
        sqlx::query("INSERT INTO streamer_configs (streamer_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(row.0)
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO queue_counters (streamer_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(row.0)
            .execute(pool)
            .await?;

        Ok(Streamer::row(row.0, row.1, row.2, row.3, row.4, row.5, row.6))
    }

    pub async fn get(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<Streamer>> {
        let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>, Option<String>, bool)>(
            &format!("SELECT {STREAMER_COLUMNS} FROM streamers WHERE id = $1"),
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| Streamer::row(r.0, r.1, r.2, r.3, r.4, r.5, r.6)))
    }

    pub async fn find_by_twitch_user_id(pool: &PgPool, twitch_user_id: &str) -> anyhow::Result<Option<Streamer>> {
        let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>, Option<String>, bool)>(
            &format!("SELECT {STREAMER_COLUMNS} FROM streamers WHERE twitch_user_id = $1"),
        )
        .bind(twitch_user_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.map(|r| Streamer::row(r.0, r.1, r.2, r.3, r.4, r.5, r.6)))
    }

    pub async fn list_active_with_channels(pool: &PgPool) -> anyhow::Result<Vec<(Uuid, String)>> {
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, twitch_login FROM streamers WHERE is_active = true",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Streamer configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct StreamerConfig {
    pub streamer_id: Uuid,
    pub queue_mode: String,
    pub max_queue_size: i32,
    pub max_requests_per_user: i32,
    pub request_cooldown_seconds: i32,
    pub explicit_filter: String,
    pub allow_direct_links: bool,
    pub fuzzy_match_threshold: f32,
    pub auto_skip_after_seconds: Option<i32>,
    pub vote_skip_enabled: bool,
    pub vote_skip_threshold: f32,
    pub blocked_artists: Vec<String>,
    pub blocked_keywords: Vec<String>,
    pub allowed_sources: Vec<String>,
    pub default_volume: f32,
    pub crossfade_seconds: f32,
}

pub mod configs {
    use super::*;

    const COLUMNS: &str = "streamer_id, queue_mode, max_queue_size, max_requests_per_user, request_cooldown_seconds, \
                            explicit_filter, allow_direct_links, fuzzy_match_threshold, auto_skip_after_seconds, \
                            vote_skip_enabled, vote_skip_threshold, blocked_artists, blocked_keywords, allowed_sources, \
                            default_volume, crossfade_seconds";

    fn row(r: (Uuid, String, i32, i32, i32, String, bool, f32, Option<i32>, bool, f32, Vec<String>, Vec<String>, Vec<String>, f32, f32)) -> StreamerConfig {
        StreamerConfig {
            streamer_id: r.0,
            queue_mode: r.1,
            max_queue_size: r.2,
            max_requests_per_user: r.3,
            request_cooldown_seconds: r.4,
            explicit_filter: r.5,
            allow_direct_links: r.6,
            fuzzy_match_threshold: r.7,
            auto_skip_after_seconds: r.8,
            vote_skip_enabled: r.9,
            vote_skip_threshold: r.10,
            blocked_artists: r.11,
            blocked_keywords: r.12,
            allowed_sources: r.13,
            default_volume: r.14,
            crossfade_seconds: r.15,
        }
    }

    pub async fn get(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<StreamerConfig> {
        sqlx::query("INSERT INTO streamer_configs (streamer_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(streamer_id)
            .execute(pool)
            .await?;

        let r = sqlx::query_as::<_, (Uuid, String, i32, i32, i32, String, bool, f32, Option<i32>, bool, f32, Vec<String>, Vec<String>, Vec<String>, f32, f32)>(
            &format!("SELECT {COLUMNS} FROM streamer_configs WHERE streamer_id = $1"),
        )
        .bind(streamer_id)
        .fetch_one(pool)
        .await?;
        Ok(row(r))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(pool: &PgPool, cfg: &StreamerConfig) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE streamer_configs SET \
               queue_mode = $2, max_queue_size = $3, max_requests_per_user = $4, request_cooldown_seconds = $5, \
               explicit_filter = $6, allow_direct_links = $7, fuzzy_match_threshold = $8, auto_skip_after_seconds = $9, \
               vote_skip_enabled = $10, vote_skip_threshold = $11, blocked_artists = $12, blocked_keywords = $13, \
               allowed_sources = $14, default_volume = $15, crossfade_seconds = $16 \
             WHERE streamer_id = $1",
        )
        .bind(cfg.streamer_id)
        .bind(&cfg.queue_mode)
        .bind(cfg.max_queue_size)
        .bind(cfg.max_requests_per_user)
        .bind(cfg.request_cooldown_seconds)
        .bind(&cfg.explicit_filter)
        .bind(cfg.allow_direct_links)
        .bind(cfg.fuzzy_match_threshold)
        .bind(cfg.auto_skip_after_seconds)
        .bind(cfg.vote_skip_enabled)
        .bind(cfg.vote_skip_threshold)
        .bind(&cfg.blocked_artists)
        .bind(&cfg.blocked_keywords)
        .bind(&cfg.allowed_sources)
        .bind(cfg.default_volume)
        .bind(cfg.crossfade_seconds)
        .execute(pool)
        .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// OAuth tokens & login state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderTokens {
    pub access_token: Option<Vec<u8>>,
    pub refresh_token: Option<Vec<u8>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope: Option<Vec<String>>,
}

pub mod oauth {
    use super::*;

    pub async fn store_state(
        pool: &PgPool,
        state_token: &str,
        data: &JsonValue,
        ttl_seconds: i64,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO oauth_states (state_token, state_data, expires_at) VALUES ($1, $2, $3)")
            .bind(state_token)
            .bind(data)
            .bind(Utc::now() + Duration::seconds(ttl_seconds))
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Atomically fetch-and-delete a login state row (single-use CSRF protection).
    /// Returns None if unknown or expired.
    pub async fn take_state(pool: &PgPool, state_token: &str) -> anyhow::Result<Option<JsonValue>> {
        let mut tx = pool.begin().await?;
        let row: Option<(JsonValue)> = sqlx::query_as(
            "SELECT state_data FROM oauth_states WHERE state_token = $1 AND expires_at > NOW() FOR UPDATE",
        )
        .bind(state_token)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(data) = row {
            sqlx::query("DELETE FROM oauth_states WHERE state_token = $1")
                .bind(state_token)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(Some(data))
        } else {
            // Clean up expired rows opportunistically.
            tx.rollback().await.ok();
            sqlx::query("DELETE FROM oauth_states WHERE expires_at <= NOW()")
                .execute(pool)
                .await
                .ok();
            Ok(None)
        }
    }

    pub async fn upsert_provider(
        pool: &PgPool,
        streamer_id: Uuid,
        provider: &str,
        access: Option<&[u8]>,
        refresh: Option<&[u8]>,
        expires_at: Option<DateTime<Utc>>,
        scope: &[String],
    ) -> anyhow::Result<()> {
        let (a_col, r_col, e_col, s_col) = match provider {
            "spotify" => ("spotify_access_token", "spotify_refresh_token", "spotify_expires_at", "spotify_scope"),
            "youtube" => ("youtube_access_token", "youtube_refresh_token", "youtube_expires_at", "youtube_scope"),
            "soundcloud" => (
                "soundcloud_access_token",
                "soundcloud_refresh_token",
                "soundcloud_expires_at",
                "soundcloud_scope",
            ),
            other => anyhow::bail!("unknown provider: {other}"),
        };

        let sql = format!(
            "INSERT INTO oauth_tokens (streamer_id, {a_col}, {r_col}, {e_col}, {s_col}) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (streamer_id) DO UPDATE SET \
               {a_col} = EXCLUDED.{a_col}, {r_col} = EXCLUDED.{r_col}, \
               {e_col} = EXCLUDED.{e_col}, {s_col} = EXCLUDED.{s_col}"
        );

        sqlx::query(&sql)
            .bind(streamer_id)
            .bind(access)
            .bind(refresh)
            .bind(expires_at)
            .bind(scope.to_vec())
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn select_provider(pool: &PgPool, streamer_id: Uuid, provider: &str) -> anyhow::Result<Option<ProviderTokens>> {
        let (a_col, r_col, e_col, s_col) = match provider {
            "spotify" => ("spotify_access_token", "spotify_refresh_token", "spotify_expires_at", "spotify_scope"),
            "youtube" => ("youtube_access_token", "youtube_refresh_token", "youtube_expires_at", "youtube_scope"),
            "soundcloud" => (
                "soundcloud_access_token",
                "soundcloud_refresh_token",
                "soundcloud_expires_at",
                "soundcloud_scope",
            ),
            other => anyhow::bail!("unknown provider: {other}"),
        };

        let row = sqlx::query_as::<_, (Option<Vec<u8>>, Option<Vec<u8>>, Option<DateTime<Utc>>, Option<Vec<String>>)>(
            &format!(
                "SELECT {a_col}, {r_col}, {e_col}, {s_col} FROM oauth_tokens WHERE streamer_id = $1"
            ),
        )
        .bind(streamer_id)
        .fetch_optional(pool)
        .await?
        .filter(|(a, _, _, _)| a.is_some());

        Ok(row.map(|(access, refresh, expires_at, scope)| ProviderTokens {
            access_token: access,
            refresh_token: refresh,
            expires_at,
            scope,
        }))
    }

    pub async fn spotify(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<Option<ProviderTokens>> {
        select_provider(pool, streamer_id, "spotify").await
    }
}

// ---------------------------------------------------------------------------
// Songs catalog
// ---------------------------------------------------------------------------

const SONG_COLUMNS: &str =
    "id, source, source_id, title, artist, duration_seconds, thumbnail_url, stream_url, explicit, metadata";

fn song_row(r: (Uuid, String, String, String, String, Option<i32>, Option<String>, Option<String>, bool, JsonValue)) -> Song {
    Song {
        id: r.0,
        source: serde_json::from_value::<MusicSource>(serde_json::Value::String(r.1))
            .unwrap_or(MusicSource::Local),
        source_id: r.2,
        title: r.3,
        artist: r.4,
        duration_seconds: r.5,
        thumbnail_url: r.6,
        stream_url: r.7,
        explicit: r.8,
        metadata: serde_json::from_value(r.9).unwrap_or_default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

type SongTuple = (Uuid, String, String, String, String, Option<i32>, Option<String>, Option<String>, bool, JsonValue);

pub mod songs {
    use super::*;

    const SEL: &str =
        "SELECT id, source, source_id, title, artist, duration_seconds, thumbnail_url, stream_url, explicit, metadata FROM songs";

    pub async fn get_by_id(pool: &PgPool, id: Uuid) -> anyhow::Result<Option<Song>> {
        let r = sqlx::query_as::<_, SongTuple>(&format!("{SEL} WHERE id = $1"))
            .bind(id)
            .fetch_optional(pool)
            .await?;
        Ok(r.map(song_row))
    }

    /// Finds an existing catalog entry by (source, source_id) or inserts it.
    /// Preserves an already-resolved stream_url when reusing rows.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_or_create(pool: &PgPool, song: &Song) -> anyhow::Result<Uuid> {
        let metadata = serde_json::to_value(&song.metadata).unwrap_or(serde_json::json!({}));
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO songs (source, source_id, title, artist, duration_seconds, thumbnail_url, stream_url, explicit, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (source, source_id) DO UPDATE SET \
               title = EXCLUDED.title, \
               artist = EXCLUDED.artist, \
               duration_seconds = EXCLUDED.duration_seconds, \
               thumbnail_url = COALESCE(EXCLUDED.thumbnail_url, songs.thumbnail_url), \
               explicit = EXCLUDED.explicit, \
               updated_at = NOW() \
             RETURNING id",
        )
        .bind(song.source.as_str())
        .bind(&song.source_id)
        .bind(&song.title)
        .bind(&song.artist)
        .bind(song.duration_seconds)
        .bind(&song.thumbnail_url)
        .bind(&song.stream_url)
        .bind(song.explicit)
        .bind(metadata)
        .fetch_one(pool)
        .await?;
        Ok(row.0)
    }

    pub async fn update_stream_url(pool: &PgPool, id: Uuid, url: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE songs SET stream_url = $2 WHERE id = $1")
            .bind(id)
            .bind(url)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn record_played(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE songs SET play_count = play_count + 1, last_played_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Queue items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueueRow {
    pub queue_item_id: Uuid,
    pub streamer_id: Uuid,
    pub song_id: Uuid,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub duration_seconds: Option<i32>,
    pub thumbnail_url: Option<String>,
    pub stream_url: Option<String>,
    pub explicit: bool,
    pub metadata: JsonValue,
    pub requested_by_user_id: String,
    pub requested_by_display_name: String,
    pub requested_by_is_mod: bool,
    pub requested_by_is_sub: bool,
    pub requested_by_is_vip: bool,
    pub status: String,
    pub position: i32,
    pub votes: i64,
    pub requested_at: DateTime<Utc>,
}

impl QueueRow {
    pub fn into_queued_song(self, required_votes: i32) -> QueuedSong {
        QueuedSong {
            queue_item_id: self.queue_item_id,
            streamer_id: self.streamer_id,
            requested_by_user_id: self.requested_by_user_id,
            requester_name: self.requested_by_display_name,
            song: Song {
                id: self.song_id,
                source: serde_json::from_value::<MusicSource>(serde_json::Value::String(self.source))
                    .unwrap_or(MusicSource::Local),
                source_id: self.source_id,
                title: self.title,
                artist: self.artist,
                duration_seconds: self.duration_seconds,
                thumbnail_url: self.thumbnail_url,
                stream_url: self.stream_url,
                explicit: self.explicit,
                metadata: serde_json::from_value(self.metadata).unwrap_or_default(),
                created_at: self.requested_at,
                updated_at: self.requested_at,
            },
            status: serde_json::from_value::<QueueStatus>(serde_json::Value::String(self.status))
                .unwrap_or(QueueStatus::Pending),
            position: self.position,
            votes: self.votes,
            required_votes,
            requested_at: self.requested_at,
        }
    }
}

pub mod queue {
    use super::*;

    const JOIN_SELECT: &str = "SELECT q.id, q.streamer_id, q.song_id, s.source, s.source_id, s.title, s.artist, s.duration_seconds, \
        s.thumbnail_url, s.stream_url, s.explicit, s.metadata, \
        q.requested_by_user_id, q.requested_by_display_name, q.requested_by_is_mod, q.requested_by_is_sub, \
        q.requested_by_is_vip, q.status, q.position, \
        (SELECT COUNT(*)::bigint FROM vote_skips v WHERE v.queue_item_id = q.id) AS votes, \
        q.requested_at \
        FROM queue_items q JOIN songs s ON s.id = q.song_id";

    #[allow(clippy::too_many_arguments)]
    pub async fn add(
        pool: &PgPool,
        streamer_id: Uuid,
        song_id: Uuid,
        user: &twitch_music_shared::TwitchUser,
        priority: i32,
    ) -> anyhow::Result<(Uuid, i32)> {
        let mut tx = pool.begin().await?;

        let (next_pos,): (i32,) = sqlx::query_as(
            "INSERT INTO queue_counters (streamer_id, next_position) VALUES ($1, 2) \
             ON CONFLICT (streamer_id) DO UPDATE SET next_position = queue_counters.next_position + 1 \
             RETURNING next_position - 1",
        )
        .bind(streamer_id)
        .fetch_one(&mut *tx)
        .await?;

        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO queue_items (streamer_id, song_id, requested_by_user_id, requested_by_login, \
               requested_by_display_name, requested_by_is_mod, requested_by_is_sub, requested_by_is_vip, \
               requested_by_badges, position, priority) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, '{}', $9, $10) RETURNING id",
        )
        .bind(streamer_id)
        .bind(song_id)
        .bind(&user.twitch_user_id)
        .bind(&user.login)
        .bind(&user.display_name)
        .bind(user.is_mod)
        .bind(user.is_sub)
        .bind(user.is_vip)
        .bind(next_pos)
        .bind(priority)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((id, next_pos))
    }

    pub async fn count_pending(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM queue_items WHERE streamer_id = $1 AND status = 'pending'",
        )
        .bind(streamer_id)
        .fetch_one(pool)
        .await?;
        Ok(n)
    }

    pub async fn user_pending_count(pool: &PgPool, streamer_id: Uuid, twitch_user_id: &str) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM queue_items WHERE streamer_id = $1 AND status = 'pending' AND requested_by_user_id = $2",
        )
        .bind(streamer_id)
        .bind(twitch_user_id)
        .fetch_one(pool)
        .await?;
        Ok(n)
    }

    pub async fn last_request_time(pool: &PgPool, streamer_id: Uuid, twitch_user_id: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        let r: Option<(DateTime<Utc>,)> = sqlx::query_as(
            "SELECT MAX(requested_at) FROM queue_items WHERE streamer_id = $1 AND requested_by_user_id = $2",
        )
        .bind(streamer_id)
        .bind(twitch_user_id)
        .fetch_one(pool)
        .await?;
        Ok(r.map(|x| x.0))
    }

    pub async fn get_queue(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<Vec<QueueRow>> {
        let rows = sqlx::query_as::<_, QueueRow>(&format!(
            "{JOIN_SELECT} WHERE q.streamer_id = $1 AND q.status = 'pending' \
             ORDER BY q.priority DESC, q.position ASC"
        ))
        .bind(streamer_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_by_id(pool: &PgPool, queue_item_id: Uuid) -> anyhow::Result<Option<QueueRow>> {
        let r = sqlx::query_as::<_, QueueRow>(&format!(
            "{JOIN_SELECT} WHERE q.id = $1"
        ))
        .bind(queue_item_id)
        .fetch_optional(pool)
        .await?;
        Ok(r)
    }

    pub async fn set_status(
        pool: &PgPool,
        queue_item_id: Uuid,
        status: QueueStatus,
        error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let played_at = matches!(status, QueueStatus::Played | QueueStatus::Skipped);
        sqlx::query(
            "UPDATE queue_items SET status = $2, error_message = $3, \
               played_at = CASE WHEN $4 THEN NOW() ELSE played_at END \
             WHERE id = $1",
        )
        .bind(queue_item_id)
        .bind(status.as_str())
        .bind(error_message)
        .bind(played_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn next_pending(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<Option<QueueRow>> {
        let r = sqlx::query_as::<_, QueueRow>(&format!(
            "{JOIN_SELECT} WHERE q.streamer_id = $1 AND q.status = 'pending' \
             ORDER BY q.priority DESC, q.position ASC LIMIT 1"
        ))
        .bind(streamer_id)
        .fetch_optional(pool)
        .await?;
        Ok(r)
    }

    pub async fn remove_item(pool: &PgPool, queue_item_id: Uuid) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM queue_items WHERE id = $1 AND status = 'pending'")
            .bind(queue_item_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn clear_pending(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM queue_items WHERE streamer_id = $1 AND status = 'pending'")
            .bind(streamer_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected())
    }

    pub async fn reorder(pool: &PgPool, streamer_id: Uuid, ordered_ids: &[Uuid]) -> anyhow::Result<u64> {
        let mut tx = pool.begin().await?;
        let mut affected = 0u64;
        for (idx, id) in ordered_ids.iter().enumerate() {
            let res = sqlx::query(
                "UPDATE queue_items SET position = $3 WHERE id = $1 AND streamer_id = $2 AND status = 'pending'",
            )
            .bind(id)
            .bind(streamer_id)
            .bind(idx as i32 + 1)
            .execute(&mut *tx)
            .await?;
            affected += res.rows_affected();
        }
        tx.commit().await?;
        Ok(affected)
    }
}

// ---------------------------------------------------------------------------
// Play history
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct HistoryRow {
    pub history_id: Uuid,
    pub song_id: Uuid,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub duration_seconds: Option<i32>,
    pub thumbnail_url: Option<String>,
    pub explicit: bool,
    pub metadata: JsonValue,
    pub played_by_display_name: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub was_skipped: bool,
    pub skip_reason: Option<String>,
}

pub mod history {
    use super::*;

    pub async fn start(
        pool: &PgPool,
        streamer_id: Uuid,
        song_id: Uuid,
        queue_item_id: Option<Uuid>,
        played_by: Option<&twitch_music_shared::TwitchUser>,
    ) -> anyhow::Result<Uuid> {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO play_history (streamer_id, song_id, queue_item_id, played_by_user_id, played_by_login, played_by_display_name) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(streamer_id)
        .bind(song_id)
        .bind(queue_item_id)
        .bind(played_by.map(|u| u.twitch_user_id.clone()))
        .bind(played_by.map(|u| u.login.clone()))
        .bind(played_by.map(|u| u.display_name.clone()))
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    pub async fn end(
        pool: &PgPool,
        history_id: Uuid,
        was_skipped: bool,
        skip_reason: Option<&str>,
        duration_played: Option<i32>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE play_history SET ended_at = NOW(), was_skipped = $2, skip_reason = $3, duration_played_seconds = $4 \
             WHERE id = $1",
        )
        .bind(history_id)
        .bind(was_skipped)
        .bind(skip_reason)
        .bind(duration_played)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn recent(pool: &PgPool, streamer_id: Uuid, limit: i64) -> anyhow::Result<Vec<HistoryRow>> {
        let rows = sqlx::query_as::<_, HistoryRow>(
            "SELECT h.id, h.song_id, s.source, s.source_id, s.title, s.artist, s.duration_seconds, \
               s.thumbnail_url, s.explicit, s.metadata, \
               h.played_by_display_name, h.started_at, h.ended_at, h.was_skipped, h.skip_reason \
             FROM play_history h JOIN songs s ON s.id = h.song_id \
             WHERE h.streamer_id = $1 AND h.ended_at IS NOT NULL \
             ORDER BY h.started_at DESC LIMIT $2",
        )
        .bind(streamer_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Vote skips
// ---------------------------------------------------------------------------

pub mod votes {
    use super::*;

    pub async fn add(
        pool: &PgPool,
        streamer_id: Uuid,
        queue_item_id: Uuid,
        voter: &twitch_music_shared::TwitchUser,
    ) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "INSERT INTO vote_skips (streamer_id, queue_item_id, voter_user_id, voter_login) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(streamer_id)
        .bind(queue_item_id)
        .bind(&voter.twitch_user_id)
        .bind(&voter.login)
        .execute(pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn count(pool: &PgPool, queue_item_id: Uuid) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vote_skips WHERE queue_item_id = $1")
            .bind(queue_item_id)
            .fetch_one(pool)
            .await?;
        Ok(n)
    }

    pub async fn has_voted(pool: &PgPool, queue_item_id: Uuid, twitch_user_id: &str) -> anyhow::Result<bool> {
        let r: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM vote_skips WHERE queue_item_id = $1 AND voter_user_id = $2",
        )
        .bind(queue_item_id)
        .bind(twitch_user_id)
        .fetch_optional(pool)
        .await?;
        Ok(r.is_some())
    }

    pub async fn clear_for_item(pool: &PgPool, queue_item_id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM vote_skips WHERE queue_item_id = $1")
            .bind(queue_item_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Blocked users
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct BlockedUser {
    pub user_id: String,
    pub user_login: String,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub mod blocked_users {
    use super::*;

    pub async fn is_blocked(pool: &PgPool, streamer_id: Uuid, twitch_user_id: &str) -> anyhow::Result<bool> {
        let r: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM blocked_users \
             WHERE streamer_id = $1 AND user_id = $2 AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .bind(streamer_id)
        .bind(twitch_user_id)
        .fetch_optional(pool)
        .await?;
        Ok(r.is_some())
    }

    pub async fn add(
        pool: &PgPool,
        streamer_id: Uuid,
        user_id: &str,
        user_login: &str,
        reason: Option<&str>,
        blocked_by_id: Option<&str>,
        blocked_by_login: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO blocked_users (streamer_id, user_id, user_login, reason, blocked_by_user_id, blocked_by_login) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (streamer_id, user_id) DO UPDATE SET \
               user_login = EXCLUDED.user_login, reason = EXCLUDED.reason, \
               blocked_by_user_id = EXCLUDED.blocked_by_user_id, blocked_by_login = EXCLUDED.blocked_by_login",
        )
        .bind(streamer_id)
        .bind(user_id)
        .bind(user_login)
        .bind(reason)
        .bind(blocked_by_id)
        .bind(blocked_by_login)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn remove(pool: &PgPool, streamer_id: Uuid, user_id: &str) -> anyhow::Result<bool> {
        let res = sqlx::query("DELETE FROM blocked_users WHERE streamer_id = $1 AND user_id = $2")
            .bind(streamer_id)
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list(pool: &PgPool, streamer_id: Uuid) -> anyhow::Result<Vec<BlockedUser>> {
        let rows = sqlx::query_as::<_, BlockedUser>(
            "SELECT user_id, user_login, reason, expires_at FROM blocked_users WHERE streamer_id = $1 ORDER BY created_at DESC",
        )
        .bind(streamer_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Rate limits (fixed windows, DB-backed so they survive restarts)
// ---------------------------------------------------------------------------

pub mod rate_limits {
    use super::*;

    /// Returns true when the action is allowed for this window.
    pub async fn check_and_increment(
        pool: &PgPool,
        streamer_id: Uuid,
        twitch_user_id: &str,
        login: &str,
        action: &str,
        window_seconds: i64,
        max_count: i32,
    ) -> anyhow::Result<bool> {
        let (count,): (i32,) = sqlx::query_as(
            "INSERT INTO rate_limits (streamer_id, user_id, user_login, action, count, window_start) \
             VALUES ($1, $2, $3, $4, 1, \
               to_timestamp(floor(EXTRACT(EPOCH FROM NOW()) / $5::double precision) * $5::double precision)) \
             ON CONFLICT (streamer_id, user_id, action, window_start) \
             DO UPDATE SET count = rate_limits.count + 1 \
             RETURNING count",
        )
        .bind(streamer_id)
        .bind(twitch_user_id)
        .bind(login)
        .bind(action)
        .bind(window_seconds)
        .fetch_one(pool)
        .await?;
        Ok(count <= max_count)
    }
}

// ---------------------------------------------------------------------------
// Audit log
// ---------------------------------------------------------------------------

pub mod audit {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    pub async fn log(
        pool: &PgPool,
        streamer_id: Uuid,
        actor_user_id: &str,
        actor_login: &str,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        details: Option<&JsonValue>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (streamer_id, actor_user_id, actor_login, action, target_type, target_id, details) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(streamer_id)
        .bind(actor_user_id)
        .bind(actor_login)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(details)
        .execute(pool)
        .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Overlay connections
// ---------------------------------------------------------------------------

pub mod overlay {
    use super::*;

    pub async fn register(pool: &PgPool, streamer_id: Uuid, connection_id: &str, user_agent: Option<&str>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO overlay_connections (streamer_id, connection_id, user_agent, connected_at, last_ping_at) \
             VALUES ($1, $2, $3, NOW(), NOW()) \
             ON CONFLICT (streamer_id, connection_id) DO UPDATE SET \
               disconnected_at = NULL, connected_at = NOW(), last_ping_at = NOW()",
        )
        .bind(streamer_id)
        .bind(connection_id)
        .bind(user_agent)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn ping(pool: &PgPool, connection_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE overlay_connections SET last_ping_at = NOW() WHERE connection_id = $1")
            .bind(connection_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn disconnect(pool: &PgPool, connection_id: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE overlay_connections SET disconnected_at = NOW() WHERE connection_id = $1")
            .bind(connection_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn cleanup_stale(pool: &PgPool, stale_seconds: i64) -> anyhow::Result<u64> {
        let res = sqlx::query(
            "UPDATE overlay_connections SET disconnected_at = NOW() \
             WHERE disconnected_at IS NULL AND last_ping_at < NOW() - ($1 * interval '1 second')",
        )
        .bind(stale_seconds)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }

    pub async fn purge_old(pool: &PgPool, keep_days: i64) -> anyhow::Result<u64> {
        let res = sqlx::query(
            "DELETE FROM overlay_connections WHERE disconnected_at IS NOT NULL \
             AND disconnected_at < NOW() - ($1 * interval '1 day')",
        )
        .bind(keep_days)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }
}


