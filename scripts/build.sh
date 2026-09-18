#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"

cargo build --release
mkdir -p bin
cp target/release/bili-web-android-stream-helper bin/

version="$(python3 -c 'import json;print(json.load(open("extension/manifest.json"))["version"])')"
mkdir -p dist
archive="dist/BiliWebAndroidStream-firefox-v${version}.zip"
rm -f "$archive"
(cd extension && zip -qr "../$archive" .)

echo "Built bin/bili-web-android-stream-helper"
echo "Packaged $archive"
