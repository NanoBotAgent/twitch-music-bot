use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::debug;
use url::Url;

use uuid::Uuid;

use crate::config::Settings;
use crate::utils::crypto;
use twitch_music_shared::*;

const ACCOUNTS_BASE: &str = "https://accounts.spotify.com";

/// Spotify client that authenticates per-streamer using OAuth tokens stored
/// (encrypted) in the database by the connect flow in `crate::auth`.
#[derive(Debug)]
pub struct SpotifyClient {
    http: Client,
    pool: PgPool,
    client_id: String,
    client_secret: String,
    aes_key: Option<aes_gcm::Key<aes_gcm::Aes256Gcm>>,
}

#[derive(Debug, Deserialize)]
struct SpotifyTokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpotifyImage {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct SpotifyArtist {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct SpotifyAlbum {
    #[serde(default)]
    images: Vec<SpotifyImage>,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyTrack {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    artists: Vec<SpotifyArtist>,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    explicit: bool,
    #[serde(default)]
    album: Option<SpotifyAlbum>,
    #[serde(rename = "is_playable", default)]
    is_playable: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SpotifySearchResponse {
    #[serde(default)]
    tracks: Option<SpotifyPaging<SpotifyTrack>>,
}

#[derive(Debug, Deserialize)]
struct SpotifyPaging<T> {
    #[serde(default)]
    items: Vec<T>,
}

impl SpotifyClient {
    pub fn new(pool: PgPool, settings: &Settings) -> Result<Self, anyhow::Error> {
        let http = ClientBuilder::new()
            .timeout(Duration::from_secs(15))
            .build()?;

        Ok(Self {
            http,
            pool,
            client_id: settings.spotify_client_id().to_string(),
            client_secret: settings.spotify_client_secret().to_string(),
            aes_key: None,
        })
    }

    fn configured(&self) -> bool {
        !self.client_id.trim().is_empty() && !self.client_secret.trim().is_empty()
    }

    /// Returns a valid access token for the streamer, refreshing it via the
    /// refresh token when expired and persisting the new token.
    async fn access_token(&self, streamer_id: Uuid) -> anyhow::Result<String> {
        if !self.configured() {
            anyhow::bail!("Spotify API credentials are not configured");
        }

        let Some(tokens) = crate::database::oauth::spotify(&self.pool, streamer_id).await? else {
            anyhow::bail!("Spotify is not connected for this streamer");
        };

        let access_enc = tokens
            .access_token
            .ok_or_else(|| anyhow::anyhow!("Spotify is not connected for this streamer"))?;
        let refresh_enc = tokens
            .refresh_token
            .ok_or_else(|| anyhow::anyhow!("Spotify refresh token missing; reconnect Spotify"))?;

        let access = crypto::decrypt(&self.aes_key(), std::str::from_utf8(&access_enc)?)?;

        // Refresh when expired or about to expire within 60 seconds.
        let needs_refresh = match tokens.expires_at {
            Some(exp) => exp - Utc::now() < chrono::Duration::seconds(60),
            None => true,
        };

        if !needs_refresh {
            return Ok(access);
        }

        debug!("Refreshing Spotify token for streamer {streamer_id}");
        let refresh = crypto::decrypt(&self.aes_key(), std::str::from_utf8(&refresh_enc)?)?;

        let resp = self
            .http
            .post(format!("{ACCOUNTS_BASE}/api/token"))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Basic {}", STANDARD.encode(format!("{}:{}", self.client_id, self.client_secret))),
            )
            .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json::<SpotifyTokenResponse>()
            .await?;

        let enc_access = crypto::encrypt(&self.aes_key(), &resp.access_token)?;
        let expires_at = resp.expires_in.map(|s| Utc::now() + chrono::Duration::seconds(s));
        let scope: Vec<String> = resp
            .scope
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();

        // Keep the existing refresh token when Spotify does not rotate it.
        crate::database::oauth::upsert_provider(
            &self.pool,
            streamer_id,
            "spotify",
            Some(enc_access.as_bytes()),
            Some(refresh_enc.as_slice()),
            expires_at,
            &scope,
        )
        .await?;

        Ok(resp.access_token)
    }

    /// Placeholder for the AES key; the real key is injected via `with_aes_key`
    /// because it is derived from the runtime secret in `AuthState`.
    fn aes_key(&self) -> aes_gcm::Key<aes_gcm::Aes256Gcm> {
        self.aes_key
            .expect("SpotifyClient AES key must be set via with_aes_key")
    }

    pub fn with_aes_key(mut self, key: aes_gcm::Key<aes_gcm::Aes256Gcm>) -> Self {
        self.aes_key = Some(key);
        self
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        streamer_id: Uuid,
        path: &str,
    ) -> anyhow::Result<T> {
        let token = self.access_token(streamer_id).await?;
        let resp = self
            .http
            .get(format!("https://api.spotify.com/v1{path}"))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json::<T>().await?)
    }

    fn to_song(track: SpotifyTrack) -> Song {
        let artist = track
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        let thumbnail = track.album.and_then(|a| a.images.first().map(|i| i.url.clone()));

        Song {
            id: Uuid::nil(),
            source: MusicSource::Spotify,
            source_id: track.id,
            title: track.name,
            artist,
            duration_seconds: Some((track.duration_ms / 1000) as i32),
            thumbnail_url: thumbnail,
            // Spotify does not expose direct stream URLs; playback resolves via YouTube.
            stream_url: None,
            explicit: track.explicit,
            metadata: std::collections::HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub async fn search(
        &self,
        streamer_id: Uuid,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let q = urlencoding::encode(query);
        let resp: SpotifySearchResponse = self
            .get_json(streamer_id, &format!("/search?type=track&limit={limit}&q={q}"))
            .await?;

        let items = resp.tracks.map(|t| t.items).unwrap_or_default();

        Ok(items
            .into_iter()
            .filter(|t| t.is_playable.unwrap_or(true))
            .take(limit)
            .map(|t| SearchResult {
                song: Self::to_song(t),
                confidence: 0.85,
                matched_query: query.to_string(),
            })
            .collect())
    }

    pub async fn get_track(&self, streamer_id: Uuid, track_or_url: &str) -> anyhow::Result<Song> {
        let track_id =
            Self::extract_track_id(track_or_url).unwrap_or_else(|| track_or_url.to_string());
        let track: SpotifyTrack = self
            .get_json(streamer_id, &format!("/tracks/{}", urlencoding::encode(&track_id)))
            .await?;
        Ok(Self::to_song(track))
    }

    /// Resolves spotify:* URIs and open.spotify.com links to track ids.
    pub fn extract_track_id(input: &str) -> Option<String> {
        if let Some(rest) = input.strip_prefix("spotify:track:") {
            return Some(rest.split('?').next()?.to_string());
        }
        let parsed = Url::parse(input).ok()?;
        if parsed.host_str()?.contains("spotify.com") {
            let parts: Vec<&str> = parsed
                .path()
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() >= 2 && parts[0] == "track" {
                return Some(parts[1].split('?').next()?.to_string());
            }
        }
        None
    }
}
