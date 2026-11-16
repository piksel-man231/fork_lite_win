<div align="center">

<img src="Assets/AppIcon.png" alt="Fork Lite Win" width="128" height="128">

# Fork Lite Win

**VLESS-прокси для Windows с правилами маршрутизации и живыми соединениями.**

Подписки, выбор сервера, TUN и правила в одном приложении.

![Windows 10+](https://img.shields.io/badge/Windows-10%2B-20232a?logo=windows&logoColor=white)
![x64](https://img.shields.io/badge/сборка-x64-20232a)
![Python](https://img.shields.io/badge/Python-3.12-3776AB?logo=python&logoColor=white)

[**Скачать ForkLiteWin.exe**](https://github.com/piksel-man231/fork_lite_win/releases/latest/download/ForkLiteWin.exe) · [Исходный код](app.py) · [Сборка](#сборка-из-исходников)

</div>

---

## Возможности

- **Один переключатель** — подключение запускается вручную и всегда выключено при старте приложения.
- **VLESS** — ссылки `vless://`, Base64-списки, Clash YAML и HTTPS-подписки. Транспорты `tcp`, `ws`, `xhttp`; Reality и TLS.
- **Правила сверху вниз** — домены, IP-подсети, процессы и порты можно направить через Xray, напрямую или заблокировать.
- **Активные соединения** — адрес, процесс, выход и объём трафика. Правый клик по строке создаёт правило.
- **Выбор по задержке** — TCP-пинг всех серверов и автовыбор при включении.

```text
Приложения Windows
        │
        ▼
   TUN · sing-box
        │
        ├─ ваши правила ──► Xray / напрямую / блок
        └─ остальной трафик ──► выбранный VLESS-сервер
```

Для `xhttp` приложение запускает Xray-core как локальный SOCKS-движок. Корпоративного VPN в Windows-версии нет. В отличие от [Fork Lite для macOS](https://github.com/piksel-man231/fork_lite), эта версия пока поддерживает только VLESS.

## Установка

1. [Скачайте ForkLiteWin.exe](https://github.com/piksel-man231/fork_lite_win/releases/latest/download/ForkLiteWin.exe).
2. Запустите его **от имени администратора**: TUN изменяет системную маршрутизацию.
3. Вставьте VLESS-ссылку или HTTPS-подписку на вкладке **«Серверы»**. Clash YAML можно добавить через **«Из файла»**.
4. Включите **Xray**. Выход из приложения останавливает TUN.

EXE содержит sing-box, Xray-core и нужные им DLL. Сборка не подписана цифровой подписью Windows. Если другой VPN-клиент уже использует TUN, выключите его перед подключением Fork Lite Win.

## Правила

Правила применяются **сверху вниз**. Если совпадений нет, трафик идёт через выбранный сервер. Кнопки **«Выше»** и **«Ниже»** меняют приоритет.

| Условие | Пример | Выход |
| --- | --- | --- |
| Домен или поддомены | `example.com` | Xray / напрямую / блок |
| Домен содержит | `example` | Xray / напрямую / блок |
| IP-подсеть | `192.168.0.0/16` | Xray / напрямую / блок |
| Процесс | `browser.exe` | Xray / напрямую / блок |
| Порт | `443` | Xray / напрямую / блок |

Новая установка добавляет прямые правила для локальных IPv4-подсетей и Tailscale-диапазона. Их можно выключить или удалить.

## Сборка из исходников

Понадобятся Windows 10+, Python 3.12 и PowerShell. Положите в `bin/` файлы `sing-box.exe`, `libcronet.dll`, `xray.exe` и `wintun.dll` из официальных архивов [sing-box 1.14.4](https://github.com/SagerNet/sing-box/releases/tag/v1.14.4) и [Xray-core 26.11.9](https://github.com/XTLS/Xray-core/releases/tag/v26.11.9). Затем:

```powershell
python -m unittest discover -s tests -v
.\build.ps1
.\dist\ForkLiteWin.exe --self-test
```

Готовый файл — `dist\ForkLiteWin.exe`. [GitHub Actions](.github/workflows/windows.yml) выполняет эти же шаги на Windows runner и сохраняет EXE как артефакт. Публичный релиз содержит готовый EXE.

## Файлы приложения

| Данные | Путь |
| --- | --- |
| Настройки и правила | `%APPDATA%\Fork Lite Win\settings.json` |
| Серверы и подписки | `%APPDATA%\Fork Lite Win\profiles.json` |
| Журнал ядра | `%APPDATA%\Fork Lite Win\core.log` |

Профили хранятся локально. Пароли и ключи VLESS находятся в `profiles.json`, поэтому доступ к этому файлу должен быть ограничен вашей учётной записью.

## Проверка

Тесты проверяют импорт, правила, подписки и генерацию конфигов. Windows-сборка в CI проверяет встроенные движки командами `sing-box check` и `xray run -test`. Реальное TUN-подключение к серверу автоматические тесты не проверяют.

---

<div align="center">

Python · [sing-box](https://sing-box.sagernet.org/) · [Xray-core](https://github.com/XTLS/Xray-core) · [Лицензии движков](Licenses)

</div>
