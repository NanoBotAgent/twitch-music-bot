"use client";

import { useState } from "react";
import type { StreamerConfig } from "@/lib/types";

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

export function ConfigPanel({
  config,
  onSave,
  saving = false,
}: {
  config: StreamerConfig;
  onSave: (config: StreamerConfig) => void;
  saving?: boolean;
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

      <button onClick={() => onSave(draft)} disabled={saving} className="btn-primary mt-6 w-full">
        {saving ? "Saving..." : "Save settings"}
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

export function Toggle({
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