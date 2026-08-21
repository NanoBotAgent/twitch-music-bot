use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use reqwest::{Client, ClientBuilder};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use url::Url;

use crate::config::Settings;
use twitch_music_shared::*;

const DISCOVERY_PAGE: &str = "https://soundcloud.com/";

/// SoundCloud public web API client.
///
/// SoundCloud closed official OAuth app registrations in 2016, so this client
/// does NOT use per-user tokens. Instead it uses the public `client_id` that
/// SoundCloud's own web player ships in its JS bundles:
///   - search:      GET /search/tracks?q=...&client_id=...
///   - track info:  GET /tracks/{id}?client_id=...   (includes media.transcodings)
///   - stream url:  GET {transcoding.url}&client_id=... -> { "url": <cdn mp3/hls> }
///
/// The client_id is discovered automatically at runtime and re-discovered when
/// SoundCloud rejects the cached one (they rotate it every few weeks). A
/// manually configured `soundcloud.client_id` always takes precedence.
#[derive(Debug)]
pub struct SoundCloudClient {
    http: Client,
    configured_client_id: Option<String>,
    discovered_client_id: Arc<RwLock<Option<String>>>,
    api_base_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ScTrack {
    id: u64,
    title: String,
    user: ScUser,
    duration: u64,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    streamable: bool,
    #[serde(default)]
    media: Option<ScMedia>,
}

#[derive(Debug, Deserialize, Default)]
struct ScUser {
    #[serde(default)]
    username: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ScMedia {
    #[serde(default)]
    transcodings: Vec<ScTranscoding>,
}

#[derive(Debug, Deserialize, Default)]
struct ScTranscoding {
    #[serde(default)]
    url: String,
    #[serde(default)]
    format: Option<ScFormat>,
    #[serde(default)]
    quality: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ScFormat {
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    mime_type: String,
}

#[derive(Debug, Deserialize)]
struct ScSearchResponse {
    #[serde(default)]
    collection: Vec<ScTrack>,
}

#[derive(Debug, Deserialize)]
struct ScAuthorizeResponse {
    #[serde(default)]
    url: Option<String>,
}

impl SoundCloudClient {
    pub fn new(settings: &Settings) -> Result<Self, anyhow::Error> {
        let http = ClientBuilder::new()
            .timeout(Duration::from_secs(15))
            .user_agent(format!("twitch-music-bot/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        let configured = settings.soundcloud_client_id();
        let configured = if configured.trim().is_empty() { None } else { Some(configured.to_string()) };

        Ok(Self {
            http,
            configured_client_id: configured,
            discovered_client_id: Arc::new(RwLock::new(None)),
            api_base_url: settings.soundcloud.api_base_url.clone(),
        })
    }

    /// Returns the active client_id: configured value first, then the cached
    /// discovered one, discovering it on first use.
    async fn ensure_client_id(&self) -> anyhow::Result<String> {
        if let Some(id) = &self.configured_client_id {
            return Ok(id.clone());
        }

        if let Some(id) = self.discovered_client_id.read().await.clone() {
            return Ok(id);
        }

        let id = self.discover_client_id().await?;
        *self.discovered_client_id.write().await = Some(id.clone());
        Ok(id)
    }

    /// Scrapes the SoundCloud homepage and its JS bundles for a client_id.
    async fn discover_client_id(&self) -> anyhow::Result<String> {
        debug!("Discovering SoundCloud client_id");

        let html = self
            .http
            .get(DISCOVERY_PAGE)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch soundcloud.com: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("soundcloud.com returned an error: {e}"))?
            .text()
            .await?;

        // Script bundles look like <script crossorigin src="https://a-v2.sndcdn.com/assets/xx-abc123.js">
        let script_re = Regex::new(r#"src="(https://a-v2\.sndcdn\.com/assets/[^"]+\.js)""#)?;
        let id_re = Regex::new(r#"client_id\s*[:=]\s*"([a-zA-Z0-9]{28,40})""#)?;

        let mut scripts: Vec<String> = script_re
            .captures_iter(&html)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        scripts.dedup();

        // The main app bundle is usually near the end; try the last 5 first.
        for script in scripts.iter().rev().take(5) {
            let body = match self.http.get(script).send().await {
                Ok(r) => match r.text().await {
                    Ok(t) => t,
                    Err(e) => {
                        debug!("failed to fetch bundle {script}: {e}");
                        continue;
                    }
                },
                Err(e) => {
                    debug!("failed to fetch bundle {script}: {e}");
                    continue;
                }
            };
            if let Some(cap) = id_re.captures(&body) {
                let id = cap[1].to_string();
                info!("Discovered SoundCloud client_id ({} chars)", id.len());
                return Ok(id);
            }
        }

        anyhow::bail!(
            "could not discover a SoundCloud client_id; set [soundcloud] client_id in config as a fallback"
        )
    }

    /// Runs an API call, re-discovering the client_id once on auth rejection.
    async fn call<T: serde::de::DeserializeOwned>(&self, url: &str) -> anyhow::Result<T> {
        for attempt in 0..2 {
            let client_id = self.ensure_client_id().await?;
            let sep = if url.contains('?') { '&' } else { '?' };
            let full = format!("{url}{sep}client_id={client_id}");

            let resp = self.http.get(&full).send().await?;

            match resp.status() {
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                    if attempt == 0 && self.configured_client_id.is_none() =>
                {
                    warn!("SoundCloud rejected cached client_id; re-discovering");
                    *self.discovered_client_id.write().await = None;
                    continue;
                }
                _ => {
                    let resp = resp.error_for_status()?;
                    return Ok(resp.json::<T>().await?);
                }
            }
        }
        anyhow::bail!("SoundCloud API rejected the client_id after retry")
    }

    fn to_song(track: ScTrack, confidence: f32, matched_query: &str) -> Song {
        let thumbnail = track
            .artwork_url
            .or_else(|| track.user.avatar_url.clone())
            .map(|u| u.replace("large.jpg", "t500x500.jpg"));

        Song {
            id: Uuid::nil(),
            source: MusicSource::SoundCloud,
            source_id: track.id.to_string(),
            title: track.title,
            artist: track.user.username,
            duration_seconds: Some((track.duration / 1000) as i32),
            thumbnail_url: thumbnail,
            stream_url: None,
            explicit: false,
            metadata: std::collections::HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        let url = format!(
            "{}/search/tracks?q={}&limit={}",
            self.api_base_url,
            urlencoding::encode(query),
            limit
        );

        let resp: ScSearchResponse = self.call(&url).await?;

        Ok(resp
            .collection
            .into_iter()
            .filter(|t| t.streamable || t.id > 0) // search results are playable unless region-locked
            .take(limit)
            .map(|t| SearchResult {
                song: Self::to_song(t, 0.8, query),
                confidence: 0.8,
                matched_query: query.to_string(),
            })
            .collect())
    }

    pub async fn get_track(&self, track_id: &str) -> anyhow::Result<Song> {
        let url = format!("{}/tracks/{}", self.api_base_url, track_id);
        let track: ScTrack = self.call(&url).await?;

        if !track.streamable {
            anyhow::bail!("track {track_id} is not streamable");
        }

        Ok(Self::to_song(track, 1.0, track_id))
    }

    /// Resolves a playable URL for a track. Prefers progressive MP3 (directly
    /// playable by <audio>), falls back to HLS (needs hls.js in the overlay).
    pub async fn get_stream_url(&self, track_id: &str) -> anyhow::Result<String> {
        let track: ScTrack = self.call(&format!("{}/tracks/{}", self.api_base_url, track_id)).await?;

        let media = track.media.ok_or_else(|| anyhow::anyhow!("track has no media transcodings"))?;

        // Progressive (mp3) first, then HLS by quality.
        let pick = |protocol: &str| -> Option<&ScTranscoding> {
            media
                .transcodings
                .iter()
                .filter(|t| t.format.as_ref().map(|f| f.protocol == protocol).unwrap_or(false))
                .max_by_key(|t| match t.quality.as_deref() {
                    Some("hq") => 3,
                    Some("high") => 2,
                    _ => 1,
                })
        };

        let transcoding = pick("progressive").or_else(|| pick("hls"));
        let transcoding = transcoding.ok_or_else(|| anyhow::anyhow!("no audio transcodings available"))?;

        let authorized: ScAuthorizeResponse = self.call(&transcoding.url).await?;
        authorized
            .url
            .ok_or_else(|| anyhow::anyhow!("SoundCloud did not return a stream URL"))
    }

    /// Resolves a permalink (https://soundcloud.com/artist/track) to a Song.
    pub async fn resolve_url(&self, url: &str) -> anyhow::Result<Song> {
        let parsed = Url::parse(url)?;
        if !parsed.host_str().map(|h| h.contains("soundcloud.com")).unwrap_or(false) {
            anyhow::bail!("not a SoundCloud URL");
        }

        let resolved: ScTrack = self
            .call(&format!("{}/resolve?url={}", self.api_base_url, urlencoding::encode(url)))
            .await?;

        Ok(Self::to_song(resolved, 1.0, url))
    }
}

/// Extracts the numeric track id from a soundcloud.com URL when present.
#[allow(dead_code)]
pub fn extract_track_id(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.host_str()?.contains("soundcloud.com") {
        let parts: Vec<&str> = parsed
            .path()
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        // /track/<id> style API links
        if parts.len() >= 2 && parts[0] == "track" {
            return Some(parts[1].split('?').next()?.to_string());
        }
    }
    None
}
