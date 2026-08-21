-- Initial schema for Twitch Music Bot
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Streamers table
CREATE TABLE streamers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    twitch_user_id VARCHAR(64) NOT NULL UNIQUE,
    twitch_login VARCHAR(64) NOT NULL,
    twitch_display_name VARCHAR(128),
    avatar_url TEXT,
    email VARCHAR(256),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_streamers_twitch_user_id ON streamers(twitch_user_id);
CREATE INDEX idx_streamers_twitch_login ON streamers(twitch_login);

-- Streamer configurations
CREATE TABLE streamer_configs (
    streamer_id UUID PRIMARY KEY REFERENCES streamers(id) ON DELETE CASCADE,
    queue_mode VARCHAR(32) NOT NULL DEFAULT 'fifo',
    max_queue_size INTEGER NOT NULL DEFAULT 50,
    max_requests_per_user INTEGER NOT NULL DEFAULT 3,
    request_cooldown_seconds INTEGER NOT NULL DEFAULT 30,
    explicit_filter VARCHAR(32) NOT NULL DEFAULT 'clean_only',
    allow_direct_links BOOLEAN NOT NULL DEFAULT true,
    fuzzy_match_threshold REAL NOT NULL DEFAULT 0.75,
    auto_skip_after_seconds INTEGER,
    vote_skip_enabled BOOLEAN NOT NULL DEFAULT true,
    vote_skip_threshold REAL NOT NULL DEFAULT 0.5,
    blocked_artists TEXT[] NOT NULL DEFAULT '{}',
    blocked_keywords TEXT[] NOT NULL DEFAULT '{}',
    allowed_sources TEXT[] NOT NULL DEFAULT '{"youtube","spotify","soundcloud"}',
    default_volume REAL NOT NULL DEFAULT 0.5,
    crossfade_seconds REAL NOT NULL DEFAULT 2.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- OAuth tokens (encrypted)
CREATE TABLE oauth_tokens (
    streamer_id UUID PRIMARY KEY REFERENCES streamers(id) ON DELETE CASCADE,
    spotify_access_token BYTEA,
    spotify_refresh_token BYTEA,
    spotify_expires_at TIMESTAMPTZ,
    spotify_scope TEXT[],
    youtube_access_token BYTEA,
    youtube_refresh_token BYTEA,
    youtube_expires_at TIMESTAMPTZ,
    youtube_scope TEXT[],
    soundcloud_access_token BYTEA,
    soundcloud_refresh_token BYTEA,
    soundcloud_expires_at TIMESTAMPTZ,
    soundcloud_scope TEXT[],
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- OAuth states for CSRF/PKCE protection (one-time use, auto-expire)
CREATE TABLE oauth_states (
    state_token VARCHAR(128) PRIMARY KEY,
    state_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_oauth_states_expires ON oauth_states(expires_at);

-- Song cache (for offline fallback and deduplication)
CREATE TABLE songs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    source VARCHAR(32) NOT NULL,
    source_id VARCHAR(256) NOT NULL,
    title VARCHAR(512) NOT NULL,
    artist VARCHAR(512) NOT NULL,
    duration_seconds INTEGER,
    thumbnail_url TEXT,
    stream_url TEXT,
    explicit BOOLEAN NOT NULL DEFAULT false,
    metadata JSONB NOT NULL DEFAULT '{}',
    play_count INTEGER NOT NULL DEFAULT 0,
    last_played_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source, source_id)
);

CREATE INDEX idx_songs_source_source_id ON songs(source, source_id);
CREATE INDEX idx_songs_title_artist ON songs(title, artist);
CREATE INDEX idx_songs_play_count ON songs(play_count DESC);
CREATE INDEX idx_songs_last_played ON songs(last_played_at DESC);

-- Queue items
CREATE TABLE queue_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    song_id UUID NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    requested_by_user_id VARCHAR(64) NOT NULL,
    requested_by_login VARCHAR(64) NOT NULL,
    requested_by_display_name VARCHAR(128) NOT NULL,
    requested_by_is_mod BOOLEAN NOT NULL DEFAULT false,
    requested_by_is_sub BOOLEAN NOT NULL DEFAULT false,
    requested_by_is_vip BOOLEAN NOT NULL DEFAULT false,
    requested_by_badges TEXT[] NOT NULL DEFAULT '{}',
    position INTEGER NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    error_message TEXT,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    played_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_queue_items_streamer_status ON queue_items(streamer_id, status);
CREATE INDEX idx_queue_items_streamer_position ON queue_items(streamer_id, position) WHERE status = 'pending';
CREATE INDEX idx_queue_items_user_lookup ON queue_items(streamer_id, requested_by_user_id, requested_at);

-- Queue counters for atomic position assignment
CREATE TABLE queue_counters (
    streamer_id UUID PRIMARY KEY REFERENCES streamers(id) ON DELETE CASCADE,
    next_position INTEGER NOT NULL DEFAULT 1
);

-- Play history
CREATE TABLE play_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    song_id UUID NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    queue_item_id UUID REFERENCES queue_items(id) ON DELETE SET NULL,
    played_by_user_id VARCHAR(64),
    played_by_login VARCHAR(64),
    played_by_display_name VARCHAR(128),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    duration_played_seconds INTEGER,
    was_skipped BOOLEAN NOT NULL DEFAULT false,
    skip_reason VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_play_history_streamer_started ON play_history(streamer_id, started_at DESC);

-- Vote skips
CREATE TABLE vote_skips (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    queue_item_id UUID NOT NULL REFERENCES queue_items(id) ON DELETE CASCADE,
    voter_user_id VARCHAR(64) NOT NULL,
    voter_login VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(queue_item_id, voter_user_id)
);

CREATE INDEX idx_vote_skips_queue_item ON vote_skips(queue_item_id);

-- Blocked users
CREATE TABLE blocked_users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    user_id VARCHAR(64) NOT NULL,
    user_login VARCHAR(64) NOT NULL,
    reason TEXT,
    blocked_by_user_id VARCHAR(64),
    blocked_by_login VARCHAR(64),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(streamer_id, user_id)
);

CREATE INDEX idx_blocked_users_streamer ON blocked_users(streamer_id);

-- Rate limiting (fixed time buckets)
CREATE TABLE rate_limits (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    user_id VARCHAR(64) NOT NULL,
    user_login VARCHAR(64) NOT NULL,
    action VARCHAR(64) NOT NULL,
    count INTEGER NOT NULL DEFAULT 1,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(streamer_id, user_id, action, window_start)
);

CREATE INDEX idx_rate_limits_streamer_user ON rate_limits(streamer_id, user_id, action);
CREATE INDEX idx_rate_limits_window ON rate_limits(window_start);

-- Overlay connections (for tracking active overlays)
CREATE TABLE overlay_connections (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    connection_id VARCHAR(128) NOT NULL,
    user_agent TEXT,
    ip_address INET,
    connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_ping_at TIMESTAMPTZ,
    disconnected_at TIMESTAMPTZ,
    UNIQUE(streamer_id, connection_id)
);

CREATE INDEX idx_overlay_connections_streamer ON overlay_connections(streamer_id);
CREATE INDEX idx_overlay_connections_last_ping ON overlay_connections(last_ping_at);

-- Audit log for security-sensitive actions
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    actor_user_id VARCHAR(64) NOT NULL,
    actor_login VARCHAR(64) NOT NULL,
    action VARCHAR(64) NOT NULL,
    target_type VARCHAR(32),
    target_id VARCHAR(128),
    details JSONB,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_streamer_time ON audit_log(streamer_id, created_at DESC);
CREATE INDEX idx_audit_log_action ON audit_log(action);

-- Updated at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_streamers_updated_at BEFORE UPDATE ON streamers FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_streamer_configs_updated_at BEFORE UPDATE ON streamer_configs FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_oauth_tokens_updated_at BEFORE UPDATE ON oauth_tokens FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_songs_updated_at BEFORE UPDATE ON songs FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_queue_items_updated_at BEFORE UPDATE ON queue_items FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_overlay_connections_updated_at BEFORE UPDATE ON overlay_connections FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();