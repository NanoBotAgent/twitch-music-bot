"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";

/**
 * OAuth lands here with tokens in the URL fragment:
 *   #access_token=...&refresh_token=...   (success)
 *   #error=<code>                          (failure)
 * Fragments never reach the server, keeping JWTs out of logs.
 */
export default function AuthCallbackPage() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fragment = new URLSearchParams(window.location.hash.slice(1));
    const accessToken = fragment.get("access_token");
    const refreshToken = fragment.get("refresh_token");
    const errorCode = fragment.get("error");

    if (accessToken && refreshToken) {
      window.localStorage.setItem("tmb_access_token", accessToken);
      window.localStorage.setItem("tmb_refresh_token", refreshToken);
      // Clear the fragment so tokens do not linger in the address bar.
      history.replaceState(null, "", "/dashboard");
      router.replace("/dashboard");
      return;
    }

    if (errorCode) {
      setError(errorDescription(errorCode));
      return;
    }

    setError("Missing login data. Please try again.");
  }, [router]);

  if (error) {
    return (
      <main className="flex min-h-screen items-center justify-center p-6">
        <div className="glass w-full max-w-md p-6 text-center sm:p-10">
          <h1 className="font-display text-xl font-bold text-white">Login failed</h1>
          <p className="mt-3 text-sm text-slate-400">{error}</p>
          <button onClick={() => router.replace("/")} className="btn-primary mt-8">
            Back to login
          </button>
        </div>
      </main>
    );
  }

  return (
    <main className="flex min-h-screen items-center justify-center">
      <div className="glass px-10 py-8 font-display text-sm text-slate-400">Signing you in...</div>
    </main>
  );
}

function errorDescription(code: string): string {
  switch (code) {
    case "oauth_denied":
      return "Twitch authorization was denied.";
    case "invalid_state":
      return "The login session expired. Please try again.";
    case "token_exchange_failed":
      return "Twitch rejected the login. Please try again.";
    case "helix_validation_failed":
      return "Could not verify your Twitch account.";
    case "persist_failed":
      return "Account creation failed. Please try again later.";
    case "not_supported":
      return "This provider is not supported yet.";
    default:
      return `Login failed (${code}).`;
  }
}
