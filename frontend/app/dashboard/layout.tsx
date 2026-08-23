"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";
import Logo from "@/components/logo";
import SiteFooter from "@/components/site-footer";
import { clearTokens, getAccessToken, getMe } from "@/lib/api";
import type { Me } from "@/lib/types";

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);

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

  const signOut = useCallback(() => {
    clearTokens();
    router.replace("/");
  }, [router]);

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
          <div className="flex items-center gap-3">
            {me && (
              <>
                {me.avatar_url && (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img src={me.avatar_url} alt="" className="h-8 w-8 rounded-full border border-slate-700" />
                )}
                <div className="hidden sm:block">
                  <p className="text-[11px] uppercase tracking-wide text-slate-500">Signed in as</p>
                  <p className="-mt-0.5 font-display text-sm text-slate-200">{me.display_name ?? me.login}</p>
                </div>
              </>
            )}
            <button onClick={signOut} className="btn-ghost px-3 py-1.5 text-xs">
              Sign out
            </button>
          </div>
        </div>
      </header>

      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-8">{children}</main>

      <SiteFooter />
    </div>
  );
}
