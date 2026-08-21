# Twitch Music Bot

A production-ready Twitch music request bot with browser-source overlay architecture, built for minimal RAM usage and maximum reliability.

## Features

### Core Architecture
- **Rust Backend** - Axum + Tokio for minimal memory footprint (~50MB RSS)
- **Browser Source Overlay** - Runs directly in OBS as a browser source, no separate window capture needed
- **Per-Streamer OAuth** - Each streamer connects their own Spotify/YouTube/SoundCloud accounts
- **Multi-Source Music** - YouTube (Invidious/Piped), Spotify, SoundCloud with automatic failover

### Queue & Moderation
- **Fully Configurable** - Queue mode (FIFO, priority for subs/mods/VIPs), limits, cooldowns
- **Explicit Content Filtering** - Allow, block, or clean-only modes with presets
- **Fuzzy Search** - Configurable confidence threshold (default 75%)
- **Direct Link Support** - Paste YouTube/Spotify/SoundCloud URLs directly
- **Vote Skip** - Community voting with configurable threshold
- **Blocked Artists/Keywords** - Per-streamer blocklists

### Reliability (99.9% Uptime Target)
- **Multiple YouTube Sources** - 3 Invidious + 2 Piped instances with automatic rotation
- **Local Cache Fallback** - Recently played songs cached in PostgreSQL for offline playback
- **Auto-Reconnect** - WebSocket, Twitch IRC, Redis, Database with exponential backoff
- **Graceful Degradation** - If Spotify API down, falls back to YouTube search
- **Health Monitoring** - Prometheus metrics, structured logging, health endpoints

### Dashboard (Next.js + Vercel)
- Real-time queue management
- Search with live results
- Spotify playlist browser
- Play history
- Full configuration UI
- OAuth connection management

## Quick Start

### Prerequisites
- Rust 1.79+
- Node.js 20+
- PostgreSQL 16+
- Redis 7+
- Twitch Developer Application
- Spotify Developer Application (optional)
- SoundCloud Developer Application (optional)

### Local Development

1. **Clone and configure**
```bash
git clone https://github.com/yourusername/twitch-music-bot
cd twitch-music-bot
cp .env.example .env
# Edit .env with your credentials
```

2. **Start infrastructure**
```bash
docker-compose up -d postgres redis
```

3. **Run backend**
```bash
cd backend
cargo run
```

4. **Run frontend**
```bash
cd frontend
npm install
npm run dev
```

5. **Open dashboard**
Navigate to `http://localhost:3000` and connect with Twitch.

### Production Deployment

#### Backend (Docker)
```bash
docker-compose up -d
```

#### Frontend (Vercel)
1. Connect repository to Vercel
2. Set environment variables:
   - `NEXT_PUBLIC_API_URL` - Your backend URL
3. Deploy

#### Overlay (OBS)
Add browser source in OBS:
```
https://your-backend-domain.com/ws/overlay/{streamer_id}
```
Or host the static `overlay/index.html` on any static hosting (Cloudflare Pages, Netlify, etc.) and use:
```
https://your-overlay-domain.com/?streamer={streamer_id}
```

## Configuration

### Streamer Settings (via Dashboard)
| Setting | Description | Default |
|---------|-------------|---------|
| Queue Mode | FIFO, Priority Subs/Mods/VIPs | FIFO |
| Max Queue Size | Maximum songs in queue | 50 |
| Max Requests/User | Per cooldown period | 3 |
| Request Cooldown | Seconds between requests | 30 |
| Explicit Filter | Allow/Clean Only/Block | Clean Only |
| Direct Links | Allow URL requests | Enabled |
| Fuzzy Threshold | Search confidence (0-1) | 0.75 |
| Vote Skip | Enable community skip | Enabled |
| Vote Threshold | % of viewers needed | 50% |
| Default Volume | 0.0 - 1.0 | 0.5 |
| Crossfade | Seconds between songs | 2.0 |

### YouTube Sources (Configurable)
The bot uses multiple Invidious/Piped instances with automatic failover:
- yewtu.be
- inv.nadeko.net
- invidious.snopyta.org
- pipedapi.kavin.rocks
- piped-api.garudalinux.org

Add/remove instances in `config/production.toml`.

## API Endpoints

### Authentication
- `GET /auth/twitch` - Initiate Twitch OAuth
- `GET /auth/spotify` - Initiate Spotify OAuth
- `GET /auth/soundcloud` - Initiate SoundCloud OAuth
- `GET /api/v1/auth/me` - Get current streamer info

### Queue Management
- `GET /api/v1/queue` - Get current queue
- `DELETE /api/v1/queue/:id` - Remove from queue
- `POST /api/v1/queue/clear` - Clear queue (mods only)
- `PUT /api/v1/queue/reorder` - Reorder queue (mods only)
- `POST /api/v1/queue/:id/skip` - Skip current song (mods only)
- `POST /api/v1/queue/:id/vote-skip` - Vote to skip

### Search & Music
- `GET /api/v1/search?q=query&limit=10` - Search songs
- `GET /api/v1/song/:id/stream` - Get stream URL
- `GET /api/v1/oauth/spotify/playlists` - Get Spotify playlists
- `GET /api/v1/oauth/spotify/playlist/:id/tracks` - Get playlist tracks

### Configuration
- `GET /api/v1/streamer/config` - Get config
- `PUT /api/v1/streamer/config` - Update config

### WebSocket Overlay
- `WS /ws/overlay/:streamer_id` - Real-time overlay updates

## Overlay Messages

The WebSocket sends these message types:
```typescript
type OverlayMessageType =
  | 'play' | 'pause' | 'resume' | 'stop' | 'skip'
  | 'volume' | 'queue_update' | 'now_playing'
  | 'error' | 'connected' | 'disconnected' | 'config_update';
```

## Monitoring

### Prometheus Metrics (Port 9090)
- `http_requests_total` - HTTP request counts
- `http_request_duration_seconds` - Request latency
- `twitch_messages_total` - Twitch chat messages
- `queue_operations_total` - Queue operations
- `music_searches_total` - Search counts by source
- `stream_url_fetches_total` - Stream URL fetch success/failure
- `overlay_connections_active` - Active overlay connections
- `queue_size` - Current queue size per streamer
- `memory_usage_bytes` - RSS memory usage

### Health Checks
- `GET /health` - Basic health
- `GET /metrics` - Prometheus metrics

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Twitch    │────▶│   Rust      │────▶│  PostgreSQL │
│   IRC       │     │   Backend   │     │  (Queue,    │
└─────────────┘     │  (Axum)     │     │   History,  │
                    └──────┬──────┘     │   Cache)    │
                           │            └─────────────┘
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ YouTube  │ │ Spotify  │ │SoundCloud│
        │(Invidious│ │  (API)   │ │  (API)   │
        │ /Piped)  │ └──────────┘ └──────────┘
        └──────────┘
              │
              ▼
        ┌──────────┐
        │  Redis   │
        │ (Pub/Sub,│
        │  Cache)  │
        └──────────┘
              │
              ▼
        ┌──────────┐
        │  WebSocket│
        │  Server  │
        └────┬─────┘
             │
             ▼
        ┌──────────┐
        │  Browser │
        │  Source  │
        │ (Overlay)│
        └──────────┘
```

## Memory Optimization

- **No Electron/Chromium** - Overlay is pure HTML/JS
- **Connection Pooling** - SQLx + deadpool-redis
- **Streaming Audio** - Direct stream URLs, no local decoding
- **Minimal Dependencies** - ~50MB RSS typical
- **LTO + Strip** - Release builds optimized for size

## License

MIT License - see LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test` and `npm test`
5. Submit a PR

## Support

- Issues: GitHub Issues
- Discord: [Join our server](https://discord.gg/yourserver)
- Documentation: [Wiki](https://github.com/yourusername/twitch-music-bot/wiki)