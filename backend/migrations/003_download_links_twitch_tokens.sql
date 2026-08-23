-- Twitch OAuth token persistence (enables chat replies via Helix)
ALTER TABLE oauth_tokens
    ADD COLUMN IF NOT EXISTS twitch_access_token BYTEA,
    ADD COLUMN IF NOT EXISTS twitch_refresh_token BYTEA,
    ADD COLUMN IF NOT EXISTS twitch_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS twitch_scope TEXT[];

-- One-time download links generated from chat (!downloadlink).
-- Rows are auto-deleted once expires_at passes.
CREATE TABLE IF NOT EXISTS download_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    streamer_id UUID NOT NULL REFERENCES streamers(id) ON DELETE CASCADE,
    song_id UUID NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    code VARCHAR(64) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_download_links_expires ON download_links(expires_at);
CREATE INDEX IF NOT EXISTS idx_download_links_song_created
    ON download_links(streamer_id, song_id, created_at DESC);
