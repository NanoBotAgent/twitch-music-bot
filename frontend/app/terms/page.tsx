import type { Metadata } from "next";
import Link from "next/link";
import SiteFooter from "@/components/site-footer";

export const metadata: Metadata = {
  title: "Terms of Service — Twitch Music Bot",
  description: "The ground rules for using Twitch Music Bot.",
};

const SECTIONS = [
  {
    heading: "The service",
    body: [
      "Twitch Music Bot is a free community project provided as-is, without warranties of any kind. Features may change and the service may be modified or discontinued at any time.",
    ],
  },
  {
    heading: "Your account",
    body: [
      "One account per streamer, tied to your own Twitch login. You are responsible for everything that happens under your account, including the settings you configure and the overlay URL you share.",
    ],
  },
  {
    heading: "Content responsibility",
    body: [
      "The bot only relays publicly available streams requested from chat. You remain responsible for making sure music played on your channel complies with the rights and licenses that apply to you and your platform's rules.",
    ],
  },
  {
    heading: "Acceptable use",
    body: [
      "Do not abuse the service: no attempts to disrupt other channels, bypass rate limits or moderation features, request illegal content, or otherwise use the bot in ways that harm other streamers or the platforms it relies on.",
    ],
  },
  {
    heading: "No affiliation",
    body: [
      "This project is independent and not affiliated with, endorsed by or sponsored by Twitch Interactive, Amazon, Google, YouTube, Spotify or SoundCloud.",
    ],
  },
  {
    heading: "Limitation of liability",
    body: [
      "To the maximum extent permitted by law, we are not liable for any damages arising from your use of (or inability to use) the service, including interrupted streams or lost queue data.",
    ],
  },
];

export default function TermsPage() {
  return (
    <div className="flex min-h-screen flex-col">
      <header className="border-b border-slate-800/80">
        <div className="mx-auto max-w-3xl px-4 py-5">
          <Link href="/" className="inline-flex items-center gap-2 text-sm text-slate-400 transition hover:text-white">
            ← Back to home
          </Link>
        </div>
      </header>
      <main className="mx-auto w-full max-w-3xl flex-1 px-4 py-12">
        <h1 className="font-display text-3xl font-bold text-white">Terms of Service</h1>
        <p className="mt-2 text-xs text-slate-500">Last updated: August 2026</p>
        <div className="mt-10 space-y-10">
          {SECTIONS.map((s) => (
            <section key={s.heading}>
              <h2 className="font-display text-lg font-semibold text-slate-100">{s.heading}</h2>
              <div className="mt-3 space-y-3">
                {s.body.map((p) => (
                  <p key={p.slice(0, 32)} className="text-sm leading-relaxed text-slate-400">
                    {p}
                  </p>
                ))}
              </div>
            </section>
          ))}
          <section>
            <h2 className="font-display text-lg font-semibold text-slate-100">Questions</h2>
            <p className="mt-3 text-sm leading-relaxed text-slate-400">
              Anything unclear? Ask in the{" "}
              <a
                href="https://github.com/NanoBotAgent/twitch-music-bot"
                target="_blank"
                rel="noopener noreferrer"
                className="text-accent-400 underline-offset-2 hover:underline"
              >
                GitHub repository
              </a>
              .
            </p>
          </section>
        </div>
      </main>
      <SiteFooter />
    </div>
  );
}
