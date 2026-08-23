import json
import os
import time
import urllib.error
import urllib.request
import base64

API = "https://api.alwaysdata.com/v1"
SITE_ID = 1070074
PUBLIC_URL = "https://twitch-bot.alwaysdata.net"
PORT = 8380

KEY = os.environ["AD_API_KEY"]
AD_SSH_USER = os.environ.get("AD_ACCOUNT", "twitch-bot")
AUTH = base64.b64encode(f"{KEY}:".encode()).decode()


def call(path, method="GET", data=None):
    req = urllib.request.Request(
        f"{API}{path}",
        method=method,
        data=json.dumps(data).encode() if data is not None else None,
        headers={"Authorization": f"Basic {AUTH}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req) as r:
            body = r.read().decode()
            return r.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:400]


DIRECTIVES = "\n".join([
    "ProxyPreserveHost On",
    'ProxyPass "/ws/overlay" "ws://services-twitch-bot.alwaysdata.net:%d/ws/overlay"' % PORT,
    'ProxyPassReverse "/ws/overlay" "ws://services-twitch-bot.alwaysdata.net:%d/ws/overlay"' % PORT,
    'ProxyPass "/api/v1/overlay" "ws://services-twitch-bot.alwaysdata.net:%d/api/v1/overlay"' % PORT,
    'ProxyPassReverse "/api/v1/overlay" "ws://services-twitch-bot.alwaysdata.net:%d/api/v1/overlay"' % PORT,
    'ProxyPass "/" "http://services-twitch-bot.alwaysdata.net:%d/"' % PORT,
    'ProxyPassReverse "/" "http://services-twitch-bot.alwaysdata.net:%d/"' % PORT,
    'RequestHeader set X-Forwarded-Proto "https"',
])

env_parts = [
    "ENVIRONMENT=production",
    "RUST_LOG=info",
    "APP__SERVER__HOST=[::]",
    f"APP__SERVER__PORT={PORT}",
    f"APP__DATABASE__URL={os.environ['NEON_DATABASE_URL']}",
    f"APP__SECURITY__JWT_SECRET={os.environ['JWT_SECRET']}",
    f"APP__SECURITY__ENCRYPTION_KEY={os.environ['ENCRYPTION_KEY']}",
]
spotify_id = os.environ.get("SPOTIFY_CLIENT_ID", "")
spotify_secret = os.environ.get("SPOTIFY_CLIENT_SECRET", "")
if spotify_id.strip() and spotify_secret.strip():
    env_parts.append(f"APP__SPOTIFY__CLIENT_ID={spotify_id}")
    env_parts.append(f"APP__SPOTIFY__CLIENT_SECRET={spotify_secret}")
else:
    print("Spotify credentials not set; skipping APP__SPOTIFY__* env vars")
twitch_id = os.environ.get("TWITCH_CLIENT_ID", "")
twitch_secret = os.environ.get("TWITCH_CLIENT_SECRET", "")
if twitch_id.strip() and twitch_secret.strip():
    env_parts.append(f"APP__TWITCH__CLIENT_ID={twitch_id}")
    env_parts.append(f"APP__TWITCH__CLIENT_SECRET={twitch_secret}")
    env_parts.append(f"APP__TWITCH__REDIRECT_URI=https://twitch-bot.alwaysdata.net/auth/twitch/callback")
else:
    print("Twitch credentials not set; skipping APP__TWITCH__* env vars")
env_string = " ".join(env_parts)

service_payload = {
    "name": "backend",
    "ssh_user": None,
    "working_directory": "app",
    "command": "sh -c 'chmod +x ./twitch-music-bot && exec ./twitch-music-bot'",
    "environment": env_string,
    "check_health_command": f"curl -fsS http://localhost:{PORT}/health || curl -fsS http://ip6-localhost:{PORT}/health",
}

st, ssh_users = call("/ssh/")
if st != 200 or not isinstance(ssh_users, list):
    raise SystemExit(f"failed to list ssh users: {st} {ssh_users}")
ssh_id = next((u["id"] for u in ssh_users if u.get("name") == AD_SSH_USER), None)
if ssh_id is None:
    raise SystemExit(f"ssh user {AD_SSH_USER!r} not found")
service_payload["ssh_user"] = ssh_id

st, _ = call(f"/site/{SITE_ID}/", "PATCH", {
    "vhost_additional_directives": DIRECTIVES,
    "ssl_force": True,
})
print("site patch:", st)

st, svcs = call("/service/")
if st != 200 or not isinstance(svcs, list):
    raise SystemExit(f"failed to list services: {st} {svcs}")


existing = [s for s in svcs if s.get("command") == service_payload["command"]]
if existing:
    sid = existing[0]["id"]
    st, cur = call(f"/service/{sid}/")
    if st == 200:
        managed = {p.split("=", 1)[0] for p in env_string.split() if "=" in p}
        preserved = []
        for pair in (cur.get("environment") or "").split():
            k = pair.split("=", 1)[0]
            if "=" in pair and k not in managed:
                preserved.append(pair)
                managed.add(k)
        if preserved:
            print("preserving existing env keys:", [p.split("=", 1)[0] for p in preserved])
            env_string = env_string + " " + " ".join(preserved)
            service_payload["environment"] = env_string
    else:
        print(f"warning: could not read current service ({st}); overwriting environment")
    st, resp = call(f"/service/{sid}/", "PATCH", service_payload)
    print(f"service patch {sid}:", st)
    if st >= 400:
        raise SystemExit(f"service patch failed")
else:
    st, resp = call("/service/", "POST", service_payload)
    print("service create:", st, str(resp)[:300])
    if st >= 400:
        raise SystemExit("service create failed")
    st2, svcs2 = call("/service/")
    cands = [s for s in (svcs2 if isinstance(svcs2, list) else [])
             if s.get("command") == service_payload["command"]]
    if not cands:
        raise SystemExit("created service not found after listing")
    sid = max(s["id"] for s in cands)

if not isinstance(sid, int):
    raise SystemExit(f"no service id: {resp}")

st, _ = call(f"/service/{sid}/restart/", "POST")
print("service restart:", st)

for i in range(30):
    time.sleep(5)
    try:
        with urllib.request.urlopen(f"{PUBLIC_URL}/health", timeout=10) as r:
            print(f"health: {r.status} after ~{(i + 1) * 5}s")
            print(r.read().decode()[:200])
            raise SystemExit(0)
    except SystemExit:
        raise
    except Exception as e:
        print(f"[{(i + 1) * 5}s] waiting: {e}")

raise SystemExit("health check failed after 150s")
