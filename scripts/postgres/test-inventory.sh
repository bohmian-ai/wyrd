#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/wyrd-pg-inventory.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT

cp "$root/mise.toml" "$temp_dir/mise.toml"
python3 - "$temp_dir/mise.toml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
needle = '[tasks."test:wyrd:inner"]\nhide = true\n'
replacement = needle + 'depends = ["test:vala"]\n'
if needle not in text:
    raise SystemExit("inventory fixture anchor is missing")
path.write_text(text.replace(needle, replacement, 1))
PY

if python3 "$root/scripts/postgres/check-inventory.py" "$temp_dir/mise.toml"; then
    echo "inventory nested-lifecycle negative test unexpectedly passed" >&2
    exit 1
fi

echo "postgres inventory negative test: PASS"
