"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { getAccessToken, getMe } from "@/lib/api";
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

  return (
    <div className="mx-auto max-w-6xl px-4 py-8">
      <header className="mb-8 flex items-center justify-between">
        <div>
          <h1 className="font-display text-xl font-bold text-white">
            Twitch <span className="text-accent-400">Music Bot</span>
          </h1>
          <p className="mt-0.5 text-xs text-slate-500">Song request dashboard</p>
        </div>
        {me && (
          <div className="flex items-center gap-3">
            {me.avatar_url && (
              // eslint-disable-next-line @next/next/no-img-element
              <img src={me.avatar_url} alt="" className="h-9 w-9 rounded-full border border-slate-700" />
            )}
            <span className="font-display text-sm text-slate-300">{me.display_name ?? me.login}</span>
          </div>
        )}
      </header>
      {children}
    </div>
  );
}
