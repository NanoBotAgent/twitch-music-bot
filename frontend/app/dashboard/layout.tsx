"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useRef, useState } from "react";
import Logo from "@/components/logo";
import SiteFooter from "@/components/site-footer";
import { clearTokens, getAccessToken, getMe } from "@/lib/api";
import type { Me } from "@/lib/types";

function AvatarFallback({ label }: { label: string }) {
  const parts = label.trim().split(/\s+/).filter(Boolean);
  const text = parts.length > 1 ? `${parts[0][0]}${parts[parts.length - 1][0]}` : label.slice(0, 2);
  return (
    <span className="flex h-8 w-8 items-center justify-center rounded-full border border-slate-700 bg-slate-800 text-xs font-semibold uppercase text-slate-300">
      {text}
    </span>
  );
}

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!getAccessToken()) {
      router.replace("/");
      return;
    }
    getMe()
      .then(setMe)
      .catch(() => {
        router.replace("/");
      });
  }, [router]);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenuOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  const signOut = useCallback(() => {
    clearTokens();
    router.replace("/");
  }, [router]);

  const displayName = me?.display_name ?? me?.login ?? "";
  const subtitle = me?.email ?? (me?.login ? `@${me.login}` : "");

  return (
    <div className="flex min-h-screen flex-col">
      <header className="sticky top-0 z-40 border-b border-slate-800/80 bg-surface/85 backdrop-blur-xl">
        <div className="mx-auto flex h-16 max-w-6xl items-center justify-between px-4">
          <Link href="/" className="flex items-center gap-2.5">
            <Logo size={26} />
            <span className="font-display text-base font-bold text-white">
              Twitch <span className="text-accent-400">Music Bot</span>
            </span>
          </Link>

          {me && (
            <div className="relative" ref={menuRef}>
              <button
                type="button"
                onClick={() => setMenuOpen((open) => !open)}
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                className="flex items-center gap-2 rounded-full border border-transparent px-1.5 py-1 transition-colors hover:border-slate-700 focus:outline-none"
              >
                {me.avatar_url ? (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img src={me.avatar_url} alt="" className="h-8 w-8 rounded-full border border-slate-700" />
                ) : (
                  <AvatarFallback label={displayName || "?"} />
                )}
                <span className="hidden max-w-[150px] truncate font-display text-sm text-slate-200 sm:block">
                  {displayName}
                </span>
                <svg
                  viewBox="0 0 12 12"
                  fill="none"
                  className={`h-3.5 w-3.5 shrink-0 text-slate-500 transition-transform duration-150 ${menuOpen ? "rotate-180" : ""}`}
                >
                  <path d="M2.5 4.5L6 8l3.5-3.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </button>

              {menuOpen && (
                <div
                  role="menu"
                  className="absolute right-0 top-[calc(100%+10px)] w-56 rounded-xl border border-slate-800 bg-slate-900/95 p-1.5 shadow-2xl shadow-black/50 backdrop-blur-xl"
                >
                  <div className="border-b border-slate-800 px-3 pb-2.5 pt-2">
                    <p className="truncate font-display text-sm text-white">{displayName}</p>
                    {subtitle && <p className="mt-0.5 truncate text-xs text-slate-500">{subtitle}</p>}
                  </div>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={signOut}
                    className="mt-1 flex w-full items-center justify-between rounded-lg px-3 py-2 text-sm font-medium text-rose-400 transition-colors hover:bg-rose-500/10"
                  >
                    Sign out
                    <svg viewBox="0 0 24 24" fill="none" className="h-4 w-4 shrink-0">
                      <path d="M15 17l5-5-5-5M20 12H9m3 9H6a2 2 0 01-2-2V5a2 2 0 012-2h6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </header>

      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-8">{children}</main>

      <SiteFooter />
    </div>
  );
}
