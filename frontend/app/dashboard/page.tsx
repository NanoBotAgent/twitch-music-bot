"use client";

import { useCallback, useEffect, useState } from "react";
import { api, ApiError, startSpotifyConnect } from "@/lib/api";
import type { BlockedUser, HistoryItem, QueuedSong, SearchResult, Song, StreamerConfig } from "@/lib/types";

export default function DashboardPage() {
  const [queue, setQueue] = useState<QueuedSong[]>([]);
  const [current, setCurrent] = useState<QueuedSong | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [blocked, setBlocked] = useState<BlockedUser[]>([]);
  const [config, setConfig] = useState<StreamerConfig | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const flash = useCallback((message: string) => {
    setNotice(message);
    setTimeout(() => setNotice(null), 3500);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [q, h, b, c] = await Promise.all([
        api<{ items: QueuedSong[] }>("/api/v1/queue"),
        api<{ items: HistoryItem[] }>("/api/v1/history?limit=15"),
        api<BlockedUser[]>("/api/v1/blocked-users"),
        api<StreamerConfig>("/api/v1/config"),
      ]);
      setQueue(q.items);
      setHistory(h.items);
      setBlocked(b);
      setConfig(c);
      try {
        setCurrent(await api<QueuedSong | null>("/api/v1/queue/current"));
      } catch {
        setCurrent(null);
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) window.location.assign("/");
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(refresh, 15000);
    return () => clearInterval(timer);
  }, [refresh]);

  async function skip() {
    if (!current) return;
    try {
      await api(`/api/v1/queue/${current.song.id}/skip`, { method: "POST" });
      await refresh();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function removeItem(id: string) {
    try {
      await api(`/api/v1/queue/${id}`, { method: "DELETE" });
      await refresh();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function clearQueue() {
    try {
      await api("/api/v1/queue/clear", { method: "POST" });
      flash("Queue cleared");
      await refresh();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function saveConfig(next: StreamerConfig) {
    try {
      const saved = await api<StreamerConfig>("/api/v1/config", {
        method: "PUT",
        body: JSON.stringify(next),
      });
      setConfig(saved);
      flash("Settings saved");
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  return (
    <main className="grid gap-6">
      {notice && (
        <div className="glass border-accent-500/40 px-5 py-3 text-sm text-accent-400">{notice}</div>
      )}

      <OnboardingCard flash={flash} />

      <NowPlaying song={current?.song ?? null} requestedBy={current?.requester_name ?? null} onSkip={skip} />

      <div className="grid gap-6 lg:grid-cols-2">
        <RequestPanel onQueued={refresh} flash={flash} />
        <QueuePanel queue={queue} onRemove={removeItem} onClear={clearQueue} onRefresh={refresh} flash={flash} />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        {config && <ConfigPanel config={config} onSave={saveConfig} />}
        <ConnectionsPanel flash={flash} />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <HistoryPanel items={history} />
        <BlockedUsersPanel items={blocked} onChanged={refresh} flash={flash} />
      </div>
    </main>
  );
}

function errorMessage(e: unknown): string {
  return e instanceof ApiError ? e.message : "Something went wrong";
}

// ---------------------------------------------------------------------------

function NowPlaying({
  song,
  requestedBy,
  onSkip,
}: {
  song: Song | null;
  requestedBy: string | null;
  onSkip: () => void;
}) {
  return (
    <section className="glass flex items-center gap-4 p-4 sm:gap-5 sm:p-6">
      <Thumbnail song={song} />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-semibold uppercase tracking-wider text-slate-500">Now playing</p>
        {song ? (
          <>
            <h2 className="mt-1 truncate font-display text-lg font-semibold text-white">{song.title}</h2>
            <p className="truncate text-sm text-slate-400">
              {song.artist}
              {requestedBy ? ` · requested by ${requestedBy}` : ""}
            </p>
          </>
        ) : (
          <h2 className="mt-1 font-display text-lg text-slate-400">Nothing playing</h2>
        )}
      </div>
      {song && (
        <button onClick={onSkip} className="btn-ghost shrink-0">
          Skip
        </button>
      )}
    </section>
  );
}

function Thumbnail({ song, small }: { song: Song | null; small?: boolean }) {
  const size = small ? "h-10 w-10 rounded-lg" : "h-14 w-14 rounded-xl sm:h-20 sm:w-20";
  if (song?.thumbnail_url) {
    // eslint-disable-next-line @next/next/no-img-element
    return <img src={song.thumbnail_url} alt="" className={`${size} shrink-0 object-cover`} />;
  }
  return <div className={`${size} shrink-0 bg-slate-800`} />;
}

// ---------------------------------------------------------------------------

function RequestPanel({
  onQueued,
  flash,
}: {
  onQueued: () => Promise<void>;
  flash: (msg: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);

  async function search() {
    if (!query.trim()) return;
    setSearching(true);
    setResults([]);
    try {
      const data = await api<{ results: SearchResult[] }>(
        `/api/v1/search?q=${encodeURIComponent(query.trim())}`
      );
      setResults(data.results);
    } catch (e) {
      flash(errorMessage(e));
    } finally {
      setSearching(false);
    }
  }

  async function request(song: Song) {
    try {
      await api("/api/v1/requests", {
        method: "POST",
        body: JSON.stringify({ query: `${song.title} ${song.artist}`, source_hint: song.source }),
      });
      flash(`Queued "${song.title}"`);
      setResults([]);
      setQuery("");
      await onQueued();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  return (
    <section className="glass p-4 sm:p-6">
      <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
        Add a song
      </h3>
      <form
        className="mt-4 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search YouTube, Spotify or SoundCloud..."
          className="input"
        />
        <button type="submit" disabled={searching} className="btn-primary shrink-0">
          {searching ? "..." : "Search"}
        </button>
      </form>

      <ul className="mt-4 space-y-2">
        {results.map((r) => (
          <li key={`${r.song.source}:${r.song.source_id}`} className="flex items-center gap-3 rounded-xl bg-slate-950/40 p-2.5">
            <Thumbnail song={r.song} small />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium text-slate-200">{r.song.title}</p>
              <p className="truncate text-xs text-slate-500">
                {r.song.artist} · {sourceLabel(r.song.source)}
                {r.song.explicit ? " · explicit" : ""}
              </p>
            </div>
            <button onClick={() => request(r.song)} className="btn-primary shrink-0 px-3 py-1.5 text-xs">
              Queue
            </button>
          </li>
        ))}
        {!results.length && !searching && (
          <li className="py-6 text-center text-xs text-slate-600">Search results appear here</li>
        )}
      </ul>
    </section>
  );
}

function sourceLabel(source: string): string {
  switch (source) {
    case "youtube":
      return "YouTube";
    case "spotify":
      return "Spotify";
    case "soundcloud":
      return "SoundCloud";
    default:
      return source;
  }
}

// ---------------------------------------------------------------------------

function QueuePanel({
  queue,
  onRemove,
  onClear,
  onRefresh,
  flash,
}: {
  queue: QueuedSong[];
  onRemove: (id: string) => void;
  onClear: () => Promise<void>;
  onRefresh: () => Promise<void>;
  flash: (msg: string) => void;
}) {
  const [order, setOrder] = useState<string[]>([]);
  const [confirmClear, setConfirmClear] = useState(false);
  const [savingOrder, setSavingOrder] = useState(false);

  useEffect(() => {
    setOrder(queue.map((q) => q.queue_item_id));
  }, [queue]);

  function move(index: number, direction: -1 | 1) {
    const next = [...order];
    const target = index + direction;
    if (target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target], next[index]];
    setOrder(next);
  }

  async function saveOrder() {
    setSavingOrder(true);
    try {
      await api("/api/v1/queue/reorder", { method: "PUT", body: JSON.stringify({ order }) });
      await onRefresh();
      flash("Queue order saved");
    } catch {
      flash("Could not reorder the queue");
    } finally {
      setSavingOrder(false);
    }
  }

  function handleClear() {
    if (!confirmClear) {
      setConfirmClear(true);
      setTimeout(() => setConfirmClear(false), 3000);
      return;
    }
    setConfirmClear(false);
    void onClear();
  }

  const byId = new Map(queue.map((q) => [q.queue_item_id, q]));
  const ordered = order.map((id) => byId.get(id)).filter(Boolean) as QueuedSong[];
  const dirty = order.join(",") !== queue.map((q) => q.queue_item_id).join(",");

  return (
    <section className="glass p-4 sm:p-6">
      <div className="flex items-center justify-between">
        <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Queue ({queue.length})
        </h3>
        {queue.length > 0 && (
          <button
            onClick={handleClear}
            className={`text-xs transition ${confirmClear ? "font-semibold text-red-400" : "text-slate-500 hover:text-red-400"}`}
          >
            {confirmClear ? "Click again to confirm" : "Clear all"}
          </button>
        )}
      </div>

      <ul className="mt-4 space-y-2">
        {ordered.map((item, index) => (
          <li key={item.queue_item_id} className="flex items-center gap-3 rounded-xl bg-slate-950/40 p-2.5">
            <span className="w-5 text-center font-display text-xs text-slate-500">{index + 1}</span>
            <Thumbnail song={item.song} small />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium text-slate-200">{item.song.title}</p>
              <p className="truncate text-xs text-slate-500">by {item.requester_name}</p>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              <IconBtn label="Move up" disabled={index === 0} onClick={() => move(index, -1)}>↑</IconBtn>
              <IconBtn label="Move down" disabled={index === ordered.length - 1} onClick={() => move(index, 1)}>↓</IconBtn>
              <IconBtn label="Remove" onClick={() => onRemove(item.queue_item_id)}>✕</IconBtn>
            </div>
          </li>
        ))}
        {!ordered.length && (
          <li className="py-8 text-center text-xs text-slate-600">
            The queue is empty. Chat can add songs with !sr
          </li>
        )}
      </ul>

      {dirty && ordered.length > 0 && (
        <button onClick={saveOrder} disabled={savingOrder} className="btn-primary mt-4 w-full py-2 text-xs">
          {savingOrder ? "Saving..." : "Save new order"}
        </button>
      )}
    </section>
  );
}

function IconBtn({
  children,
  label,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className="grid h-9 w-9 place-items-center rounded-lg border border-slate-800 text-sm text-slate-400 transition hover:border-slate-600 hover:text-white disabled:opacity-30"
    >
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------

function ConfigPanel({
  config,
  onSave,
}: {
  config: StreamerConfig;
  onSave: (config: StreamerConfig) => void;
}) {
  const [draft, setDraft] = useState(config);

  function field<K extends keyof StreamerConfig>(key: K, value: StreamerConfig[K]) {
    setDraft({ ...draft, [key]: value });
  }

  return (
    <section className="glass p-4 sm:p-6">
      <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
        Request settings
      </h3>

      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        <NumberField label="Max queue size" value={draft.max_queue_size} min={1} max={500}
          onChange={(v) => field("max_queue_size", v)} />
        <NumberField label="Songs per user" value={draft.max_requests_per_user} min={1} max={50}
          onChange={(v) => field("max_requests_per_user", v)} />
        <NumberField label="Cooldown (seconds)" value={draft.request_cooldown_seconds} min={0} max={3600}
          onChange={(v) => field("request_cooldown_seconds", v)} />

        <label className="block">
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Explicit content</span>
          <select
            value={draft.explicit_filter}
            onChange={(e) => field("explicit_filter", e.target.value)}
            className="input"
          >
            <option value="allow">Allow</option>
            <option value="block">Block</option>
          </select>
        </label>
      </div>

      <Toggle
        label="Vote skip"
        hint="Chat can vote to skip with !voteskip"
        checked={draft.vote_skip_enabled}
        onChange={(v) => field("vote_skip_enabled", v)}
      />

      <div className="mt-5 space-y-3">
        {(Object.values(["youtube", "soundcloud", "spotify"]) as string[]).map((src) => {
          const enabled = draft.allowed_sources.includes(src);
          return (
            <Toggle
              key={src}
              label={`Enable ${sourceLabel(src)}`}
              checked={enabled}
              onChange={(on) =>
                field(
                  "allowed_sources",
                  on ? [...draft.allowed_sources, src] : draft.allowed_sources.filter((s) => s !== src)
                )
              }
            />
          );
        })}
      </div>

      <button onClick={() => onSave(draft)} className="btn-primary mt-6 w-full">
        Save settings
      </button>
    </section>
  );
}

function NumberField({
  label,
  value,
  onChange,
  min,
  max,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium text-slate-500">{label}</span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number.parseInt(e.target.value || "0", 10))}
        className="input"
      />
    </label>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="mt-5 flex cursor-pointer items-start gap-3">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="peer sr-only" />
      <span className="mt-0.5 h-5 w-9 shrink-0 rounded-full bg-slate-700 p-0.5 transition peer-checked:bg-accent-500">
        <span className="block h-4 w-4 rounded-full bg-white transition peer-checked:translate-x-4" style={{ transform: checked ? "translateX(16px)" : undefined }} />
      </span>
      <span>
        <span className="block text-sm text-slate-200">{label}</span>
        {hint && <span className="block text-xs text-slate-500">{hint}</span>}
      </span>
    </label>
  );
}

// ---------------------------------------------------------------------------

function ConnectionsPanel({ flash }: { flash: (m: string) => void }) {
  const [connecting, setConnecting] = useState(false);

  async function connectSpotify() {
    setConnecting(true);
    try {
      const url = await startSpotifyConnect();
      window.location.assign(url);
    } catch (e) {
      flash(errorMessage(e));
      setConnecting(false);
    }
  }

  return (
    <section className="glass p-4 sm:p-6">
      <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
        Connections
      </h3>

      <div className="mt-4 space-y-3">
        <div className="flex items-center justify-between rounded-xl bg-slate-950/40 p-3">
          <div>
            <p className="text-sm font-medium text-slate-200">Spotify</p>
            <p className="text-xs text-slate-500">Search and resolve tracks via your account</p>
          </div>
          <button onClick={connectSpotify} disabled={connecting} className="btn-ghost shrink-0 text-xs">
            {connecting ? "..." : "Connect"}
          </button>
        </div>

        <div className="flex items-center justify-between rounded-xl bg-slate-950/40 p-3">
          <div>
            <p className="text-sm font-medium text-slate-200">OBS overlay</p>
            <p className="text-xs text-slate-500">Browser-source page that shows and plays music</p>
          </div>
          <CopyOverlayUrl label="Copy link" />
        </div>
      </div>

      <p className="mt-4 rounded-xl bg-slate-950/60 p-3 text-xs leading-relaxed text-slate-500">
        SoundCloud works automatically using its public web client id, no
        account connection needed. YouTube uses public Invidious/Piped mirrors.
      </p>
    </section>
  );
}

function CopyOverlayUrl({ label = "Copy overlay link" }: { label?: string }) {
  const [copied, setCopied] = useState(false);
  const streamerId = typeof window !== "undefined" ? getStreamerIdFromToken() : null;

  function copy() {
    if (!streamerId) return;
    const url = `${window.location.origin}/overlay/${streamerId}`;
    navigator.clipboard.writeText(url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <button onClick={copy} disabled={!streamerId} className="btn-ghost shrink-0 text-xs">
      {copied ? "Copied!" : label}
    </button>
  );
}

function OnboardingCard({ flash }: { flash: (m: string) => void }) {
  const [hidden, setHidden] = useState(true);

  useEffect(() => {
    setHidden(window.localStorage.getItem("tmb_setup_done") === "1");
  }, []);

  if (hidden) return null;

  function finish() {
    window.localStorage.setItem("tmb_setup_done", "1");
    setHidden(true);
    flash("You're all set . Have fun streaming!");
  }

  return (
    <section className="glass p-4 sm:p-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-accent-400">
            Getting started
          </h3>
          <p className="mt-1 text-sm text-slate-400">Three quick steps and song requests are live on your channel.</p>
        </div>
        <button onClick={finish} className="shrink-0 text-xs text-slate-500 transition hover:text-white">
          Hide
        </button>
      </div>
      <ol className="mt-4 grid gap-3 md:grid-cols-3">
        <li className="rounded-xl bg-slate-950/50 p-4">
          <p className="font-display text-xs font-semibold text-accent-400">1. Add the overlay to OBS</p>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-400">
            Paste your overlay link into a browser source. That page is where requested songs play.
          </p>
          <div className="mt-3">
            <CopyOverlayUrl />
          </div>
        </li>
        <li className="rounded-xl bg-slate-950/50 p-4">
          <p className="font-display text-xs font-semibold text-accent-400">2. Tell chat how to request</p>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-400">
            Drop <code className="rounded bg-slate-900 px-1 py-0.5 font-mono text-accent-400">!sr &lt;song&gt;</code> in a
            panel or your title so viewers know the command.
          </p>
        </li>
        <li className="rounded-xl bg-slate-950/50 p-4">
          <p className="font-display text-xs font-semibold text-accent-400">3. Tune your rules</p>
          <p className="mt-1.5 text-xs leading-relaxed text-slate-400">
            Set queue limits, cooldowns and filters under Request settings below.
          </p>
        </li>
      </ol>
    </section>
  );
}

function getStreamerIdFromToken(): string | null {
  try {
    const token = localStorage.getItem("tmb_access_token");
    if (!token) return null;
    const payload = JSON.parse(atob(token.split(".")[1])) as { sub?: string };
    return payload.sub ?? null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------

function HistoryPanel({ items }: { items: HistoryItem[] }) {
  return (
    <section className="glass p-4 sm:p-6">
      <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
        Recently played
      </h3>
      <ul className="mt-4 space-y-2">
        {items.map((item) => (
          <li key={item.history_id} className="flex items-center gap-3 rounded-xl bg-slate-950/40 p-2.5">
            <span className="shrink-0 text-xs text-slate-600">
              {new Date(item.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm text-slate-300">{item.title}</p>
              <p className="truncate text-xs text-slate-500">{item.artist}</p>
            </div>
            {item.was_skipped && <span className="shrink-0 text-xs text-slate-600">skipped</span>}
          </li>
        ))}
        {!items.length && <li className="py-8 text-center text-xs text-slate-600">No history yet</li>}
      </ul>
    </section>
  );
}

function BlockedUsersPanel({
  items,
  onChanged,
  flash,
}: {
  items: BlockedUser[];
  onChanged: () => Promise<void>;
  flash: (m: string) => void;
}) {
  const [userId, setUserId] = useState("");
  const [login, setLogin] = useState("");

  async function block() {
    try {
      await api("/api/v1/blocked-users", {
        method: "POST",
        body: JSON.stringify({ user_id: userId.trim(), user_login: login.trim() }),
      });
      setUserId("");
      setLogin("");
      await onChanged();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function unblock(id: string) {
    try {
      await api(`/api/v1/blocked-users/${encodeURIComponent(id)}`, { method: "DELETE" });
      await onChanged();
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  return (
    <section className="glass p-4 sm:p-6">
      <h3 className="font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
        Blocked users ({items.length})
      </h3>

      <form
        className="mt-4 flex flex-col gap-2 sm:flex-row"
        onSubmit={(e) => {
          e.preventDefault();
          void block();
        }}
      >
        <input value={login} onChange={(e) => setLogin(e.target.value)} placeholder="Twitch login" className="input" />
        <input value={userId} onChange={(e) => setUserId(e.target.value)} placeholder="Twitch user id" className="input" />
        <button type="submit" disabled={!userId.trim() || !login.trim()} className="btn-primary shrink-0">
          Block
        </button>
      </form>

      <ul className="mt-4 space-y-2">
        {items.map((u) => (
          <li key={u.user_id} className="flex items-center justify-between rounded-xl bg-slate-950/40 p-2.5">
            <div className="min-w-0">
              <p className="truncate text-sm text-slate-300">{u.user_login}</p>
              {u.reason && <p className="truncate text-xs text-slate-500">{u.reason}</p>}
            </div>
            <button onClick={() => unblock(u.user_id)} className="shrink-0 text-xs text-slate-500 hover:text-white">
              Unblock
            </button>
          </li>
        ))}
        {!items.length && (
          <li className="py-6 text-center text-xs text-slate-600">Nobody is blocked</li>
        )}
      </ul>
    </section>
  );
}
