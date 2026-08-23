#!/usr/bin/env python3
"""Deploy the Next.js frontend to Vercel via the REST files API.

Reads VERCEL_TOKEN from /home/applepie69/nanobot/.env.
Uploads every file under frontend/ (excluding node_modules/.next),
creates a production deployment, waits for READY, and re-points the
production aliases.

Usage:
  python3 scripts/vercel_deploy.py            # full deploy + alias
  python3 scripts/vercel_deploy.py --inspect  # just show latest deployment file layout
"""

import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request

FRONTEND_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "frontend")
ENV_FILE = "/home/applepie69/nanobot/.env"
PROJECT_ID = "prj_eN6bSylZ0cCoiMhwrg8ptU47z2qf"
TEAM_ID = "team_xRiEmDsyOjLzRdBPlruVa1Sk"
ALIASES = [
    "twitch-music-bot.vercel.app",
    "twitch-music-bot-nanobotagents-projects.vercel.app",
]
SKIP_DIRS = {"node_modules", ".next", ".git"}
API = "https://api.vercel.com"


def load_token():
    with open(ENV_FILE) as f:
        for line in f:
            if line.startswith("VERCEL_TOKEN="):
                return line.strip().split("=", 1)[1]
    raise SystemExit("VERCEL_TOKEN not found in " + ENV_FILE)


TOKEN = load_token()


def request(method, path, body=None, raw=None, content_type="application/json", extra_headers=None):
    url = path if path.startswith("http") else API + path
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", "Bearer " + TOKEN)
    if data is not None:
        req.add_header("Content-Type", content_type)
    for k, v in (extra_headers or {}).items():
        req.add_header(k, v)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            payload = resp.read()
        return resp.status, json.loads(payload) if payload and payload.strip().startswith((b"{", b"[")) else payload
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")


def collect_files():
    files = []
    for root, dirs, names in os.walk(FRONTEND_DIR):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for name in names:
            full = os.path.join(root, name)
            rel = os.path.relpath(full, FRONTEND_DIR)
            files.append((rel.replace(os.sep, "/"), full))
    return sorted(files)


def inspect():
    status, data = request("GET", f"/v6/deployments?projectId={PROJECT_ID}&teamId={TEAM_ID}&limit=3&state=READY")
    if status != 200:
        print("list failed:", status, data)
        return
    for d in data.get("deployments", []):
        print(d["uid"], d.get("state"), d.get("url"), "files:", len(d.get("files", [])))
        for f in d.get("files", [])[:40]:
            print("   ", f if isinstance(f, str) else f.get("name"))


def upload(files):
    entries = []
    for rel, full in files:
        content = open(full, "rb").read()
        sha = hashlib.sha1(content).hexdigest()
        qs = f"?sha={sha}&size={len(content)}"
        req_headers_extra = {"x-vercel-digest": sha}
        status, body = request("POST", "/v2/files" + qs + f"&teamId={TEAM_ID}", raw=content,
                               content_type="application/octet-stream", extra_headers=req_headers_extra)
        if status not in (200, 201):
            print(f"upload failed {rel}: {status} {str(body)[:200]}")
            sys.exit(1)
        entries.append({"file": rel, "sha": sha, "size": len(content)})
    return entries


def deploy(files):
    entries = upload(files)
    body = {
        "name": "twitch-music-bot",
        "project": PROJECT_ID,
        "target": "production",
        "files": entries,
        "projectSettings": {"framework": "nextjs"},
    }
    status, data = request("POST", f"/v13/deployments?teamId={TEAM_ID}&skipAutoDetectionConfirmation=1", body=body)
    if status not in (200, 201):
        print("deploy create failed:", status, str(data)[:500])
        sys.exit(1)
    dep_id = data["id"]
    url = data.get("url")
    print("deployment created:", dep_id, url)
    for _ in range(120):
        time.sleep(5)
        s, d = request("GET", f"/v13/deployments/{dep_id}?teamId={TEAM_ID}")
        state = d.get("readyState") or d.get("state")
        if state in ("READY", "ERROR", "CANCELED"):
            print("final state:", state)
            if state != "READY":
                print("build logs hint:", json.dumps(d.get("buildErrorMessage") or d.get("errorMessage") or "")[:500])
                sys.exit(2)
            return d
    print("timed out waiting for deployment")
    sys.exit(3)


def alias(deployment_id):
    for name in ALIASES:
        status, data = request("POST", f"/v13/deployments/{deployment_id}/aliases?teamId={TEAM_ID}",
                               body={"alias": name})
        print(f"alias {name}: {status} {str(data)[:120]}")


def main():
    if "--inspect" in sys.argv:
        inspect()
        return
    files = collect_files()
    print(f"deploying {len(files)} files from {FRONTEND_DIR}")
    dep = deploy(files)
    alias(dep["id"])
    print("done:", dep.get("url"))


if __name__ == "__main__":
    main()
