#!/usr/bin/env bash
# Verifies that a release directory holds exactly the bytes its build jobs
# recorded after their package checks passed. Each build job writes a
# SHA256SUMS.<artifact> file beside its outputs; publication downloads them and
# runs this before anything is uploaded. A missing record, an unrecorded file,
# or a changed byte fails closed, so a rebuilt or substituted artifact cannot be
# published.
# Usage: verify-release-artifacts.sh <dir>
set -euo pipefail

dir="${1:?usage: $0 <release-artifact-dir>}"
cd "$dir"

shopt -s nullglob
records=(SHA256SUMS.*)
if (( ${#records[@]} == 0 )); then
  echo "no SHA256SUMS.* digest records in $dir" >&2
  exit 1
fi

cat "${records[@]}" | sha256sum --check --strict

recorded="$(cat "${records[@]}" | awk '{sub(/^\*/, "", $2); print $2}' | sort)"
present="$(find . -type f ! -name 'SHA256SUMS.*' | sed 's|^\./||' | sort)"
if [[ "$recorded" != "$present" ]]; then
  echo "release files differ from the recorded set:" >&2
  diff <(echo "$recorded") <(echo "$present") >&2 || true
  exit 1
fi
echo "verified $(wc -l <<< "$recorded") release files against ${#records[@]} digest records"
