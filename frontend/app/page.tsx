"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import Logo from "@/components/logo";
import SiteFooter from "@/components/site-footer";
import { getAccessToken, startTwitchLogin } from "@/lib/api";

const FEATURES = [
  {
    title: "Requests from chat",
    body: "Viewers queue songs with !sr. No accounts, no links to click. If they can type, they can request.",
    icon: "!sr",
  },
  {
    title: "Plays right in OBS",
    body: "A personal browser-source overlay renders and plays the queue locally, so the audio goes straight into your stream.",
    icon: "▶",
  },
  {
    title: "YouTube and SoundCloud built in",
    body: "Works out of the box through public mirrors. Connect your own Spotify for track search if you want it.",
    icon: "♪",
  },
  {
    title: "You run the queue",
    body: "Reorder, remove, skip or clear everything from a dashboard only you can see.",
    icon: "≡",
  },
  {
    title: "Moderation built in",
    body: "Per-user cooldowns, queue caps, explicit-content filter, vote skip and blocked users.",
    icon: "⛨",
  },
  {
    title: "Your channel, your space",
    body: "Log in with your own Twitch account and get an isolated queue, settings and overlay URL. Nothing is shared between channels.",
    icon: "@",
  },
];

const STEPS = [
  {
    title: "Log in with Twitch",
    body: "One click with your existing Twitch account, no passwords here. Your dashboard is tied to your channel.",
  },
  {
    title: "Add the overlay to OBS",
    body: "Paste your personal overlay URL into a browser source. That page is where requested songs play.",
  },
  {
    title: "Chat starts requesting",
    body: 'Viewers type !sr followed by a song name or link. You watch everything live and stay in control.',
  },
];

const FAQS = [
  {
    q: "Does it cost anything?",
    a: "No. The whole service is free: you only need a Twitch account to claim your dashboard and overlay.",
  },
  {
    q: "Do my viewers need an account?",
    a: "Never. Anyone watching your stream can request by typing in chat. Requests are matched against your settings like cooldowns and filters automatically.",
  },
  {
    q: "Where does the music come from?",
    a: "Public YouTube mirrors (Invidious/Piped) and SoundCloud's web player work without any setup. You can optionally connect your own Spotify account for richer search.",
  },
  {
    q: "Can other streamers see my queue?",
    a: "No. Every Twitch account gets its own isolated queue, history, settings and overlay URL. Other channels have no way to see or touch yours.",
  },
  {
    q: "What about age-restricted videos?",
    a: "Those can't be resolved without logging into YouTube, so requests that resolve to them are rejected. Everything else works normally.",
  },
];

export default function LandingPage() {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openFaq, setOpenFaq] = useState<number | null>(0);

  useEffect(() => {
    // Already logged in? Go straight to your dashboard.
    if (getAccessToken()) {
      router.replace("/dashboard");
    }
  }, [router]);

  async function login() {
    setLoading(true);
    setError(null);
    try {
      const url = await startTwitchLogin();
      window.location.assign(url);
    } catch {
      setError("Could not start login. The server may still be starting up, try again in a minute.");
      setLoading(false);
    }
  }

  return (
    <div className="relative min-h-screen overflow-x-hidden">
      <div
        aria-hidden
        className="pointer-events-none absolute -top-44 left-1/2 h-[420px] w-[720px] max-w-none -translate-x-1/2 rounded-full bg-accent-500/10 blur-[120px]"
      />

      <header className="sticky top-0 z-40 border-b border-slate-800/80 bg-surface/85 backdrop-blur-xl">
        <div className="mx-auto flex h-16 max-w-6xl items-center justify-between px-4">
          <a href="#top" className="flex items-center gap-2.5">
            <Logo size={26} />
            <span className="font-display text-base font-bold text-white">
              Twitch <span className="text-accent-400">Music Bot</span>
            </span>
          </a>
          <nav className="hidden items-center gap-8 text-sm text-slate-400 md:flex">
            <a href="#features" className="transition hover:text-white">Features</a>
            <a href="#how" className="transition hover:text-white">How it works</a>
            <a href="#commands" className="transition hover:text-white">Commands</a>
            <a href="#faq" className="transition hover:text-white">FAQ</a>
          </nav>
          <button onClick={login} disabled={loading} className="btn-primary px-4 py-2 text-sm">
            Log in
          </button>
        </div>
      </header>

      <main id="top">
        {/* Hero */}
        <section className="mx-auto grid max-w-6xl items-center gap-12 px-4 pb-20 pt-20 lg:grid-cols-2 lg:pt-28">
          <div>
            <p className="inline-flex rounded-full border border-accent-500/30 bg-accent-500/10 px-3 py-1 text-xs font-medium text-accent-400">
              Free · Open source · One account per channel
            </p>
            <h1 className="mt-6 font-display text-4xl font-bold leading-tight text-white sm:text-5xl">
              Song requests, <span className="text-accent-400">straight from your chat</span>.
            </h1>
            <p className="mt-5 max-w-xl text-base leading-relaxed text-slate-400">
              Viewers type !sr and their pick lands in a queue only you control.
              Music plays through a browser-source overlay in OBS: no voice
              channels, no bots sitting in your chat client, nothing to install.
            </p>
            <div className="mt-8 flex flex-wrap items-center gap-4">
              <button onClick={login} disabled={loading} className="btn-primary px-6 py-3 text-base">
                {loading ? "Redirecting..." : "Log in with Twitch"}
              </button>
              <a href="#how" className="btn-ghost px-5 py-3 text-sm">
                See how it works
              </a>
            </div>
            {error && <p className="mt-3 text-sm text-red-400">{error}</p>}
          </div>

          {/* Chat mock */}
          <div className="glass mx-auto w-full max-w-md p-5 lg:max-w-none">
            <div className="flex items-center gap-2 border-b border-slate-800 pb-3">
              <span className="h-2.5 w-2.5 rounded-full bg-red-500/70" />
              <span className="h-2.5 w-2.5 rounded-full bg-yellow-500/70" />
              <span className="h-2.5 w-2.5 rounded-full bg-emerald-500/70" />
              <span className="ml-2 text-xs text-slate-500">Channel point rewards? Nah, just chat.</span>
            </div>
            <div className="space-y-3 pt-4 text-sm">
              <p className="leading-relaxed">
                <span className="font-semibold text-sky-400">pixel_paul:</span>{" "}
                <span className="text-slate-200">!sr darude sandstorm</span>
              </p>
              <p className="rounded-lg bg-accent-500/10 px-3 py-2 leading-relaxed text-accent-400">
                Queued: Darude - Sandstorm · position #3
              </p>
              <p className="leading-relaxed">
                <span className="font-semibold text-purple-400">luna_streams:</span>{" "}
                <span className="text-slate-200">!voteskip</span>
              </p>
              <p className="rounded-lg bg-slate-950/60 px-3 py-2 leading-relaxed text-xs text-slate-400">
                Vote skip counted (2/5 needed)
              </p>
              <p className="leading-relaxed">
                <span className="font-semibold text-rose-400">dj_waffles:</span>{" "}
                <span className="text-slate-200">!sr https://youtu.be/dQw4w9WgXcQ</span>
              </p>
              <p className="rounded-lg bg-accent-500/10 px-3 py-2 leading-relaxed text-accent-400">
                Queued: Rick Astley - Never Gonna Give You Up · position #4
              </p>
            </div>
            <div className="mt-4 flex items-center gap-2 rounded-full border border-slate-700 px-4 py-2 text-xs text-slate-600">
              Send a message
              <span className="ml-auto h-4 w-px animate-pulse bg-slate-500" />
            </div>
          </div>
        </section>

        {/* Features */}
        <section id="features" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-16">
          <h2 className="font-display text-2xl font-bold text-white sm:text-3xl">Everything a song request bot should do</h2>
          <p className="mt-3 max-w-2xl text-sm leading-relaxed text-slate-400">
            Built for streamers who want chat-driven music without babysitting another app.
          </p>
          <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {FEATURES.map((f) => (
              <div key={f.title} className="glass p-5 transition hover:border-slate-700">
                <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-accent-500/15 font-display text-sm font-bold text-accent-400">
                  {f.icon}
                </span>
                <h3 className="mt-4 font-display text-base font-semibold text-white">{f.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-slate-400">{f.body}</p>
              </div>
            ))}
          </div>
        </section>

        {/* How it works */}
        <section id="how" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-16">
          <h2 className="font-display text-2xl font-bold text-white sm:text-3xl">Live in about two minutes</h2>
          <div className="mt-8 grid gap-4 md:grid-cols-3">
            {STEPS.map((s, i) => (
              <div key={s.title} className="glass relative p-6">
                <span className="font-display text-4xl font-bold text-accent-500/25">{String(i + 1).padStart(2, "0")}</span>
                <h3 className="mt-3 font-display text-lg font-semibold text-white">{s.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-slate-400">{s.body}</p>
              </div>
            ))}
          </div>
        </section>

        {/* Commands */}
        <section id="commands" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-16">
          <h2 className="font-display text-2xl font-bold text-white sm:text-3xl">Chat commands</h2>
          <div className="glass mt-8 divide-y divide-slate-800/80">
            <CommandRow cmd="!sr &lt;song&gt;" aliases="!songrequest · !playsong" desc="Request a song by name or paste a YouTube / SoundCloud link." />
            <CommandRow cmd="!voteskip" aliases="!skip" desc="Vote to skip the current song when vote skip is enabled." />
          </div>
          <p className="mt-4 text-xs text-slate-500">
            Commands only work in your own channel, and every rule you set (cooldowns, caps, filters) is enforced before a song lands in the queue.
          </p>
        </section>

        {/* FAQ */}
        <section id="faq" className="mx-auto max-w-3xl scroll-mt-20 px-4 py-16">
          <h2 className="font-display text-2xl font-bold text-white sm:text-3xl">Questions streamers ask</h2>
          <div className="mt-8 space-y-3">
            {FAQS.map((item, i) => (
              <div key={item.q} className="glass overflow-hidden">
                <button
                  onClick={() => setOpenFaq(openFaq === i ? null : i)}
                  className="flex w-full items-center justify-between gap-4 px-5 py-4 text-left"
                >
                  <span className="font-display text-sm font-semibold text-slate-200">{item.q}</span>
                  <span className={`shrink-0 text-accent-400 transition-transform ${openFaq === i ? "rotate-45" : ""}`}>+</span>
                </button>
                {openFaq === i && (
                  <p className="border-t border-slate-800/80 px-5 py-4 text-sm leading-relaxed text-slate-400">{item.a}</p>
                )}
              </div>
            ))}
          </div>
        </section>

        {/* Final CTA */}
        <section className="mx-auto max-w-6xl px-4 pb-24 pt-8">
          <div className="glass relative overflow-hidden p-10 text-center">
            <div aria-hidden className="pointer-events-none absolute inset-x-0 -top-24 mx-auto h-48 w-96 rounded-full bg-accent-500/15 blur-[80px]" />
            <h2 className="relative font-display text-2xl font-bold text-white sm:text-3xl">Ready to let chat pick the music?</h2>
            <p className="relative mt-3 text-sm text-slate-400">Free forever for any channel. Your queue stays yours.</p>
            <button onClick={login} disabled={loading} className="btn-primary relative mt-7 px-7 py-3 text-base">
              {loading ? "Redirecting..." : "Claim your channel"}
            </button>
            {error && <p className="relative mt-3 text-sm text-red-400">{error}</p>}
          </div>
        </section>
      </main>

      <SiteFooter />
    </div>
  );
}

function CommandRow({ cmd, aliases, desc }: { cmd: string; aliases: string; desc: string }) {
  return (
    <div className="flex flex-col gap-2 p-5 sm:flex-row sm:items-center sm:gap-6">
      <code className="w-fit shrink-0 rounded-lg bg-slate-950/70 px-3 py-1.5 font-mono text-sm text-accent-400">{cmd}</code>
      <p className="flex-1 text-sm leading-relaxed text-slate-400">{desc}</p>
      <span className="shrink-0 text-xs text-slate-600">also: {aliases}</span>
    </div>
  );
}
