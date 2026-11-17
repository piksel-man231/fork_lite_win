use crate::core::{self, Runtime, Speeds};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(15, 18, 27);
const CARD: Color32 = Color32::from_rgb(25, 30, 42);
const SURFACE: Color32 = Color32::from_rgb(33, 39, 53);
const MUTED: Color32 = Color32::from_rgb(147, 159, 182);
const ACCENT: Color32 = Color32::from_rgb(113, 167, 255);
const GREEN: Color32 = Color32::from_rgb(90, 212, 165);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
enum Tab {
    Servers,
    Rules,
    Connections,
    Settings,
}

pub struct ForkApp {
    logo: egui::TextureHandle,
    settings: Value,
    store: Value,
    runtime: Runtime,
    tab: Tab,
    input: String,
    message: String,
    connections: Vec<Value>,
    speeds: Speeds,
    last_poll: Instant,
    sort: String,
    descending: bool,
    dragging_rule: Option<usize>,
    editing: Option<String>,
    ping: std::collections::HashMap<String, Option<u128>>,
    download_total: u64,
    upload_total: u64,
    autostart: bool,
    auto_connect_pending: bool,
    #[cfg(windows)]
    tray: Option<Tray>,
    #[cfg(windows)]
    exit_requested: bool,
}

#[cfg(windows)]
struct Tray {
    _icon: tray_icon::TrayIcon,
    toggle: tray_icon::menu::MenuItem,
    next: tray_icon::menu::MenuItem,
    show: tray_icon::menu::MenuItem,
    quit: tray_icon::menu::MenuItem,
}

#[cfg(windows)]
fn create_tray() -> Option<Tray> {
    use tray_icon::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        Icon, TrayIconBuilder,
    };
    let png = image::load_from_memory(include_bytes!("../Assets/AppIcon.png"))
        .ok()?
        .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let icon = Icon::from_rgba(png.clone().into_raw(), 32, 32).ok()?;
    let menu = Menu::new();
    let show = MenuItem::new("Открыть Fork", true, None);
    let toggle = MenuItem::new("Подключить", true, None);
    let next = MenuItem::new("Следующий сервер", true, None);
    let quit = MenuItem::new("Выход", true, None);
    menu.append(&show).ok()?;
    menu.append(&toggle).ok()?;
    menu.append(&next).ok()?;
    menu.append(&PredefinedMenuItem::separator()).ok()?;
    menu.append(&quit).ok()?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("Fork Lite Win")
        .with_menu(Box::new(menu))
        .build()
        .ok()?;
    Some(Tray {
        _icon: tray,
        toggle,
        next,
        show,
        quit,
    })
}

impl ForkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.widgets.inactive.bg_fill = SURFACE;
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 56, 78);
        style.visuals.widgets.active.bg_fill = ACCENT;
        style.visuals.selection.bg_fill = Color32::from_rgb(55, 91, 145);
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 9.0);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let logo_png = image::load_from_memory(include_bytes!("../Assets/AppIcon.png"))
            .expect("logo")
            .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        let logo = cc.egui_ctx.load_texture(
            "fork-logo",
            egui::ColorImage::from_rgba_unmultiplied([64, 64], logo_png.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        let settings = core::read_json("settings.json", core::default_settings());
        let auto_connect_pending = cfg!(windows) && settings["autoConnect"] == true;
        Self {
            logo,
            settings,
            store: core::read_json(
                "profiles.json",
                json!({"items":[],"selected":null,"subscriptions":[]}),
            ),
            runtime: Runtime::new(),
            tab: Tab::Servers,
            input: String::new(),
            message: String::new(),
            connections: Vec::new(),
            speeds: Speeds::new(),
            last_poll: Instant::now(),
            sort: "download".into(),
            descending: true,
            dragging_rule: None,
            editing: None,
            ping: Default::default(),
            download_total: 0,
            upload_total: 0,
            autostart: crate::autostart::is_enabled(),
            auto_connect_pending,
            #[cfg(windows)]
            tray: create_tray(),
            #[cfg(windows)]
            exit_requested: false,
        }
    }
    fn save_settings(&mut self) {
        if let Err(e) = core::write_json("settings.json", &self.settings) {
            self.message = e;
        }
    }
    fn save_store(&mut self) {
        if let Err(e) = core::write_json("profiles.json", &self.store) {
            self.message = e;
        }
    }
    fn items(&self) -> Vec<Value> {
        self.store["items"].as_array().cloned().unwrap_or_default()
    }
    fn rule_items(&self) -> Vec<Value> {
        self.settings["rules"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
    fn selected_profile(&self) -> Option<Value> {
        self.items()
            .into_iter()
            .find(|v| v["id"] == self.store["selected"])
    }
    fn toggle(&mut self) {
        if self.runtime.running() {
            self.runtime.stop();
            self.connections.clear();
            self.message = "Отключено".into();
            return;
        }
        if self.settings["autoBest"] == true {
            let mut best: Option<(u128, String)> = None;
            for profile in self.items() {
                if let Some(ms) = core::probe(&profile) {
                    let id = profile["id"].as_str().unwrap_or("").to_owned();
                    self.ping.insert(id.clone(), Some(ms));
                    if best.as_ref().is_none_or(|(current, _)| ms < *current) {
                        best = Some((ms, id));
                    }
                }
            }
            if let Some((_, id)) = best {
                self.store["selected"] = json!(id);
                self.save_store();
            }
        }
        let Some(profile) = self.selected_profile() else {
            self.message = "Добавьте и выберите сервер".into();
            return;
        };
        match self.runtime.start(&profile, &self.rule_items()) {
            Ok(()) => self.message = "Подключено".into(),
            Err(e) => self.message = e,
        }
    }
    fn maybe_auto_connect(&mut self) {
        if self.auto_connect_pending {
            self.auto_connect_pending = false;
            self.toggle();
        }
    }
    fn import(&mut self) {
        let raw = self.input.trim().to_owned();
        if raw.is_empty() {
            return;
        }
        let result = if raw.starts_with("https://") {
            core::fetch_subscription(&raw)
        } else {
            core::parse_profiles(&raw)
        };
        match result {
            Ok(mut incoming) => {
                let count = incoming.len();
                if raw.starts_with("https://") {
                    let previous = self.items();
                    for profile in &mut incoming {
                        profile["sourceURL"] = json!(raw);
                        if let Some(existing) = previous.iter().find(|old| {
                            old["sourceURL"] == raw
                                && ["server", "port", "uuid", "flow", "network", "security"]
                                    .iter()
                                    .all(|k| old.get(*k) == profile.get(*k))
                        }) {
                            profile["id"] = existing["id"].clone();
                        }
                    }
                    let mut kept: Vec<_> = previous
                        .into_iter()
                        .filter(|v| v["sourceURL"] != raw)
                        .collect();
                    kept.extend(incoming);
                    self.store["items"] = json!(kept);
                    let subscriptions = self.store["subscriptions"].as_array_mut().unwrap();
                    if !subscriptions.iter().any(|v| v == &raw) {
                        subscriptions.push(json!(raw));
                    }
                } else {
                    self.store["items"].as_array_mut().unwrap().extend(incoming);
                }
                if self.selected_profile().is_none() {
                    self.store["selected"] = self.store["items"][0]["id"].clone();
                }
                self.input.clear();
                self.save_store();
                self.message = format!("Добавлено серверов: {count}");
            }
            Err(e) => self.message = e,
        }
    }
    fn poll(&mut self) {
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_poll = Instant::now();
        if self.runtime.running() {
            if let Ok(snapshot) = self.runtime.connections() {
                self.download_total = snapshot["downloadTotal"].as_u64().unwrap_or(0);
                self.upload_total = snapshot["uploadTotal"].as_u64().unwrap_or(0);
                let mut items = core::visible_connections(
                    snapshot["connections"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                );
                self.speeds.update(&mut items);
                self.connections = items;
            }
        } else {
            self.connections.clear();
        }
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        let active = self.runtime.running();
        ui.horizontal(|ui| {
            ui.image((self.logo.id(), Vec2::splat(50.0)));
            ui.vertical(|ui| {
                ui.label(RichText::new("Fork Lite").size(25.0).strong());
                ui.label(RichText::new("WINDOWS · VLESS").size(10.0).color(MUTED));
            });
            ui.add_space((ui.available_width() - 130.0).max(0.0));
            ui.label(
                RichText::new(if active {
                    "Подключено"
                } else {
                    "Отключено"
                })
                .color(if active { GREEN } else { MUTED }),
            );
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            for (tab, title) in [
                (Tab::Servers, "Серверы"),
                (Tab::Rules, "Правила"),
                (Tab::Connections, "Соединения"),
                (Tab::Settings, "Настройки"),
            ] {
                let selected = self.tab == tab;
                if ui
                    .selectable_label(
                        selected,
                        RichText::new(title).size(15.0).color(if selected {
                            ACCENT
                        } else {
                            MUTED
                        }),
                    )
                    .clicked()
                {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
    }
    fn servers(&mut self, ui: &mut egui::Ui) {
        let running = self.runtime.running();
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(14)
            .inner_margin(22)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("Подключение").size(21.0).strong());
                ui.add_space(4.0);
                let selected = self.selected_profile();
                ui.label(
                    RichText::new(
                        selected
                            .as_ref()
                            .and_then(|p| p["name"].as_str())
                            .unwrap_or("Сервер не выбран"),
                    )
                    .size(17.0)
                    .color(ACCENT),
                );
                if let Some(p) = selected {
                    ui.label(
                        RichText::new(format!(
                            "{}:{} · {} · {}",
                            p["server"].as_str().unwrap_or(""),
                            p["port"],
                            p["network"].as_str().unwrap_or(""),
                            p["security"].as_str().unwrap_or("")
                        ))
                        .color(MUTED),
                    );
                }
                ui.add_space(13.0);
                let text = if running {
                    "Отключить"
                } else {
                    "Подключить"
                };
                if ui
                    .add_sized(
                        [180.0, 39.0],
                        egui::Button::new(RichText::new(text).size(15.0).strong())
                            .fill(if running { SURFACE } else { ACCENT }),
                    )
                    .clicked()
                {
                    self.toggle();
                }
            });
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            ui.heading("Мои серверы");
            ui.add_space(12.0);
            let mut auto = self.settings["autoBest"].as_bool().unwrap_or(true);
            if ui.checkbox(&mut auto, "Выбирать быстрый").changed() {
                self.settings["autoBest"] = json!(auto);
                self.save_settings();
            }
        });
        egui::ScrollArea::vertical()
            .max_height(250.0)
            .show(ui, |ui| {
                for profile in self.items() {
                    let id = profile["id"].as_str().unwrap_or("").to_owned();
                    let selected = self.store["selected"] == profile["id"];
                    egui::Frame::new()
                        .fill(if selected {
                            Color32::from_rgb(30, 45, 67)
                        } else {
                            CARD
                        })
                        .corner_radius(10)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let name_width = (ui.available_width() - 190.0).max(100.0);
                                if ui
                                    .add_sized(
                                        [name_width, 29.0],
                                        egui::Button::new(
                                            RichText::new(
                                                profile["name"].as_str().unwrap_or("Сервер"),
                                            )
                                            .size(15.0)
                                            .strong(),
                                        )
                                        .selected(selected)
                                        .truncate(),
                                    )
                                    .clicked()
                                {
                                    self.store["selected"] = json!(id);
                                    self.save_store();
                                    if running {
                                        self.runtime.stop();
                                        self.message = "Сервер изменён. Подключите снова".into();
                                    }
                                }
                                let ping = self
                                    .ping
                                    .get(&id)
                                    .and_then(|v| *v)
                                    .map(|ms| format!("{ms} мс"))
                                    .unwrap_or("—".into());
                                ui.label(RichText::new(ping).color(MUTED));
                                if ui
                                    .small_button("Тест")
                                    .on_hover_text("Проверить задержку")
                                    .clicked()
                                {
                                    self.ping.insert(id.clone(), core::probe(&profile));
                                }
                                if ui
                                    .small_button("X")
                                    .on_hover_text("Удалить сервер")
                                    .clicked()
                                {
                                    self.store["items"]
                                        .as_array_mut()
                                        .unwrap()
                                        .retain(|v| v["id"] != id);
                                    if self.store["selected"] == id {
                                        self.store["selected"] = self.store["items"]
                                            .as_array()
                                            .and_then(|v| v.first())
                                            .map(|v| v["id"].clone())
                                            .unwrap_or(Value::Null);
                                    }
                                    self.save_store();
                                }
                            });
                        });
                    ui.add_space(5.0);
                }
            });
        ui.add_space(9.0);
        ui.label(
            RichText::new("Добавить VLESS-ссылку, список, Clash YAML или HTTPS-подписку")
                .color(MUTED),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.input)
                .desired_rows(3)
                .hint_text("vless://… или https://…")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.button("Добавить").clicked() {
                self.import();
            }
            if ui.button("Вставить").clicked() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        self.input = text;
                    }
                }
            }
            if ui.button("Проверить все").clicked() {
                for profile in self.items() {
                    if let Some(id) = profile["id"].as_str() {
                        self.ping.insert(id.to_owned(), core::probe(&profile));
                    }
                }
            }
        });
    }
    fn rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Правила маршрутизации");
        ui.label(
            RichText::new("Сверху вниз · перетащите строку, чтобы изменить приоритет").color(MUTED),
        );
        ui.add_space(10.0);
        let mut rules = self.rule_items();
        let mut changed = false;
        let mut remove = None;
        let mut move_to = None;
        ui.scope(|ui| {
            for index in 0..rules.len() {
                let id = rules[index]["id"].as_str().unwrap_or("").to_owned();
                let mut active = rules[index]["enabled"].as_bool().unwrap_or(true);
                let row = egui::Frame::new()
                    .fill(CARD)
                    .corner_radius(9)
                    .inner_margin(9)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let handle = drag_handle(ui);
                            if handle.drag_started() {
                                self.dragging_rule = Some(index);
                            }
                            if ui.checkbox(&mut active, "").changed() {
                                rules[index]["enabled"] = json!(active);
                                changed = true;
                            }
                            let kind = rules[index]["kind"].as_str().unwrap_or("domain").to_owned();
                            egui::ComboBox::from_id_salt(format!("kind-{id}"))
                                .selected_text(kind_label(&kind))
                                .width(145.0)
                                .show_ui(ui, |ui| {
                                    for (key, label) in kinds() {
                                        if ui.selectable_label(kind == key, label).clicked() {
                                            rules[index]["kind"] = json!(key);
                                            changed = true;
                                        }
                                    }
                                });
                            let target = rules[index]["target"]
                                .as_str()
                                .unwrap_or("direct")
                                .to_owned();
                            egui::ComboBox::from_id_salt(format!("target-{id}"))
                                .selected_text(target_label(&target))
                                .width(115.0)
                                .show_ui(ui, |ui| {
                                    for (key, label) in [
                                        ("xray", "Xray"),
                                        ("direct", "Напрямую"),
                                        ("block", "Блок"),
                                    ] {
                                        if ui.selectable_label(target == key, label).clicked() {
                                            rules[index]["target"] = json!(key);
                                            changed = true;
                                        }
                                    }
                                });
                            if ui.small_button("X").clicked() {
                                remove = Some(index);
                            }
                        });
                        let value = rules[index]["value"].as_str().unwrap_or("").to_owned();
                        if self.editing.as_deref() == Some(&id) {
                            let mut edited = value;
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut edited)
                                    .desired_width(ui.available_width()),
                            );
                            if response.changed() {
                                rules[index]["value"] = json!(edited);
                                changed = true;
                            }
                            if response.lost_focus() {
                                self.editing = None;
                            }
                        } else if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(if value.is_empty() {
                                        "Дважды нажмите для ввода"
                                    } else {
                                        &value
                                    })
                                    .color(
                                        if value.is_empty() {
                                            MUTED
                                        } else {
                                            Color32::WHITE
                                        },
                                    ),
                                )
                                .truncate()
                                .sense(Sense::click()),
                            )
                            .double_clicked()
                        {
                            self.editing = Some(id.clone());
                        }
                    })
                    .response;
                if row.hovered() && ui.input(|i| i.pointer.any_released()) {
                    if let Some(from) = self.dragging_rule.take() {
                        if from != index {
                            move_to = Some((from, index));
                        }
                    }
                }
                ui.add_space(6.0);
            }
        });
        if ui.input(|i| i.pointer.any_released()) {
            self.dragging_rule = None;
        }
        if let Some((from, to)) = move_to {
            let rule = rules.remove(from);
            rules.insert(to, rule);
            changed = true;
        }
        if let Some(index) = remove {
            rules.remove(index);
            changed = true;
        }
        if ui.button("+ Добавить правило").clicked() {
            let id = uuid::Uuid::new_v4().to_string();
            rules.push(json!({"id":id,"kind":"domain","value":"","target":"xray","enabled":true}));
            self.editing = Some(id);
            changed = true;
        }
        if changed {
            self.settings["rules"] = json!(rules);
            self.save_settings();
            if self.runtime.running() {
                self.runtime.stop();
                self.message = "Правила изменены. Подключите снова".into();
            }
        }
    }
    fn sort_by(&mut self, column: &str) {
        if self.sort == column {
            self.descending = !self.descending;
        } else {
            self.sort = column.into();
            self.descending = matches!(column, "download" | "upload" | "downSpeed" | "upSpeed");
        }
    }
    fn connections(&mut self, ui: &mut egui::Ui) {
        ui.heading("Соединения");
        ui.label(
            RichText::new(format!(
                "Получено: {}    Отдано: {}",
                core::format_bytes(self.download_total),
                core::format_bytes(self.upload_total)
            ))
            .color(MUTED),
        );
        ui.label(
            RichText::new(
                "Нажмите заголовок для сортировки · правый клик по строке создаёт правило",
            )
            .color(MUTED),
        );
        ui.add_space(10.0);
        let host_width = (ui.available_width() - 700.0).max(220.0);
        let columns = [
            ("host", "Адрес", host_width),
            ("process", "Процесс", 140.0),
            ("outbound", "Выход", 95.0),
            ("download", "Получено", 95.0),
            ("upload", "Отдано", 95.0),
            ("downSpeed", "Вход/с", 105.0),
            ("upSpeed", "Выход/с", 105.0),
        ];
        let numeric = ["download", "upload", "downSpeed", "upSpeed"].contains(&self.sort.as_str());
        self.connections.sort_by(|a, b| {
            if numeric {
                a[&self.sort]
                    .as_u64()
                    .unwrap_or(0)
                    .cmp(&b[&self.sort].as_u64().unwrap_or(0))
            } else {
                core::connection_key(a, &self.sort).cmp(&core::connection_key(b, &self.sort))
            }
        });
        if self.descending {
            self.connections.reverse();
        }
        let mut quick_rule = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.set_min_width(columns.iter().map(|(_, _, width)| width).sum::<f32>() + 95.0);
            ui.horizontal(|ui| {
                for (key, label, width) in columns {
                    let response = ui.add_sized(
                        [width, 27.0],
                        egui::Button::new(RichText::new(label).color(MUTED)).frame(false),
                    );
                    if self.sort == key {
                        paint_sort_indicator(ui, response.rect, self.descending);
                    }
                    if response.clicked() {
                        self.sort_by(key);
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(470.0)
                .show(ui, |ui| {
                    for item in &self.connections {
                        let metadata = &item["metadata"];
                        let host = metadata["host"]
                            .as_str()
                            .filter(|v| !v.is_empty())
                            .or_else(|| metadata["destinationIP"].as_str())
                            .unwrap_or("—");
                        let process = core::process_name(item);
                        let chain = item["chains"]
                            .as_array()
                            .and_then(|v| v.last())
                            .and_then(Value::as_str)
                            .unwrap_or("—");
                        let cells = [
                            host.to_owned(),
                            process.clone(),
                            chain.to_owned(),
                            core::format_bytes(item["download"].as_u64().unwrap_or(0)),
                            core::format_bytes(item["upload"].as_u64().unwrap_or(0)),
                            format!(
                                "{}/с",
                                core::format_bytes(item["downSpeed"].as_u64().unwrap_or(0))
                            ),
                            format!(
                                "{}/с",
                                core::format_bytes(item["upSpeed"].as_u64().unwrap_or(0))
                            ),
                        ];
                        let response = egui::Frame::new()
                            .fill(CARD)
                            .corner_radius(6)
                            .inner_margin(5)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (idx, cell) in cells.iter().enumerate() {
                                        ui.add_sized(
                                            [columns[idx].2, 24.0],
                                            egui::Label::new(cell).truncate(),
                                        );
                                    }
                                });
                            })
                            .response;
                        response.context_menu(|ui| {
                            ui.label("Направить адрес");
                            let address_rule = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                                (
                                    "ipCidr",
                                    format!("{ip}/{}", if ip.is_ipv4() { 32 } else { 128 }),
                                )
                            } else {
                                ("domain", host.to_owned())
                            };
                            for (target, label) in [
                                ("xray", "Через Xray"),
                                ("direct", "Напрямую"),
                                ("block", "Блокировать"),
                            ] {
                                if ui.button(label).clicked() {
                                    quick_rule =
                                        Some((address_rule.0, address_rule.1.clone(), target));
                                    ui.close();
                                }
                            }
                            if !process.is_empty() {
                                ui.separator();
                                if ui.button("Правило для процесса").clicked() {
                                    quick_rule = Some(("processName", process.clone(), "direct"));
                                    ui.close();
                                }
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
        });
        if let Some((kind, value, target)) = quick_rule {
            self.settings["rules"].as_array_mut().unwrap().push(json!({"id":uuid::Uuid::new_v4().to_string(),"kind":kind,"value":value,"target":target,"enabled":true}));
            self.save_settings();
            self.message = "Правило добавлено. Переподключитесь для применения".into();
        }
        if self.connections.is_empty() {
            ui.add_space(35.0);
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Активных соединений пока нет").color(MUTED));
            });
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Настройки");
        ui.label(RichText::new("Поведение приложения при запуске Windows").color(MUTED));
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(12)
            .inner_margin(18)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let mut autostart = self.autostart;
                ui.add_enabled_ui(cfg!(windows), |ui| {
                    if ui.checkbox(&mut autostart, "Запускать при входе в Windows").changed() {
                        match crate::autostart::set_enabled(autostart) {
                            Ok(()) => {
                                self.autostart = crate::autostart::is_enabled();
                                self.message = if self.autostart {
                                    "Автозапуск включён".into()
                                } else {
                                    "Автозапуск выключен".into()
                                };
                            }
                            Err(error) => self.message = error,
                        }
                    }
                });
                ui.label(
                    RichText::new("Приложение откроется с правами администратора через Планировщик заданий.")
                        .color(MUTED),
                );
                ui.add_space(14.0);
                let mut auto_connect = self.settings["autoConnect"].as_bool().unwrap_or(false);
                if ui.checkbox(&mut auto_connect, "Подключаться при запуске").changed() {
                    self.settings["autoConnect"] = json!(auto_connect);
                    self.save_settings();
                }
                ui.label(
                    RichText::new("После запуска выберет сервер и включит TUN. Если соединение не удастся, покажет ошибку в окне.")
                        .color(MUTED),
                );
            });
    }
    #[cfg(windows)]
    fn handle_tray(&mut self, ctx: &egui::Context) {
        use tray_icon::menu::MenuEvent;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(tray) = &self.tray {
                let id = event.id();
                if id == tray.quit.id() {
                    self.runtime.stop();
                    self.exit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else if id == tray.show.id() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                } else if id == tray.toggle.id() {
                    self.toggle();
                } else if id == tray.next.id() {
                    let items = self.items();
                    if !items.is_empty() {
                        let current = items
                            .iter()
                            .position(|p| p["id"] == self.store["selected"])
                            .unwrap_or(0);
                        self.store["selected"] = items[(current + 1) % items.len()]["id"].clone();
                        self.save_store();
                        if self.runtime.running() {
                            self.runtime.stop();
                            self.message = "Сервер изменён. Подключите снова".into();
                        }
                    }
                }
            }
        }
        if let Some(tray) = &self.tray {
            tray.toggle.set_text(if self.runtime.running() {
                "Отключить"
            } else {
                "Подключить"
            });
        }
    }
}

fn drag_handle(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(20.0, 22.0), Sense::drag());
    let center = rect.center();
    for offset in [-5.0, 0.0, 5.0] {
        ui.painter().line_segment(
            [
                egui::pos2(center.x - 6.0, center.y + offset),
                egui::pos2(center.x + 6.0, center.y + offset),
            ],
            Stroke::new(1.5, MUTED),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::Grab)
}

fn paint_sort_indicator(ui: &egui::Ui, rect: egui::Rect, descending: bool) {
    let x = rect.right() - 9.0;
    let y = rect.center().y;
    let points = if descending {
        vec![
            egui::pos2(x - 4.0, y - 2.0),
            egui::pos2(x + 4.0, y - 2.0),
            egui::pos2(x, y + 3.0),
        ]
    } else {
        vec![
            egui::pos2(x - 4.0, y + 2.0),
            egui::pos2(x + 4.0, y + 2.0),
            egui::pos2(x, y - 3.0),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(points, ACCENT, Stroke::NONE));
}

fn kinds() -> [(&'static str, &'static str); 6] {
    [
        ("domain", "Домен"),
        ("domainSuffix", "Суффикс домена"),
        ("domainKeyword", "Содержит"),
        ("ipCidr", "IP-подсеть"),
        ("processName", "Процесс"),
        ("port", "Порт"),
    ]
}
fn kind_label(key: &str) -> &str {
    kinds()
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("Домен")
}
fn target_label(key: &str) -> &str {
    match key {
        "xray" => "Xray",
        "direct" => "Напрямую",
        "block" => "Блок",
        _ => "Xray",
    }
}

impl eframe::App for ForkApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.maybe_auto_connect();
        self.poll();
        #[cfg(windows)]
        self.handle_tray(ctx);
        ctx.request_repaint_after(Duration::from_millis(500));
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.maybe_auto_connect();
        self.poll();
        #[cfg(windows)]
        self.handle_tray(&ctx);
        #[cfg(windows)]
        if ctx.input(|i| i.viewport().close_requested())
            && self.tray.is_some()
            && !self.exit_requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24))
            .show(ui, |ui| {
                self.header(ui);
                egui::ScrollArea::vertical()
                    .id_salt(self.tab)
                    .show(ui, |ui| {
                        ui.add_space(12.0);
                        match self.tab {
                            Tab::Servers => self.servers(ui),
                            Tab::Rules => self.rules(ui),
                            Tab::Connections => self.connections(ui),
                            Tab::Settings => self.settings(ui),
                        }
                        if !self.message.is_empty() {
                            ui.add_space(12.0);
                            egui::Frame::new()
                                .fill(SURFACE)
                                .corner_radius(7)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.label(RichText::new(&self.message).color(ACCENT));
                                });
                        }
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
