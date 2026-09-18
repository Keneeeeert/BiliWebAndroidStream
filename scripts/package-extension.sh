#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="${project_dir}/dist"
mkdir -p "$output_dir"
archive="${output_dir}/BiliWebAndroidStream-firefox.zip"
python3 - "$archive" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
if path.exists():
    path.unlink()
PY
(cd "$project_dir/extension" && zip -qr "$archive" .)
echo "Packaged $archive"
