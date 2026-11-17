<div align="center">

<img src="Assets/AppIcon.png" alt="Fork Lite Win" width="128" height="128">

# Fork Lite Win

**VLESS для Windows с правилами, подписками и живыми соединениями.**

Тёмный интерфейс, TUN, выбор сервера и управление из меню у часов.

![Windows 10+](https://img.shields.io/badge/Windows-10%2B-20232a?logo=windows&logoColor=white)
![x64](https://img.shields.io/badge/сборка-x64-20232a)
![Rust](https://img.shields.io/badge/Rust-1.100-20232a?logo=rust&logoColor=white)

[**Скачать ForkLiteWin.exe**](https://github.com/piksel-man231/fork_lite_win/releases/latest/download/ForkLiteWin.exe) · [Исходный код](src) · [Сборка на Mac](#сборка-на-mac)

</div>

## Возможности

- **VLESS**: ссылки, Base64-списки, Clash YAML и HTTPS-подписки. Транспорты TCP, WebSocket и XHTTP; TLS и Reality. Для XHTTP и `xtls-rprx-vision-udp443` используется Xray-core.
- **TUN**: sing-box направляет приложения через выбранный сервер, напрямую или в блок по правилам. Корпоративного VPN в Windows-версии нет.
- **Правила мышкой**: строки можно перетаскивать; тип и выход выбираются в меню, значение редактируется двойным кликом. Правила применяются сверху вниз.
- **Соединения**: сортировка по любой колонке, объём в КБ/МБ/ГБ, скорость получения и отдачи. Служебные соединения Xray и sing-box скрыты. Правый клик по строке добавляет правило.
- **Меню у часов**: показать окно, подключиться или отключиться, переключить сервер и выйти. Закрытие окна оставляет приложение в трее.
- **Автовыбор сервера**: при подключении измеряется TCP-задержка и выбирается сервер с наименьшим ответом. Опцию можно отключить.

## Установка

Скачайте [ForkLiteWin.exe](https://github.com/piksel-man231/fork_lite_win/releases/latest/download/ForkLiteWin.exe) и запустите. Windows запросит права администратора через UAC: они нужны для TUN. При первом подключении приложение распакует встроенные sing-box, Xray-core и DLL в `%APPDATA%\Fork Lite Win\bin`.

Вставьте VLESS-ссылку, список, Clash YAML или HTTPS-адрес подписки в поле на вкладке «Серверы», нажмите «Добавить» и «Подключить». Сборка не подписана цифровой подписью Windows.

## Сборка на Mac

Требуются macOS, Homebrew и 64-битный Windows target Rust. **GitHub Actions не участвует в сборке**: скрипт запускается на вашем Mac и создаёт готовый EXE локально.

```bash
brew install rustup mingw-w64
rustup toolchain install stable --profile minimal --target x86_64-pc-windows-gnu
./scripts/build-windows-macos.sh
```

Результат: `dist/ForkLiteWin.exe`. Скрипт сам скачивает официальные архивы [sing-box 1.14.4](https://github.com/SagerNet/sing-box/releases/tag/v1.14.4) и [Xray-core 26.11.9](https://github.com/XTLS/Xray-core/releases/tag/v26.11.9), проверяет ZIP, запускает тесты и кросс-компилирует Rust-приложение. Windows-движки встроены в один EXE. На Mac можно проверить логику и формат EXE; реальное TUN-подключение проверяется на Windows.

Опубликовать локальную сборку в релиз GitHub после коммита и `git push`:

```bash
./scripts/publish-windows-macos.sh v0.4.0
```

## Данные

| Данные | Путь |
| --- | --- |
| Настройки и правила | `%APPDATA%\Fork Lite Win\settings.json` |
| Серверы и подписки | `%APPDATA%\Fork Lite Win\profiles.json` |
| Журнал ядра | `%APPDATA%\Fork Lite Win\core.log` |

Формат файлов совместим с прежней Python-версией. Профили хранятся локально и могут содержать секреты VLESS. История Python-версии остаётся в Git.

---

<div align="center">

[Rust](https://www.rust-lang.org/) · [sing-box](https://sing-box.sagernet.org/) · [Xray-core](https://github.com/XTLS/Xray-core) · [Лицензии компонентов](Licenses) · [Fork Lite для macOS](https://github.com/piksel-man231/fork_lite)

</div>
