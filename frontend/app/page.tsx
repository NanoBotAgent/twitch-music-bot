"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { getAccessToken, startTwitchLogin } from "@/lib/api";

export default function LandingPage() {
  const router = useRouter();
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    // Already logged in? Go straight to the dashboard.
    if (getAccessToken()) {
      router.replace("/dashboard");
    }
  }, [router]);

  async function login() {
    setLoading(true);
    try {
      const url = await startTwitchLogin();
      window.location.assign(url);
    } catch {
      setLoading(false);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center p-6">
      <div className="glass w-full max-w-md p-10 text-center">
        <h1 className="font-display text-3xl font-bold text-white">
          Twitch <span className="text-accent-400">Music Bot</span>
        </h1>
        <p className="mt-4 text-sm leading-relaxed text-slate-400">
          Let your chat request songs with !sr. Manage the queue, connect
          Spotify, and drop a browser-source overlay into OBS.
        </p>
        <button onClick={login} disabled={loading} className="btn-primary mt-8 w-full py-3 text-base">
          {loading ? "Redirecting..." : "Log in with Twitch"}
        </button>
      </div>
    </main>
  );
}
