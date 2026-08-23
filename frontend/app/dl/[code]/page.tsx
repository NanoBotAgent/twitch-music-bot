"use client";

import { useParams } from "next/navigation";
import { useCallback, useEffect, useState } from "react";
import SiteFooter from "@/components/site-footer";

const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

interface DownloadMeta {
  title: string;
  artist: string;
  thumbnail_url?: string | null;
  expires_at: string;
}

export default function DownloadPage() {
  const params = useParams<{ code: string }>();
  const code = params?.code;

  const [meta, setMeta] = useState<DownloadMeta | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "expired" | "error">("loading");

  const load = useCallback(async () => {
    if (!code) return;
    try {
      const res = await fetch(`${API_URL}/api/v1/public/download/${code}/meta`, { cache: "no-store" });
      if (res.status === 404 || res.status === 410) {
        setState("expired");
        return;
      }
      const body = (await res.json()) as { success: boolean; data?: DownloadMeta };
      if (!body.success || !body.data) {
        setState("expired");
        return;
      }
      setMeta(body.data);
      setState("ready");
    } catch {
      setState("error");
    }
  }, [code]);

  useEffect(() => {
    void load();
    const t = setInterval(load, 30_000);
    return () => clearInterval(t);
  }, [load]);

  const [secondsLeft, setSecondsLeft] = useState<number | null>(null);

  useEffect(() => {
    if (!meta?.expires_at) return;
    const tick = () => {
      const left = Math.max(0, Math.floor((new Date(meta.expires_at).getTime() - Date.now()) / 1000));
      setSecondsLeft(left);
      if (left <= 0) setState("expired");
    };
    tick();
    const t = setInterval(tick, 1000);
    return () => clearInterval(t);
  }, [meta?.expires_at]);

  function formatCountdown(total: number): string {
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  return (
    <div className="mx-auto flex min-h-dvh w-full max-w-md flex-col px-4 py-10">
      <header className="mb-8 text-center">
        <p className="text-[11px] uppercase tracking-widest text-slate-500">Song download</p>
        <h1 className="mt-1 font-display text-3xl font-semibold text-slate-100">Download</h1>
      </header>

      <main className="flex flex-1 flex-col items-center justify-center pb-16">
        {state === "loading" && (
          <div className="glass h-56 w-full animate-pulse" aria-label="Loading" />
        )}

        {state === "error" && (
          <div className="glass w-full p-6 text-center text-sm text-slate-400">
            Could not reach the server. Please try again.
          </div>
        )}

        {state === "expired" && (
          <div className="glass w-full p-8 text-center">
            <p className="mb-2 text-4xl" aria-hidden>
              ⏳
            </p>
            <p className="font-display font-medium text-slate-200">This link has expired</p>
            <p className="mt-2 text-sm text-slate-500">
              Download links stay live for 15 minutes. Ask the bot for a new one with{" "}
              <span className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-xs text-emerald-400">
                !downloadlink
              </span>{" "}
              in chat.
            </p>
          </div>
        )}

        {state === "ready" && meta && (
          <div className="glass w-full overflow-hidden p-6 text-center">
            {meta.thumbnail_url && (
              <img
                src={meta.thumbnail_url}
                alt=""
                className="mx-auto mb-4 h-32 w-32 rounded-2xl object-cover"
                onError={(e) => ((e.target as HTMLImageElement).style.display = "none")}
              />
            )}
            <p className="font-display text-lg font-medium text-slate-100">{meta.title}</p>
            <p className="mt-0.5 text-sm text-slate-400">{meta.artist}</p>

            {secondsLeft !== null && secondsLeft > 0 && (
              <p className="mt-4 inline-flex items-center gap-2 rounded-full bg-emerald-500/10 px-3 py-1 text-xs text-emerald-400">
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" />
                Link expires in {formatCountdown(secondsLeft)}
              </p>
            )}

            <a
              href={`${API_URL}/api/v1/public/download/${code}`}
              className="btn-primary mt-6 w-full"
              rel="noopener"
            >
              ⬇ Download song
            </a>

            <p className="mt-4 text-[11px] leading-relaxed text-slate-600">
              For personal use only. Support the artists.
            </p>
          </div>
        )}
      </main>

      <SiteFooter />
    </div>
  );
}
