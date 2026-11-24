#!/usr/bin/env bash
# Taskline installer for Omarchy.
#
# Installs the danso.tasks shell plugin (bar widget + task panel), the
# Taskwarrior on-exit hook that refreshes it, and enables the widget in the
# Omarchy bar. Safe to run more than once: every step checks before it writes,
# and every user config file it edits is backed up first.
#
# Usage:
#   ./install.sh                      # from a clone or an extracted release
#   curl -fsSL https://raw.githubusercontent.com/DanielPuertaBernal/taskline/main/install.sh | bash
#
# Environment:
#   TASKLINE_SECTION   bar section for the widget (default: right)
#   TASKLINE_SRC       directory holding omarchy/ (skips auto-detection/download)
#   TASKLINE_RELEASE_URL  tarball to download when run without local files
#   TASKLINE_NO_IPC=1  do not talk to a running omarchy-shell
#   TASKLINE_TASK_CMD  Taskwarrior binary to check (default: task)

set -euo pipefail

PLUGIN_ID="danso.tasks"
HOOK_NAME="on-exit.omarchy-bar"
WINDOW_RULE='o.window({ class = "^org.quickshell$", title = "^Tareas$" }, { tag = "+floating-window" })'
SECTION="${TASKLINE_SECTION:-right}"
RELEASE_URL="${TASKLINE_RELEASE_URL:-https://github.com/DanielPuertaBernal/taskline/releases/latest/download/taskline-omarchy.tar.gz}"
STAMP="$(date +%Y%m%d%H%M%S)"

info() { printf '\033[1;34m::\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

backup() {
  # backup <file>: copy next to the original with a timestamp suffix.
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

# ---------------------------------------------------------------- sources ---
resolve_source() {
  if [[ -n ${TASKLINE_SRC:-} ]]; then
    printf '%s\n' "$TASKLINE_SRC"
    return
  fi
  local here=""
  if [[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]]; then
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  fi
  if [[ -n $here && -f $here/omarchy/$PLUGIN_ID/manifest.json ]]; then
    printf '%s\n' "$here"
    return
  fi
  # Piped through curl: fetch the packaged release.
  command -v curl >/dev/null || die "curl is required to download Taskline"
  local tmp
  tmp="$(mktemp -d)"
  info "Downloading $RELEASE_URL" >&2
  curl -fsSL "$RELEASE_URL" | tar -xz -C "$tmp" || die "could not download $RELEASE_URL"
  local dir
  dir="$(dirname "$(find "$tmp" -path "*/omarchy/$PLUGIN_ID/manifest.json" -print -quit)")"
  [[ -n $dir && $dir != "." ]] || die "downloaded archive does not contain the plugin"
  (cd "$dir/../.." && pwd)
}

# ----------------------------------------------------------- dependencies ---
check_dependencies() {
  command -v jq >/dev/null || die "jq is required (Omarchy ships it): sudo pacman -S jq"
  local task_cmd="${TASKLINE_TASK_CMD:-task}"
  if command -v "$task_cmd" >/dev/null; then
    local version
    version="$("$task_cmd" --version 2>/dev/null || true)"
    if [[ ${version%%.*} =~ ^[0-9]+$ ]] && (( ${version%%.*} >= 3 )); then
      ok "Taskwarrior $version found"
    else
      warn "Taskwarrior '$version' found; Taskline is built for Taskwarrior 3.x"
    fi
  else
    warn "Taskwarrior (task) is not installed. The widget stays hidden until it is."
    warn "Install it with: omarchy pkg add task   (or: sudo pacman -S task)"
  fi
}

# ----------------------------------------------------------------- plugin ---
install_plugin() {
  local src="$1/omarchy/$PLUGIN_ID"
  local dest="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
  mkdir -p "$(dirname "$dest")"
  if [[ -d $dest ]]; then
    if diff -rq "$src" "$dest" >/dev/null 2>&1; then
      ok "Plugin already up to date at $dest"
      return
    fi
    mv "$dest" "$dest.bak.taskline.$STAMP"
    info "Previous plugin moved to $dest.bak.taskline.$STAMP"
  fi
  cp -r "$src" "$dest"
  ok "Plugin installed to $dest"
}

# ---------------------------------------------------------------- taskrc ----
find_taskrc() {
  if [[ -n ${TASKRC:-} ]]; then
    expand_tilde "$TASKRC"
  elif [[ -f $HOME/.taskrc ]]; then
    printf '%s\n' "$HOME/.taskrc"
  else
    printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/task/taskrc"
  fi
}

install_hook() {
  local src="$1/omarchy/taskwarrior-hooks/$HOOK_NAME"
  local taskrc hooks_dir line
  taskrc="$(find_taskrc)"

  if [[ ! -f $taskrc ]]; then
    mkdir -p "$(dirname "$taskrc")"
    : >"$taskrc"
    info "Created $taskrc"
  fi

  line="$(grep -E '^[[:space:]]*hooks\.location[[:space:]]*=' "$taskrc" | tail -n1 || true)"
  if [[ -n $line ]]; then
    hooks_dir="$(expand_tilde "$(sed -E 's/^[^=]*=[[:space:]]*//; s/[[:space:]]*$//' <<<"$line")")"
  else
    hooks_dir="${XDG_CONFIG_HOME:-$HOME/.config}/task/hooks"
    [[ -s $taskrc ]] && backup "$taskrc"
    printf '\n# Added by Taskline: refresh the Omarchy bar after task commands\nhooks.location=%s\n' \
      "${hooks_dir/#$HOME/\~}" >>"$taskrc"
    ok "Added hooks.location to $taskrc"
  fi

  if grep -qE '^[[:space:]]*hooks[[:space:]]*=[[:space:]]*(off|0|no|false)' "$taskrc"; then
    warn "hooks are disabled in $taskrc; the bar will refresh once a minute instead of instantly"
  fi

  mkdir -p "$hooks_dir"
  if [[ -f $hooks_dir/$HOOK_NAME ]] && cmp -s "$src" "$hooks_dir/$HOOK_NAME"; then
    ok "Hook already installed at $hooks_dir/$HOOK_NAME"
  else
    [[ -f $hooks_dir/$HOOK_NAME ]] && backup "$hooks_dir/$HOOK_NAME"
    install -m 0755 "$src" "$hooks_dir/$HOOK_NAME"
    ok "Hook installed to $hooks_dir/$HOOK_NAME"
  fi
  chmod +x "$hooks_dir/$HOOK_NAME"
}

# ------------------------------------------------------------- shell.json ---
enable_widget() {
  local shell_json="$HOME/.config/omarchy/shell.json"
  local defaults="${OMARCHY_PATH:-/usr/share/omarchy}/config/omarchy/shell.json"

  if [[ ! -f $shell_json ]]; then
    if [[ -f $defaults ]]; then
      mkdir -p "$(dirname "$shell_json")"
      cp "$defaults" "$shell_json"
      info "Created $shell_json from Omarchy defaults"
    else
      warn "No $shell_json found and no Omarchy defaults to copy."
      warn "Enable the widget later with: omarchy plugin enable $PLUGIN_ID --section $SECTION"
      return
    fi
  fi

  jq -e . "$shell_json" >/dev/null 2>&1 || die "$shell_json is not valid JSON; fix it and run again"

  local present
  present="$(jq --arg id "$PLUGIN_ID" '
    [(.bar.layout // {}) | to_entries[] | .value[]? | select(.id == $id)] | length > 0
  ' "$shell_json")"
  local disabled
  disabled="$(jq --arg id "$PLUGIN_ID" '(.disabledPlugins // []) | index($id) != null' "$shell_json")"

  if [[ $present == true && $disabled == false ]]; then
    ok "Widget already enabled in $shell_json"
    return
  fi

  backup "$shell_json"
  local tmp
  tmp="$(mktemp "$shell_json.XXXXXX")"
  jq --arg id "$PLUGIN_ID" --arg section "$SECTION" '
    .disabledPlugins = ((.disabledPlugins // []) - [$id])
    | if ([(.bar.layout // {}) | to_entries[] | .value[]? | select(.id == $id)] | length) > 0
      then .
      else .bar.layout[$section] = ((.bar.layout[$section] // []) + [{id: $id}])
      end
    | if (.disabledPlugins | length) == 0 then del(.disabledPlugins) else . end
  ' "$shell_json" >"$tmp"
  mv "$tmp" "$shell_json"
  ok "Widget enabled in bar section '$SECTION' ($shell_json)"
}

# ---------------------------------------------------------- hyprland rule ---
install_window_rule() {
  local hypr="$HOME/.config/hypr/hyprland.lua"
  [[ -f $hypr ]] || return 0
  if grep -qF 'title = "^Tareas$"' "$hypr"; then
    ok "Hyprland floating rule for the task panel already present"
    return
  fi
  backup "$hypr"
  printf '\n-- Taskline: float the task panel like the Omarchy floating terminal\n%s\n' "$WINDOW_RULE" >>"$hypr"
  ok "Added a Hyprland floating rule for the task panel to $hypr"
}

reload_shell() {
  [[ ${TASKLINE_NO_IPC:-0} == 1 ]] && return 0
  if command -v omarchy-shell >/dev/null; then
    omarchy-shell shell rescanPlugins >/dev/null 2>&1 || true
    omarchy-shell "$PLUGIN_ID" refresh >/dev/null 2>&1 || true
  fi
}

main() {
  info "Installing Taskline for Omarchy"
  check_dependencies
  local src
  src="$(resolve_source)"
  [[ -f $src/omarchy/$PLUGIN_ID/manifest.json ]] || die "plugin files not found under $src"
  install_plugin "$src"
  install_hook "$src"
  enable_widget
  install_window_rule
  reload_shell
  ok "Done. Click the bar widget to open the task panel; add tasks with: task add <text>"
}

main "$@"
