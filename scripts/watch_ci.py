import json
import re
import sys
import time
import urllib.request

tok = re.search(r'https://x-access-token:(ghp_[^@]+)@github\.com',
                open('/home/applepie69/.git-credentials').read()).group(1)
REPO = "NanoBotAgent/twitch-music-bot"


def api(path):
    req = urllib.request.Request(f"https://api.github.com{path}",
        headers={"Authorization": f"Bearer {tok}", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req) as r:
        return json.load(r)


runs = api(f"/repos/{REPO}/actions/runs?per_page=3")["workflow_runs"]
for run in runs:
    print(run["id"], run["name"], "|", run["head_sha"][:7], "|", run["status"],
          "|", run["conclusion"], "|", run["created_at"])

if len(sys.argv) > 1 and sys.argv[1] == "watch":
    run_id = runs[0]["id"]
    while True:
        r = [x for x in api(f"/repos/{REPO}/actions/runs?per_page=5")["workflow_runs"] if x["id"] == run_id][0]
        if r["status"] == "completed":
            print("DONE:", r["conclusion"])
            break
        jobs = api(f"/repos/{REPO}/actions/runs/{run_id}/jobs")["jobs"]
        for j in jobs:
            steps = " ".join(f"{s['name']}:{'.' if s['status'] != 'completed' else s['conclusion'][0]}"
                             for s in j.get("steps", [])[-4:])
            print(f"  {j['name']} [{j['status']}{'/' + str(j['conclusion']) if j['conclusion'] else ''}] {steps}")
        time.sleep(20)
