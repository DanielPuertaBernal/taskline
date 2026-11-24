#!/usr/bin/env bash
# Taskline uninstaller for Omarchy.
#
# Removes the danso.tasks plugin, its Taskwarrior hook and its bar entry.
# Your Taskwarrior tasks and taskrc settings are never touched; every config
# file edited is backed up first.

set -euo pipefail

PLUGIN_ID="danso.tasks"
HOOK_NAME="on-exit.omarchy-bar"
HOOK_MARKER="omarchy-shell danso.tasks refresh"
STAMP="$(date +%Y%m%d%H%M%S)"

info() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }

backup() {
  [[ -e $1 ]] || return 0
  cp -a "$1" "$1.bak.taskline.$STAMP"
  info "Backed up $1 -> $1.bak.taskline.$STAMP"
}

expand_tilde() {
  local p="$1"
  # shellcheck disable=SC2088 # literal tilde is what we are matching
  if [[ $p == "~" ]]; then p="$HOME"; elif [[ $p == "~/"* ]]; then p="$HOME/${p#\~/}"; fi
  printf '%s\n' "$p"
}

find_taskrc() {
  if [[ -n ${TASKRC:-} ]]; then
    expand_tilde "$TASKRC"
  elif [[ -f $HOME/.taskrc ]]; then
    printf '%s\n' "$HOME/.taskrc"
  else
    printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/task/taskrc"
  fi
}

remove_widget() {
  local shell_json="$HOME/.config/omarchy/shell.json"
  [[ -f $shell_json ]] || return 0
  if ! jq -e . "$shell_json" >/dev/null 2>&1; then
    warn "$shell_json is not valid JSON; remove '$PLUGIN_ID' from it by hand"
    return 0
  fi
  local present
  present="$(jq --arg id "$PLUGIN_ID" '
    ([(.bar.layout // {}) | to_entries[] | .value[]? | select(.id == $id)] | length > 0)
    or ((.plugins // []) | any(.id == $id))
  ' "$shell_json")"
  [[ $present == true ]] || { ok "Widget not in $shell_json"; return 0; }
  backup "$shell_json"
  local tmp
  tmp="$(mktemp "$shell_json.XXXXXX")"
  jq --arg id "$PLUGIN_ID" '
    (if .bar.layout then .bar.layout |= map_values(map(select(.id != $id))) else . end)
    | (if .plugins then .plugins |= map(select(.id != $id)) else . end)
  ' "$shell_json" >"$tmp"
  mv "$tmp" "$shell_json"
  ok "Widget removed from $shell_json"
}

remove_hook() {
  local taskrc hooks_dir line
  taskrc="$(find_taskrc)"
  hooks_dir="${XDG_CONFIG_HOME:-$HOME/.config}/task/hooks"
  if [[ -f $taskrc ]]; then
    line="$(grep -E '^[[:space:]]*hooks\.location[[:space:]]*=' "$taskrc" | tail -n1 || true)"
    [[ -n $line ]] && hooks_dir="$(expand_tilde "$(sed -E 's/^[^=]*=[[:space:]]*//; s/[[:space:]]*$//' <<<"$line")")"
  fi
  local hook="$hooks_dir/$HOOK_NAME"
  if [[ -f $hook ]]; then
    if grep -qF "$HOOK_MARKER" "$hook"; then
      rm -f "$hook"
      ok "Hook removed from $hook"
    else
      warn "$hook does not look like the Taskline hook; left in place"
    fi
  fi
}

remove_plugin() {
  local dest="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
  if [[ -d $dest ]]; then
    rm -rf "$dest"
    ok "Plugin removed from $dest"
  fi
}

remove_window_rule() {
  local hypr="$HOME/.config/hypr/hyprland.lua"
  [[ -f $hypr ]] || return 0
  grep -qF -- '-- Taskline: float the task panel' "$hypr" || return 0
  backup "$hypr"
  local tmp
  tmp="$(mktemp "$hypr.XXXXXX")"
  awk '
    /^-- Taskline: float the task panel/ { skip = 1; next }
    skip == 1 && /title = "\^Tareas\$"/ { skip = 0; next }
    { skip = 0; print }
  ' "$hypr" >"$tmp"
  mv "$tmp" "$hypr"
  ok "Removed the Taskline window rule from $hypr"
}

main() {
  info "Uninstalling Taskline from Omarchy"
  command -v jq >/dev/null || { warn "jq not found; skipping shell.json cleanup"; }
  command -v jq >/dev/null && remove_widget
  remove_hook
  remove_plugin
  remove_window_rule
  if [[ ${TASKLINE_NO_IPC:-0} != 1 ]] && command -v omarchy-shell >/dev/null; then
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
  fi
  ok "Taskline removed. Your Taskwarrior data and taskrc were not modified."
}

main "$@"
