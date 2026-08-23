import type { Metadata } from "next";
import Link from "next/link";
import SiteFooter from "@/components/site-footer";

export const metadata: Metadata = {
  title: "Privacy Policy | Twitch Music Bot",
  description: "What data Twitch Music Bot stores and how it is used.",
};

const SECTIONS = [
  {
    heading: "What we collect",
    body: [
      "Your public Twitch profile basics: user ID, username, display name and profile image.",
      "OAuth tokens for Twitch (and for Spotify, if you choose to connect it) so the bot can act on your behalf.",
      "Song requests, playback history and the bot settings you configure for your channel.",
    ],
  },
  {
    heading: "How your data is used",
    body: [
      "Tokens are stored encrypted at rest on our server and are used only to operate your bot: joining your chat, resolving requested songs and refreshing sessions.",
      "Your browser keeps a session token in local storage until you sign out. Nothing is shared across streamer accounts: each channel's queue, history and settings are isolated.",
    ],
  },
  {
    heading: "What we never do",
    body: [
      "We do not sell or share your data, show ads, or embed third-party analytics or tracking scripts anywhere in the product.",
    ],
  },
  {
    heading: "Deleting your data",
    body: [
      "Signing out removes your session from the browser. Revoking this app in your Twitch connections settings immediately invalidates the tokens we hold.",
      "To fully wipe your stored account data, open an issue on our GitHub repository and we will remove it.",
    ],
  },
];

export default function PrivacyPage() {
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
        <h1 className="font-display text-3xl font-bold text-white">Privacy Policy</h1>
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
            <h2 className="font-display text-lg font-semibold text-slate-100">Contact</h2>
            <p className="mt-3 text-sm leading-relaxed text-slate-400">
              Questions about this policy? Reach us via the{" "}
              <a
                href="https://github.com/NanoBotAgent/twitch-music-bot"
                target="_blank"
                rel="noopener noreferrer"
                className="text-accent-400 underline-offset-2 hover:underline"
              >
                project repository on GitHub
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
