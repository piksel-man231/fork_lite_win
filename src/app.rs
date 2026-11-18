use crate::core::{self, Runtime, Speeds};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(17, 18, 21);
const CARD: Color32 = Color32::from_rgb(27, 29, 34);
const SURFACE: Color32 = Color32::from_rgb(38, 41, 48);
const BORDER: Color32 = Color32::from_rgb(53, 56, 65);
const MUTED: Color32 = Color32::from_rgb(151, 156, 167);
const ACCENT: Color32 = Color32::from_rgb(112, 170, 255);
const GREEN: Color32 = Color32::from_rgb(108, 211, 159);

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
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "inter".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../Assets/InterVariable.ttf"
            ))),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "inter".into());
        cc.egui_ctx.set_fonts(fonts);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.extreme_bg_color = Color32::from_rgb(22, 24, 28);
        style.visuals.faint_bg_color = Color32::from_rgb(32, 35, 41);
        style.visuals.selection.bg_fill = Color32::from_rgb(47, 91, 151);
        style.visuals.widgets.inactive.bg_fill = SURFACE;
        style.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(226, 229, 235));
        style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.inactive.corner_radius = 8.into();
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 55, 65);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 55, 65);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(77, 86, 102));
        style.visuals.widgets.hovered.corner_radius = 8.into();
        style.visuals.widgets.active.bg_fill = ACCENT;
        style.visuals.widgets.active.corner_radius = 8.into();
        style.visuals.window_corner_radius = 12.into();
        style.visuals.menu_corner_radius = 10.into();
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.interact_size.y = 32.0;
        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::proportional(22.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
            (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
        ]
        .into();
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
        let can_toggle = active || self.selected_profile().is_some();
        ui.horizontal(|ui| {
            ui.image((self.logo.id(), Vec2::splat(36.0)));
            ui.vertical(|ui| {
                ui.label(RichText::new("Fork Lite").size(17.0).strong());
                ui.label(RichText::new("Сеть под контролем").size(11.0).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(
                    RichText::new(if active {
                        "Отключить VPN"
                    } else {
                        "Подключить VPN"
                    })
                    .color(if active {
                        GREEN
                    } else if can_toggle {
                        ACCENT
                    } else {
                        MUTED
                    }),
                )
                .fill(CARD)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(20);
                if ui
                    .add_enabled(can_toggle, button)
                    .on_disabled_hover_text("Сначала выберите сервер")
                    .clicked()
                {
                    self.toggle();
                }
            });
        });
        ui.add_space(20.0);
        ui.horizontal_wrapped(|ui| {
            for (tab, title) in [
                (Tab::Servers, "Серверы"),
                (Tab::Rules, "Правила"),
                (Tab::Connections, "Соединения"),
                (Tab::Settings, "Настройки"),
            ] {
                let selected = self.tab == tab;
                let button = egui::Button::new(RichText::new(title).color(if selected {
                    Color32::WHITE
                } else {
                    MUTED
                }))
                .fill(if selected {
                    SURFACE
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if selected {
                    Stroke::new(1.0, BORDER)
                } else {
                    Stroke::NONE
                });
                if ui.add(button).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.add_space(12.0);
        ui.separator();
    }
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.image((self.logo.id(), Vec2::splat(34.0)));
            ui.label(RichText::new("Fork Lite").size(17.0).strong());
        });
        ui.add_space(30.0);
        ui.label(RichText::new("Навигация").size(11.0).color(MUTED));
        ui.add_space(8.0);
        for (tab, title) in [
            (Tab::Servers, "Серверы"),
            (Tab::Rules, "Правила"),
            (Tab::Connections, "Соединения"),
            (Tab::Settings, "Настройки"),
        ] {
            let selected = self.tab == tab;
            let button = egui::Button::new(RichText::new(title).color(if selected {
                Color32::WHITE
            } else {
                MUTED
            }))
            .fill(if selected {
                SURFACE
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE);
            if ui.add_sized([ui.available_width(), 38.0], button).clicked() {
                self.tab = tab;
            }
            ui.add_space(4.0);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let active = self.runtime.running();
            ui.label(
                RichText::new(if active {
                    "●  Подключено"
                } else {
                    "●  Отключено"
                })
                .color(if active { GREEN } else { MUTED })
                .size(12.0),
            );
            ui.add_space(14.0);
            let can_toggle = active || self.selected_profile().is_some();
            let button = egui::Button::new(
                RichText::new(if active {
                    "Отключить VPN"
                } else {
                    "Подключить VPN"
                })
                .strong()
                .color(if active {
                    Color32::WHITE
                } else if can_toggle {
                    BG
                } else {
                    MUTED
                }),
            )
            .fill(if active || !can_toggle {
                SURFACE
            } else {
                ACCENT
            });
            let width = ui.available_width();
            if ui
                .add_enabled_ui(can_toggle, |ui| ui.add_sized([width, 38.0], button))
                .inner
                .on_disabled_hover_text("Сначала выберите сервер")
                .clicked()
            {
                self.toggle();
            }
            ui.add_space(14.0);
            ui.separator();
        });
    }
    fn body(&mut self, ui: &mut egui::Ui) {
        if self.tab == Tab::Connections {
            ui.add_space(12.0);
            self.connections(ui);
        } else {
            egui::ScrollArea::vertical()
                .id_salt(self.tab)
                .show(ui, |ui| {
                    ui.add_space(12.0);
                    let content_width = ui.available_width().min(860.0);
                    let side_space = ((ui.available_width() - content_width) / 2.0).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(side_space);
                        ui.vertical(|ui| {
                            ui.set_width(content_width);
                            match self.tab {
                                Tab::Servers => self.servers(ui),
                                Tab::Rules => self.rules(ui),
                                Tab::Settings => self.settings(ui),
                                Tab::Connections => unreachable!(),
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
                });
        }
    }
    fn servers(&mut self, ui: &mut egui::Ui) {
        let running = self.runtime.running();
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(14)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let selected = self.selected_profile();
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Подключение").size(11.0).color(MUTED));
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(
                                selected
                                    .as_ref()
                                    .and_then(|p| p["name"].as_str())
                                    .unwrap_or("Сервер не выбран"),
                            )
                            .size(20.0)
                            .strong(),
                        );
                        ui.add_space(2.0);
                        if let Some(p) = &selected {
                            ui.label(
                                RichText::new(format!(
                                    "{}:{}  ·  {}",
                                    p["server"].as_str().unwrap_or(""),
                                    p["port"],
                                    p["network"].as_str().unwrap_or("")
                                ))
                                .color(MUTED)
                                .size(12.0),
                            );
                        } else {
                            ui.label(
                                RichText::new("Выберите сервер из списка ниже")
                                    .color(MUTED)
                                    .size(12.0),
                            );
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = if running {
                            "Отключить"
                        } else {
                            "Подключить"
                        };
                        let can_connect = selected.is_some();
                        let button = egui::Button::new(
                            RichText::new(text)
                                .strong()
                                .color(if running || !can_connect { MUTED } else { BG }),
                        )
                        .fill(if running || !can_connect {
                            SURFACE
                        } else {
                            ACCENT
                        });
                        if ui
                            .add_enabled_ui(selected.is_some() || running, |ui| {
                                ui.add_sized([142.0, 38.0], button)
                            })
                            .inner
                            .on_disabled_hover_text("Сначала выберите сервер")
                            .clicked()
                        {
                            self.toggle();
                        }
                    });
                });
            });
        ui.add_space(25.0);
        ui.horizontal(|ui| {
            ui.heading("Серверы");
            ui.add_space(16.0);
            let mut auto = self.settings["autoBest"].as_bool().unwrap_or(true);
            if ui
                .checkbox(&mut auto, "Автовыбор")
                .on_hover_text("При подключении выбрать сервер с наименьшей задержкой")
                .changed()
            {
                self.settings["autoBest"] = json!(auto);
                self.save_settings();
            }
        });
        if self.items().is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Пока нет серверов. Добавьте VLESS-ссылку или подписку ниже.")
                    .color(MUTED),
            );
        }
        egui::ScrollArea::vertical()
            .max_height(330.0)
            .show(ui, |ui| {
                for profile in self.items() {
                    let id = profile["id"].as_str().unwrap_or("").to_owned();
                    let selected = self.store["selected"] == profile["id"];
                    egui::Frame::new()
                        .fill(if selected {
                            Color32::from_rgb(34, 43, 57)
                        } else {
                            CARD
                        })
                        .stroke(Stroke::new(
                            1.0,
                            if selected {
                                Color32::from_rgb(67, 105, 156)
                            } else {
                                BORDER
                            },
                        ))
                        .corner_radius(10)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let name_width = (ui.available_width() - 230.0).max(100.0);
                                if ui
                                    .add_sized(
                                        [name_width, 31.0],
                                        egui::Button::new(
                                            RichText::new(
                                                profile["name"].as_str().unwrap_or("Сервер"),
                                            )
                                            .size(14.0)
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
                                    .small_button("Пинг")
                                    .on_hover_text("Проверить задержку")
                                    .clicked()
                                {
                                    self.ping.insert(id.clone(), core::probe(&profile));
                                }
                                if ui
                                    .small_button("Удалить")
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
        ui.add_space(16.0);
        egui::CollapsingHeader::new("Добавить сервер или подписку")
            .default_open(self.items().is_empty())
            .show(ui, |ui| {
                ui.label(
                    RichText::new("VLESS-ссылка, список, Clash YAML или HTTPS-подписка")
                        .color(MUTED),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.input)
                        .desired_rows(2)
                        .hint_text("vless://… или https://…")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.input.trim().is_empty(),
                            egui::Button::new("Добавить").fill(if self.input.trim().is_empty() {
                                SURFACE
                            } else {
                                ACCENT
                            }),
                        )
                        .on_disabled_hover_text("Вставьте ссылку или конфигурацию")
                        .clicked()
                    {
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
            });
    }
    fn rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Правила маршрутизации");
        ui.label(
            RichText::new(
                "Правила применяются сверху вниз. Потяните за левый край, чтобы изменить порядок.",
            )
            .color(MUTED),
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
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(9)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            let handle = drag_handle(ui);
                            if handle.drag_started() {
                                self.dragging_rule = Some(index);
                            }
                            if ui.checkbox(&mut active, "Включено").changed() {
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
                            if ui.small_button("Удалить").clicked() {
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
                                        "Нажмите, чтобы задать значение"
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
                            .clicked()
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
        if ui
            .add(egui::Button::new("Добавить правило").fill(ACCENT))
            .clicked()
        {
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
        ui.label(RichText::new("Заголовок — сортировка · кнопка в строке — правило").color(MUTED));
        if !self.message.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(&self.message).color(ACCENT));
        }
        ui.add_space(10.0);
        let column_gap = ui.spacing().item_spacing.x;
        let fixed_width = 140.0 + 95.0 + 95.0 + 95.0 + 105.0 + 105.0;
        let host_width = (ui.available_width() - fixed_width - column_gap * 6.0 - 20.0)
            .max(220.0);
        let columns = [
            ("host", "Адрес", host_width),
            ("process", "Процесс", 140.0),
            ("outbound", "Выход", 95.0),
            ("download", "Получено", 95.0),
            ("upload", "Отдано", 95.0),
            ("downSpeed", "Вход/с", 105.0),
            ("upSpeed", "Выход/с", 105.0),
        ];
        let table_width = columns.iter().map(|(_, _, width)| width).sum::<f32>()
            + column_gap * (columns.len() - 1) as f32
            + 10.0;
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
            ui.set_min_width(table_width);
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
            let list_height = ui.available_height().max(80.0);
            egui::ScrollArea::vertical()
                .max_height(list_height)
                .min_scrolled_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(table_width);
                    if self.connections.is_empty() {
                        ui.add_space(35.0);
                        ui.label(RichText::new("Активных соединений пока нет").color(MUTED));
                    }
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
                                ui.set_min_width(table_width - 10.0);
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(columns[0].2, 24.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add_sized(
                                                [columns[0].2 - 90.0, 24.0],
                                                egui::Label::new(&cells[0]).truncate(),
                                            );
                                            ui.menu_button("Правило", |ui| {
                                                quick_rule_menu(
                                                    ui,
                                                    host,
                                                    &process,
                                                    &mut quick_rule,
                                                );
                                            });
                                        },
                                    );
                                    for (idx, cell) in cells.iter().enumerate().skip(1) {
                                        ui.add_sized(
                                            [columns[idx].2, 24.0],
                                            egui::Label::new(cell).truncate(),
                                        );
                                    }
                                });
                            })
                            .response;
                        response.container_context_menu(|ui| {
                            quick_rule_menu(ui, host, &process, &mut quick_rule);
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
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Настройки");
        ui.label(RichText::new("Управление запуском и подключением").color(MUTED));
        ui.add_space(20.0);
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("При входе в Windows").size(16.0).strong());
                ui.add_space(7.0);
                let mut autostart = self.autostart;
                ui.add_enabled_ui(cfg!(windows), |ui| {
                    if ui.checkbox(&mut autostart, "Запускать Fork Lite").changed() {
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
                    RichText::new("Приложение откроется с правами администратора.")
                        .color(MUTED)
                        .size(12.0),
                );
            });
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(12)
            .inner_margin(20)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("Подключение").size(16.0).strong());
                ui.add_space(7.0);
                let mut auto_connect = self.settings["autoConnect"].as_bool().unwrap_or(false);
                if ui
                    .checkbox(&mut auto_connect, "Подключаться автоматически")
                    .changed()
                {
                    self.settings["autoConnect"] = json!(auto_connect);
                    self.save_settings();
                }
                ui.label(
                    RichText::new("При запуске выберет сервер и включит TUN.")
                        .color(MUTED)
                        .size(12.0),
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

fn quick_rule_menu(
    ui: &mut egui::Ui,
    host: &str,
    process: &str,
    quick_rule: &mut Option<(&'static str, String, &'static str)>,
) {
    if host != "—" {
        let (kind, value) = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            (
                "ipCidr",
                format!("{ip}/{}", if ip.is_ipv4() { 32 } else { 128 }),
            )
        } else {
            ("domain", host.to_owned())
        };
        ui.label(format!("Адрес: {host}"));
        for (target, label) in [
            ("xray", "Через Xray"),
            ("direct", "Напрямую"),
            ("block", "Блокировать"),
        ] {
            if ui.button(label).clicked() {
                *quick_rule = Some((kind, value.clone(), target));
                ui.close();
            }
        }
    }
    if !process.is_empty() {
        if host != "—" {
            ui.separator();
        }
        ui.label(format!("Процесс: {process}"));
        for (target, label) in [
            ("xray", "Через Xray"),
            ("direct", "Напрямую"),
            ("block", "Блокировать"),
        ] {
            if ui.button(label).clicked() {
                *quick_rule = Some(("processName", process.to_owned(), target));
                ui.close();
            }
        }
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
        if ui.available_width() >= 900.0 {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG))
                .show(ui, |ui| {
                    egui::Panel::left("fork-sidebar")
                        .exact_size(194.0)
                        .resizable(false)
                        .frame(
                            egui::Frame::new()
                                .fill(CARD)
                                .inner_margin(egui::Margin::symmetric(16, 24))
                                .stroke(Stroke::new(1.0, BORDER)),
                        )
                        .show(ui, |ui| self.sidebar(ui));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(BG).inner_margin(24))
                        .show(ui, |ui| self.body(ui));
                });
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG).inner_margin(20))
                .show(ui, |ui| {
                    self.header(ui);
                    self.body(ui);
                });
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
