#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
version="${1:?Укажите тег релиза, например v0.4.0}"

if ! command -v gh >/dev/null; then
  echo "Для публикации нужен GitHub CLI: brew install gh" >&2
  exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Сначала закоммитьте изменения: релиз должен соответствовать исходникам в Git." >&2
  exit 1
fi

./scripts/build-windows-macos.sh
gh release create "$version" dist/ForkLiteWin.exe \
  --title "Fork Lite Win $version" \
  --notes "EXE собран локально на macOS из исходников тега $version."
