"""Fork for Windows. Run with python app.py or package with PyInstaller."""

from __future__ import annotations

import ipaddress
import os
from pathlib import Path
import subprocess
import sys
import threading
import tkinter as tk
import ctypes
from tkinter import messagebox, ttk
from urllib.parse import urlsplit
from uuid import uuid4
from PIL import Image, ImageTk

from fork_core import (APP_DIR, Runtime, default_settings, fetch_subscription, load_json, parse_profiles,
                       probe, reorder_rules, save_json, sync_subscription, build_config, executable, profile_from_link,
                       visible_connections, connection_process, xray_sidecar_config)


TARGETS = {"xray": "Xray", "direct": "Напрямую", "block": "Блок"}
KINDS = {"domain": "Домен", "domainSuffix": "Домен + поддомены", "domainKeyword": "Домен содержит",
         "ipCidr": "IP / CIDR", "processName": "Процесс", "port": "Порт"}
BG = "#101722"
PANEL = "#192332"
FIELD = "#111b29"
BORDER = "#2a394c"
TEXT = "#ecf3fc"
MUTED = "#93a6bd"
ACCENT = "#64b5ff"
GREEN = "#62d5a4"


class ForkApp(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Fork Lite Win")
        icon = Path(getattr(sys, "_MEIPASS", Path(__file__).parent)) / "Assets" / "AppIcon.ico"
        if icon.exists():
            self.iconbitmap(str(icon))
        self.geometry("1060x710")
        self.minsize(820, 560)
        self.settings = load_json("settings.json", default_settings())
        self.settings.setdefault("rules", default_settings()["rules"])
        self.profiles = load_json("profiles.json", {"items": [], "selected": None, "subscriptions": []})
        self.profiles.setdefault("subscriptions", [])
        self.runtime = Runtime()
        self.running = False
        self.busy = False
        self.pings = {}
        self.status = tk.StringVar(value="Выключен")
        self.enabled = tk.BooleanVar(value=False)
        self.auto_best = tk.BooleanVar(value=self.settings.get("autoBest", True))
        self.input_text = tk.StringVar()
        self.rule_kind = tk.StringVar(value=KINDS["domainSuffix"])
        self.rule_value = tk.StringVar()
        self.rule_target = tk.StringVar(value=TARGETS["direct"])
        self.profile_title = tk.StringVar(value="Выберите сервер")
        self.search = tk.StringVar()
        self.connection_count = tk.StringVar(value="0 соединений")
        self.conn_data = {}
        self.conn_loading = False
        self.tray = None
        self._tray_api = None
        self._rule_drag = None
        self._rule_dragged = False
        self._rule_editor = None
        self._popup = None
        self._theme()
        self._build()
        self._dark_titlebar()
        self.status.trace_add("write", lambda *_: self._refresh_header())
        self.enabled.trace_add("write", lambda *_: self._refresh_header())
        self.search.trace_add("write", lambda *_: self._render_connections())
        self._draw_profiles()
        self._draw_rules()
        self._refresh_header()
        self.after(1500, self._tick)
        self.after(0, self._start_tray)
        self.bind_all("<Button-1>", self._dismiss_popup_click, add="+")
        self.protocol("WM_DELETE_WINDOW", self._hide_window)

    def _theme(self):
        self.configure(bg=BG)
        self.option_add("*TCombobox*Listbox.background", FIELD)
        self.option_add("*TCombobox*Listbox.foreground", TEXT)
        self.option_add("*TCombobox*Listbox.selectBackground", BORDER)
        style = ttk.Style(self)
        style.theme_use("clam")
        style.configure("TFrame", background=BG)
        style.configure("Card.TFrame", background=PANEL)
        style.configure("TLabel", background=BG, foreground=TEXT, font=("Segoe UI", 10))
        style.configure("Card.TLabel", background=PANEL, foreground=TEXT, font=("Segoe UI", 10))
        style.configure("Title.TLabel", background=PANEL, foreground=TEXT, font=("Segoe UI Semibold", 19))
        style.configure("Section.TLabel", background=BG, foreground=TEXT, font=("Segoe UI Semibold", 15))
        style.configure("Muted.TLabel", background=BG, foreground=MUTED, font=("Segoe UI", 10))
        style.configure("CardMuted.TLabel", background=PANEL, foreground=MUTED, font=("Segoe UI", 9))
        style.configure("TButton", background=PANEL, foreground=TEXT, bordercolor=BORDER,
                        lightcolor=PANEL, darkcolor=PANEL, focuscolor=PANEL, padding=(12, 8), font=("Segoe UI", 10))
        style.map("TButton", background=[("active", BORDER)], foreground=[("disabled", MUTED)])
        style.configure("Accent.TButton", background=ACCENT, foreground=BG, bordercolor=ACCENT,
                        lightcolor=ACCENT, darkcolor=ACCENT, focuscolor=ACCENT,
                        padding=(18, 10), font=("Segoe UI Semibold", 10))
        style.map("Accent.TButton", background=[("active", "#8ac7ff")])
        style.configure("TEntry", fieldbackground=FIELD, foreground=TEXT, bordercolor=BORDER,
                        lightcolor=BORDER, darkcolor=BORDER, focuscolor=BORDER,
                        insertcolor=TEXT, relief="flat", padding=8)
        style.configure("TCombobox", fieldbackground=FIELD, background=FIELD, foreground=TEXT,
                        arrowcolor=MUTED, bordercolor=BORDER, lightcolor=BORDER,
                        darkcolor=BORDER, focuscolor=BORDER, padding=7)
        style.map("TCombobox", fieldbackground=[("readonly", FIELD)], foreground=[("readonly", TEXT)],
                  selectbackground=[("readonly", FIELD)], selectforeground=[("readonly", TEXT)])
        style.configure("TCheckbutton", background=BG, foreground=TEXT, font=("Segoe UI", 10))
        style.map("TCheckbutton", background=[("active", BG)], foreground=[("active", TEXT)])
        style.configure("Card.TCheckbutton", background=PANEL, foreground=TEXT, font=("Segoe UI", 10))
        style.map("Card.TCheckbutton", background=[("active", PANEL)], foreground=[("active", TEXT)])
        style.configure("TNotebook", background=BG, borderwidth=0, bordercolor=BG,
                        lightcolor=BG, darkcolor=BG, focuscolor=BG, tabmargins=(0, 0, 0, 8))
        style.configure("TNotebook.Tab", background=BG, foreground=MUTED, borderwidth=0,
                        bordercolor=BG, lightcolor=BG, darkcolor=BG, focuscolor=BG,
                        padding=(20, 10), font=("Segoe UI Semibold", 10))
        style.layout("TNotebook.Tab", [("Notebook.padding", {"sticky": "nswe", "children": [
            ("Notebook.label", {"sticky": ""})]})])
        style.map("TNotebook.Tab", background=[("selected", PANEL), ("active", BORDER)],
                  foreground=[("selected", TEXT), ("active", TEXT)])
        style.configure("Treeview", background=PANEL, fieldbackground=PANEL, foreground=TEXT,
                        bordercolor=PANEL, lightcolor=PANEL, darkcolor=PANEL, focuscolor=PANEL,
                        relief="flat", borderwidth=0, rowheight=34, font=("Segoe UI", 10))
        style.map("Treeview", background=[("selected", "#28527a")], foreground=[("selected", TEXT)])
        style.configure("Treeview.Heading", background="#233146", foreground=MUTED, relief="flat",
                        padding=(9, 9), font=("Segoe UI Semibold", 9))
        style.map("Treeview.Heading", background=[("active", BORDER)])

    def _dark_titlebar(self):
        if os.name != "nt":
            return
        self.update_idletasks()
        enabled = ctypes.c_int(1)
        try:
            for attribute in (20, 19):
                if ctypes.windll.dwmapi.DwmSetWindowAttribute(
                        self.winfo_id(), attribute, ctypes.byref(enabled), ctypes.sizeof(enabled)) == 0:
                    break
        except (AttributeError, OSError):
            pass

    def _build(self):
        top = ttk.Frame(self, style="Card.TFrame", padding=(20, 17))
        top.pack(fill="x", padx=18, pady=(18, 12))
        logo_path = Path(getattr(sys, "_MEIPASS", Path(__file__).parent)) / "Assets" / "AppIcon.png"
        if logo_path.exists():
            self.logo = ImageTk.PhotoImage(Image.open(logo_path).resize((48, 48), Image.Resampling.LANCZOS))
            tk.Label(top, image=self.logo, bg=PANEL).pack(side="left", padx=(0, 16))
        identity = ttk.Frame(top, style="Card.TFrame")
        identity.pack(side="left", fill="x", expand=True)
        ttk.Label(identity, text="Fork Lite Win", style="Title.TLabel").pack(anchor="w")
        ttk.Label(identity, textvariable=self.profile_title, style="CardMuted.TLabel").pack(anchor="w", pady=(2, 0))
        self.connect_button = ttk.Button(top, style="Accent.TButton", command=self._toggle_click)
        self.connect_button.pack(side="right")
        self.status_label = tk.Label(top, textvariable=self.status, bg=PANEL, fg=MUTED,
                                     font=("Segoe UI Semibold", 10), padx=18)
        self.status_label.pack(side="right")
        ttk.Button(top, text="Журнал", command=self._show_log).pack(side="right", padx=(0, 6))

        tabs = ttk.Notebook(self)
        self.tabs = tabs
        tabs.pack(fill="both", expand=True, padx=18, pady=(0, 18))
        connections = ttk.Frame(tabs, padding=18, style="Card.TFrame")
        servers = ttk.Frame(tabs, padding=18, style="Card.TFrame")
        rules = ttk.Frame(tabs, padding=18, style="Card.TFrame")
        tabs.add(connections, text="Соединения")
        tabs.add(servers, text="Серверы")
        tabs.add(rules, text="Правила")

        heading = ttk.Frame(servers, style="Card.TFrame")
        heading.pack(fill="x", pady=(0, 14))
        ttk.Label(heading, text="Серверы", style="Title.TLabel").pack(anchor="w")
        ttk.Label(heading, text="VLESS-ссылки, Clash YAML и подписки", style="CardMuted.TLabel").pack(anchor="w")
        bar = ttk.Frame(servers, style="Card.TFrame")
        bar.pack(fill="x", pady=(0, 8))
        ttk.Entry(bar, textvariable=self.input_text).pack(side="left", fill="x", expand=True)
        ttk.Button(bar, text="Добавить", style="Accent.TButton", command=self._import_input).pack(side="left", padx=(8, 0))
        actions = ttk.Frame(servers, style="Card.TFrame")
        actions.pack(fill="x", pady=(0, 12))
        ttk.Button(actions, text="Из буфера", command=self._from_clipboard).pack(side="left")
        ttk.Button(actions, text="Из файла", command=self._from_file).pack(side="left", padx=6)
        ttk.Button(actions, text="Обновить подписки", command=self._refresh_subscriptions).pack(side="left")
        ttk.Button(actions, text="Пинг всех", command=self._ping_all).pack(side="right")
        self.server_tree = ttk.Treeview(servers, columns=("name", "server", "kind", "ping"), show="headings", selectmode="browse")
        for key, title, width in (("name", "Имя", 210), ("server", "Адрес", 270), ("kind", "Тип", 130), ("ping", "TCP", 80)):
            self.server_tree.heading(key, text=title)
            self.server_tree.column(key, width=width)
        self.server_tree.pack(fill="both", expand=True)
        self.server_tree.bind("<<TreeviewSelect>>", self._select_profile)
        controls = ttk.Frame(servers, style="Card.TFrame")
        controls.pack(fill="x", pady=(12, 0))
        ttk.Checkbutton(controls, text="При включении выбирать самый быстрый", style="Card.TCheckbutton", variable=self.auto_best,
                        command=self._save_auto_best).pack(side="left")
        ttk.Button(controls, text="Удалить сервер", command=self._remove_profile).pack(side="right")

        ttk.Label(rules, text="Ваши правила", style="Title.TLabel").pack(anchor="w")
        ttk.Label(rules, text="Применяются сверху вниз; остальное идёт через выбранный сервер.",
                  style="CardMuted.TLabel").pack(anchor="w", pady=(2, 16))
        form = ttk.Frame(rules, style="Card.TFrame")
        form.pack(fill="x", pady=(0, 12))
        ttk.Combobox(form, textvariable=self.rule_kind, values=list(KINDS.values()), width=20, state="readonly").pack(side="left")
        ttk.Entry(form, textvariable=self.rule_value).pack(side="left", fill="x", expand=True, padx=5)
        ttk.Combobox(form, textvariable=self.rule_target, values=list(TARGETS.values()), width=13, state="readonly").pack(side="left")
        ttk.Button(form, text="Добавить", style="Accent.TButton", command=self._add_rule).pack(side="left", padx=(8, 0))
        ttk.Label(rules, text="Перетащите строку для приоритета · двойной клик для правки · × для удаления",
                  style="CardMuted.TLabel").pack(anchor="w", pady=(0, 10))
        self.rule_tree = ttk.Treeview(rules, columns=("grip", "enabled", "kind", "value", "target", "delete"),
                                      show="headings", selectmode="browse")
        for key, title, width in (("grip", "", 34), ("enabled", "Вкл.", 50), ("kind", "Вид", 160),
                                  ("value", "Значение", 360), ("target", "Выход", 120), ("delete", "", 42)):
            self.rule_tree.heading(key, text=title)
            self.rule_tree.column(key, width=width, stretch=key == "value",
                                  anchor="center" if key in ("grip", "enabled", "delete") else "w")
        self.rule_tree.pack(fill="both", expand=True)
        self.rule_tree.bind("<Button-1>", self._rule_press)
        self.rule_tree.bind("<B1-Motion>", self._rule_motion)
        self.rule_tree.bind("<ButtonRelease-1>", self._rule_release)
        self.rule_tree.bind("<Double-1>", self._rule_edit)
        ttk.Label(connections, text="Активные соединения", style="Title.TLabel").pack(anchor="w")
        ttk.Label(connections, text="Правый клик по строке создаёт правило. Соединения движков скрыты.",
                  style="CardMuted.TLabel").pack(anchor="w", pady=(2, 14))
        conn_bar = ttk.Frame(connections, style="Card.TFrame")
        conn_bar.pack(fill="x", pady=(0, 10))
        ttk.Label(conn_bar, text="Поиск", style="Card.TLabel").pack(side="left", padx=(0, 8))
        ttk.Entry(conn_bar, textvariable=self.search).pack(side="left", fill="x", expand=True)
        ttk.Button(conn_bar, text="Обновить", command=self._draw_connections).pack(side="right")
        ttk.Label(conn_bar, textvariable=self.connection_count, style="CardMuted.TLabel").pack(side="right", padx=14)
        self.conn_tree = ttk.Treeview(connections, columns=("host", "process", "outbound", "up", "down"), show="headings")
        for key, title, width in (("host", "Адрес", 300), ("process", "Процесс", 160),
                                  ("outbound", "Выход", 110), ("up", "↑ байт", 100), ("down", "↓ байт", 100)):
            self.conn_tree.heading(key, text=title)
            self.conn_tree.column(key, width=width)
        self.conn_tree.pack(fill="both", expand=True)
        self.empty_connections = ttk.Label(self.conn_tree, text="Включите подключение — соединения появятся здесь",
                                           style="CardMuted.TLabel")
        self.empty_connections.place(relx=.5, rely=.52, anchor="center")
        self.conn_tree.bind("<Button-3>", self._connection_menu)

    def _toggle_click(self):
        self.enabled.set(not self.enabled.get())
        self._toggle()

    def _refresh_header(self):
        self.connect_button.configure(text="Отключить" if self.enabled.get() else "Подключить")
        self.status_label.configure(fg=GREEN if self.running else ("#ff8d9b" if self.status.get() == "Ошибка" else MUTED))
        self._sync_tray()

    def _start_tray(self):
        if os.name != "nt":
            return
        try:
            import pystray
        except ImportError as error:
            messagebox.showerror("Fork Lite Win", f"Не удалось запустить значок у часов: {error}")
            return
        self._tray_api = pystray
        logo_path = Path(getattr(sys, "_MEIPASS", Path(__file__).parent)) / "Assets" / "AppIcon.png"
        image = Image.open(logo_path).convert("RGBA").resize((64, 64), Image.Resampling.LANCZOS)
        self.tray = pystray.Icon("fork-lite-win", image, "Fork Lite Win", self._tray_menu())
        icon = self.tray
        def run_tray():
            try:
                icon.run()
            except Exception as error:
                self.after(0, lambda e=error: messagebox.showerror("Fork Lite Win", f"Значок у часов: {e}"))
        threading.Thread(target=run_tray, daemon=True).start()

    def _tray_menu(self):
        api = self._tray_api
        item = api.MenuItem
        def select_action(profile_id):
            return lambda icon, menu_item: self.after(0, lambda: self._set_profile(profile_id))
        profiles = [item(p["name"],
                         select_action(p["id"]),
                         checked=lambda menu_item, pid=p["id"]: self.profiles.get("selected") == pid)
                    for p in self.profiles["items"]]
        profile_menu = item("Сервер", api.Menu(*profiles)) if profiles else item("Серверов пока нет", None, enabled=False)
        return api.Menu(
            item(f"Статус: {self.status.get()}", None, enabled=False),
            item("Отключить" if self.enabled.get() else "Подключить",
                 lambda icon, menu_item: self.after(0, self._toggle_click), enabled=not self.busy),
            profile_menu,
            api.Menu.SEPARATOR,
            item("Открыть окно", lambda icon, menu_item: self.after(0, self._show_window), default=True),
            item("Журнал", lambda icon, menu_item: self.after(0, self._show_log)),
            item("Выход", lambda icon, menu_item: self.after(0, self._exit)),
        )

    def _sync_tray(self):
        if self.tray:
            self.tray.title = f"Fork Lite Win — {self.status.get()}"
            self.tray.menu = self._tray_menu()
            self.tray.update_menu()

    def _show_window(self):
        self.deiconify()
        self.lift()
        self.focus_force()

    def _hide_window(self):
        if self.tray:
            self.withdraw()
        else:
            self._exit()

    def _exit(self):
        self.runtime.stop()
        if self.tray:
            self.tray.stop()
            self.tray = None
        self.destroy()

    def _background(self, job, done):
        def run():
            try:
                value = job()
                self.after(0, lambda: done(value, None))
            except Exception as error:
                self.after(0, lambda e=error: done(None, e))
        threading.Thread(target=run, daemon=True).start()

    def _toggle(self):
        if self.busy:
            self.enabled.set(self.running)
            return
        if not self.enabled.get():
            self.runtime.stop()
            self.running = False
            self.status.set("Выключен")
            return
        profile = self._active_profile()
        if not profile:
            self.enabled.set(False)
            messagebox.showerror("Fork", "Сначала добавьте VLESS-сервер")
            return
        self.busy = True
        self.status.set("Подключается…")
        auto_best = self.auto_best.get()
        items = list(self.profiles["items"])
        rules = list(self.settings["rules"])

        def job():
            chosen = profile
            if auto_best and len(items) > 1:
                ranked = [(probe(p), p) for p in items]
                good = [(latency, p) for latency, p in ranked if latency is not None]
                if good:
                    chosen = min(good, key=lambda x: x[0])[1]
            self.runtime.start(chosen, rules)
            return chosen

        def done(chosen, error):
            self.busy = False
            if error:
                self.enabled.set(False)
                self.running = False
                self.status.set("Ошибка")
                messagebox.showerror("Fork", str(error))
            else:
                self.running = True
                self.profiles["selected"] = chosen["id"]
                save_json("profiles.json", self.profiles)
                self._draw_profiles()
                self.status.set("Работает")
        self._background(job, done)

    def _restart(self):
        if self.running:
            self.runtime.stop()
            self.running = False
            self.enabled.set(True)
            self._toggle()

    def _active_profile(self):
        return next((p for p in self.profiles["items"] if p["id"] == self.profiles.get("selected")),
                    self.profiles["items"][0] if self.profiles["items"] else None)

    def _draw_profiles(self):
        self.server_tree.delete(*self.server_tree.get_children())
        active = self._active_profile()
        self.profile_title.set(f"Сервер: {active['name']}" if active else "Добавьте VLESS-сервер для подключения")
        for p in self.profiles["items"]:
            marker = "● " if p["id"] == self.profiles.get("selected") else ""
            self.server_tree.insert("", "end", iid=p["id"], values=(marker + p["name"],
                                    f"{p['server']}:{p['port']}", f"{p.get('network', 'tcp')} · {p.get('security', 'reality')}",
                                    self.pings.get(p["id"], "")))
        self._sync_tray()

    def _select_profile(self, _event=None):
        selection = self.server_tree.selection()
        if selection:
            self._set_profile(selection[0])

    def _set_profile(self, profile_id):
        if self.profiles.get("selected") == profile_id or not any(p["id"] == profile_id for p in self.profiles["items"]):
            return
        self.profiles["selected"] = profile_id
        self.auto_best.set(False)
        self._save_auto_best()
        save_json("profiles.json", self.profiles)
        self._draw_profiles()
        self._restart()

    def _add_profiles(self, incoming):
        added = 0
        for p in incoming:
            if not any(all(p.get(k) == existing.get(k) for k in ("server", "port", "uuid", "network", "security", "path"))
                       for existing in self.profiles["items"]):
                self.profiles["items"].append(p)
                added += 1
        if not self.profiles.get("selected") and self.profiles["items"]:
            self.profiles["selected"] = self.profiles["items"][0]["id"]
        save_json("profiles.json", self.profiles)
        self._draw_profiles()
        self.status.set(f"Добавлено серверов: {added}")

    def _import_input(self):
        text = self.input_text.get().strip()
        if not text:
            return
        if urlsplit(text).scheme in ("http", "https"):
            self._import_subscription(text)
        else:
            try:
                self._add_profiles(parse_profiles(text))
                self.input_text.set("")
            except Exception as error:
                messagebox.showerror("Импорт", str(error))

    def _from_clipboard(self):
        try:
            self.input_text.set(self.clipboard_get())
            self._import_input()
        except tk.TclError:
            pass

    def _from_file(self):
        from tkinter import filedialog
        path = filedialog.askopenfilename(filetypes=[("Профили", "*.yaml *.yml *.txt"), ("Все файлы", "*.*")])
        if path:
            try:
                self._add_profiles(parse_profiles(Path(path).read_text(encoding="utf-8")))
            except Exception as error:
                messagebox.showerror("Импорт", str(error))

    def _import_subscription(self, url):
        self.status.set("Загружаю подписку…")
        def done(incoming, error):
            if error:
                messagebox.showerror("Подписка", str(error))
                self.status.set("Ошибка подписки")
                return
            sync_subscription(self.profiles, url, incoming)
            save_json("profiles.json", self.profiles)
            self._draw_profiles()
            self.status.set(f"Подписка обновлена: {len(incoming)} серверов")
            self._restart()
        self._background(lambda: fetch_subscription(url), done)

    def _refresh_subscriptions(self):
        for url in self.profiles["subscriptions"]:
            self._import_subscription(url)

    def _remove_profile(self):
        selected = self.server_tree.selection()
        if not selected:
            return
        self.profiles["items"] = [p for p in self.profiles["items"] if p["id"] != selected[0]]
        if self.profiles.get("selected") == selected[0]:
            self.profiles["selected"] = self.profiles["items"][0]["id"] if self.profiles["items"] else None
        save_json("profiles.json", self.profiles)
        self._draw_profiles()
        if self.running and self._active_profile():
            self._restart()
        elif self.running:
            self.enabled.set(False)
            self._toggle()

    def _save_auto_best(self):
        self.settings["autoBest"] = self.auto_best.get()
        save_json("settings.json", self.settings)

    def _ping_all(self):
        if self.running:
            messagebox.showinfo("Fork", "Выключите Xray перед пингом серверов")
            return
        items = list(self.profiles["items"])
        def job():
            return {p["id"]: probe(p) for p in items}
        def done(result, error):
            if error:
                messagebox.showerror("Пинг", str(error))
                return
            self.pings = {key: f"{value} мс" if value is not None else "—" for key, value in result.items()}
            self._draw_profiles()
        self._background(job, done)

    def _draw_rules(self):
        self.rule_tree.delete(*self.rule_tree.get_children())
        for rule in self.settings["rules"]:
            if rule.get("corpBlock") or rule.get("target") == "corp":
                continue
            self.rule_tree.insert("", "end", iid=rule["id"], values=("⋮⋮", "✓" if rule.get("enabled", True) else "",
                                  KINDS.get(rule["kind"], rule["kind"]), rule["value"],
                                  TARGETS.get(rule["target"], rule["target"]), "×"))

    def _selected_rule(self):
        selected = self.rule_tree.selection()
        return next((r for r in self.settings["rules"] if selected and r["id"] == selected[0]), None)

    def _save_rules(self, selected_id=None):
        save_json("settings.json", self.settings)
        self._draw_rules()
        if selected_id and self.rule_tree.exists(selected_id):
            self.rule_tree.selection_set(selected_id)
        self._restart()

    def _add_rule(self):
        kind = next((key for key, label in KINDS.items() if label == self.rule_kind.get()), None)
        target = next((key for key, label in TARGETS.items() if label == self.rule_target.get()), None)
        value = self.rule_value.get().strip()
        if not kind or not target:
            return
        if not value:
            return
        if not self._valid_rule(kind, value):
            return
        self.settings["rules"].insert(0, {"id": str(uuid4()), "kind": kind, "value": value,
                                             "target": target, "enabled": True})
        self.rule_value.set("")
        self._save_rules()

    def _valid_rule(self, kind, value):
        if not value:
            return False
        if kind == "ipCidr":
            try:
                ipaddress.ip_network(value, strict=False)
            except ValueError:
                messagebox.showerror("Правило", "Укажите IP или CIDR")
                return False
        if kind == "port" and (not value.isdigit() or not 1 <= int(value) <= 65535):
            messagebox.showerror("Правило", "Порт должен быть от 1 до 65535")
            return False
        return True

    def _rule_press(self, event):
        item = self.rule_tree.identify_row(event.y)
        column = self.rule_tree.identify_column(event.x)
        self._rule_drag = None
        self._rule_dragged = False
        if not item:
            return
        rule = next((r for r in self.settings["rules"] if r["id"] == item), None)
        if not rule:
            return
        if column == "#2":
            rule["enabled"] = not rule.get("enabled", True)
            self._save_rules(item)
            return "break"
        if column == "#6":
            self.settings["rules"].remove(rule)
            self._save_rules()
            return "break"
        self._rule_drag = item
        self._rule_drag_y = event.y

    def _rule_motion(self, event):
        item = self._rule_drag
        if not item or not self.rule_tree.exists(item):
            return
        if not self._rule_dragged and abs(event.y - self._rule_drag_y) < 7:
            return
        self._rule_dragged = True
        target = self.rule_tree.identify_row(event.y)
        if target and target != item:
            self.rule_tree.move(item, "", self.rule_tree.index(target))
            self.rule_tree.selection_set(item)
        return "break"

    def _rule_release(self, _event):
        item = self._rule_drag
        self._rule_drag = None
        if item and self._rule_dragged:
            self.settings["rules"] = reorder_rules(self.settings["rules"], list(self.rule_tree.get_children()))
            self._save_rules(item)
        self._rule_dragged = False

    def _rule_edit(self, event):
        item = self.rule_tree.identify_row(event.y)
        column = self.rule_tree.identify_column(event.x)
        if not item or column not in ("#3", "#4", "#5"):
            return
        self._rule_drag = None
        rule = next((r for r in self.settings["rules"] if r["id"] == item), None)
        box = self.rule_tree.bbox(item, column)
        if not rule or not box:
            return
        if self._rule_editor and self._rule_editor.winfo_exists():
            self._rule_editor.destroy()
        x, y, width, height = box
        if column == "#4":
            editor = ttk.Entry(self.rule_tree)
            editor.insert(0, rule["value"])
            editor.selection_range(0, "end")
        else:
            labels = KINDS if column == "#3" else TARGETS
            key = rule["kind"] if column == "#3" else rule["target"]
            editor = ttk.Combobox(self.rule_tree, values=list(labels.values()), state="readonly")
            editor.set(labels[key])
        self._rule_editor = editor
        editor.place(x=x, y=y, width=width, height=height)
        editor.focus_set()
        committed = False

        def finish(save=True):
            nonlocal committed
            if committed:
                return
            committed = True
            value = editor.get().strip()
            editor.destroy()
            self._rule_editor = None
            if not save:
                return
            key = {"#3": "kind", "#4": "value", "#5": "target"}[column]
            if column == "#3":
                value = next((k for k, title in KINDS.items() if title == value), rule[key])
            elif column == "#5":
                value = next((k for k, title in TARGETS.items() if title == value), rule[key])
            if value == rule[key]:
                return
            kind = value if key == "kind" else rule["kind"]
            rule_value = value if key == "value" else rule["value"]
            if not self._valid_rule(kind, rule_value):
                return
            rule[key] = value
            self._save_rules(item)

        editor.bind("<Return>", lambda event: finish())
        editor.bind("<Escape>", lambda event: finish(False))
        editor.bind("<FocusOut>", lambda event: finish())
        if column != "#4":
            editor.bind("<<ComboboxSelected>>", lambda event: finish())
        return "break"

    def _draw_connections(self):
        if self.conn_loading:
            return
        self.conn_loading = True
        def done(connections, error):
            self.conn_loading = False
            if error:
                return
            self.conn_data = {c["id"]: c for c in visible_connections(connections)}
            self._render_connections()
        self._background(self.runtime.connections, done)

    def _render_connections(self):
        self.conn_tree.delete(*self.conn_tree.get_children())
        query = self.search.get().casefold().strip()
        count = 0
        for c in self.conn_data.values():
            meta = c.get("metadata") or {}
            process = connection_process(c)
            host = meta.get("host") or meta.get("destinationIP") or ""
            port = meta.get("destinationPort") or ""
            outbound = " → ".join(reversed(c.get("chains", [])))
            if query and query not in f"{host} {process} {outbound}".casefold():
                continue
            self.conn_tree.insert("", "end", iid=c["id"], values=(f"{host}:{port}", process,
                                  outbound, c.get("upload", 0), c.get("download", 0)))
            count += 1
        self.connection_count.set(f"{count} соединений")
        if count:
            self.empty_connections.place_forget()
        else:
            self.empty_connections.configure(text="Нет совпадений" if query else
                                             "Включите подключение — соединения появятся здесь" if not self.running else
                                             "Активных соединений пока нет")
            self.empty_connections.place(relx=.5, rely=.52, anchor="center")

    def _connection_menu(self, event):
        item = self.conn_tree.identify_row(event.y)
        if not item or item not in self.conn_data:
            return
        self.conn_tree.selection_set(item)
        meta = self.conn_data[item].get("metadata") or {}
        domain = meta.get("host", "")
        ip = meta.get("destinationIP", "")
        process = connection_process(self.conn_data[item])
        choices = []
        if domain:
            try:
                ipaddress.ip_address(domain)
            except ValueError:
                choices.append(("domain", domain))
        try:
            addr = ipaddress.ip_address(ip)
            choices.append(("ipCidr", f"{addr}/{32 if addr.version == 4 else 128}"))
        except ValueError:
            pass
        if process:
            choices.append(("processName", process))
        if choices:
            self._quick_rule_popup(choices, event.x_root, event.y_root)

    def _quick_rule_popup(self, choices, x, y):
        if self._popup and self._popup.winfo_exists():
            self._popup.destroy()
        popup = tk.Toplevel(self)
        self._popup = popup
        popup.overrideredirect(True)
        popup.attributes("-topmost", True)
        popup.configure(bg=PANEL, highlightthickness=0, bd=0)
        tk.Label(popup, text="СОЗДАТЬ ПРАВИЛО", bg=PANEL, fg=MUTED,
                 font=("Segoe UI Semibold", 9), anchor="w", padx=14, pady=10).pack(fill="x")
        for kind, value in choices:
            row = tk.Frame(popup, bg=PANEL, padx=12, pady=5)
            row.pack(fill="x")
            title = value if len(value) <= 28 else value[:25] + "…"
            tk.Label(row, text=f"{KINDS[kind]}: {title}", bg=PANEL, fg=TEXT,
                     font=("Segoe UI", 9), width=34, anchor="w").pack(side="left")
            for target, label in TARGETS.items():
                def choose(k=kind, v=value, t=target):
                    popup.destroy()
                    self._popup = None
                    self._quick_rule(k, v, t)
                tk.Button(row, text=label, command=choose, bg=FIELD, fg=TEXT,
                          activebackground=BORDER, activeforeground=TEXT, relief="flat",
                          bd=0, highlightthickness=0, padx=8, pady=5,
                          font=("Segoe UI", 9), cursor="hand2").pack(side="left", padx=(4, 0))
        popup.update_idletasks()
        x = min(x, popup.winfo_screenwidth() - popup.winfo_reqwidth() - 8)
        y = min(y, popup.winfo_screenheight() - popup.winfo_reqheight() - 8)
        popup.geometry(f"+{max(0, x)}+{max(0, y)}")
        popup.bind("<Escape>", lambda event: popup.destroy())
        popup.bind("<FocusOut>", lambda event: self.after(100, lambda: self._close_popup_if_unfocused(popup)))
        popup.focus_force()

    def _close_popup_if_unfocused(self, popup):
        if not popup.winfo_exists():
            return
        focused = self.focus_get()
        if focused is None or focused.winfo_toplevel() != popup:
            popup.destroy()
            self._popup = None

    def _dismiss_popup_click(self, event):
        popup = self._popup
        if not popup or not popup.winfo_exists():
            return
        widget = popup.winfo_containing(event.x_root, event.y_root)
        if widget is None or widget.winfo_toplevel() != popup:
            popup.destroy()
            self._popup = None

    def _quick_rule(self, kind, value, target):
        self.rule_kind.set(KINDS[kind])
        self.rule_value.set(value)
        self.rule_target.set(TARGETS[target])
        self._add_rule()

    def _show_log(self):
        window = tk.Toplevel(self)
        window.title("Журнал sing-box")
        window.geometry("800x500")
        window.configure(bg=BG)
        text = tk.Text(window, wrap="none", bg=FIELD, fg=TEXT, insertbackground=TEXT,
                       selectbackground=BORDER, relief="flat", padx=14, pady=14,
                       font=("Consolas", 10))
        text.pack(fill="both", expand=True)
        try:
            text.insert("end", (APP_DIR / "core.log").read_text(encoding="utf-8", errors="replace")[-100_000:])
        except OSError:
            text.insert("end", "Журнал пока пуст")
        text.configure(state="disabled")

    def _tick(self):
        if self.running and (not self.runtime.core or self.runtime.core.poll() is not None):
            self.running = False
            self.enabled.set(False)
            self.status.set("Ядро остановилось — см. журнал")
        if self.running:
            self._draw_connections()
        self.after(1500, self._tick)

    def _close(self):
        self._exit()


def self_test():
    """Check bundled engines without creating a TUN or opening a window."""
    import json
    import tempfile
    if os.name == "nt":
        import pystray
        if not pystray.Icon.HAS_MENU:
            raise RuntimeError("Windows tray menu недоступно")

    base = ("vless://123e4567-e89b-12d3-a456-426614174000@example.com:443"
            "?security=reality&pbk=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&type=")
    sing = executable("sing-box.exe")
    xray = executable("xray.exe")
    if not sing or not xray:
        raise RuntimeError("В сборке нет sing-box.exe или xray.exe")
    with tempfile.TemporaryDirectory() as directory:
        for label, profile in (
            ("xhttp", profile_from_link(base + "xhttp#Smoke")),
            ("vision-udp443", profile_from_link(base + "tcp&flow=xtls-rprx-vision-udp443#Smoke")),
        ):
            for engine, args, config in (
                (sing, ["check", "-c"], build_config(profile, [], "smoke", tun=True)),
                (xray, ["run", "-test", "-config"], xray_sidecar_config(profile)),
            ):
                path = Path(directory) / (f"{label}-sing.json" if engine == sing else f"{label}-xray.json")
                path.write_text(json.dumps(config), encoding="utf-8")
                subprocess.run([engine, *args, str(path)], check=True, capture_output=True, timeout=30)
    print("Fork Lite Win: bundled engines and configs OK")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
    else:
        ForkApp().mainloop()
