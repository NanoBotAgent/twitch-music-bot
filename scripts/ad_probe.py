import base64
import json
import urllib.error
import urllib.request

API = "https://api.alwaysdata.com" + "/v1"
KEY = "f8659c49c25e498aa62af2f35c28bbff"
AUTH = base64.b64encode(f"{KEY}:".encode()).decode()


def call(path, method="GET", data=None):
    req = urllib.request.Request(
        API + path,
        method=method,
        data=json.dumps(data).encode() if data is not None else None,
        headers={"Authorization": f"Basic {AUTH}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req) as r:
            body = r.read().decode()
            return r.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:800]


if __name__ == "__main__":
    st, svcs = call("/service/")
    print(st)
    print(json.dumps(svcs, indent=1)[:1000])
