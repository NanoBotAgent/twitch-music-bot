"use client";

import { useCallback, useEffect, useState } from "react";
import { api, ApiError, getConfig, updateConfig } from "@/lib/api";
import type { StreamerConfig } from "@/lib/types";

export default function SettingsPage() {
  const [config, setConfig] = useState<StreamerConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<{ text: string; kind: "ok" | "err" } | null>(null);

  const flash = useCallback((text: string, kind: "ok" | "err" = "ok") => {
    setNotice({ text, kind });
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [notice]);

  async function loadConfig() {
    try {
      const cfg = await getConfig();
      setConfig(cfg);
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) window.location.assign("/");
    }
  }

  useEffect(() => {
    void loadConfig();
  }, []);

  async function saveConfig(next: StreamerConfig) {
    setSaving(true);
    try {
      const saved = await updateConfig(next);
      setConfig(saved);
      flash("Settings saved");
    } catch (e) {
      flash(errorMessage(e), "err");
    } finally {
      setSaving(false);
    }
  }

  if (!config) {
    return (
      <main className="flex items-center justify-center min-h-[40vh]">
        <p className="text-slate-500">Loading…</p>
      </main>
    );
  }

  return (
    <main className="mx-auto max-w-2xl space-y-6">
      {notice && (
        <div
          role="status"
          className={`fixed inset-x-4 bottom-[max(1rem,env(safe-area-inset-bottom))] z-50 mx-auto max-w-md rounded-xl border px-5 py-3 text-center text-sm shadow-xl backdrop-blur-md ${
            notice.kind === "err"
              ? "border-rose-500/40 bg-rose-950/90 text-rose-200"
              : "border-accent-500/40 bg-slate-900/90 text-accent-300"
          }`}
        >
          {notice.text}
        </div>
      )}

      <section className="glass p-4 sm:p-6">
        <header className="flex items-center justify-between">
          <h2 className="font-display text-lg font-semibold text-white">Settings</h2>
          <a
            href="/dashboard"
            className="rounded-lg border border-slate-700 px-3 py-1.5 text-xs font-medium text-slate-300 transition-colors hover:border-slate-600 hover:bg-slate-800/50"
          >
            Back to dashboard
          </a>
        </header>

        <h3 className="mt-6 font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Request settings
        </h3>

        <div className="mt-4 grid gap-4 sm:grid-cols-2">
          <NumberField
            label="Max queue size"
            value={config.max_queue_size}
            min={1}
            max={500}
            onChange={(v) => setConfig({ ...config, max_queue_size: v })}
          />
          <NumberField
            label="Songs per user"
            value={config.max_requests_per_user}
            min={1}
            max={50}
            onChange={(v) => setConfig({ ...config, max_requests_per_user: v })}
          />
          <NumberField
            label="Cooldown (seconds)"
            value={config.request_cooldown_seconds}
            min={0}
            max={3600}
            onChange={(v) => setConfig({ ...config, request_cooldown_seconds: v })}
          />
          <NumberField
            label="Auto-skip after (seconds)"
            value={config.auto_skip_after_seconds ?? 0}
            min={0}
            max={3600}
            onChange={(v) => setConfig({ ...config, auto_skip_after_seconds: v || null })}
          />
        </div>

        <label className="mt-4 block">
          <span className="mb-1.5 block text-xs font-medium text-slate-500">Explicit content</span>
          <select
            value={config.explicit_filter}
            onChange={(e) => setConfig({ ...config, explicit_filter: e.target.value })}
            className="input"
          >
            <option value="allow">Allow</option>
            <option value="block">Block</option>
          </select>
        </label>

        <Toggle
          label="Vote skip"
          hint="Chat can vote to skip with !voteskip"
          checked={config.vote_skip_enabled}
          onChange={(v) => setConfig({ ...config, vote_skip_enabled: v })}
        />

        <h3 className="mt-8 font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Allowed sources
        </h3>

        <div className="mt-4 space-y-3">
          {(["youtube", "soundcloud", "spotify"] as const).map((src) => {
            const enabled = config.allowed_sources.includes(src);
            return (
              <Toggle
                key={src}
                label={sourceLabel(src)}
                checked={enabled}
                onChange={(on) =>
                  setConfig({
                    ...config,
                    allowed_sources: on
                      ? [...config.allowed_sources, src]
                      : config.allowed_sources.filter((s) => s !== src),
                  })
                }
              />
            );
          })}
        </div>

        <h3 className="mt-8 font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Blocked items
        </h3>

        <div className="mt-4 space-y-4">
          <TextareaField
            label="Blocked artists (one per line)"
            value={config.blocked_artists.join("\n")}
            onChange={(v) => setConfig({ ...config, blocked_artists: v.split("\n").map((s) => s.trim()).filter(Boolean) })}
          />
          <TextareaField
            label="Blocked keywords (one per line)"
            value={config.blocked_keywords.join("\n")}
            onChange={(v) => setConfig({ ...config, blocked_keywords: v.split("\n").map((s) => s.trim()).filter(Boolean) })}
          />
        </div>

        <h3 className="mt-8 font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Playback
        </h3>

        <div className="mt-4 space-y-5">
          <SliderField
            label="Default volume"
            value={config.default_volume}
            min={0}
            max={1}
            step={0.05}
            onChange={(v) => setConfig({ ...config, default_volume: v })}
            format={(v) => `${Math.round(v * 100)}%`}
          />
          <SliderField
            label="Crossfade"
            value={config.crossfade_seconds}
            min={0}
            max={10}
            step={0.5}
            onChange={(v) => setConfig({ ...config, crossfade_seconds: v })}
            format={(v) => `${v}s`}
          />
        </div>

        <h3 className="mt-8 font-display text-sm font-semibold uppercase tracking-wider text-slate-400">
          Fuzzy matching
        </h3>

        <div className="mt-4">
          <NumberField
            label="Fuzzy match threshold (0-1)"
            value={Math.round(config.fuzzy_match_threshold * 100)}
            min={0}
            max={100}
            onChange={(v) => setConfig({ ...config, fuzzy_match_threshold: v / 100 })}
          />
          <p className="mt-1.5 text-xs text-slate-500">
            Lower = stricter matching. Higher = more permissive. Default ~0.6.
          </p>
        </div>

        <button
          onClick={() => saveConfig(config)}
          disabled={saving}
          className="btn-primary mt-8 w-full"
        >
          {saving ? "Saving…" : "Save all settings"}
        </button>
      </section>
    </main>
  );
}

function errorMessage(e: unknown): string {
  return e instanceof ApiError ? e.message : "Something went wrong";
}

function sourceLabel(src: string): string {
  return src.charAt(0).toUpperCase() + src.slice(1);
}

function NumberField({
  label,
  value,
  onChange,
  min,
  max,
  step = 1,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium text-slate-500">{label}</span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => {
          const v = Number.parseFloat(e.target.value);
          if (!Number.isNaN(v)) onChange(v);
        }}
        className="input"
      />
    </label>
  );
}

function SliderField({
  label,
  value,
  onChange,
  min,
  max,
  step = 0.01,
  format,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  format?: (value: number) => string;
}) {
  return (
    <label className="block">
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-xs font-medium text-slate-500">{label}</span>
        <span className="text-xs font-mono text-accent-400">{format ? format(value) : value}</span>
      </div>
      <input
        type="range"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => onChange(Number.parseFloat(e.target.value))}
        className="w-full h-2 appearance-none bg-slate-800 rounded-full accent-accent-500"
      />
    </label>
  );
}

function TextareaField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium text-slate-500">{label}</span>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={4}
        className="input w-full resize-y"
        placeholder="One per line…"
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
        <span className="block h-4 w-4 rounded-full bg-white transition peer-checked:translate-x-4" />
      </span>
      <span>
        <span className="block text-sm text-slate-200">{label}</span>
        {hint && <span className="block text-xs text-slate-500">{hint}</span>}
      </span>
    </label>
  );
}