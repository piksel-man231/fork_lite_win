#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Этот скрипт предназначен для macOS." >&2
  exit 1
fi

if command -v brew >/dev/null && [[ -d "$(brew --prefix rustup 2>/dev/null)/bin" ]]; then
  export PATH="$(brew --prefix rustup)/bin:$PATH"
fi

for command in rustup cargo x86_64-w64-mingw32-gcc curl; do
  if ! command -v "$command" >/dev/null; then
    echo "Не найден $command. Установите rustup и mingw-w64: brew install rustup mingw-w64" >&2
    exit 1
  fi
done

rustup target add x86_64-pc-windows-gnu
mkdir -p vendor dist

if [[ ! -s vendor/sing-box.zip ]]; then
  curl --fail --location --retry 3 \
    'https://github.com/SagerNet/sing-box/releases/download/v1.14.4/sing-box-1.14.4-windows-amd64.zip' \
    -o vendor/sing-box.zip
fi
if [[ ! -s vendor/xray.zip ]]; then
  curl --fail --location --retry 3 \
    'https://github.com/XTLS/Xray-core/releases/download/v26.11.9/Xray-windows-64.zip' \
    -o vendor/xray.zip
fi

unzip -tq vendor/sing-box.zip >/dev/null
unzip -tq vendor/xray.zip >/dev/null
printf '%s\n' \
  '99185d75d314a07dbba4a130c8ad986cbeb629a07927975636b341ca61f991df  vendor/sing-box.zip' \
  '1262522266337b824f07beb8e1375786b437683b3890105469088f8e17e0a69d  vendor/xray.zip' \
  | shasum -a 256 -c
cargo test --quiet
cargo build --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/ForkLiteWin.exe dist/ForkLiteWin.exe

file dist/ForkLiteWin.exe
shasum -a 256 dist/ForkLiteWin.exe
echo "Готово: $(pwd)/dist/ForkLiteWin.exe"
