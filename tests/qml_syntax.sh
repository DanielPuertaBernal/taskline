#!/usr/bin/env bash
# Syntax check for the plugin's QML. qmllint cannot resolve the shell's own
# qs.* imports outside Omarchy, so only [syntax] findings fail the check.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
QMLLINT="${QMLLINT:-$(command -v qmllint6 || command -v qmllint || echo /usr/lib/qt6/bin/qmllint)}"
[[ -x $QMLLINT ]] || { echo "qmllint not found (install qt6-declarative / qt6-declarative-dev-tools)"; exit 1; }
status=0
for file in "$ROOT"/omarchy/danso.tasks/*.qml; do
  out="$("$QMLLINT" "$file" 2>&1)"
  if grep -q '\[syntax\]' <<<"$out"; then
    grep -A2 '\[syntax\]' <<<"$out"
    status=1
  else
    printf 'ok   %s\n' "${file#"$ROOT"/}"
  fi
done
exit "$status"
