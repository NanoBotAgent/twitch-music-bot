"use client";

import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { useOverlaySocket } from "@/lib/useOverlaySocket";
import type { QueuedSongSummary, Song } from "@/lib/types";

interface NowPlayingPayload {
  song: Song | null;
  requested_by: string | null;
  url: string | null;
}

export default function OverlayPage({ params }: { params: { streamerId: string } }) {
  const { streamerId } = params;
  const { connected, lastMessage } = useOverlaySocket(streamerId);

  const [now, setNow] = useState<NowPlayingPayload>({ song: null, requested_by: null, url: null });
  const [queue, setQueue] = useState<QueuedSongSummary[]>([]);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const hlsRef = useRef<{ destroy: () => void } | null>(null);

  // Initial fetch of the currently playing song (public endpoint).
  useEffect(() => {
    if (!streamerId) return;
    fetchCurrent(streamerId).then(setNow).catch(() => undefined);
  }, [streamerId]);

  // React to live events.
  useEffect(() => {
    if (!lastMessage) return;
    const payload = lastMessage.payload;

    switch (payload.type) {
      case "now_playing":
      case "song_ended":
      case "song_skipped":
        fetchCurrent(streamerId)
          .then(setNow)
          .catch(() => undefined);
        break;
      case "queue_updated":
        setQueue(payload.queue);
        break;
      case "streamer_offline":
        setNow({ song: null, requested_by: null, url: null });
        setQueue([]);
        break;
      default:
        break;
    }
  }, [lastMessage, streamerId]);

  // Drive the <audio> element, switching between direct MP3 and HLS.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    // Tear down any previous HLS instance.
    if (hlsRef.current) {
      hlsRef.current.destroy();
      hlsRef.current = null;
    }

    if (!now.url) {
      audio.pause();
      audio.removeAttribute("src");
      return;
    }

    if (now.url.includes(".m3u8")) {
      import("hls.js")
        .then(({ default: Hls }) => {
          if (Hls.isSupported()) {
            const hls = new Hls({ autoStartLoad: true });
            hls.loadSource(now.url as string);
            hls.attachMedia(audio);
            hls.on(Hls.Events.MANIFEST_PARSED, () => void audio.play().catch(() => undefined));
            hlsRef.current = hls;
          } else if (audio.canPlayType("application/vnd.apple.mpegurl")) {
            audio.src = now.url as string;
            void audio.play().catch(() => undefined);
          }
        })
        .catch(() => undefined);
    } else {
      audio.src = now.url;
      void audio.play().catch(() => undefined);
    }

    return () => {
      if (hlsRef.current) {
        hlsRef.current.destroy();
        hlsRef.current = null;
      }
    };
  }, [now.url]);

  return (
    <div className="transparent min-h-screen bg-transparent p-4 text-white">
      <audio ref={audioRef} className="hidden" />

      {now.song ? (
        <div className="glass flex max-w-xl items-center gap-4 !border-slate-700/60 !bg-slate-950/70 p-4">
          {now.song.thumbnail_url && (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={now.song.thumbnail_url} alt="" className="h-16 w-16 rounded-xl object-cover" />
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate font-display text-base font-semibold">{now.song.title}</p>
            <p className="truncate text-sm text-slate-400">
              {now.song.artist}
              {now.requested_by ? ` · ${now.requested_by}` : ""}
            </p>
          </div>
          <span
            className={`h-2.5 w-2.5 shrink-0 rounded-full ${connected ? "bg-accent-400" : "bg-slate-600"}`}
            title={connected ? "Connected" : "Reconnecting"}
          />
        </div>
      ) : (
        <div className="glass inline-flex items-center gap-3 !border-slate-700/60 !bg-slate-950/70 px-5 py-3">
          <span className="text-sm text-slate-400">No song playing. Chat can request with !sr</span>
          <span className={`h-2.5 w-2.5 rounded-full ${connected ? "bg-accent-400" : "bg-slate-600"}`} />
        </div>
      )}

      {queue.length > 0 && (
        <div className="glass mt-3 max-w-xl !border-slate-700/60 !bg-slate-950/70 p-4">
          <p className="text-xs font-semibold uppercase tracking-wider text-slate-500">Up next</p>
          <ol className="mt-2 space-y-1.5">
            {queue.slice(0, 3).map((item, index) => (
              <li key={item.queue_item_id} className="truncate text-sm text-slate-300">
                <span className="mr-2 text-slate-600">{index + 1}.</span>
                {item.title} · {item.requested_by}
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}

async function fetchCurrent(streamerId: string): Promise<NowPlayingPayload> {
  const data = await api<{ song: Song | null; requested_by?: string | null; url?: string | null }>(
    `/api/v1/overlay/${streamerId}/current`
  );
  return { song: data.song, requested_by: data.requested_by ?? null, url: data.url ?? null };
}
