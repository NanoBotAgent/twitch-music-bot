import json
import re
import sys
import urllib.request

tok = re.search(r'https://x-access-token:(ghp_[^@]+)@github\.com',
                open('/home/applepie69/.git-credentials').read()).group(1)
REPO = "NanoBotAgent/twitch-music-bot"
run_id = int(sys.argv[1])
needle = sys.argv[2] if len(sys.argv) > 2 else "error"
ctx = int(sys.argv[3]) if len(sys.argv) > 3 else 4


def api(path):
    req = urllib.request.Request(f"https://api.github.com{path}",
        headers={"Authorization": f"Bearer {tok}", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req) as r:
        return json.load(r)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


opener = urllib.request.build_opener(NoRedirect)


def job_log(job_id):
    req = urllib.request.Request(
        f"https://api.github.com/repos/{REPO}/actions/jobs/{job_id}/logs",
        headers={"Authorization": f"Bearer {tok}"})
    try:
        opener.open(req)
    except urllib.error.HTTPError as e:
        loc = e.headers.get("Location")
    log_req = urllib.request.Request(loc)
    with urllib.request.urlopen(log_req) as r:
        return r.read().decode("utf-8", "replace")


for j in api(f"/repos/{REPO}/actions/runs/{run_id}/jobs")["jobs"]:
    if j["conclusion"] != "success":
        print(f"=== JOB {j['name']} ({j['id']}) ===")
        lines = job_log(j["id"]).splitlines()
        hits = [i for i, l in enumerate(lines) if needle.lower() in l.lower()]
        shown = set()
        for h in hits[:30]:
            for i in range(max(0, h - ctx), min(len(lines), h + 1)):
                if i not in shown:
                    shown.add(i)
                    print(lines[i][:300])
            print("---")
        if not hits:
            print(f"(no lines matching '{needle}'; last 25 lines:)")
            for l in lines[-25:]:
                print(l[:300])
