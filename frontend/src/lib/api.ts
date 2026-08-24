const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";
const TOKEN_KEY = "tmb_access_token";
const REFRESH_KEY = "tmb_refresh_token";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export function getAccessToken(): string | null {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(TOKEN_KEY);
}

export function getRefreshToken(): string | null {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(REFRESH_KEY);
}

export function setTokens(access: string, refresh: string): void {
  window.localStorage.setItem(TOKEN_KEY, access);
  window.localStorage.setItem(REFRESH_KEY, refresh);
}

export function clearTokens(): void {
  window.localStorage.removeItem(TOKEN_KEY);
  window.localStorage.removeItem(REFRESH_KEY);
}

interface Envelope<T> {
  success: boolean;
  data?: T;
  error?: { code: string; message: string };
}

async function refreshTokens(): Promise<boolean> {
  const refresh = getRefreshToken();
  if (!refresh) return false;

  try {
    const res = await fetch(`${API_URL}/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refresh }),
    });
    if (!res.ok) return false;
    const body = (await res.json()) as Envelope<{ access_token: string; refresh_token: string }>;
    if (!body.success || !body.data) return false;
    setTokens(body.data.access_token, body.data.refresh_token);
    return true;
  } catch {
    return false;
  }
}

/** Authenticated fetch with one transparent token-refresh retry on 401. */
export async function api<T>(path: string, init: RequestInit = {}, retried = false): Promise<T> {
  const token = getAccessToken();
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...init.headers,
    },
  });

  if (res.status === 401 && !retried) {
    const refreshed = await refreshTokens();
    if (refreshed) return api<T>(path, init, true);
    clearTokens();
    throw new ApiError(401, "SESSION_EXPIRED", "Please log in again");
  }

  let envelope: Envelope<T>;
  try {
    envelope = (await res.json()) as Envelope<T>;
  } catch {
    throw new ApiError(res.status, "BAD_RESPONSE", `Unexpected response (${res.status})`);
  }

  if (!res.ok || !envelope.success) {
    const err = envelope.error ?? { code: "UNKNOWN", message: res.statusText };
    throw new ApiError(res.status, err.code, err.message);
  }

  return envelope.data as T;
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

export async function startTwitchLogin(): Promise<string> {
  const res = await fetch(`${API_URL}/auth/twitch`, { method: "POST" });
  const body = (await res.json()) as Envelope<{ authorize_url: string }>;
  if (!body.success || !body.data) throw new ApiError(res.status, "LOGIN_FAILED", "Could not start login");
  return body.data.authorize_url;
}

export async function startSpotifyConnect(): Promise<string> {
  const data = await api<{ authorize_url: string }>("/auth/spotify/start", { method: "POST" });
  return data.authorize_url;
}

export async function disconnectProvider(provider: string): Promise<void> {
  await api(`/auth/${provider}`, { method: "DELETE" });
}

export async function getMe(): Promise<import("./types").Me> {
  return api<import("./types").Me>("/auth/me");
}

export async function getConfig(): Promise<import("./types").StreamerConfig> {
  return api<import("./types").StreamerConfig>("/api/v1/config");
}

export async function updateConfig(
  config: import("./types").StreamerConfig
): Promise<import("./types").StreamerConfig> {
  return api<import("./types").StreamerConfig>("/api/v1/config", {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export function wsUrl(path: string): string {
  return `${API_URL.replace(/^http/, "ws")}${path}`;
}
