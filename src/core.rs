use base64::Engine as _;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use url::Url;

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        });
    base.join("Fork Lite Win")
}

pub fn read_json(name: &str, fallback: Value) -> Value {
    fs::read(app_dir().join(name))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or(fallback)
}

pub fn write_json(name: &str, value: &Value) -> Result<(), String> {
    fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
    let path = app_dir().join(name);
    let temporary = path.with_extension("tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

pub fn default_settings() -> Value {
    let rules: Vec<Value> = ["100.64.0.0/10", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"]
        .iter().map(|value| json!({"id": uuid::Uuid::new_v4().to_string(), "kind": "ipCidr", "value": value, "target": "direct", "enabled": true})).collect();
    json!({"autoBest": true, "autoConnect": false, "rules": rules})
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub fn profile_from_link(link: &str) -> Result<Value, String> {
    let url = Url::parse(link.trim()).map_err(|_| "Некорректная ссылка VLESS".to_owned())?;
    if url.scheme() != "vless"
        || url.username().is_empty()
        || url.host_str().is_none()
        || url.port().is_none()
    {
        return Err("Некорректная ссылка VLESS".into());
    }
    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    let q = |key: &str| query.get(key).cloned().unwrap_or_default();
    let network = match q("type").as_str() {
        "" => "tcp".to_owned(),
        "splithttp" => "xhttp".to_owned(),
        value => value.to_owned(),
    };
    let security = if q("security").is_empty() {
        "none".to_owned()
    } else {
        q("security")
    };
    if !["tcp", "ws", "xhttp"].contains(&network.as_str())
        || !["none", "tls", "reality"].contains(&security.as_str())
    {
        return Err("Поддерживаются tcp/ws/xhttp и none/tls/reality".into());
    }
    if security == "reality" && (network == "ws" || q("pbk").is_empty()) {
        return Err("Reality требует pbk и tcp/xhttp".into());
    }
    if !q("encryption").is_empty() && q("encryption") != "none" {
        return Err("VLESS encryption не поддерживается".into());
    }
    let server = url.host_str().unwrap_or_default().to_owned();
    let name = url
        .fragment()
        .filter(|s| !s.is_empty())
        .unwrap_or(&server)
        .to_owned();
    let server_name = if !q("sni").is_empty() {
        q("sni")
    } else if !q("host").is_empty() {
        q("host")
    } else {
        server.clone()
    };
    let path = if q("path").is_empty() && network != "tcp" {
        "/".to_owned()
    } else {
        q("path")
    };
    Ok(
        json!({"id": uuid::Uuid::new_v4().to_string(), "name": name, "server": server, "port": url.port().unwrap(),
        "uuid": url.username(), "flow": if network == "tcp" { q("flow") } else { String::new() },
        "serverName": server_name, "publicKey": q("pbk"), "shortId": q("sid"),
        "fingerprint": if q("fp").is_empty() { "chrome".to_owned() } else { q("fp") },
        "network": network, "security": security, "path": path, "host": q("host"), "mode": q("mode"),
        "alpn": q("alpn"), "xhttpExtra": q("extra")}),
    )
}

pub fn parse_profiles(input: &str) -> Result<Vec<Value>, String> {
    let mut text = input.trim().to_owned();
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD
        .decode(text.split_whitespace().collect::<String>())
    {
        if let Ok(value) = String::from_utf8(decoded) {
            if value.contains("vless://") {
                text = value;
            }
        }
    }
    let mut result = Vec::new();
    if text.contains("proxies:") {
        let yaml: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
        let proxies = yaml
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
            .ok_or("Нет серверов в Clash YAML")?;
        for item in proxies {
            if item.get("type").and_then(serde_yaml::Value::as_str) != Some("vless") {
                continue;
            }
            let get = |key: &str| {
                item.get(key)
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let options = item.get("ws-opts").or_else(|| item.get("xhttp-opts"));
            let opt = |key: &str| {
                options
                    .and_then(|v| v.get(key))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let reality = item.get("reality-opts");
            let ropt = |key: &str| {
                reality
                    .and_then(|v| v.get(key))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let network = if get("network").is_empty() {
                "tcp".to_owned()
            } else {
                get("network")
            };
            let security = if !ropt("public-key").is_empty() {
                "reality"
            } else if item.get("tls").and_then(serde_yaml::Value::as_bool) == Some(true) {
                "tls"
            } else {
                "none"
            };
            let port = item
                .get("port")
                .and_then(serde_yaml::Value::as_u64)
                .unwrap_or(0);
            let header_host = options
                .and_then(|v| v.get("headers"))
                .and_then(|v| v.get("Host"))
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or("");
            let host = if header_host.is_empty() {
                opt("host")
            } else {
                header_host.to_owned()
            };
            let alpn = item
                .get("alpn")
                .and_then(serde_yaml::Value::as_sequence)
                .map(|v| {
                    v.iter()
                        .filter_map(serde_yaml::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let mut url = format!("vless://{}@{}:{}", get("uuid"), get("server"), port);
            let query = [
                ("type", network),
                ("security", security.to_owned()),
                ("sni", get("servername")),
                ("pbk", ropt("public-key")),
                ("sid", ropt("short-id")),
                ("fp", get("client-fingerprint")),
                ("flow", get("flow")),
                ("path", opt("path")),
                ("host", host),
                ("mode", opt("mode")),
                ("alpn", alpn),
            ];
            url.push('?');
            url.push_str(
                &url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(query.iter().map(|(a, b)| (*a, b.as_str())))
                    .finish(),
            );
            url.push('#');
            url.push_str(&get("name"));
            if let Ok(profile) = profile_from_link(&url) {
                result.push(profile);
            }
        }
    } else {
        for line in text.lines() {
            if line.trim().starts_with("vless://") {
                if let Ok(profile) = profile_from_link(line) {
                    result.push(profile);
                }
            }
        }
    }
    result.dedup_by(|a, b| {
        [
            "server", "port", "uuid", "network", "security", "path", "flow",
        ]
        .iter()
        .all(|key| a.get(key) == b.get(key))
    });
    if result.is_empty() {
        Err("Не найдено подходящих VLESS-серверов".into())
    } else {
        Ok(result)
    }
}

pub fn fetch_subscription(url: &str) -> Result<Vec<Value>, String> {
    if !url.starts_with("https://") {
        return Err("Подписка должна использовать HTTPS".into());
    }
    let response = ureq::get(url)
        .set("User-Agent", "Fork-Lite-Win/0.4")
        .timeout(Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?;
    if !response.get_url().starts_with("https://") {
        return Err("Подписка перенаправила на HTTP".into());
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(2_000_001)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > 2_000_000 {
        return Err("Подписка больше 2 МБ".into());
    }
    parse_profiles(&String::from_utf8(body).map_err(|e| e.to_string())?)
}

pub fn needs_sidecar(profile: &Value) -> bool {
    string(profile, "network") == "xhttp" || string(profile, "flow") == "xtls-rprx-vision-udp443"
}

pub fn profile_outbound(profile: &Value) -> Value {
    if needs_sidecar(profile) {
        return json!({"type":"socks","tag":"xray","server":"127.0.0.1","server_port":19080,"version":"5"});
    }
    let mut outbound = json!({"type":"vless","tag":"xray","server":profile["server"],"server_port":profile["port"],"uuid":profile["uuid"],"domain_resolver":"dns-local"});
    let security = string(profile, "security");
    if security == "tls" || security == "reality" {
        let mut tls = json!({"enabled":true,"server_name":profile["serverName"],"utls":{"enabled":true,"fingerprint":profile["fingerprint"]}});
        if security == "reality" {
            tls["reality"] = json!({"enabled":true,"public_key":profile["publicKey"],"short_id":profile["shortId"]});
        } else if !string(profile, "alpn").is_empty() {
            tls["alpn"] = json!(string(profile, "alpn")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>());
        }
        outbound["tls"] = tls;
    }
    if string(profile, "network") == "ws" {
        let mut transport = json!({"type":"ws","path":profile["path"]});
        if !string(profile, "host").is_empty() {
            transport["headers"] = json!({"Host":profile["host"]});
        }
        outbound["transport"] = transport;
    }
    if string(profile, "network") == "tcp" && !string(profile, "flow").is_empty() {
        outbound["flow"] = profile["flow"].clone();
    }
    outbound
}

pub fn sidecar_config(profile: &Value) -> Result<Value, String> {
    let network = string(profile, "network");
    let mut stream = json!({"network":network,"security":profile["security"]});
    if string(profile, "security") == "reality" {
        stream["realitySettings"] = json!({"serverName":profile["serverName"],"fingerprint":profile["fingerprint"],"publicKey":profile["publicKey"],"shortId":profile["shortId"],"spiderX":""});
    }
    if string(profile, "security") == "tls" {
        stream["tlsSettings"] =
            json!({"serverName":profile["serverName"],"fingerprint":profile["fingerprint"]});
    }
    if network == "xhttp" {
        let mut options = json!({"path":profile["path"],"mode":if string(profile,"mode").is_empty(){"auto"}else{string(profile,"mode")}});
        if !string(profile, "host").is_empty() {
            options["host"] = profile["host"].clone();
        }
        if !string(profile, "xhttpExtra").is_empty() {
            options["extra"] =
                serde_json::from_str(string(profile, "xhttpExtra")).map_err(|e| e.to_string())?;
        }
        stream["xhttpSettings"] = options;
    }
    let mut user = json!({"id":profile["uuid"],"encryption":"none"});
    if network == "tcp" && !string(profile, "flow").is_empty() {
        user["flow"] = profile["flow"].clone();
    }
    Ok(
        json!({"log":{"loglevel":"warning"},"inbounds":[{"tag":"socks-in","listen":"127.0.0.1","port":19080,"protocol":"socks","settings":{"auth":"noauth","udp":true}}],"outbounds":[{"tag":"proxy","protocol":"vless","settings":{"vnext":[{"address":profile["server"],"port":profile["port"],"users":[user]}]},"streamSettings":stream}]}),
    )
}

pub fn build_config(profile: &Value, rules: &[Value], secret: &str, tun: bool) -> Value {
    let mut route = vec![json!({"action":"sniff"})];
    if tun {
        route.push(json!({"protocol":"dns","action":"hijack-dns"}));
        route.push(json!({"process_name":["sing-box.exe","xray.exe"],"action":"route","outbound":"direct"}));
    }
    let mut dns = Vec::new();
    if string(profile, "server").chars().any(char::is_alphabetic)
        && !string(profile, "server").contains(':')
    {
        dns.push(json!({"domain":[profile["server"]],"server":"dns-local"}));
    }
    for rule in rules {
        if rule.get("enabled") == Some(&Value::Bool(false))
            || rule.get("corpBlock") == Some(&Value::Bool(true))
        {
            continue;
        }
        let field = match string(rule, "kind") {
            "domain" => "domain",
            "domainSuffix" => "domain_suffix",
            "domainKeyword" => "domain_keyword",
            "ipCidr" => "ip_cidr",
            "processName" => "process_name",
            "port" => "port",
            _ => continue,
        };
        let value = string(rule, "value").trim();
        let target = string(rule, "target");
        if value.is_empty() || !["xray", "direct", "block"].contains(&target) {
            continue;
        }
        let item = if field == "port" {
            match value.parse::<u16>() {
                Ok(p) if p > 0 => json!(p),
                _ => continue,
            }
        } else {
            json!(value)
        };
        let mut entry = json!({});
        entry[field] = json!([item]);
        if target == "block" {
            entry["action"] = json!("reject");
        } else {
            entry["action"] = json!("route");
            entry["outbound"] = json!(target);
        }
        route.push(entry);
        if field.starts_with("domain") && target != "block" {
            let mut entry = json!({"server":if target=="direct"{"dns-local"}else{"dns-xray"}});
            entry[field] = json!([value]);
            dns.push(entry);
        }
    }
    let inbound = if tun {
        json!({"type":"tun","tag":"tun-in","address":["172.19.0.1/30"],"auto_route":true,"strict_route":true})
    } else {
        json!({"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":19091})
    };
    json!({"log":{"level":"info","timestamp":true},"dns":{"servers":[{"type":"udp","tag":"dns-local","server":"223.5.5.5"},{"type":"https","tag":"dns-xray","server":"1.1.1.1","detour":"xray"}],"rules":dns,"final":"dns-xray","strategy":"ipv4_only","reverse_mapping":true},
        "inbounds":[inbound],"outbounds":[{"type":"direct","tag":"direct"},profile_outbound(profile)],
        "route":{"rules":route,"final":"xray","auto_detect_interface":true,"find_process":true,"default_domain_resolver":"dns-local"},
        "experimental":{"clash_api":{"external_controller":"127.0.0.1:19090","secret":secret}}})
}

fn engine_path(name: &str) -> PathBuf {
    app_dir().join("bin").join(name)
}

pub fn install_engines() -> Result<(), String> {
    let destination = app_dir().join("bin");
    fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    for (data, files) in [
        (
            include_bytes!("../vendor/sing-box.zip").as_slice(),
            ["sing-box.exe", "libcronet.dll"].as_slice(),
        ),
        (
            include_bytes!("../vendor/xray.zip").as_slice(),
            ["xray.exe", "wintun.dll"].as_slice(),
        ),
    ] {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
        for name in files {
            let path = destination.join(name);
            if path.is_file() {
                continue;
            }
            let index = (0..archive.len())
                .find(|&i| {
                    archive
                        .by_index(i)
                        .ok()
                        .map(|entry| entry.name().ends_with(name))
                        .unwrap_or(false)
                })
                .ok_or(format!("Нет {name} в архиве"))?;
            let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
            let mut temporary =
                fs::File::create(path.with_extension("tmp")).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut temporary).map_err(|e| e.to_string())?;
            drop(temporary);
            fs::rename(path.with_extension("tmp"), path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}
#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

pub struct Runtime {
    pub core: Option<Child>,
    pub sidecar: Option<Child>,
    secret: String,
}
impl Runtime {
    pub fn new() -> Self {
        Self {
            core: None,
            sidecar: None,
            secret: uuid::Uuid::new_v4().to_string(),
        }
    }
    pub fn running(&mut self) -> bool {
        self.core
            .as_mut()
            .and_then(|p| p.try_wait().ok())
            .flatten()
            .is_none()
            && self.core.is_some()
    }
    pub fn stop(&mut self) {
        for child in [&mut self.core, &mut self.sidecar] {
            if let Some(mut process) = child.take() {
                let _ = process.kill();
                let _ = process.wait();
            }
        }
    }
    pub fn start(&mut self, profile: &Value, rules: &[Value]) -> Result<(), String> {
        self.stop();
        install_engines()?;
        fs::create_dir_all(app_dir()).map_err(|e| e.to_string())?;
        if needs_sidecar(profile) {
            write_json("xray-runtime.json", &sidecar_config(profile)?)?;
            let mut command = Command::new(engine_path("xray.exe"));
            command
                .args(["run", "-config"])
                .arg(app_dir().join("xray-runtime.json"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            hide_console(&mut command);
            self.sidecar = Some(command.spawn().map_err(|e| e.to_string())?);
            let mut ready = false;
            for _ in 0..20 {
                if TcpStream::connect_timeout(
                    &"127.0.0.1:19080".parse().unwrap(),
                    Duration::from_millis(200),
                )
                .is_ok()
                {
                    ready = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            if !ready {
                self.stop();
                return Err("Xray не открыл локальный SOCKS-порт".into());
            }
        }
        write_json(
            "sing-box-runtime.json",
            &build_config(profile, rules, &self.secret, true),
        )?;
        let config = app_dir().join("sing-box-runtime.json");
        let mut check = Command::new(engine_path("sing-box.exe"));
        check.args(["check", "-c"]).arg(&config);
        hide_console(&mut check);
        let output = check.output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            self.stop();
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        let log = fs::File::create(app_dir().join("core.log")).map_err(|e| e.to_string())?;
        let mut command = Command::new(engine_path("sing-box.exe"));
        command
            .args(["run", "-c"])
            .arg(config)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
            .stderr(Stdio::from(log));
        hide_console(&mut command);
        self.core = Some(command.spawn().map_err(|e| e.to_string())?);
        std::thread::sleep(Duration::from_millis(500));
        if !self.running() {
            self.stop();
            return Err("sing-box завершился при запуске. См. core.log".into());
        }
        Ok(())
    }
    pub fn connections(&mut self) -> Result<Value, String> {
        if !self.running() {
            return Ok(json!({"connections":[],"downloadTotal":0,"uploadTotal":0}));
        }
        let response = ureq::get("http://127.0.0.1:19090/connections")
            .set("Authorization", &format!("Bearer {}", self.secret))
            .timeout(Duration::from_secs(2))
            .call()
            .map_err(|e| e.to_string())?;
        serde_json::from_reader(response.into_reader()).map_err(|e| e.to_string())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn probe(profile: &Value) -> Option<u128> {
    let address = format!("{}:{}", string(profile, "server"), number(profile, "port"));
    let socket = address.to_socket_addrs().ok()?.next()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&socket, Duration::from_millis(1500)).ok()?;
    Some(start.elapsed().as_millis())
}

pub fn process_name(connection: &Value) -> String {
    string(&connection["metadata"], "processPath")
        .split(" (")
        .next()
        .unwrap_or("")
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_owned()
}
pub fn visible_connections(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .filter(|v| {
            !["xray.exe", "sing-box.exe", "xray", "sing-box"]
                .contains(&process_name(v).to_lowercase().as_str())
        })
        .cloned()
        .collect()
}
pub fn format_bytes(bytes: u64) -> String {
    let mut amount = bytes as f64;
    for unit in ["Б", "КБ", "МБ", "ГБ", "ТБ"] {
        if amount < 1024.0 || unit == "ТБ" {
            return format!("{amount:.1} {unit}")
                .replace(".0 ", " ")
                .replace('.', ",");
        }
        amount /= 1024.0;
    }
    unreachable!()
}

pub struct Speeds {
    previous: std::collections::HashMap<String, (u64, u64)>,
    at: Option<Instant>,
}
impl Speeds {
    pub fn new() -> Self {
        Self {
            previous: Default::default(),
            at: None,
        }
    }
    pub fn update(&mut self, items: &mut [Value]) {
        let now = Instant::now();
        let seconds = self
            .at
            .map(|at| now.duration_since(at).as_secs_f64())
            .unwrap_or(0.0)
            .max(0.001);
        let mut next = std::collections::HashMap::new();
        for item in items {
            let id = string(item, "id").to_owned();
            let up = number(item, "upload");
            let down = number(item, "download");
            let old = self.previous.get(&id).copied();
            item["upSpeed"] = json!(old
                .map(|(u, _)| (up.saturating_sub(u) as f64 / seconds) as u64)
                .unwrap_or(0));
            item["downSpeed"] = json!(old
                .map(|(_, d)| (down.saturating_sub(d) as f64 / seconds) as u64)
                .unwrap_or(0));
            next.insert(id, (up, down));
        }
        self.previous = next;
        self.at = Some(now);
    }
}

pub fn connection_key(item: &Value, column: &str) -> String {
    match column {
        "host" => {
            let host = string(&item["metadata"], "host");
            if host.is_empty() {
                string(&item["metadata"], "destinationIP").to_lowercase()
            } else {
                host.to_lowercase()
            }
        }
        "process" => process_name(item).to_lowercase(),
        "outbound" => item["chains"]
            .as_array()
            .and_then(|v| v.last())
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vless_and_sidecar() {
        let profile = profile_from_link(
            "vless://abc@host.example:443?security=reality&pbk=public&type=xhttp#Test",
        )
        .unwrap();
        assert!(needs_sidecar(&profile));
        assert_eq!(profile["name"], "Test");
        assert_eq!(profile_outbound(&profile)["type"], "socks");
    }
    #[test]
    fn connection_units_and_filter() {
        assert_eq!(format_bytes(1_048_576), "1 МБ");
        assert!(
            visible_connections(&[json!({"metadata":{"processPath":"C:\\xray.exe"}})]).is_empty()
        );
    }
    #[test]
    fn route_rule_uses_selected_field_and_target() {
        let profile = profile_from_link("vless://abc@example.com:443?security=tls").unwrap();
        let config = build_config(
            &profile,
            &[
                json!({"kind":"domainSuffix","value":"example.org","target":"direct","enabled":true}),
            ],
            "secret",
            true,
        );
        assert_eq!(
            config["route"]["rules"][3]["domain_suffix"],
            json!(["example.org"])
        );
        assert_eq!(config["route"]["rules"][3]["outbound"], "direct");
        assert_eq!(
            config["dns"]["rules"][1]["domain_suffix"],
            json!(["example.org"])
        );
    }
    #[test]
    fn speeds_use_counter_deltas() {
        let mut tracker = Speeds::new();
        let mut first = vec![json!({"id":"1","upload":10,"download":20})];
        tracker.update(&mut first);
        tracker.at = Some(Instant::now() - Duration::from_secs(2));
        let mut second = vec![json!({"id":"1","upload":1034,"download":2068})];
        tracker.update(&mut second);
        assert!((500..=520).contains(&second[0]["upSpeed"].as_u64().unwrap()));
        assert!((1000..=1030).contains(&second[0]["downSpeed"].as_u64().unwrap()));
    }
    #[test]
    fn clash_ws_import_keeps_host_header() {
        let yaml="proxies:\n  - name: Demo\n    type: vless\n    server: example.com\n    port: 443\n    uuid: abc\n    tls: true\n    network: ws\n    ws-opts:\n      path: /proxy\n      headers:\n        Host: front.example\n";
        let profiles = parse_profiles(yaml).unwrap();
        assert_eq!(profiles[0]["host"], "front.example");
        assert_eq!(profiles[0]["path"], "/proxy");
    }
    #[test]
    fn installed_sing_box_accepts_generated_config() {
        if Command::new("sing-box").arg("version").output().is_err() {
            return;
        }
        let profile = profile_from_link(
            "vless://00000000-0000-4000-8000-000000000000@example.com:443?security=tls",
        )
        .unwrap();
        let config = build_config(&profile, &[], "test-secret", true);
        let path = std::env::temp_dir().join(format!("fork-config-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let output = Command::new("sing-box")
            .args(["check", "-c"])
            .arg(&path)
            .output()
            .unwrap();
        let _ = fs::remove_file(&path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn installed_xray_accepts_sidecar_config() {
        if Command::new("xray").arg("version").output().is_err() {
            return;
        }
        let profile=profile_from_link("vless://00000000-0000-4000-8000-000000000000@example.com:443?security=tls&type=xhttp&path=%2Fproxy").unwrap();
        let config = sidecar_config(&profile).unwrap();
        let path = std::env::temp_dir().join(format!("fork-xray-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let output = Command::new("xray")
            .args(["run", "-test", "-config"])
            .arg(&path)
            .output()
            .unwrap();
        let _ = fs::remove_file(&path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
