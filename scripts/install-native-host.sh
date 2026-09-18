#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_dir="$(cd "$script_dir/.." && pwd)"
helper_source="${BILI_HELPER_BINARY:-${script_dir}/bili-web-android-stream-helper}"
if [[ ! -x "$helper_source" ]]; then helper_source="${project_dir}/bin/bili-web-android-stream-helper"; fi
install_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/biliwebandroidstream"
host_dir="${HOME}/.mozilla/native-messaging-hosts"
if [[ "${OSTYPE:-}" == darwin* ]]; then host_dir="${HOME}/Library/Application Support/Mozilla/NativeMessagingHosts"; fi
if [[ ! -x "$helper_source" ]]; then echo "Helper not found: $helper_source" >&2; exit 1; fi
mkdir -p "$install_dir" "$host_dir"
cp "$helper_source" "$install_dir/bili-web-android-stream-helper"
chmod 700 "$install_dir/bili-web-android-stream-helper"
python3 - "$host_dir/com.biliwebandroidstream.helper.json" "$install_dir/bili-web-android-stream-helper" <<'PY'
import json, pathlib, sys
manifest, helper = sys.argv[1:]
pathlib.Path(manifest).write_text(json.dumps({"name":"com.biliwebandroidstream.helper","description":"BiliWebAndroidStream native helper","path":helper,"type":"stdio","allowed_extensions":["biliwebandroidstream@example.invalid"]}, ensure_ascii=False, indent=2) + "\n")
PY
chmod 600 "$host_dir/com.biliwebandroidstream.helper.json"
echo "Installed helper to $install_dir"
echo "Installed native host manifest to $host_dir"
