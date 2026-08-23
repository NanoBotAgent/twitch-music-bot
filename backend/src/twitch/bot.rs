use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{PrivmsgMessage, ServerMessage};
use twitch_irc::transport::tcp::{TCPTransport, TLS};
use twitch_irc::TwitchIRCClient;
use uuid::Uuid;

use crate::auth::frontend_url;
use crate::config::Settings;
use crate::database;
use crate::metrics;
use crate::queue::QueueManager;
use twitch_music_shared::*;

pub type IrcClient = TwitchIRCClient<TCPTransport<TLS>, StaticLoginCredentials>;

/// How long a generated download link stays valid.
const DOWNLOAD_LINK_TTL: StdDuration = StdDuration::from_secs(15 * 60);

/// Per-channel chat bot that listens for music commands. Uses an anonymous
/// read-only connection ("justinfan" users) for listening; replies are sent
/// through the Helix Send Chat Message endpoint using the streamer's stored
/// OAuth token.
///
/// Joins the channel and spawns the message-processing loop.
#[allow(clippy::too_many_arguments)]
pub fn spawn_for_channel(
    pool: sqlx::PgPool,
    settings: Arc<Settings>,
    http: reqwest::Client,
    aes_key: aes_gcm::Key<aes_gcm::Aes256Gcm>,
    streamer_id: Uuid,
    twitch_login: String,
    queue_manager: Arc<QueueManager>,
    notification_tx: mpsc::Sender<crate::queue::QueueNotification>,
) -> anyhow::Result<(IrcClient, tokio::task::JoinHandle<()>)> {
    let config = twitch_irc::ClientConfig {
        login_credentials: StaticLoginCredentials::anonymous(),
        ..twitch_irc::ClientConfig::default()
    };

    let (mut incoming_msgs, client) =
        TwitchIRCClient::<TCPTransport<TLS>, StaticLoginCredentials>::new(config);

    client.join(twitch_login.to_lowercase())?;

    info!("Bot joining #{twitch_login} for streamer {streamer_id}");

    let ctx = ChatContext {
        pool,
        settings,
        http,
        aes_key,
        streamer_id,
        channel_login: twitch_login.to_lowercase(),
        queue_manager,
    };

    let handle = tokio::spawn(async move {
        while let Some(message) = incoming_msgs.recv().await {
            match message {
                ServerMessage::Privmsg(pm) => {
                    if let Err(e) = handle_privmsg(&ctx, &pm).await {
                        debug!("privmsg handling failed: {e:#}");
                    }
                }
                ServerMessage::Join(join) => {
                    info!("Bot joined #{}", join.channel_login);
                }
                _ => {}
            }
        }

        warn!("Twitch message stream closed for #{}", ctx.channel_login);
        let _ = notification_tx;
    });

    Ok((client, handle))
}

struct ChatContext {
    pool: sqlx::PgPool,
    settings: Arc<Settings>,
    http: reqwest::Client,
    aes_key: aes_gcm::Key<aes_gcm::Aes256Gcm>,
    streamer_id: Uuid,
    channel_login: String,
    queue_manager: Arc<QueueManager>,
}

async fn handle_privmsg(ctx: &ChatContext, pm: &PrivmsgMessage) -> anyhow::Result<()> {
    let user = TwitchUser {
        id: pm.sender.id.clone(),
        twitch_user_id: pm.sender.id.clone(),
        login: pm.sender.login.clone(),
        display_name: pm.sender.name.clone(),
        is_mod: pm.badges.iter().any(|b| b.name == "moderator"),
        is_sub: pm.badges.iter().any(|b| b.name == "subscriber"),
        is_vip: pm.badges.iter().any(|b| b.name == "vip"),
    };

    let Some(command) = parse_command(&pm.channel_login, &user, &pm.message_text) else {
        return Ok(());
    };

    match command.command.as_str() {
        "sr" | "songrequest" | "playsong" => {
            metrics::record_twitch_message("sr", true);
            let query = command.args.join(" ");
            if query.is_empty() {
                return Ok(());
            }

            match ctx
                .queue_manager
                .add_request(ctx.streamer_id, &user, &query, None)
                .await
            {
                Ok(song) => {
                    info!(
                        "#{channel}: {name} queued '{title}' by {artist}",
                        channel = ctx.channel_login,
                        name = user.display_name,
                        title = song.song.title,
                        artist = song.song.artist
                    );
                    metrics::record_queue_operation("chat_add", true);
                }
                Err(BotError::UserBlocked) => {
                    debug!("blocked user {} attempted request", user.login)
                }
                Err(BotError::RateLimited) => debug!("{} hit rate limit", user.login),
                Err(BotError::QueueFull) => debug!("queue full for #{}", ctx.channel_login),
                Err(e) => {
                    debug!("request failed for {}: {e}", user.login);
                    metrics::record_queue_operation("chat_add", false);
                }
            }
        }
        "skip" | "voteskip" => {
            metrics::record_twitch_message("voteskip", true);
            if let Err(e) = ctx.queue_manager.vote_skip(ctx.streamer_id, &user).await {
                warn!("vote skip failed in #{}: {e:#}", ctx.channel_login);
            }
        }
        "queue" | "songs" => {
            metrics::record_twitch_message("queue", true);
            match ctx.queue_manager.get_queue(ctx.streamer_id).await {
                Ok(queue) => {
                    debug!("#{} queue length: {}", ctx.channel_login, queue.len());
                    let link = format!("{}/queue/{}", frontend_url(&ctx.settings), ctx.streamer_id);
                    send_chat_reply(ctx, "See the current song queue here:", &link).await;
                }
                Err(e) => error!("failed to fetch queue: {e:#}"),
            }
        }
        "downloadlink" | "dl" => {
            metrics::record_twitch_message("downloadlink", true);
            handle_download_link(ctx).await;
        }
        _ => {}
    }

    Ok(())
}

async fn handle_download_link(ctx: &ChatContext) {
    let Some(current) = ctx.queue_manager.get_current_song(ctx.streamer_id).await else {
        send_chat_reply(ctx, "Nothing is playing right now!", "").await;
        return;
    };
    let song = &current.song;

    // One link per song per day. Within its 15-minute lifetime the same link
    // is handed out again; after that the command stays locked until tomorrow.
    let day_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc));

    let Some(day_start) = day_start else {
        return;
    };

    match database::download_links::latest_since(&ctx.pool, ctx.streamer_id, song.id, day_start)
        .await
    {
        Ok(Some(link)) if link.expires_at > Utc::now() => {
            let url = download_page_url(ctx, &link.code);
            send_chat_reply(ctx, &format!("Download '{}':", truncate_title(&song.title)), &url).await;
        }
        Ok(Some(_)) => {
            send_chat_reply(
                ctx,
                &format!(
                    "A download link for '{}' was already used today - try again tomorrow.",
                    truncate_title(&song.title)
                ),
                "",
            )
            .await;
        }
        Ok(None) => {
            let code = Uuid::new_v4().simple().to_string();
            let expires_at = Utc::now() + Duration::from_std(DOWNLOAD_LINK_TTL).unwrap_or(Duration::minutes(15));
            match database::download_links::create(
                &ctx.pool,
                ctx.streamer_id,
                song.id,
                &code,
                expires_at,
            )
            .await
            {
                Ok(()) => {
                    info!(
                        "#{}: generated download link for '{}' (expires {})",
                        ctx.channel_login,
                        song.title,
                        expires_at.to_rfc3339()
                    );
                    let url = download_page_url(ctx, &code);
                    send_chat_reply(ctx, &format!("Download '{}':", truncate_title(&song.title)), &url).await;
                }
                Err(e) => error!("failed to store download link: {e:#}"),
            }
        }
        Err(e) => error!("download link lookup failed: {e:#}"),
    }
}

fn download_page_url(ctx: &ChatContext, code: &str) -> String {
    format!("{}/dl/{code}", frontend_url(&ctx.settings))
}

fn truncate_title(title: &str) -> String {
    if title.chars().count() > 60 {
        let cut: String = title.chars().take(57).collect();
        format!("{cut}...")
    } else {
        title.to_string()
    }
}

/// Sends a chat message via the Helix API using the streamer's OAuth token.
/// Two-part messages are joined with a space; Twitch merges them into one
/// message under our rate budget.
async fn send_chat_reply(ctx: &ChatContext, text_a: &str, text_b: &str) {
    let mut message = String::new();
    if !text_a.is_empty() {
        message.push_str(text_a);
    }
    if !text_b.is_empty() {
        if !message.is_empty() {
            message.push(' ');
        }
        message.push_str(text_b);
    }
    if message.is_empty() {
        return;
    }

    if let Err(e) = helix_send_message(ctx, &message).await {
        debug!("failed to send chat reply in #{}: {e:#}", ctx.channel_login);
    }
}

async fn helix_send_message(ctx: &ChatContext, message: &str) -> anyhow::Result<()> {
    let tokens = database::oauth::twitch(&ctx.pool, ctx.streamer_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no twitch oauth tokens stored"))?;

    let access = tokens
        .access_token
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("twitch access token missing"))?
        .clone();
    let access = crypto_decrypt(ctx, &access)?;

    let Some(streamer) = database::streamers::get(&ctx.pool, ctx.streamer_id).await? else {
        anyhow::bail!("streamer not found");
    };
    let broadcaster = streamer.twitch_user_id.clone();
    let message = message.to_string();

    async fn post(
        ctx: &ChatContext,
        broadcaster: &str,
        token: &str,
        message: &str,
    ) -> anyhow::Result<reqwest::Response> {
        Ok(ctx
            .http
            .post("https://api.twitch.tv/helix/chat/messages")
            .header("Client-Id", ctx.settings.twitch_client_id())
            .bearer_auth(token)
            .json(&serde_json::json!({
                "broadcaster_id": broadcaster,
                "sender_id": broadcaster,
                "message": message,
            }))
            .send()
            .await?)
    }

    let resp = post(ctx, &broadcaster, &access, &message).await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        // Token expired - refresh once and retry.
        let refresh_token = tokens
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("twitch token expired and no refresh token"))?
            .clone();
        let refresh_token = crypto_decrypt(ctx, &refresh_token)?;
        let scope = tokens.scope.clone().unwrap_or_default();
        let (new_access, _new_refresh, _expires_at) =
            refresh_twitch_token(ctx, &refresh_token, &scope).await?;

        let resp2 = post(ctx, &broadcaster, &new_access, &message).await?;
        if !resp2.status().is_success() {
            anyhow::bail!("helix send failed after refresh: {}", resp2.status());
        }
        return Ok(());
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("helix send failed ({status}): {body}");
    }
    Ok(())
}

fn crypto_decrypt(ctx: &ChatContext, blob: &[u8]) -> anyhow::Result<String> {
    let encoded = std::str::from_utf8(blob)?;
    crate::utils::crypto::decrypt(&ctx.aes_key, encoded)
}

async fn refresh_twitch_token(
    ctx: &ChatContext,
    refresh_token: &str,
    scope: &[String],
) -> anyhow::Result<(String, Option<String>, Option<DateTime<Utc>>)> {
    #[derive(serde::Deserialize)]
    struct RefreshResp {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_in: i64,
    }

    let resp = ctx
        .http
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[
            ("client_id", ctx.settings.twitch_client_id()),
            ("client_secret", ctx.settings.twitch_client_secret()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("token refresh failed: {}", resp.status());
    }

    let body: RefreshResp = resp.json().await?;
    let expires_at = (body.expires_in > 0)
        .then(|| Utc::now() + Duration::seconds(body.expires_in));

    let enc_access = crate::utils::crypto::encrypt(&ctx.aes_key, &body.access_token)?;
    let enc_refresh = match &body.refresh_token {
        Some(rt) => Some(crate::utils::crypto::encrypt(&ctx.aes_key, rt)?),
        None => None,
    };
    database::oauth::upsert_provider(
        &ctx.pool,
        ctx.streamer_id,
        "twitch",
        Some(enc_access.as_bytes()),
        enc_refresh.as_deref().map(str::as_bytes),
        expires_at,
        scope,
    )
    .await?;

    Ok((body.access_token, body.refresh_token, expires_at))
}

fn parse_command(channel: &str, user: &TwitchUser, text: &str) -> Option<ChatCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('!') {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('!').to_lowercase();

    // Ignore commands clearly aimed at other bots (e.g. !sr@nightbot).
    if let Some((_, target)) = command.split_once('@') {
        if !target.eq_ignore_ascii_case(channel) && !target.eq_ignore_ascii_case("tmi.js") {
            return None;
        }
    }

    let args = parts
        .next()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();

    Some(ChatCommand {
        command,
        args,
        raw_message: trimmed.to_string(),
        user: user.clone(),
    })
}
