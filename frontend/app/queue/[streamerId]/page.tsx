"use client";

import { useParams } from "next/navigation";
import { useCallback, useEffect, useState } from "react";
import SiteFooter from "@/components/site-footer";

const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

interface QueueEntry {
  title: string;
  artist: string;
  duration_seconds?: number | null;
  thumbnail_url?: string | null;
  requested_by?: string | null;
}

interface PublicQueue {
  current: QueueEntry | null;
  items: Array<QueueEntry & { position: number; votes?: number }>;
}

function formatDuration(seconds?: number | null): string {
  if (!seconds || seconds <= 0) return "--:--";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function Thumb({ entry, size }: { entry: QueueEntry | null; size: "lg" | "sm" }) {
  if (!entry) return null;
  if (entry.thumbnail_url) {
    return (
      <img
        src={entry.thumbnail_url}
        alt=""
        className={size === "lg" ? "h-20 w-20 rounded-xl object-cover" : "h-10 w-10 rounded-lg object-cover"}
        onError={(e) => ((e.target as HTMLImageElement).style.visibility = "hidden")}
      />
    );
  }
  return (
    <div
      className={
        size === "lg"
          ? "flex h-20 w-20 items-center justify-center rounded-xl bg-slate-800 text-2xl"
          : "flex h-10 w-10 items-center justify-center rounded-lg bg-slate-800 text-sm"
      }
    >
      ♪
    </div>
  );
}

export default function PublicQueuePage() {
  const params = useParams<{ streamerId: string }>();
  const streamerId = params?.streamerId;

  const [queue, setQueue] = useState<PublicQueue | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!streamerId) return;
    try {
      const res = await fetch(`${API_URL}/api/v1/public/streamers/${streamerId}/queue`, {
        cache: "no-store",
      });
      const body = (await res.json()) as {
        success: boolean;
        data?: PublicQueue;
        error?: { code: string; message: string };
      };
      if (!body.success || !body.data) {
        setError(body.error?.code === "NOT_FOUND" ? "This queue does not exist." : "Could not load the queue.");
        return;
      }
      setQueue(body.data);
      setError(null);
    } catch {
      setError("Could not reach the server.");
    }
  }, [streamerId]);

  useEffect(() => {
    void load();
    const t = setInterval(load, 10_000);
    return () => clearInterval(t);
  }, [load]);

  return (
    <div className="mx-auto flex min-h-dvh w-full max-w-2xl flex-col px-4 py-8">
      <header className="mb-8 text-center">
        <p className="text-[11px] uppercase tracking-widest text-slate-500">Live song queue</p>
        <h1 className="mt-1 font-display text-3xl font-semibold text-slate-100">Queue</h1>
      </header>

      <main className="flex-1 space-y-6">
        {error && (
          <p className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 text-center text-sm text-slate-400 backdrop-blur-xl">
            {error}
          </p>
        )}

        {!error && (
          <>
            <section>
              <h2 className="mb-3 text-[11px] uppercase tracking-widest text-emerald-400">Now playing</h2>
              {queue === null ? (
                <div className="h-[104px] animate-pulse rounded-2xl border border-slate-800 bg-slate-900/50 backdrop-blur-xl" />
              ) : queue.current ? (
                <div className="flex items-center gap-4 rounded-2xl border border-emerald-500/30 bg-slate-900/50 p-4 shadow-[0_0_24px_-12px_rgba(52,211,153,0.5)] backdrop-blur-xl">
                  <Thumb entry={queue.current} size="lg" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-display font-medium text-slate-100">{queue.current.title}</p>
                    <p className="truncate text-sm text-slate-400">{queue.current.artist}</p>
                    <p className="mt-1 flex items-center gap-2 text-xs text-slate-500">
                      <span>{formatDuration(queue.current.duration_seconds)}</span>
                      {queue.current.requested_by && (
                        <>
                          <span aria-hidden>•</span>
                          <span>requested by {queue.current.requested_by}</span>
                        </>
                      )}
                    </p>
                  </div>
                  <span className="relative flex h-3 w-3 shrink-0" title="Playing now">
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-40" />
                    <span className="relative inline-flex h-3 w-3 rounded-full bg-emerald-400" />
                  </span>
                </div>
              ) : (
                <div className="rounded-2xl border border-dashed border-slate-800 bg-slate-900/30 p-6 text-center text-sm text-slate-500 backdrop-blur-xl">
                  Nothing is playing right now.
                </div>
              )}
            </section>

            <section>
              <h2 className="mb-3 text-[11px] uppercase tracking-widest text-emerald-400">Up next</h2>
              {queue !== null && queue.items.length === 0 && (
                <div className="rounded-2xl border border-dashed border-slate-800 bg-slate-900/30 p-6 text-center text-sm text-slate-500 backdrop-blur-xl">
                  The queue is empty. Type !sr in chat to add a song!
                </div>
              )}
              <ul className="space-y-2">
                {(queue?.items ?? []).map((item) => (
                  <li
                    key={`${item.position}-${item.title}`}
                    className="flex items-center gap-3 rounded-xl border border-slate-800 bg-slate-900/50 p-3 backdrop-blur-xl transition-colors hover:border-slate-700"
                  >
                    <span className="w-6 shrink-0 text-center font-mono text-sm text-slate-600">{item.position}</span>
                    <Thumb entry={item} size="sm" />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium text-slate-200">{item.title}</p>
                      <p className="truncate text-xs text-slate-500">
                        {item.artist}
                        {item.requested_by ? ` · ${item.requested_by}` : ""}
                      </p>
                    </div>
                    {item.votes > 0 && (
                      <span className="shrink-0 rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs text-emerald-400">
                        +{item.votes}
                      </span>
                    )}
                    <span className="shrink-0 font-mono text-xs text-slate-500">
                      {formatDuration(item.duration_seconds)}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          </>
        )}
      </main>

      <SiteFooter />
    </div>
  );
}
