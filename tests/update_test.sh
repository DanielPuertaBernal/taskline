#!/usr/bin/env bash
# Tests for omarchy/danso.tasks/taskline-update against a fake GitHub release.
# Every case runs in a temporary HOME with fake curl and notify-send, so no
# network is used and the real user's config and tasks are never touched.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PASS=0
FAIL=0
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
API="https://api.test/repos/taskline/releases/latest"

check() {
  local name="$1"; shift
  if "$@"; then PASS=$((PASS + 1)); printf 'ok   %s\n' "$name"
  else FAIL=$((FAIL + 1)); printf 'FAIL %s\n' "$name"; fi
}

# make_release <dir> <version> [tamper]: tarball + SHA256SUMS.txt + API reply.
make_release() {
  local dir="$1" version="$2" stage
  stage="$(mktemp -d "$SCRATCH/stage.XXXX")"
  mkdir -p "$dir" "$stage/taskline-omarchy"
  cp -r "$ROOT/omarchy" "$ROOT/install.sh" "$ROOT/uninstall.sh" "$stage/taskline-omarchy/"
  jq --arg v "$version" '.version = $v' "$ROOT/omarchy/danso.tasks/manifest.json" \
    >"$stage/taskline-omarchy/omarchy/danso.tasks/manifest.json"
  tar -czf "$dir/taskline-omarchy.tar.gz" -C "$stage" taskline-omarchy
  (cd "$dir" && sha256sum taskline-omarchy.tar.gz >SHA256SUMS.txt)
  [[ ${3:-} == tamper ]] && echo "extra bytes" >>"$dir/taskline-omarchy.tar.gz"
  jq -n --arg tag "v$version" '{tag_name: $tag, assets: [
    {name: "taskline.exe", browser_download_url: ("https://dl.test/\($tag)/taskline.exe")},
    {name: "taskline-omarchy.tar.gz", browser_download_url: ("https://dl.test/\($tag)/taskline-omarchy.tar.gz")},
    {name: "SHA256SUMS.txt", browser_download_url: ("https://dl.test/\($tag)/SHA256SUMS.txt")}]}' >"$dir/latest"
}

new_home() {
  local home
  home="$(mktemp -d "$SCRATCH/home.XXXX")"
  mkdir -p "$home/bin" "$home/omarchy/config/omarchy" "$home/.local/share/task" "$home/release"
  printf '#!/bin/sh\necho 3.5.0\n' >"$home/bin/task"
  # shellcheck disable=SC2016 # expanded by the fake notify-send, not here
  printf '#!/bin/sh\necho "$*" >>"$HOME/notify.log"\n' >"$home/bin/notify-send"
  cat >"$home/bin/curl" <<'CURL'
#!/usr/bin/env bash
# Serves https://*/<name> from $FAKE_DIR/<name>; -o writes to a file.
out="" url=""
while (($#)); do
  case $1 in
    -o|-H|--max-time) [[ $1 == -o ]] && out="$2"; shift ;;
    http*) url="$1" ;;
  esac
  shift
done
[[ -f $FAKE_DIR/offline ]] && exit 7
echo "$url" >>"$FAKE_DIR/calls"
file="$FAKE_DIR/${url##*/}"
[[ -f $file ]] || exit 22
if [[ -n $out ]]; then cp "$file" "$out"; else cat "$file"; fi
CURL
  chmod +x "$home/bin/task" "$home/bin/notify-send" "$home/bin/curl"
  echo '{"bar":{"layout":{"right":[]}}}' >"$home/omarchy/config/omarchy/shell.json"
  echo 'precious' >"$home/.local/share/task/taskchampion.sqlite3"
  env -i HOME="$home" PATH="$home/bin:/usr/bin:/bin" OMARCHY_PATH="$home/omarchy" TASKLINE_NO_IPC=1 \
    bash "$ROOT/install.sh" >"$home/install.log" 2>&1
  printf '%s\n' "$home"
}

update() { # update <home> [args...]
  local home="$1"; shift
  env -i HOME="$home" PATH="$home/bin:/usr/bin:/bin" TASKLINE_NO_IPC=1 TASKLINE_API_URL="$API" \
    FAKE_DIR="$home/release" bash "$home/.config/omarchy/plugins/danso.tasks/taskline-update" "$@" \
    >"$home/update.out" 2>&1
}

installed_version() { jq -r .version "$1/.config/omarchy/plugins/danso.tasks/manifest.json"; }
CURRENT="$(jq -r .version "$ROOT/omarchy/danso.tasks/manifest.json")"

# --- newer release is verified and installed --------------------------------
H="$(new_home)"
make_release "$H/release" 9.9.9
check "install starts at the repo version" test "$(installed_version "$H")" = "$CURRENT"
check "auto update exits 0" update "$H" --auto
check "newer version installed" test "$(installed_version "$H")" = 9.9.9
check "notification sent" grep -q 'Actualizado a v9.9.9' "$H/notify.log"
check "previous plugin backed up" bash -c "jq -e '.version == \"$CURRENT\"' '$H'/.local/state/taskline/backups/danso.tasks.*/manifest.json >/dev/null"
check "task data untouched" test "$(cat "$H/.local/share/task/taskchampion.sqlite3")" = precious
check "nothing left in plugins/ but the plugin" \
  test "$(find "$H/.config/omarchy/plugins" -mindepth 1 -maxdepth 1 | wc -l)" = 1
check "daily stamp written" test "$(cat "$H/.local/state/taskline/last-update-check")" = "$(date +%F)"
: >"$H/release/calls"
check "second auto run the same day exits 0" update "$H" --auto
check "second auto run makes no request" test ! -s "$H/release/calls"

# --- checksum mismatch aborts ------------------------------------------------
H="$(new_home)"
make_release "$H/release" 9.9.9 tamper
update "$H" --auto; check "mismatch exits non-zero" test $? -ne 0
check "mismatch installs nothing" test "$(installed_version "$H")" = "$CURRENT"
check "mismatch is logged" grep -q 'checksum mismatch' "$H/.local/state/taskline/update.log"
check "mismatch notifies" grep -q 'no pasó la verificación' "$H/notify.log"
check "mismatch leaves no backup" test ! -e "$H/.local/state/taskline/backups"

# --- release without SHA256SUMS.txt is not installed -------------------------
H="$(new_home)"
make_release "$H/release" 9.9.9
jq '.assets |= map(select(.name != "SHA256SUMS.txt"))' "$H/release/latest" >"$H/release/l" && mv "$H/release/l" "$H/release/latest"
check "no checksum file exits 0" update "$H" --auto
check "no checksum file installs nothing" test "$(installed_version "$H")" = "$CURRENT"
check "tarball never downloaded without checksums" bash -c "! grep -q tar.gz '$H/release/calls'"

# --- same version: nothing downloaded ----------------------------------------
H="$(new_home)"
make_release "$H/release" "$CURRENT"
check "up to date exits 0" update "$H"
check "up to date downloads nothing" test "$(wc -l <"$H/release/calls")" = 1
check "up to date says so" grep -q "is up to date" "$H/update.out"

# --- offline is silent and retried next time ----------------------------------
H="$(new_home)"
make_release "$H/release" 9.9.9
touch "$H/release/offline"
check "offline exits 0" update "$H" --auto
check "offline prints nothing" test ! -s "$H/update.out"
check "offline writes no stamp (retry next launch)" test ! -e "$H/.local/state/taskline/last-update-check"
rm "$H/release/offline"
check "back online exits 0" update "$H" --auto
check "back online installs" test "$(installed_version "$H")" = 9.9.9

# --- opt-out -----------------------------------------------------------------
H="$(new_home)"
make_release "$H/release" 9.9.9
mkdir -p "$H/.config/taskline"
echo 'auto_update = false' >"$H/.config/taskline/omarchy.conf"
check "opt-out exits 0" update "$H" --auto
check "opt-out makes no request" test ! -e "$H/release/calls"
check "manual run still works when opted out" update "$H"
check "manual run installed" test "$(installed_version "$H")" = 9.9.9

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
(( FAIL == 0 ))
