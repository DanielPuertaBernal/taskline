#!/usr/bin/env bash
# Integration tests for install.sh / uninstall.sh. Every case runs in a fresh
# temporary HOME, so the real user's config and Taskwarrior data are never read
# or written.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PASS=0
FAIL=0
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

check() {
  local name="$1"; shift
  if "$@"; then PASS=$((PASS + 1)); printf 'ok   %s\n' "$name"
  else FAIL=$((FAIL + 1)); printf 'FAIL %s\n' "$name"; fi
}

new_home() {
  local home
  home="$(mktemp -d "$SCRATCH/home.XXXX")"
  mkdir -p "$home/bin" "$home/omarchy/config/omarchy"
  # Fake Taskwarrior 3 so the dependency check passes without a real install.
  printf '#!/bin/sh\necho 3.5.0\n' >"$home/bin/task"
  chmod +x "$home/bin/task"
  cat >"$home/omarchy/config/omarchy/shell.json" <<'JSON'
{"bar":{"layout":{"left":[{"id":"omarchy.menu"}],"center":[{"id":"omarchy.clock"}],"right":[{"id":"omarchy.audio"}]}},"version":1}
JSON
  printf '%s\n' "$home"
}

run() { # run <home> <script>
  env -i HOME="$1" PATH="$1/bin:/usr/bin:/bin" OMARCHY_PATH="$1/omarchy" \
    TASKLINE_NO_IPC=1 bash "$ROOT/$2" >"$1/out.log" 2>&1
}

widget_count() { jq '[.bar.layout[][] | select(.id == "danso.tasks")] | length' "$1/.config/omarchy/shell.json"; }

# --- fresh install ---------------------------------------------------------
H="$(new_home)"
check "fresh install exits 0" run "$H" install.sh
check "plugin manifest installed" test -f "$H/.config/omarchy/plugins/danso.tasks/manifest.json"
check "plugin qml installed" test -f "$H/.config/omarchy/plugins/danso.tasks/Tasks.qml"
check "taskrc created with hooks.location" grep -qx 'hooks.location=~/.config/task/hooks' "$H/.config/task/taskrc"
check "hook installed and executable" test -x "$H/.config/task/hooks/on-exit.omarchy-bar"
check "shell.json created from defaults with widget in right" \
  test "$(jq -c '.bar.layout.right' "$H/.config/omarchy/shell.json")" = '[{"id":"omarchy.audio"},{"id":"danso.tasks"}]'
check "defaults layout kept" test "$(jq -c '.bar.layout.center' "$H/.config/omarchy/shell.json")" = '[{"id":"omarchy.clock"}]'

# --- idempotent ------------------------------------------------------------
cp "$H/.config/omarchy/shell.json" "$SCRATCH/before.json"
cp "$H/.config/task/taskrc" "$SCRATCH/before.taskrc"
check "second install exits 0" run "$H" install.sh
check "second install keeps one widget" test "$(widget_count "$H")" = 1
check "second install leaves shell.json unchanged" cmp -s "$SCRATCH/before.json" "$H/.config/omarchy/shell.json"
check "second install leaves taskrc unchanged" cmp -s "$SCRATCH/before.taskrc" "$H/.config/task/taskrc"
check "second install makes no new backups" test -z "$(find "$H" -name '*.bak.taskline.*' -newer "$SCRATCH/before.json" -print -quit)"

# --- existing user config ----------------------------------------------------
H="$(new_home)"
mkdir -p "$H/.config/omarchy" "$H/.config/hypr" "$H/.local/share/task"
echo '{"bar":{"layout":{"right":[{"id":"omarchy.network"}]}},"disabledPlugins":["danso.tasks","omarchy.lock"],"idle":{"lock":600}}' \
  >"$H/.config/omarchy/shell.json"
printf 'data.location=~/.local/share/task\nhooks.location=~/my-hooks\n' >"$H/.taskrc"
echo 'precious' >"$H/.local/share/task/taskchampion.sqlite3"
printf 'local o = require("omarchy")\n' >"$H/.config/hypr/hyprland.lua"
cp "$H/.taskrc" "$SCRATCH/user.taskrc"
check "install over existing config exits 0" run "$H" install.sh
check "user taskrc untouched when hooks.location set" cmp -s "$SCRATCH/user.taskrc" "$H/.taskrc"
check "hook goes to user's hooks.location" test -x "$H/my-hooks/on-exit.omarchy-bar"
check "widget re-enabled (removed from disabledPlugins)" \
  test "$(jq -c '.disabledPlugins' "$H/.config/omarchy/shell.json")" = '["omarchy.lock"]'
check "other settings preserved" test "$(jq '.idle.lock' "$H/.config/omarchy/shell.json")" = 600
check "shell.json backed up" test -n "$(find "$H/.config/omarchy" -name 'shell.json.bak.taskline.*' -print -quit)"
check "task data untouched" test "$(cat "$H/.local/share/task/taskchampion.sqlite3")" = precious
check "hyprland floating rule added" grep -qF 'title = "^Tareas$"' "$H/.config/hypr/hyprland.lua"

# --- invalid shell.json is refused, not overwritten -------------------------
H2="$(new_home)"
mkdir -p "$H2/.config/omarchy"
echo '{broken' >"$H2/.config/omarchy/shell.json"
check "invalid shell.json makes install fail" bash -c "! env -i HOME='$H2' PATH='$H2/bin:/usr/bin:/bin' TASKLINE_NO_IPC=1 bash '$ROOT/install.sh' >/dev/null 2>&1"
check "invalid shell.json left as is" test "$(cat "$H2/.config/omarchy/shell.json")" = '{broken'

# --- uninstall ---------------------------------------------------------------
check "uninstall exits 0" run "$H" uninstall.sh
check "plugin removed" test ! -e "$H/.config/omarchy/plugins/danso.tasks"
check "hook removed" test ! -e "$H/my-hooks/on-exit.omarchy-bar"
check "widget removed from layout" test "$(widget_count "$H")" = 0
check "other widgets kept" test "$(jq -c '.bar.layout.right' "$H/.config/omarchy/shell.json")" = '[{"id":"omarchy.network"}]'
check "window rule removed" bash -c "! grep -q Tareas '$H/.config/hypr/hyprland.lua'"
check "hyprland config otherwise kept" grep -qF 'require("omarchy")' "$H/.config/hypr/hyprland.lua"
check "taskrc kept on uninstall" cmp -s "$SCRATCH/user.taskrc" "$H/.taskrc"
check "task data kept on uninstall" test "$(cat "$H/.local/share/task/taskchampion.sqlite3")" = precious
echo '#!/bin/sh' >"$H/my-hooks/on-exit.omarchy-bar"
check "uninstall twice exits 0" run "$H" uninstall.sh
check "foreign hook with same name is kept" test -e "$H/my-hooks/on-exit.omarchy-bar"

# --- upgrade: backups live outside plugins/ and hooks/ ----------------------
H="$(new_home)"
check "install for upgrade test" run "$H" install.sh
P="$H/.config/omarchy/plugins"
B="$H/.local/state/taskline/backups"
check "updater installed and executable" test -x "$P/danso.tasks/taskline-update"
mkdir -p "$P/danso.tasks.bak.taskline.20991231000000"           # left by Taskline 1.0.0
echo '{"id":"danso.tasks"}' >"$P/danso.tasks.bak.taskline.20991231000000/manifest.json"
for i in 1 2 3 4 5 6; do mkdir -p "$B/danso.tasks.2025010100000$i"; done
echo '// local edit' >>"$P/danso.tasks/Tasks.qml"
echo '# local edit' >>"$H/.config/task/hooks/on-exit.omarchy-bar"
check "upgrade install exits 0" run "$H" install.sh
check "plugin replaced by the packaged one" cmp -s "$ROOT/omarchy/danso.tasks/Tasks.qml" "$P/danso.tasks/Tasks.qml"
check "no backup folders left inside plugins/" test -z "$(find "$P" -mindepth 1 -maxdepth 1 -name '*.bak*' -print -quit)"
check "old 1.0.0 backup moved out of plugins/" test -f "$B/danso.tasks.20991231000000/manifest.json"
check "replaced plugin backed up with the local edit" \
  bash -c "grep -l '// local edit' '$B'/danso.tasks.*/Tasks.qml >/dev/null"
check "only the last 5 plugin backups kept" test "$(find "$B" -mindepth 1 -maxdepth 1 -type d -name 'danso.tasks.*' | wc -l)" = 5
check "hook backup not in the hooks folder" test -z "$(find "$H/.config/task/hooks" -name '*.bak*' -print -quit)"
check "hook backup kept in backups" bash -c "grep -l '# local edit' '$B'/on-exit.omarchy-bar.* >/dev/null"

# --- missing Taskwarrior only warns ----------------------------------------
H="$(new_home)"
check "install without task exits 0" env -i HOME="$H" PATH="$H/bin:/usr/bin:/bin" OMARCHY_PATH="$H/omarchy" \
  TASKLINE_NO_IPC=1 TASKLINE_TASK_CMD=taskline-missing-task bash "$ROOT/install.sh" >"$H/out.log" 2>&1
check "install without task explains dependency" grep -q 'Taskwarrior (task) is not installed' "$H/out.log"

# --- release tarball path (curl | bash) ------------------------------------
H="$(new_home)"
mkdir -p "$SCRATCH/pkg/taskline-omarchy"
cp -r "$ROOT/omarchy" "$ROOT/install.sh" "$ROOT/uninstall.sh" "$SCRATCH/pkg/taskline-omarchy/"
tar -czf "$SCRATCH/taskline-omarchy.tar.gz" -C "$SCRATCH/pkg" taskline-omarchy
printf '#!/bin/sh\ncat "%s"\n' "$SCRATCH/taskline-omarchy.tar.gz" >"$H/bin/curl"
chmod +x "$H/bin/curl"
check "piped install downloads release and installs" bash -c \
  "cat '$ROOT/install.sh' | env -i HOME='$H' PATH='$H/bin:/usr/bin:/bin' OMARCHY_PATH='$H/omarchy' TASKLINE_NO_IPC=1 bash >'$H/out.log' 2>&1"
check "piped install placed plugin" test -f "$H/.config/omarchy/plugins/danso.tasks/manifest.json"

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
(( FAIL == 0 ))
