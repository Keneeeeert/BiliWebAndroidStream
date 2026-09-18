#!/usr/bin/env bash
# Package the Firefox extension into dist/
set -euo pipefail
cd "$(dirname "$0")/.."
version="$(python3 -c 'import json;print(json.load(open("extension/manifest.json"))["version"])')"
out="dist/BiliWebAndroidStream-firefox-v${version}"
mkdir -p dist
(cd extension && zip -qr "../${out}.zip" .)
echo "built ${out}.zip"
