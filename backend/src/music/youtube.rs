#![allow(dead_code)]

use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, warn};
use url::Url;

use uuid::Uuid;

use crate::config::Settings;
use twitch_music_shared::*;

#[derive(Debug)]
pub struct YouTubeClient {
    client: Client,
    api_key: String,
    invidious_instances: Vec<String>,
    piped_instances: Vec<String>,
    current_invidious: std::sync::atomic::AtomicUsize,
    current_piped: std::sync::atomic::AtomicUsize,
    request_timeout: Duration,
    max_retries: u32,
    fallback_to_ytdlp: bool,
    ytdlp_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InvidiousVideo {
    video_id: String,
    title: String,
    author: String,
    author_id: String,
    length_seconds: u32,
    view_count: u64,
    video_thumbnails: Vec<InvidiousThumbnail>,
    is_live: bool,
    paid: bool,
    premiere: bool,
    allowed_regions: Option<Vec<String>>,
    genre: Option<String>,
    genre_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InvidiousThumbnail {
    quality: String,
    url: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct InvidiousSearchResponse {
    items: Vec<InvidiousVideo>,
}

#[derive(Debug, Deserialize)]
struct InvidiousStreamResponse {
    adaptive_formats: Vec<InvidiousFormat>,
    format_streams: Vec<InvidiousFormat>,
}

#[derive(Debug, Deserialize)]
struct InvidiousFormat {
    url: String,
    itag: u32,
    mime_type: String,
    bitrate: Option<u32>,
    quality: Option<String>,
    content_length: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PipedVideo {
    video_id: String,
    title: String,
    channel_name: String,
    channel_id: String,
    duration: u32,
    thumbnails: Vec<PipedThumbnail>,
    is_live: bool,
}

#[derive(Debug, Deserialize)]
struct PipedThumbnail {
    url: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
struct PipedSearchResponse {
    items: Vec<PipedVideo>,
}

#[derive(Debug, Deserialize)]
struct PipedStreamResponse {
    audio_streams: Vec<PipedAudioStream>,
}

#[derive(Debug, Deserialize)]
struct PipedAudioStream {
    url: String,
    bitrate: u32,
    mime_type: String,
    quality: String,
}

impl YouTubeClient {
    pub fn new(settings: &Settings) -> Result<Self, anyhow::Error> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(settings.youtube.request_timeout_seconds))
            .user_agent("twitch-music-bot/0.1")
            .build()?;

        Ok(Self {
            client,
            api_key: settings.youtube.api_key.expose_secret().to_string(),
            invidious_instances: settings.youtube.invidious_instances.clone(),
            piped_instances: settings.youtube.piped_instances.clone(),
            current_invidious: std::sync::atomic::AtomicUsize::new(0),
            current_piped: std::sync::atomic::AtomicUsize::new(0),
            request_timeout: Duration::from_secs(settings.youtube.request_timeout_seconds),
            max_retries: settings.youtube.max_retries,
            fallback_to_ytdlp: settings.youtube.fallback_to_ytdlp,
            ytdlp_path: settings.youtube.ytdlp_path.clone(),
        })
    }

    fn get_invidious_url(&self) -> anyhow::Result<&str> {
        self.invidious_instances
            .get(self.current_invidious.load(std::sync::atomic::Ordering::Relaxed) % self.invidious_instances.len().max(1))
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("no Invidious instances configured"))
    }

    fn get_piped_url(&self) -> anyhow::Result<&str> {
        self.piped_instances
            .get(self.current_piped.load(std::sync::atomic::Ordering::Relaxed) % self.piped_instances.len().max(1))
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("no Piped instances configured"))
    }

    fn rotate_invidious(&self) {
        if self.invidious_instances.is_empty() {
            return;
        }
        let next = (self.current_invidious.load(std::sync::atomic::Ordering::Relaxed) + 1) % self.invidious_instances.len();
        self.current_invidious.store(next, std::sync::atomic::Ordering::Relaxed);
        debug!("Rotated to Invidious instance: {}", self.invidious_instances[next]);
    }

    fn rotate_piped(&self) {
        if self.piped_instances.is_empty() {
            return;
        }
        let next = (self.current_piped.load(std::sync::atomic::Ordering::Relaxed) + 1) % self.piped_instances.len();
        self.current_piped.store(next, std::sync::atomic::Ordering::Relaxed);
        debug!("Rotated to Piped instance: {}", self.piped_instances[next]);
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, anyhow::Error> {
        let mut results = Vec::new();

        if !self.api_key.is_empty() {
            match self.search_data_api(query, limit).await {
                Ok(api_results) if !api_results.is_empty() => return Ok(api_results),
                Ok(_) => warn!("YouTube Data API search returned no results for '{}'", query),
                Err(e) => warn!("YouTube Data API search failed, falling back to mirrors: {}", e),
            }
        }

        if let Ok(invidious_results) = self.search_invidious(query, limit).await {
            results.extend(invidious_results);
        }

        if results.len() < limit {
            if let Ok(piped_results) = self.search_piped(query, limit - results.len()).await {
                results.extend(piped_results);
            }
        }

        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        Ok(results)
    }

    async fn search_data_api(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, anyhow::Error> {
        let limit_str = limit.clamp(1, 50).to_string();
        let url = "https://www.googleapis.com/youtube/v3/search";
        let response: YtDataSearchResponse = self
            .client
            .get(url)
            .query(&[
                ("part", "snippet"),
                ("type", "video"),
                ("q", query),
                ("maxResults", limit_str.as_str()),
                ("key", self.api_key.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut results = Vec::new();
        for item in response.items {
            let Some(video_id) = item.id.video_id else { continue };
            if item.snippet.live_broadcast_content.as_deref() == Some("live") {
                continue;
            }
            results.push(SearchResult {
                song: Song {
                    id: Uuid::nil(),
                    source: MusicSource::YouTube,
                    source_id: video_id,
                    title: item.snippet.title,
                    artist: item.snippet.channel_title,
                    duration_seconds: None,
                    thumbnail_url: pick_thumbnail(&item.snippet.thumbnails),
                    stream_url: None,
                    explicit: false,
                    metadata: HashMap::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                },
                confidence: 0.95,
                matched_query: query.to_string(),
            });
            if results.len() >= limit {
                break;
            }
        }
        Ok(results)
    }

    async fn get_video_info_data_api(&self, video_id: &str) -> Result<Song, anyhow::Error> {
        let url = "https://www.googleapis.com/youtube/v3/videos";
        let response: YtDataVideosResponse = self
            .client
            .get(url)
            .query(&[
                ("part", "snippet,contentDetails"),
                ("id", video_id),
                ("key", self.api_key.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let video = response
            .items
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("video {} not found on YouTube Data API", video_id))?;

        Ok(Song {
            id: Uuid::nil(),
            source: MusicSource::YouTube,
            source_id: video.id,
            title: video.snippet.title,
            artist: video.snippet.channel_title,
            duration_seconds: parse_iso8601_duration(&video.content_details.duration).map(|d| d as i32),
            thumbnail_url: pick_thumbnail(&video.snippet.thumbnails),
            stream_url: None,
            explicit: false,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn search_invidious(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, anyhow::Error> {
        let url = format!("{}/api/v1/search?q={}&type=video", self.get_invidious_url()?, urlencoding::encode(query));
        let response: InvidiousSearchResponse = self.client.get(&url).send().await?.json().await?;

        let mut results = Vec::new();
        for video in response.items.into_iter().take(limit) {
            if video.is_live || video.paid || video.premiere {
                continue;
            }

            let thumbnail = video.video_thumbnails.iter()
                .max_by_key(|t| t.width * t.height)
                .map(|t| t.url.clone());

            let song = Song {
                id: Uuid::nil(),
                source: MusicSource::YouTube,
                source_id: video.video_id.clone(),
                title: video.title,
                artist: video.author,
                duration_seconds: Some(video.length_seconds as i32),
                thumbnail_url: thumbnail,
                stream_url: None,
                explicit: false,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            results.push(SearchResult {
                song,
                confidence: 0.9,
                matched_query: query.to_string(),
            });
        }

        Ok(results)
    }

    async fn search_piped(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, anyhow::Error> {
        let url = format!("{}/search?q={}&filter=video", self.get_piped_url()?, urlencoding::encode(query));
        let response: PipedSearchResponse = self.client.get(&url).send().await?.json().await?;

        let mut results = Vec::new();
        for video in response.items.into_iter().take(limit) {
            if video.is_live {
                continue;
            }

            let thumbnail = video.thumbnails.iter()
                .max_by_key(|t| t.width * t.height)
                .map(|t| t.url.clone());

            let song = Song {
                id: Uuid::nil(),
                source: MusicSource::YouTube,
                source_id: video.video_id.clone(),
                title: video.title,
                artist: video.channel_name,
                duration_seconds: Some(video.duration as i32),
                thumbnail_url: thumbnail,
                stream_url: None,
                explicit: false,
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            results.push(SearchResult {
                song,
                confidence: 0.85,
                matched_query: query.to_string(),
            });
        }

        Ok(results)
    }

    pub async fn get_stream_url(&self, video_id: &str) -> Result<String, anyhow::Error> {
        if let Ok(url) = self.get_stream_url_invidious(video_id).await {
            return Ok(url);
        }

        if let Ok(url) = self.get_stream_url_piped(video_id).await {
            return Ok(url);
        }

        if self.fallback_to_ytdlp {
            if let Ok(url) = self.get_stream_url_ytdlp(video_id).await {
                return Ok(url);
            }
        }

        Err(anyhow::anyhow!("All YouTube sources failed for video: {}", video_id))
    }

    async fn get_stream_url_invidious(&self, video_id: &str) -> Result<String, anyhow::Error> {
        let mut attempts = 0;
        while attempts < self.invidious_instances.len() {
            let url = format!("{}/api/v1/videos/{}?fields=adaptive_formats", self.get_invidious_url()?, video_id);
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let data: InvidiousStreamResponse = resp.json().await?;
                    let audio_format = data.adaptive_formats.iter()
                        .chain(data.format_streams.iter())
                        .filter(|f| f.mime_type.starts_with("audio/"))
                        .max_by_key(|f| f.bitrate.unwrap_or(0));

                    if let Some(format) = audio_format {
                        return Ok(format.url.clone());
                    }
                }
                Ok(resp) => {
                    warn!("Invidious returned status {} for {}", resp.status(), video_id);
                }
                Err(e) => {
                    warn!("Invidious request failed: {}", e);
                }
            }
            self.rotate_invidious();
            attempts += 1;
        }
        Err(anyhow::anyhow!("All Invidious instances failed"))
    }

    async fn get_stream_url_piped(&self, video_id: &str) -> Result<String, anyhow::Error> {
        let mut attempts = 0;
        while attempts < self.piped_instances.len() {
            let url = format!("{}/streams/{}", self.get_piped_url()?, video_id);
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let data: PipedStreamResponse = resp.json().await?;
                    let audio_stream = data.audio_streams.iter()
                        .max_by_key(|s| s.bitrate);

                    if let Some(stream) = audio_stream {
                        return Ok(stream.url.clone());
                    }
                }
                Ok(resp) => {
                    warn!("Piped returned status {} for {}", resp.status(), video_id);
                }
                Err(e) => {
                    warn!("Piped request failed: {}", e);
                }
            }
            self.rotate_piped();
            attempts += 1;
        }
        Err(anyhow::anyhow!("All Piped instances failed"))
    }

    async fn get_stream_url_ytdlp(&self, video_id: &str) -> Result<String, anyhow::Error> {
        let ytdlp = self.ytdlp_path.as_deref().unwrap_or("yt-dlp");
        let url = format!("https://www.youtube.com/watch?v={}", video_id);

        let output = tokio::process::Command::new(ytdlp)
            .args(["-f", "bestaudio", "-g", &url])
            .output()
            .await?;

        if output.status.success() {
            let stream_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stream_url.is_empty() {
                return Ok(stream_url);
            }
        }

        Err(anyhow::anyhow!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr)))
    }

    pub async fn get_video_info(&self, video_id: &str) -> Result<Song, anyhow::Error> {
        if !self.api_key.is_empty() {
            match self.get_video_info_data_api(video_id).await {
                Ok(song) => return Ok(song),
                Err(e) => warn!("YouTube Data API video info failed for {}, falling back to mirrors: {}", video_id, e),
            }
        }

        let url = format!("{}/api/v1/videos/{}", self.get_invidious_url()?, video_id);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get video info"));
        }

        let video: InvidiousVideo = response.json().await?;
        let thumbnail = video.video_thumbnails.iter()
            .max_by_key(|t| t.width * t.height)
            .map(|t| t.url.clone());

        Ok(Song {
            id: Uuid::nil(),
            source: MusicSource::YouTube,
            source_id: video.video_id,
            title: video.title,
            artist: video.author,
            duration_seconds: Some(video.length_seconds as i32),
            thumbnail_url: thumbnail,
            stream_url: None,
            explicit: false,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    pub fn extract_video_id(url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        let host = parsed.host_str()?;

        if host.contains("youtube.com") || host.contains("youtu.be") {
            if host.contains("youtu.be") {
                return parsed.path_segments()?.next().map(|s| s.to_string());
            }

            if let Some(v) = parsed.query_pairs().find(|(k, _)| k == "v") {
                return Some(v.1.to_string());
            }

            if let Some(path) = parsed.path().strip_prefix("/shorts/") {
                return Some(path.split('/').next()?.to_string());
            }

            if let Some(path) = parsed.path().strip_prefix("/embed/") {
                return Some(path.split('/').next()?.to_string());
            }
        }

        None
    }
}

fn pick_thumbnail(thumbs: &YtDataThumbnails) -> Option<String> {
    thumbs
        .high
        .as_ref()
        .or(thumbs.medium.as_ref())
        .or(thumbs.default_.as_ref())
        .map(|t| t.url.clone())
}

fn parse_iso8601_duration(s: &str) -> Option<i64> {
    let s = s.strip_prefix("PT")?;
    let s = s.split('.').next()?;
    let mut total = 0i64;
    let mut num = String::new();
    for c in s.chars() {
        match c {
            '0'..='9' => num.push(c),
            'H' => {
                total += num.parse::<i64>().ok()? * 3600;
                num.clear();
            }
            'M' => {
                total += num.parse::<i64>().ok()? * 60;
                num.clear();
            }
            'S' => {
                total += num.parse::<i64>().ok()?;
                num.clear();
            }
            _ => return None,
        }
    }
    Some(total)
}

#[derive(Debug, Deserialize)]
struct YtDataSearchResponse {
    items: Vec<YtDataSearchItem>,
}

#[derive(Debug, Deserialize)]
struct YtDataSearchItem {
    id: YtDataVideoId,
    snippet: YtDataSnippet,
}

#[derive(Debug, Deserialize)]
struct YtDataVideoId {
    video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtDataVideosResponse {
    items: Vec<YtDataVideo>,
}

#[derive(Debug, Deserialize)]
struct YtDataVideo {
    id: String,
    snippet: YtDataSnippet,
    content_details: YtDataContentDetails,
}

#[derive(Debug, Deserialize)]
struct YtDataContentDetails {
    duration: String,
}

#[derive(Debug, Deserialize)]
struct YtDataSnippet {
    title: String,
    channel_title: String,
    live_broadcast_content: Option<String>,
    thumbnails: YtDataThumbnails,
}

#[derive(Debug, Deserialize)]
struct YtDataThumbnails {
    #[serde(rename = "default")]
    default_: Option<YtDataThumb>,
    medium: Option<YtDataThumb>,
    high: Option<YtDataThumb>,
}

#[derive(Debug, Deserialize)]
struct YtDataThumb {
    url: String,
}
