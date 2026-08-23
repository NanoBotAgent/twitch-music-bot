import Link from "next/link";
import Logo from "./logo";

export default function SiteFooter() {
  return (
    <footer className="border-t border-slate-800/80">
      <div className="mx-auto grid max-w-6xl gap-6 px-4 py-10 sm:grid-cols-3 sm:items-center">
        <div className="flex items-center justify-center gap-2.5 sm:justify-start">
          <Logo size={22} />
          <span className="font-display text-sm font-semibold text-slate-300">Twitch Music Bot</span>
        </div>
        <nav className="flex flex-wrap justify-center gap-x-6 gap-y-2 text-xs text-slate-500">
          <a
            href="https://github.com/NanoBotAgent/twitch-music-bot"
            target="_blank"
            rel="noopener noreferrer"
            className="transition hover:text-slate-300"
          >
            Source on GitHub
          </a>
          <Link href="/privacy" className="transition hover:text-slate-300">
            Privacy
          </Link>
          <Link href="/terms" className="transition hover:text-slate-300">
            Terms
          </Link>
        </nav>
        <p className="text-center text-xs leading-relaxed text-slate-600 sm:text-right">
          Free community project. Not affiliated with Twitch, YouTube, Spotify or SoundCloud.
        </p>
      </div>
    </footer>
  );
}
