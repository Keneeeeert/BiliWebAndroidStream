#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
cargo build --release
mkdir -p "$project_dir/bin"
cp target/release/bili-web-android-stream-helper "$project_dir/bin/"
echo "Built bin/bili-web-android-stream-helper"
