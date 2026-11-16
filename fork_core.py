"""Windows runtime and portable configuration for Fork."""

from __future__ import annotations

import base64
import ctypes
import ipaddress
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import parse_qs, unquote, urlsplit
from urllib.request import Request, urlopen
from uuid import uuid4


APP_DIR = Path(os.environ.get("APPDATA", Path.home() / "AppData/Roaming")) / "Fork Lite Win"
DEFAULT_RULES = [
    {"kind": "ipCidr", "value": "100.64.0.0/10", "target": "direct"},
    {"kind": "ipCidr", "value": "10.0.0.0/8", "target": "direct"},
    {"kind": "ipCidr", "value": "172.16.0.0/12", "target": "direct"},
    {"kind": "ipCidr", "value": "192.168.0.0/16", "target": "direct"},
]
FIELDS = {"domain": "domain", "domainSuffix": "domain_suffix", "domainKeyword": "domain_keyword",
          "ipCidr": "ip_cidr", "processName": "process_name", "port": "port"}
PROFILE_FIELDS = ("name", "server", "port", "uuid", "flow", "serverName", "publicKey", "shortId",
                  "fingerprint", "network", "security", "path", "host", "mode", "alpn", "xhttpExtra")


def load_json(name: str, default):
    try:
        return json.loads((APP_DIR / name).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return default


def save_json(name: str, value):
    APP_DIR.mkdir(parents=True, exist_ok=True)
    path = APP_DIR / name
    fd, temp = tempfile.mkstemp(prefix=".fork-", dir=APP_DIR)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as out:
            json.dump(value, out, ensure_ascii=False, indent=2)
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def default_settings():
    return {"autoBest": True, "rules": [dict(r, id=str(uuid4()), enabled=True) for r in DEFAULT_RULES]}


def profile_from_link(link: str):
    u = urlsplit(link.strip())
    if u.scheme != "vless" or not u.username or not u.hostname or not u.port:
        raise ValueError("Некорректная ссылка vless://")
    q = {k: v[0] for k, v in parse_qs(u.query, keep_blank_values=True).items()}
    network = q.get("type", "tcp")
    if network == "splithttp":
        network = "xhttp"
    security = q.get("security", "none")
    if network not in ("tcp", "ws", "xhttp") or security not in ("none", "tls", "reality"):
        raise ValueError("Поддерживаются tcp/ws/xhttp и none/tls/reality")
    if security == "reality" and (network == "ws" or not q.get("pbk")):
        raise ValueError("Reality требует pbk и транспорт tcp или xhttp")
    if q.get("encryption", "none") != "none":
        raise ValueError("VLESS encryption не поддерживается")
    return {
        "id": str(uuid4()), "name": unquote(u.fragment) or u.hostname, "server": u.hostname, "port": u.port,
        "uuid": unquote(u.username), "flow": q.get("flow", "") if network == "tcp" else "",
        "serverName": q.get("sni") or q.get("host") or u.hostname,
        "publicKey": q.get("pbk", ""), "shortId": q.get("sid", ""), "fingerprint": q.get("fp") or "chrome",
        "network": network, "security": security, "path": q.get("path") or ("/" if network != "tcp" else ""),
        "host": q.get("host", ""), "mode": q.get("mode", ""), "alpn": q.get("alpn", ""),
        "xhttpExtra": q.get("extra", ""),
    }


def parse_profiles(text: str):
    raw = text.strip()
    try:
        decoded = base64.b64decode("".join(raw.split()), validate=True).decode("utf-8")
        if "vless://" in decoded:
            raw = decoded
    except (ValueError, UnicodeError):
        pass
    result = []
    if "proxies:" in raw:
        try:
            import yaml
        except ImportError as error:
            raise ValueError("Для Clash YAML установите PyYAML: pip install -r requirements.txt") from error
        data = yaml.safe_load(raw) or {}
        for item in data.get("proxies", []):
            if not isinstance(item, dict) or item.get("type") != "vless":
                continue
            opts = item.get("ws-opts") or item.get("xhttp-opts") or {}
            reality = item.get("reality-opts") or {}
            network = item.get("network", "tcp")
            query = {
                "type": network, "security": "reality" if reality.get("public-key") else ("tls" if item.get("tls") else "none"),
                "sni": item.get("servername", ""), "pbk": reality.get("public-key", ""),
                "sid": reality.get("short-id", ""), "fp": item.get("client-fingerprint", "chrome"),
                "flow": item.get("flow", ""), "path": opts.get("path", ""),
                "host": (opts.get("headers") or {}).get("Host", opts.get("host", "")),
                "mode": opts.get("mode", ""), "alpn": ",".join(item.get("alpn") or []),
            }
            from urllib.parse import urlencode, quote
            link = f"vless://{item['uuid']}@{item['server']}:{item['port']}?{urlencode(query)}#{quote(str(item.get('name', item['server'])))}"
            try:
                result.append(profile_from_link(link))
            except (KeyError, ValueError):
                continue
    else:
        for line in raw.splitlines():
            if line.strip().startswith("vless://"):
                try:
                    result.append(profile_from_link(line))
                except ValueError:
                    continue
    unique = []
    for profile in result:
        if not any(all(profile[k] == old[k] for k in PROFILE_FIELDS) for old in unique):
            unique.append(profile)
    if not unique:
        raise ValueError("Не найдено подходящих VLESS-серверов")
    return unique


def fetch_subscription(url: str):
    if urlsplit(url).scheme != "https":
        raise ValueError("Подписка должна использовать HTTPS")
    request = Request(url, headers={"User-Agent": "Fork-Windows/1"})
    with urlopen(request, timeout=20) as response:
        if urlsplit(response.url).scheme != "https":
            raise ValueError("Подписка перенаправила на HTTP")
        data = response.read(2_000_001)
    if len(data) > 2_000_000:
        raise ValueError("Подписка больше 2 МБ")
    return parse_profiles(data.decode("utf-8"))


def sync_subscription(store: dict, url: str, incoming: list[dict]):
    old = [p for p in store["items"] if p.get("sourceURL") == url]
    replacement = []
    for profile in incoming:
        profile = dict(profile, sourceURL=url)
        match = next((p for p in old if all(p.get(k) == profile.get(k) for k in PROFILE_FIELDS)), None)
        if match:
            profile["id"] = match["id"]
        replacement.append(profile)
    store["items"] = [p for p in store["items"] if p.get("sourceURL") != url] + replacement
    if url not in store["subscriptions"]:
        store["subscriptions"].append(url)
    if not any(p["id"] == store.get("selected") for p in store["items"]):
        store["selected"] = replacement[0]["id"] if replacement else None


def profile_outbound(profile: dict):
    if profile.get("network") == "xhttp":
        return {"type": "socks", "tag": "xray", "server": "127.0.0.1", "server_port": 19080, "version": "5"}
    outbound = {"type": "vless", "tag": "xray", "server": profile["server"], "server_port": profile["port"],
                "uuid": profile["uuid"], "domain_resolver": "dns-local"}
    security = profile.get("security", "reality")
    if security in ("tls", "reality"):
        tls = {"enabled": True, "server_name": profile.get("serverName") or profile["server"],
               "utls": {"enabled": True, "fingerprint": profile.get("fingerprint") or "chrome"}}
        if security == "reality":
            tls["reality"] = {"enabled": True, "public_key": profile["publicKey"], "short_id": profile.get("shortId", "")}
        elif profile.get("alpn"):
            tls["alpn"] = [s.strip() for s in profile["alpn"].split(",") if s.strip()]
        outbound["tls"] = tls
    if profile.get("network") == "ws":
        transport = {"type": "ws", "path": profile.get("path") or "/"}
        if profile.get("host"):
            transport["headers"] = {"Host": profile["host"]}
        outbound["transport"] = transport
    if profile.get("network", "tcp") == "tcp" and profile.get("flow"):
        outbound["flow"] = profile["flow"]
    return outbound


def xray_sidecar_config(profile: dict):
    stream = {"network": "xhttp", "security": profile.get("security", "none")}
    if stream["security"] == "reality":
        stream["realitySettings"] = {"serverName": profile["serverName"], "fingerprint": profile.get("fingerprint", "chrome"),
                                     "publicKey": profile["publicKey"], "shortId": profile.get("shortId", ""), "spiderX": ""}
    elif stream["security"] == "tls":
        stream["tlsSettings"] = {"serverName": profile["serverName"], "fingerprint": profile.get("fingerprint", "chrome")}
    opts = {"path": profile.get("path") or "/", "mode": profile.get("mode") or "auto"}
    if profile.get("host"):
        opts["host"] = profile["host"]
    if profile.get("xhttpExtra"):
        opts["extra"] = json.loads(profile["xhttpExtra"])
    stream["xhttpSettings"] = opts
    user = {"id": profile["uuid"], "encryption": "none"}
    return {"log": {"loglevel": "warning"},
            "inbounds": [{"tag": "socks-in", "listen": "127.0.0.1", "port": 19080, "protocol": "socks", "settings": {"auth": "noauth", "udp": True}}],
            "outbounds": [{"tag": "proxy", "protocol": "vless", "settings": {"vnext": [{"address": profile["server"], "port": profile["port"], "users": [user]}]}, "streamSettings": stream}]}


def build_config(profile: dict, rules: list[dict], secret: str, *, tun: bool = True):
    route_rules = [{"action": "sniff"}]
    if tun:
        route_rules += [{"protocol": "dns", "action": "hijack-dns"},
                        {"process_name": ["sing-box.exe", "xray.exe"], "action": "route", "outbound": "direct"}]
    dns_rules = []
    if any(c.isalpha() for c in profile["server"]) and ":" not in profile["server"]:
        dns_rules.append({"domain": [profile["server"]], "server": "dns-local"})
    for rule in rules:
        if not rule.get("enabled", True) or rule.get("corpBlock") or rule.get("target") == "corp":
            continue
        field = FIELDS.get(rule.get("kind"))
        value = str(rule.get("value", "")).strip()
        if not field or not value or rule.get("target") not in ("xray", "direct", "block"):
            continue
        if field == "port":
            try:
                value = int(value)
                if not 1 <= value <= 65535:
                    continue
            except ValueError:
                continue
        elif field == "ip_cidr":
            try:
                ipaddress.ip_network(value, strict=False)
            except ValueError:
                continue
        match = {field: [value]}
        if rule["target"] == "block":
            match["action"] = "reject"
        else:
            match.update(action="route", outbound=rule["target"])
        route_rules.append(match)
        if field in ("domain", "domain_suffix", "domain_keyword") and rule["target"] != "block":
            dns_rules.append({field: [value], "server": "dns-local" if rule["target"] == "direct" else "dns-xray"})
    inbound = ({"type": "tun", "tag": "tun-in", "address": ["172.19.0.1/30"], "auto_route": True,
                "strict_route": True} if tun else
               {"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": 19091})
    return {
        "log": {"level": "info", "timestamp": True},
        "dns": {"servers": [{"type": "udp", "tag": "dns-local", "server": "223.5.5.5"},
                            {"type": "https", "tag": "dns-xray", "server": "1.1.1.1", "detour": "xray"}],
                "rules": dns_rules, "final": "dns-xray", "strategy": "ipv4_only", "reverse_mapping": True},
        "inbounds": [inbound], "outbounds": [{"type": "direct", "tag": "direct"}, profile_outbound(profile)],
        "route": {"rules": route_rules, "final": "xray", "auto_detect_interface": True,
                  "find_process": True, "default_domain_resolver": "dns-local"},
        "experimental": {"clash_api": {"external_controller": "127.0.0.1:19090", "secret": secret}},
    }


def executable(name: str):
    bundled = Path(getattr(sys, "_MEIPASS", Path(__file__).parent)) / "bin" / name
    return str(bundled) if bundled.is_file() else shutil.which(name)


def is_admin():
    return os.name == "nt" and bool(ctypes.windll.shell32.IsUserAnAdmin())


def probe(profile: dict, timeout=1.5):
    start = time.monotonic()
    try:
        with socket.create_connection((profile["server"], int(profile["port"])), timeout=timeout):
            return int((time.monotonic() - start) * 1000)
    except OSError:
        return None


class Runtime:
    def __init__(self):
        self.secret = secrets.token_urlsafe(24)
        self.core = None
        self.sidecar = None
        self.log = None

    def stop(self):
        for proc in (self.core, self.sidecar):
            if proc and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        self.core = self.sidecar = None
        if self.log:
            self.log.close()
            self.log = None

    def start(self, profile: dict, rules: list[dict], *, tun=True):
        self.stop()
        if tun and not is_admin():
            raise RuntimeError("Для TUN запустите Fork от имени администратора")
        sing = executable("sing-box.exe")
        if not sing:
            raise RuntimeError("Не найден sing-box.exe: положите его в windows/bin или добавьте в PATH")
        APP_DIR.mkdir(parents=True, exist_ok=True)
        if profile.get("network") == "xhttp":
            xray = executable("xray.exe")
            if not xray:
                raise RuntimeError("Для xhttp нужен xray.exe в windows/bin или PATH")
            sidecar_path = APP_DIR / "xray-runtime.json"
            save_json(sidecar_path.name, xray_sidecar_config(profile))
            self.sidecar = subprocess.Popen([xray, "run", "-config", str(sidecar_path)],
                                             cwd=APP_DIR, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                             creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            for _ in range(20):
                try:
                    with socket.create_connection(("127.0.0.1", 19080), timeout=.2):
                        break
                except OSError:
                    if self.sidecar.poll() is not None:
                        self.stop()
                        raise RuntimeError("xray.exe завершился при запуске")
                    time.sleep(.2)
            else:
                self.stop()
                raise RuntimeError("xray.exe не открыл SOCKS-порт")
        config = APP_DIR / "sing-box-runtime.json"
        save_json(config.name, build_config(profile, rules, self.secret, tun=tun))
        check = subprocess.run([sing, "check", "-c", str(config)], capture_output=True, text=True,
                               timeout=15, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        if check.returncode:
            self.stop()
            raise RuntimeError((check.stderr or check.stdout).strip()[-800:])
        self.log = open(APP_DIR / "core.log", "w", encoding="utf-8")
        self.core = subprocess.Popen([sing, "run", "-c", str(config)], cwd=APP_DIR,
                                     stdout=self.log, stderr=subprocess.STDOUT,
                                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        time.sleep(.5)
        if self.core.poll() is not None:
            code = self.core.returncode
            self.stop()
            raise RuntimeError(f"sing-box завершился с кодом {code}; см. core.log")

    def connections(self):
        if not self.core or self.core.poll() is not None:
            return []
        request = Request("http://127.0.0.1:19090/connections", headers={"Authorization": "Bearer " + self.secret})
        with urlopen(request, timeout=2) as response:
            return json.load(response).get("connections", [])
