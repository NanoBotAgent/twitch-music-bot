export type MusicSource = "youtube" | "spotify" | "soundcloud" | "local";

export interface Song {
  id: string;
  source: MusicSource;
  source_id: string;
  title: string;
  artist: string;
  duration_seconds: number | null;
  thumbnail_url: string | null;
  stream_url: string | null;
  explicit: boolean;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface QueuedSong {
  queue_item_id: string;
  streamer_id: string;
  requested_by_user_id: string;
  requester_name: string;
  song: Song;
  status: "pending" | "playing" | "played" | "skipped" | "failed";
  position: number;
  votes: number;
  required_votes: number;
  requested_at: string;
}

export interface QueuedSongSummary {
  position: number;
  title: string;
  artist: string;
  thumbnail_url: string | null;
  source: MusicSource;
  requested_by: string;
  duration_seconds: number | null;
  explicit: boolean;
  votes: number;
  required_votes: number;
  queue_item_id: string;
}

export interface OverlayMessage {
  streamer_id: string;
  payload:
    | { type: "now_playing"; song: Song; requested_by: string }
    | { type: "song_ended"; song_id: string }
    | { type: "song_skipped"; song_id: string; skipped_by: string }
    | { type: "queue_updated"; queue: QueuedSongSummary[] }
    | { type: "vote_progress"; queue_item_id: string; current_votes: number; required_votes: number }
    | { type: "streamer_offline" };
}

export interface StreamerConfig {
  streamer_id: string;
  queue_mode: string;
  max_queue_size: number;
  max_requests_per_user: number;
  request_cooldown_seconds: number;
  explicit_filter: string;
  allow_direct_links: boolean;
  fuzzy_match_threshold: number;
  auto_skip_after_seconds: number | null;
  vote_skip_enabled: boolean;
  vote_skip_threshold: number;
  blocked_artists: string[];
  blocked_keywords: string[];
  allowed_sources: string[];
  default_volume: number;
  crossfade_seconds: number;
}

export interface HistoryItem {
  history_id: string;
  song_id: string;
  source: MusicSource;
  title: string;
  artist: string;
  duration_seconds: number | null;
  thumbnail_url: string | null;
  explicit: boolean;
  played_by_display_name: string | null;
  started_at: string;
  ended_at: string | null;
  was_skipped: boolean;
  skip_reason: string | null;
}

export interface SearchResult {
  song: Song;
  confidence: number;
  matched_query: string;
}

export interface BlockedUser {
  user_id: string;
  user_login: string;
  reason: string | null;
  expires_at: string | null;
}

export interface Me {
  id: string;
  twitch_user_id: string;
  login: string;
  display_name: string | null;
  avatar_url: string | null;
  email: string | null;
}
