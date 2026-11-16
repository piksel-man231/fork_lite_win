import base64
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fork_core import (build_config, engine_error, parse_profiles, profile_from_link, sync_subscription,
                       visible_connections, xray_sidecar_config)


LINK = "vless://123e4567-e89b-12d3-a456-426614174000@example.com:443?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&type=tcp#Main"


class ForkCoreTests(unittest.TestCase):
    def test_link_and_base64_import(self):
        p = profile_from_link(LINK)
        self.assertEqual((p["network"], p["security"], p["name"]), ("tcp", "reality", "Main"))
        encoded = base64.b64encode((LINK + "\n" + LINK).encode()).decode()
        self.assertEqual(len(parse_profiles(encoded)), 1)

    def test_invalid_reality_ws(self):
        with self.assertRaises(ValueError):
            profile_from_link(LINK.replace("type=tcp", "type=ws"))

    def test_clash_yaml_import(self):
        yaml = """proxies:
  - name: Work
    type: vless
    server: example.com
    port: 443
    uuid: 123e4567-e89b-12d3-a456-426614174000
    network: ws
    tls: true
    ws-opts:
      path: /web
      headers:
        Host: edge.example.com
"""
        profile = parse_profiles(yaml)[0]
        self.assertEqual((profile["network"], profile["security"], profile["path"]), ("ws", "tls", "/web"))
        self.assertEqual(profile["host"], "edge.example.com")

    def test_rules_order_and_disabled(self):
        p = profile_from_link(LINK)
        rules = [
            {"kind": "domain", "value": "private.example", "target": "direct", "enabled": True},
            {"kind": "ipCidr", "value": "10.0.0.0/8", "target": "block", "enabled": True},
            {"kind": "domain", "value": "ignored.example", "target": "direct", "enabled": False},
            {"kind": "domain", "value": "corp.example", "target": "corp", "enabled": True},
        ]
        config = build_config(p, rules, "secret", tun=False)
        route = config["route"]["rules"]
        self.assertEqual(route[1]["domain"], ["private.example"])
        self.assertEqual(route[2]["action"], "reject")
        self.assertEqual(len(route), 3)
        self.assertEqual(config["route"]["final"], "xray")
        self.assertEqual(config["experimental"]["clash_api"]["secret"], "secret")
        json.dumps(config)

    def test_subscription_keeps_selected_id(self):
        old = profile_from_link(LINK)
        old["sourceURL"] = "https://example.com/sub"
        local = profile_from_link(LINK.replace("example.com", "other.example.com"))
        store = {"items": [old, local], "selected": old["id"], "subscriptions": []}
        sync_subscription(store, "https://example.com/sub", [profile_from_link(LINK)])
        self.assertEqual(store["selected"], old["id"])
        self.assertEqual({p["id"] for p in store["items"]}, {old["id"], local["id"]})

    def test_xhttp_uses_sidecar(self):
        p = profile_from_link(LINK.replace("type=tcp", "type=xhttp"))
        config = build_config(p, [], "secret")
        self.assertEqual(config["outbounds"][1]["type"], "socks")
        sidecar = xray_sidecar_config(p)
        self.assertEqual(sidecar["outbounds"][0]["streamSettings"]["network"], "xhttp")

    def test_vision_udp443_keeps_flow_in_xray(self):
        profile = profile_from_link(LINK.replace("type=tcp", "type=tcp&flow=xtls-rprx-vision-udp443"))
        config = build_config(profile, [], "secret")
        self.assertEqual(config["outbounds"][1]["type"], "socks")
        sidecar = xray_sidecar_config(profile)
        outbound = sidecar["outbounds"][0]
        self.assertEqual(outbound["streamSettings"]["network"], "tcp")
        self.assertEqual(outbound["settings"]["vnext"][0]["users"][0]["flow"], "xtls-rprx-vision-udp443")

    def test_filter_engine_connections(self):
        connections = [
            {"id": "1", "metadata": {"processPath": r"C:\Fork\xray.exe"}},
            {"id": "2", "metadata": {"processPath": r"C:\Fork\SING-BOX.EXE (user)"}},
            {"id": "3", "metadata": {"processPath": r"C:\Apps\browser.exe"}},
        ]
        self.assertEqual([c["id"] for c in visible_connections(connections)], ["3"])

    def test_engine_error_removes_terminal_colors(self):
        self.assertEqual(engine_error("\x1b[31mFATAL\x1b[0m unsupported flow"), "FATAL unsupported flow")


if __name__ == "__main__":
    unittest.main()
