#!/usr/bin/env bash

set -euo pipefail

mode="${1:---write}"
case "$mode" in
  --write|--check) ;;
  *)
    echo "usage: $0 [--write|--check]" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_root="$repo_root/.agents/skills"
mirror_root="$repo_root/.claude/skills"

if [[ "$mode" == "--write" ]]; then
  mkdir -p "$mirror_root"
elif [[ ! -d "$mirror_root" ]]; then
  echo "missing Claude skill root: .claude/skills" >&2
  exit 1
fi

while IFS= read -r skill_dir; do
  if [[ ! -f "$skill_dir/SKILL.md" ]]; then
    echo "canonical skill is missing SKILL.md: ${skill_dir#"$repo_root/"}" >&2
    exit 1
  fi
done < <(find "$source_root" -mindepth 1 -maxdepth 1 -type d | sort)

if [[ "$mode" == "--write" ]]; then
  rsync -a --delete --delete-excluded \
    --exclude '*/agents/' \
    --exclude '*/__pycache__/' \
    "$source_root/" "$mirror_root/"
  exit 0
fi

changes="$(rsync -aicn --delete --delete-excluded \
  --exclude '*/agents/' \
  --exclude '*/__pycache__/' \
  "$source_root/" "$mirror_root/")"
if [[ -n "$changes" ]]; then
  echo "Claude skill tree drift:" >&2
  echo "$changes" >&2
  exit 1
fi
