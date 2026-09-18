#!/usr/bin/env bash
# Package the Firefox extension into dist/ as an installable .xpi
set -euo pipefail
cd "$(dirname "$0")/.."
version="$(python3 -c 'import json;print(json.load(open("extension/manifest.json"))["version"])')"
out="dist/BiliWebAndroidStream-v${version}.xpi"
mkdir -p dist
rm -f "$out"
(cd extension && zip -qr "../${out}" .)
echo "built ${out}"
