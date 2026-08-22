import json, ctypes, urllib.request, urllib.error, base64, re

creds = open('/home/applepie69/.git-credentials').read()
m = re.search(r'https://x-access-token:(ghp_[^@]+)@github\.com', creds)
tok = m.group(1)

def gh(path, method="GET", data=None):
    req = urllib.request.Request(f"https://api.github.com{path}", method=method,
        data=json.dumps(data).encode() if data else None,
        headers={"Authorization": f"Bearer {tok}", "Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(req) as r:
            body = r.read().decode()
            return r.status, json.loads(body) if body.strip().startswith(('{','[')) else body
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:300]

repo = "NanoBotAgent/twitch-music-bot"
st, pk = gh(f"/repos/{repo}/actions/secrets/public-key")
print("public-key:", st)
if st != 200:
    raise SystemExit(pk)
st2, secrets = gh(f"/repos/{repo}/actions/secrets")
names = [s["name"] for s in secrets.get("secrets", [])] if isinstance(secrets, dict) else []
print("existing secrets:", names)

sodium = ctypes.CDLL("libsodium.so.23")
assert sodium.sodium_init() == 0

def sealed_box(msg, pubkey_b64):
    pk_raw = base64.b64decode(pubkey_b64)
    buf = ctypes.create_string_buffer(len(msg) + 48)
    rc = sodium.crypto_box_seal(buf, msg, ctypes.c_ulonglong(len(msg)), pk_raw)
    assert rc == 0
    return buf.raw

env = {}
for line in open('/home/applepie69/nanobot/.env'):
    line = line.strip()
    if line and '=' in line:
        k, v = line.split('=', 1)
        env[k] = v

for name in ("ALWAYSDATA_API_KEY", "ALWAYSDATA_WEBDAV_PASSWORD"):
    val = env[name].encode()
    enc = sealed_box(val, pk["key"])
    st3, resp = gh(f"/repos/{repo}/actions/secrets/{name}", "PUT",
                   {"encrypted_value": base64.b64encode(enc).decode(), "key_id": pk["key_id"]})
    print(name, "->", st3)

st4, secrets = gh(f"/repos/{repo}/actions/secrets")
print("after:", [s["name"] for s in secrets.get("secrets", [])])
