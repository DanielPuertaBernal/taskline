#!/usr/bin/env bash
# Build dist/taskline-omarchy.tar.gz: install.sh, uninstall.sh, the plugin and the hook.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/taskline-omarchy" "$root/dist"
cp -r "$root/omarchy" "$root/install.sh" "$root/uninstall.sh" "$root/LICENSE" "$stage/taskline-omarchy/"
tar -czf "$root/dist/taskline-omarchy.tar.gz" -C "$stage" taskline-omarchy
echo "$root/dist/taskline-omarchy.tar.gz"
