import base64
import sys
import urllib.request

HOST = "https://webdav-twitch-bot.alwaysdata.net"
KEY = open("/home/applepie69/nanobot/.env").read()
import re

m = re.search(r"ALWAYSDATA_WEBDAV_PASSWORD=(\S+)", KEY)
PWD = m.group(1) if m else ""
AUTH = base64.b64encode(f"twitch-bot:{PWD}".encode()).decode()


def propfind(path):
    req = urllib.request.Request(HOST + path, method="PROPFIND",
                                 headers={"Authorization": f"Basic {AUTH}", "Depth": "1"})
    try:
        with urllib.request.urlopen(req) as r:
            body = r.read().decode()
            import re as _re
            names = _re.findall(r"<D:href>([^<]+)</D:href>", body) or \
                    _re.findall(r"<d:href>([^<]+)</d:href>", body)
            print(path, r.status, names[:12])
    except Exception as e:
        print(path, "ERR", e)


for p in ["/app/", "/app/config/", "/logs/", "/admin/logs/", "/"]:
    propfind(p)
