import ctypes
import base64
import json
import re
import secrets as pysecrets
import urllib.error
import urllib.request

repo = "NanoBotAgent/twitch-music-bot"
creds = open('/home/applepie69/.git-credentials').read()
tok = re.search(r'https://x-access-token:(ghp_[^@]+)@github\.com', creds).group(1)


def gh(path, method="GET", data=None):
    req = urllib.request.Request(f"https://api.github.com{path}", method=method,
        data=json.dumps(data).encode() if data else None,
        headers={"Authorization": f"Bearer {tok}", "Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(req) as r:
            body = r.read().decode()
            return r.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:300]


sodium = ctypes.CDLL("libsodium.so.23")
assert sodium.sodium_init() == 0


def sealed_box(msg, pubkey_b64):
    buf = ctypes.create_string_buffer(len(msg) + 48)
    rc = sodium.crypto_box_seal(buf, msg, ctypes.c_ulonglong(len(msg)),
                                base64.b64decode(pubkey_b64))
    assert rc == 0
    return buf.raw


st, pk = gh(f"/repos/{repo}/actions/secrets/public-key")
assert st == 200, pk

env_path = '/home/applepie69/nanobot/.env'
env = {}
for line in open(env_path):
    line = line.strip()
    if line and '=' in line:
        k, v = line.split('=', 1)
        env[k] = v

for name in ("JWT_SECRET", "ENCRYPTION_KEY"):
    val = env.get(name) or pysecrets.token_hex(32)
    st2, _ = gh(f"/repos/{repo}/actions/secrets/{name}", "PUT",
                {"encrypted_value": base64.b64encode(sealed_box(val.encode(), pk["key"])).decode(),
                 "key_id": pk["key_id"]})
    print(name, "-> repo:", st2, "| local .env:", "updated" if env.get(name) else "created")
    if not env.get(name):
        with open(env_path, 'a') as f:
            f.write(f"{name}={val}\n")

st3, allsecrets = gh(f"/repos/{repo}/actions/secrets")
print("repo secrets now:", [s["name"] for s in allsecrets.get("secrets", [])])
