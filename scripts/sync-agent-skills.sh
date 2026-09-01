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

drift=0
skills=(
  wyrd-spec
  wyrd-plan
  wyrd-task-readiness
  wyrd-implement
  wyrd-task-review
  wyrd-change-review
)
retired_skills=(
  wyrd-plan-review
  wyrd-review
  wyrd-review-and-plan
)

for skill_name in "${skills[@]}"; do
  source_dir="$source_root/$skill_name"
  if [[ ! -d "$source_dir" ]]; then
    echo "missing canonical skill: .agents/skills/$skill_name" >&2
    exit 1
  fi
  mirror_dir="$mirror_root/$skill_name"

  if [[ "$mode" == "--write" ]]; then
    mkdir -p "$mirror_dir"
    rsync -a --delete \
      --exclude '/agents/' \
      --exclude '/__pycache__/' \
      "$source_dir/" "$mirror_dir/"
    continue
  fi

  if [[ ! -d "$mirror_dir" ]]; then
    echo "missing Claude skill mirror: .claude/skills/$skill_name" >&2
    drift=1
    continue
  fi

  changes="$(rsync -aicn --delete \
    --exclude '/agents/' \
    --exclude '/__pycache__/' \
    "$source_dir/" "$mirror_dir/")"
  if [[ -n "$changes" ]]; then
    echo "Claude skill mirror drift: $skill_name" >&2
    echo "$changes" >&2
    drift=1
  fi
done

if [[ "$mode" == "--check" ]]; then
  for skill_name in "${retired_skills[@]}"; do
    for retired_entry in \
      "$source_root/$skill_name/SKILL.md" \
      "$mirror_root/$skill_name/SKILL.md"; do
      if [[ -e "$retired_entry" ]]; then
        echo "retired workflow skill is still discoverable: ${retired_entry#"$repo_root/"}" >&2
        drift=1
      fi
    done
  done
fi

exit "$drift"
