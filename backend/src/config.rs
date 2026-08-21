use config::{Config, Environment, File};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::Path;


use secrecy::ExposeSecret;
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub twitch: TwitchConfig,
    pub spotify: SpotifyConfig,
    pub youtube: YouTubeConfig,
    pub soundcloud: SoundCloudConfig,
    pub overlay: OverlayConfig,
    pub security: SecurityConfig,
    pub logging: LoggingConfig,
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub request_timeout_seconds: u64,
    pub body_limit_mb: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: SecretString,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub run_migrations: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: SecretString,
    pub max_connections: u32,
    pub connection_timeout_seconds: u64,
    pub command_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwitchConfig {
    pub client_id: SecretString,
    pub client_secret: SecretString,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub bot_username: String,
    pub bot_oauth_token: SecretString,
    pub irc_reconnect_interval_seconds: u64,
    pub irc_ping_interval_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpotifyConfig {
    pub client_id: SecretString,
    pub client_secret: SecretString,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub api_base_url: String,
    pub rate_limit_per_second: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YouTubeConfig {
    pub invidious_instances: Vec<String>,
    pub piped_instances: Vec<String>,
    pub fallback_to_ytdlp: bool,
    pub ytdlp_path: Option<String>,
    pub request_timeout_seconds: u64,
    pub max_retries: u32,
    pub preferred_quality: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SoundCloudConfig {
    pub client_id: SecretString,
    pub api_base_url: String,
    pub rate_limit_per_second: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverlayConfig {
    pub ws_path: String,
    pub ping_interval_seconds: u64,
    pub max_message_size: usize,
    pub reconnect_interval_seconds: u64,
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub jwt_secret: SecretString,
    pub jwt_expiry_hours: i64,
    #[serde(default = "default_refresh_token_days")]
    pub refresh_token_days: i64,
    pub encryption_key: SecretString,
    pub cors_allowed_origins: Vec<String>,
    pub rate_limit_requests_per_minute: u32,
    pub rate_limit_burst: u32,
}

fn default_refresh_token_days() -> i64 {
    30
}

/// Values that must be overridden before the service accepts production traffic.
const PLACEHOLDER_SECRETS: &[&str] = &[
    "change-me-in-production",
    "32-byte-encryption-key-change-me!!",
    "test",
    "secret",
];

impl SecurityConfig {
    /// Returns Err when a secret is still a known insecure placeholder.
    pub fn validate(&self) -> Result<(), String> {
        let jwt = self.jwt_secret.expose_secret();
        let enc = self.encryption_key.expose_secret();

        if jwt.len() < 32 {
            return Err("security.jwt_secret must be at least 32 characters".to_string());
        }
        if enc.len() < 32 {
            return Err("security.encryption_key must be at least 32 bytes".to_string());
        }
        for placeholder in PLACEHOLDER_SECRETS {
            if jwt.contains(placeholder) {
                return Err("security.jwt_secret is still set to an insecure placeholder".to_string());
            }
            if enc.contains(placeholder) {
                return Err("security.encryption_key is still set to an insecure placeholder".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub json_output: bool,
    pub file_path: Option<String>,
    pub max_file_size_mb: u64,
    pub max_files: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub path: String,
    pub port: Option<u16>,
}

impl Settings {
    pub fn new() -> Result<Self, config::ConfigError> {
        let base_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config".to_string());
        let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

        let mut builder = Config::builder()
            .add_source(File::from(Path::new(&base_path).join("default")).required(false))
            .add_source(File::from(Path::new(&base_path).join(&environment)).required(false))
            .add_source(File::from(Path::new(&base_path).join("local")).required(false))
            .add_source(Environment::with_prefix("APP").separator("__"));

        if let Ok(secrets_path) = std::env::var("SECRETS_FILE") {
            builder = builder.add_source(File::from(Path::new(&secrets_path)).required(false));
        }

        builder.build()?.try_deserialize()
    }

    pub fn database_url(&self) -> &str {
        self.database.url.expose_secret()
    }

    pub fn redis_url(&self) -> &str {
        self.redis.url.expose_secret()
    }

    pub fn twitch_client_id(&self) -> &str {
        self.twitch.client_id.expose_secret()
    }

    pub fn twitch_client_secret(&self) -> &str {
        self.twitch.client_secret.expose_secret()
    }

    pub fn twitch_bot_token(&self) -> &str {
        self.twitch.bot_oauth_token.expose_secret()
    }

    pub fn spotify_client_id(&self) -> &str {
        self.spotify.client_id.expose_secret()
    }

    pub fn spotify_client_secret(&self) -> &str {
        self.spotify.client_secret.expose_secret()
    }

    pub fn soundcloud_client_id(&self) -> &str {
        self.soundcloud.client_id.expose_secret()
    }

    pub fn jwt_secret(&self) -> &str {
        self.security.jwt_secret.expose_secret()
    }

    pub fn encryption_key(&self) -> &str {
        self.security.encryption_key.expose_secret()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                workers: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
                request_timeout_seconds: 30,
                body_limit_mb: 10,
            },
            database: DatabaseConfig {
                url: SecretString::new("postgresql://localhost/twitch_music_bot".into()),
                max_connections: 10,
                min_connections: 2,
                connect_timeout_seconds: 10,
                idle_timeout_seconds: 300,
                run_migrations: true,
            },
            redis: RedisConfig {
                url: SecretString::new("redis://localhost:6379".into()),
                max_connections: 20,
                connection_timeout_seconds: 5,
                command_timeout_seconds: 10,
            },
            twitch: TwitchConfig {
                client_id: SecretString::new("".into()),
                client_secret: SecretString::new("".into()),
                redirect_uri: "http://localhost:3000/auth/twitch/callback".to_string(),
                scopes: vec!["chat:read".to_string(), "chat:edit".to_string(), "channel:manage:broadcast".to_string(), "user:read:email".to_string()],
                bot_username: "musicbot".to_string(),
                bot_oauth_token: SecretString::new("".into()),
                irc_reconnect_interval_seconds: 10,
                irc_ping_interval_seconds: 60,
            },
            spotify: SpotifyConfig {
                client_id: SecretString::new("".into()),
                client_secret: SecretString::new("".into()),
                redirect_uri: "http://localhost:3000/auth/spotify/callback".to_string(),
                scopes: vec!["user-read-email".to_string(), "user-read-private".to_string(), "playlist-read-private".to_string(), "playlist-read-collaborative".to_string()],
                api_base_url: "https://api.spotify.com/v1".to_string(),
                rate_limit_per_second: 10,
            },
            youtube: YouTubeConfig {
                invidious_instances: vec![
                    "https://yewtu.be".to_string(),
                    "https://inv.nadeko.net".to_string(),
                    "https://invidious.snopyta.org".to_string(),
                ],
                piped_instances: vec![
                    "https://pipedapi.kavin.rocks".to_string(),
                    "https://piped-api.garudalinux.org".to_string(),
                ],
                fallback_to_ytdlp: true,
                ytdlp_path: None,
                request_timeout_seconds: 15,
                max_retries: 3,
                preferred_quality: "audio_only".to_string(),
            },
            soundcloud: SoundCloudConfig {
                client_id: SecretString::new("".into()),
                api_base_url: "https://api-v2.soundcloud.com".to_string(),
                rate_limit_per_second: 5,
            },
            overlay: OverlayConfig {
                ws_path: "/ws/overlay".to_string(),
                ping_interval_seconds: 30,
                max_message_size: 65536,
                reconnect_interval_seconds: 5,
                allowed_origins: vec!["*".to_string()],
            },
            security: SecurityConfig {
                jwt_secret: SecretString::new("change-me-in-production".into()),
                jwt_expiry_hours: 24,
                refresh_token_days: default_refresh_token_days(),
                encryption_key: SecretString::new("32-byte-encryption-key-change-me!!".into()),
                cors_allowed_origins: vec!["http://localhost:3000".to_string(), "https://*.vercel.app".to_string()],
                rate_limit_requests_per_minute: 60,
                rate_limit_burst: 10,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                json_output: true,
                file_path: None,
                max_file_size_mb: 100,
                max_files: 10,
            },
            metrics: MetricsConfig {
                enabled: true,
                path: "/metrics".to_string(),
                port: Some(9090),
            },
        }
    }
}