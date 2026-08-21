use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MusicSource {
    YouTube,
    Spotify,
    SoundCloud,
    Local,
}

impl MusicSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MusicSource::YouTube => "youtube",
            MusicSource::Spotify => "spotify",
            MusicSource::SoundCloud => "soundcloud",
            MusicSource::Local => "local",
        }
    }
}

impl std::fmt::Display for MusicSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: Uuid,
    pub source: MusicSource,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    pub explicit: bool,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub song: Song,
    pub confidence: f32,
    pub matched_query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueStatus {
    Pending,
    Playing,
    Played,
    Skipped,
    Failed,
}

impl QueueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueueStatus::Pending => "pending",
            QueueStatus::Playing => "playing",
            QueueStatus::Played => "played",
            QueueStatus::Skipped => "skipped",
            QueueStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedSong {
    pub queue_item_id: Uuid,
    pub streamer_id: Uuid,
    pub requested_by_user_id: String,
    pub requester_name: String,
    pub song: Song,
    pub status: QueueStatus,
    pub position: i32,
    pub votes: i64,
    pub required_votes: i32,
    pub requested_at: DateTime<Utc>,
}

/// Events sent to overlay WebSocket clients. Each event is tagged with the
/// streamer it belongs to so a single shared broadcast channel can be
/// filtered per connection without leaking other channels' data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct OverlayMessage {
    pub streamer_id: Uuid,
    pub payload: OverlayEvent,
}

impl OverlayMessage {
    pub fn new(streamer_id: Uuid, payload: OverlayEvent) -> Self {
        Self { streamer_id, payload }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayEvent {
    NowPlaying {
        song: Box<Song>,
        requested_by: String,
    },
    SongEnded {
        song_id: Uuid,
    },
    SongSkipped {
        song_id: Uuid,
        skipped_by: String,
    },
    QueueUpdated {
        queue: Vec<QueuedSongSummary>,
    },
    VoteProgress {
        queue_item_id: Uuid,
        current_votes: i64,
        required_votes: i32,
    },
    StreamerOffline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedSongSummary {
    pub position: i32,
    pub title: String,
    pub artist: String,
    pub thumbnail_url: Option<String>,
    pub source: MusicSource,
    pub requested_by: String,
    pub duration_seconds: Option<i32>,
    pub explicit: bool,
    pub votes: i64,
    pub required_votes: i32,
    pub queue_item_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitchUser {
    pub id: String,
    pub twitch_user_id: String,
    pub login: String,
    pub display_name: String,
    pub is_mod: bool,
    pub is_sub: bool,
    pub is_vip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCommand {
    pub command: String,
    pub args: Vec<String>,
    pub raw_message: String,
    pub user: TwitchUser,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Music service error: {0}")]
    Music(#[from] anyhow::Error),
    #[error("Twitch error: {0}")]
    Twitch(String),
    #[error("User blocked from music requests")]
    UserBlocked,
    #[error("Rate limit exceeded")]
    RateLimited,
    #[error("Song not found or not streamable")]
    NotFound,
    #[error("Queue is full")]
    QueueFull,
}
