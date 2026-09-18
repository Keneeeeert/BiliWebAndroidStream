#!/usr/bin/env bash
set -euo pipefail
install_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/biliwebandroidstream"
host_dir="${HOME}/.mozilla/native-messaging-hosts"
if [[ "${OSTYPE:-}" == darwin* ]]; then host_dir="${HOME}/Library/Application Support/Mozilla/NativeMessagingHosts"; fi
rm -f "$host_dir/com.biliwebandroidstream.helper.json"
rm -rf "$install_dir"
echo "Removed BiliWebAndroidStream helper and native host manifest"
