pub mod soundcloud;
pub mod spotify;
pub mod youtube;

use std::sync::Arc;

use sqlx::PgPool;
use tracing::{debug, info, warn};

use crate::config::Settings;
use crate::metrics;
use soundcloud::SoundCloudClient;
use spotify::SpotifyClient;
use twitch_music_shared::*;
use youtube::YouTubeClient;

/// Coordinates all music sources. Spotify requests are per-streamer (OAuth),
/// YouTube and SoundCloud use global public clients.
pub struct MusicManager {
    pub youtube: Arc<YouTubeClient>,
    pub spotify: Arc<SpotifyClient>,
    pub soundcloud: Arc<SoundCloudClient>,
    pool: PgPool,
}

impl MusicManager {
    pub fn new(pool: PgPool, settings: Arc<Settings>, aes_key: aes_gcm::Key<aes_gcm::Aes256Gcm>) -> anyhow::Result<Self> {
        let youtube = Arc::new(YouTubeClient::new(&settings)?);
        let spotify = Arc::new(SpotifyClient::new(pool.clone(), &settings)?.with_aes_key(aes_key));
        let soundcloud = Arc::new(SoundCloudClient::new(&settings)?);

        info!("MusicManager initialized (spotify configured: {})", !settings.spotify_client_id().is_empty());

        Ok(Self { youtube, spotify, soundcloud, pool })
    }

    fn parse_source(s: &str) -> Option<MusicSource> {
        match s {
            "youtube" => Some(MusicSource::YouTube),
            "spotify" => Some(MusicSource::Spotify),
            "soundcloud" => Some(MusicSource::SoundCloud),
            _ => None,
        }
    }

    /// Searches all enabled sources in parallel and ranks results.
    /// `streamer_id` is required for Spotify (per-streamer tokens); other
    /// sources ignore it.
    pub async fn search(
        &self,
        streamer_id: Uuid,
        query: &str,
        limit: usize,
        allowed_sources: &[String],
    ) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = Vec::new();

        for source in allowed_sources {
            let Some(source) = Self::parse_source(source) else {
                debug!("skipping unknown source: {source}");
                continue;
            };

            let started = std::time::Instant::now();
            let outcome = match source {
                MusicSource::YouTube => self.youtube.search(query, limit).await,
                MusicSource::SoundCloud => self.soundcloud.search(query, limit).await,
                MusicSource::Spotify => self.spotify.search(streamer_id, query, limit).await,
                MusicSource::Local => Ok(Vec::new()),
            };

            match outcome {
                Ok(mut found) => {
                    metrics::record_music_search(source.as_str(), found.len(), started.elapsed());
                    results.append(&mut found);
                }
                Err(e) => {
                    metrics::record_music_error(source.as_str());
                    warn!("search on {source} failed: {e:#}");
                }
            }
        }

        // Best matches first: confidence desc, then title similarity to query.
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let sa = jaro_winkler(&query.to_lowercase(), &a.song.title.to_lowercase());
                    let sb = jaro_winkler(&query.to_lowercase(), &b.song.title.to_lowercase());
                    sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        results.truncate(limit);
        results
    }

    /// Fetches full track metadata for a source-specific id or URL.
    pub async fn resolve(
        &self,
        streamer_id: Uuid,
        source: MusicSource,
        id_or_url: &str,
    ) -> anyhow::Result<Song> {
        match source {
            MusicSource::YouTube => {
                metrics::record_stream_url_fetch("youtube");
                // Accept raw video ids as well as full URLs.
                match YouTubeClient::extract_video_id(id_or_url) {
                    Some(video_id) => self.youtube.get_video_info(&video_id).await,
                    None => self.youtube.get_video_info(id_or_url).await,
                }
            }
            MusicSource::SoundCloud => {
                metrics::record_stream_url_fetch("soundcloud");
                self.soundcloud.resolve_url(id_or_url).await
            }
            MusicSource::Spotify => {
                metrics::record_stream_url_fetch("spotify");
                self.spotify.get_track(streamer_id, id_or_url).await
            }
            MusicSource::Local => anyhow::bail!("local source resolution is not supported"),
        }
    }

    /// Returns a playable stream URL for a song, resolving on demand and
    /// caching the result in the songs table.
    ///
    /// Spotify tracks have no direct stream URLs, so they are resolved by
    /// searching YouTube for "<artist> <title> <topic>".
    pub async fn get_stream_url(&self, streamer_id: Uuid, song: &mut Song) -> anyhow::Result<String> {
        if let Some(url) = &song.stream_url {
            return Ok(url.clone());
        }

        let started = std::time::Instant::now();
        let result = self.resolve_stream_url(streamer_id, song).await;
        metrics::record_stream_url_fetch(song.source.as_str(), result.is_ok(), started.elapsed());
        let resolved = result?;

        song.stream_url = Some(resolved.clone());
        if song.id != Uuid::nil() {
            if let Err(e) = crate::database::songs::update_stream_url(&self.pool, song.id, &resolved).await {
                warn!("failed to cache stream url: {e:#}");
            }
        }

        Ok(resolved)
    }

    async fn resolve_stream_url(&self, streamer_id: Uuid, song: &Song) -> anyhow::Result<String> {
        match song.source {
            MusicSource::YouTube => self.youtube.get_stream_url(&song.source_id).await,
            MusicSource::SoundCloud => self.soundcloud.get_stream_url(&song.source_id).await,
            MusicSource::Spotify => {
                // Spotify does not expose stream URLs; resolve via YouTube.
                let query = format!("{} {}", song.artist, song.title);
                let results = self.youtube.search(&query, 1).await?;
                let best = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no YouTube match found for Spotify track"))?;
                self.youtube.get_stream_url(&best.song.source_id).await
            }
            MusicSource::Local => anyhow::bail!("local playback is not supported"),
        }
    }

    /// Persists a search hit into the catalog and returns its DB id.
    pub async fn persist_song(&self, song: &Song) -> anyhow::Result<Uuid> {
        crate::database::songs::get_or_create(&self.pool, song).await
    }
}

/// Jaro-Winkler string similarity (0..1) used for ranking search hits.
fn jaro_winkler(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let match_distance = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_matches = vec![false; a.len()];
    let mut b_matches = vec![false; b.len()];
    let mut matches = 0usize;

    for (i, ca) in a.iter().enumerate() {
        let start = i.saturating_sub(match_distance);
        let end = (i + match_distance + 1).min(b.len());
        for j in start..end {
            if !b_matches[j] && *ca == b[j] {
                a_matches[i] = true;
                b_matches[j] = true;
                matches += 1;
                break;
            }
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut transpositions = 0usize;
    let mut k = 0usize;
    for (i, matched) in a_matches.iter().enumerate() {
        if *matched {
            while !b_matches[k] {
                k += 1;
            }
            if a[i] != b[k] {
                transpositions += 1;
            }
            k += 1;
        }
    }

    let m = matches as f32;
    let jaro = (m / a.len() as f32 + m / b.len() as f32 + (m - transpositions as f32 / 2.0) / m) / 3.0;

    // Winkler bonus for common prefix (up to 4 chars).
    let prefix = a
        .iter()
        .zip(b.iter())
        .take(4)
        .take_while(|(x, y)| x == y)
        .count();
    jaro + 0.1 * prefix as f32 * (1.0 - jaro)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jaro_winkler_identical() {
        assert!((jaro_winkler("hello", "hello") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn jaro_winkler_similar() {
        let s = jaro_winkler("never gonna give you up", "never gonna give you up!");
        assert!(s > 0.95);
    }

    #[test]
    fn jaro_winkler_different() {
        assert!(jaro_winkler("abc", "xyz") < 0.1);
    }

    #[test]
    fn jaro_winkler_empty() {
        assert_eq!(jaro_winkler("", "abc"), 0.0);
    }
}
