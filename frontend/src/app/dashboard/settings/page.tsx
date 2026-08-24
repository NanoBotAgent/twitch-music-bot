"use client";

import { useCallback, useEffect, useState } from "react";
import { api, ApiError } from "@/lib/api";
import type { StreamerConfig } from "@/lib/types";
import { ConfigPanel } from "@/components/config-panel";

export default function SettingsPage() {
  const [config, setConfig] = useState<StreamerConfig | null>(null);
  const [notice, setNotice] = useState<{ text: string; kind: "ok" | "err" } | null>(null);
  const [saving, setSaving] = useState(false);

  const flash = useCallback((message: string, kind: "ok" | "err" = "ok") => {
    setNotice({ text: message, kind });
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [notice]);

  const load = useCallback(async () => {
    try {
      setConfig(await api<StreamerConfig>("/api/v1/config"));
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) window.location.assign("/");
      else flash(e instanceof ApiError ? e.message : "Could not load settings", "err");
    }
  }, [flash]);

  useEffect(() => {
    void load();
  }, [load]);

  async function save(next: StreamerConfig) {
    setSaving(true);
    try {
      const saved = await api<StreamerConfig>("/api/v1/config", {
        method: "PUT",
        body: JSON.stringify(next),
      });
      setConfig(saved);
      flash("Settings saved");
    } catch (e) {
      flash(e instanceof ApiError ? e.message : "Could not save settings", "err");
    } finally {
      setSaving(false);
    }
  }

  return (
    <main className="grid gap-6">
      {notice && (
        <div
          role="status"
          className={`fixed inset-x-4 bottom-[max(1rem,env(safe-area-inset-bottom))] z-50 mx-auto max-w-md rounded-xl border px-5 py-3 text-center text-sm shadow-xl backdrop-blur-md sm:left-auto sm:right-6 ${
            notice.kind === "err"
              ? "border-rose-500/40 bg-rose-950/90 text-rose-200"
              : "border-accent-500/40 bg-slate-900/90 text-accent-300"
          }`}
        >
          {notice.text}
        </div>
      )}

      <header className="glass p-4 sm:p-6">
        <h1 className="font-display text-lg font-semibold text-white">Settings</h1>
        <p className="mt-1 text-sm text-slate-400">
          Tune how song requests behave on your channel.
        </p>
      </header>

      {config && <ConfigPanel config={config} onSave={save} saving={saving} />}
    </main>
  );
}