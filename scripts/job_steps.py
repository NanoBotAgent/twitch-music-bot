import json
import re
import sys
import urllib.request

tok = re.search(r'https://x-access-token:(ghp_[^@]+)@github\.com',
                open('/home/applepie69/.git-credentials').read()).group(1)
REPO = "NanoBotAgent/twitch-music-bot"
run_id = int(sys.argv[1])


def api(path):
    req = urllib.request.Request(f"https://api.github.com{path}",
        headers={"Authorization": f"Bearer {tok}", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req) as r:
        return json.load(r)


jobs = api(f"/repos/{REPO}/actions/runs/{run_id}/jobs")["jobs"]
for j in jobs:
    print("JOB:", j["name"], j["conclusion"])
    for s in j["steps"]:
        print(f"  step: {s['name']} -> {s['conclusion']}")
