use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::{PrivmsgMessage, ServerMessage};
use twitch_irc::transport::tcp::{TCPTransport, TLS};
use twitch_irc::TwitchIRCClient;

use crate::metrics;
use crate::queue::QueueManager;
use twitch_music_shared::*;

type IrcClient = TwitchIRCClient<TCPTransport<TLS>, StaticLoginCredentials>;

/// Per-channel chat bot that listens for music commands. Uses an anonymous
/// read-only connection ("justinfan" users); it never sends chat messages,
/// it only observes requests and mutates queue state directly.
pub struct TwitchBot {
    pub streamer_id: Uuid,
    pub twitch_login: String,
    pub queue_manager: Arc<QueueManager>,
}

impl TwitchBot {
    /// Joins the channel and spawns the message-processing loop.
    pub fn spawn_for_channel(
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
}

struct ChatContext {
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

            match ctx.queue_manager.add_request(ctx.streamer_id, &user, &query, None).await {
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
                Err(BotError::UserBlocked) => debug!("blocked user {} attempted request", user.login),
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
            // Queue contents are surfaced through the dashboard/overlay; we
            // only log here to keep the anonymous connection read-only.
            match ctx.queue_manager.get_queue(ctx.streamer_id).await {
                Ok(queue) => debug!("#{} queue length: {}", ctx.channel_login, queue.len()),
                Err(e) => error!("failed to fetch queue: {e:#}"),
            }
        }
        _ => {}
    }

    Ok(())
}

fn parse_command(channel: &str, user: &TwitchUser, text: &str) -> Option<ChatCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('!') {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('!').to_lowercase();

    // Ignore commands that are clearly meant for other bots (e.g. !sr@nightbot).
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
