#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/wyrd-plan-runner.sh PLAN_DIR [options]

Runs a Wyrd implementation plan directory sequentially with Codex implementers
and a gated reviewer.

Options:
  --context FILE                 Extra context file passed to every agent.
  --note TEXT                    Extra inline context note passed to every agent.
  --include REGEX                Include plan basenames matching REGEX.
                                 Default: ^[0-9]{2}[a-z]?[-_].*\.md$
  --exclude REGEX                Exclude plan basenames matching REGEX.
                                 Default: ^(00.*|README)\.md$
  --skip REGEX                   Skip implementation plan basenames or paths
                                 matching REGEX. Repeatable.
  --start-at SELECTOR            Start at this implementation plan, including it.
                                 SELECTOR may be a basename, relative path,
                                 or prefix like 05.
  --start-after SELECTOR         Start after this implementation plan.
                                 SELECTOR may be a basename, relative path,
                                 or prefix like 05.
  --per-plan-check CMD           Per-slice check command. Repeatable.
                                 Must be a mise task.
                                 Default: mise run fmt
  --final-check CMD              Final check command. Repeatable.
                                 Must be a mise task.
                                 Default: mise run pre-pr
  --implementer-model MODEL      First-pass implementer model.
                                 Default: gpt-5.4-mini
  --implementer-effort EFFORT    First-pass implementer effort.
                                 Default: medium
  --reviewer-engine ENGINE       Review engine: claude or codex.
                                 Default: claude
  --reviewer-model MODEL         Plan-conformance reviewer model.
                                 Default: claude-opus-4-7
  --reviewer-effort EFFORT       Plan-conformance reviewer effort.
                                 Default: high
  --escalation-model MODEL       Retry implementer model after failures.
                                 Default: gpt-5.5
  --escalation-effort EFFORT     Retry implementer effort after failures.
                                 Default: high
  --max-review-loops N           Maximum implementation/review loops per plan.
                                 Default: 2
  --fast                         Request Codex fast service tier.
  --persist-codex-sessions       Do not pass --ephemeral to codex exec.
  --dry-run                      Print resolved execution plan and exit.
  -h, --help                     Show this help.
USAGE
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '==> %s\n' "$*" >&2
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

quote_cmd() {
  printf '%q ' "$@"
}

repo_root() {
  git rev-parse --show-toplevel 2>/dev/null
}

relpath() {
  local path=$1
  python3 - "$path" "$REPO_ROOT" <<'PY'
import os
import sys

print(os.path.relpath(os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])))
PY
}

sanitize_name() {
  local value=$1
  value=${value##*/}
  value=${value%.*}
  value=$(printf '%s' "$value" | tr -cs '[:alnum:]._-' '-')
  value=${value#-}
  value=${value%-}
  printf '%s' "${value:-plan}"
}

selector_matches_file() {
  local file=$1
  local selector=$2
  local base rel

  base=${file##*/}
  rel=$(relpath "$file")

  [[ "$selector" == "$base" \
    || "$selector" == "$rel" \
    || "$selector" == "$file" \
    || "$base" == "$selector"-* \
    || "$base" == "$selector"_* ]]
}

is_clean_worktree() {
  [[ -z "$(git status --porcelain)" ]]
}

run_command_logged() {
  local label=$1
  local command_text=$2
  local log_file=$3

  info "$label: $command_text"
  {
    printf '$ %s\n' "$command_text"
    bash -lc "$command_text"
  } >"$log_file" 2>&1
}

write_review_schema() {
  local path=$1
  cat >"$path" <<'JSON'
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "status": {
      "type": "string",
      "enum": ["approved", "changes_required"]
    },
    "summary": {
      "type": "string"
    },
    "findings": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "severity": {
            "type": "string",
            "enum": ["critical", "important", "minor"]
          },
          "path": {
            "type": "string"
          },
          "line": {
            "type": ["integer", "null"]
          },
          "issue": {
            "type": "string"
          },
          "required_change": {
            "type": "string"
          }
        },
        "required": ["severity", "path", "line", "issue", "required_change"]
      }
    },
    "recommended_checks": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "commit_title": {
      "type": "string"
    },
    "commit_body": {
      "type": "string"
    }
  },
  "required": [
    "status",
    "summary",
    "findings",
    "recommended_checks",
    "commit_title",
    "commit_body"
  ]
}
JSON
}

build_codex_args() {
  local model=$1
  local effort=$2
  local sandbox=$3
  local output_file=$4
  local schema_file=${5:-}

  CODEX_ARGS=(
    exec
    -C "$REPO_ROOT"
    -m "$model"
    -c "model_reasoning_effort=\"$effort\""
    -c 'approval_policy="never"'
    --sandbox "$sandbox"
    --json
    --output-last-message "$output_file"
  )

  if [[ "$PERSIST_CODEX_SESSIONS" != "1" ]]; then
    CODEX_ARGS+=(--ephemeral)
  fi

  if [[ "$FAST_MODE" == "1" ]]; then
    CODEX_ARGS+=(
      -c 'service_tier="fast"'
      -c 'features.fast_mode=true'
    )
  fi

  if [[ -n "$schema_file" ]]; then
    CODEX_ARGS+=(--output-schema "$schema_file")
  fi

  CODEX_ARGS+=(-)
}

append_file_block() {
  local out=$1
  local label=$2
  local file=$3

  {
    printf '\n## %s\n\n' "$label"
    printf 'Path: %s\n\n' "$(relpath "$file")"
    printf '```text\n'
    cat "$file"
    printf '\n```\n'
  } >>"$out"
}

append_reference_paths() {
  local out=$1
  local context_file

  {
    printf '\n## Reference Paths\n\n'
    printf 'Read these files by path only when they are relevant to the current slice:\n\n'
    printf -- '- %s\n' 'AGENTS.md'
    printf -- '- %s\n' 'architecture/wyrd-design.md'
  } >>"$out"

  if ((${#CONTEXT_FILES[@]} > 0)); then
    {
      printf '\nPlan-directory reference files available by path:\n\n'
      for context_file in "${CONTEXT_FILES[@]}"; do
        printf -- '- %s\n' "$(relpath "$context_file")"
      done
    } >>"$out"
  fi
}

write_slice_manifest() {
  local out=$1
  local plan_file=$2
  local token path
  local paths=()

  while IFS= read -r token; do
    token=${token#./}
    token=${token%:}
    token=${token%,}
    token=${token%;}
    token=${token%.}
    token=${token%\)}
    token=${token%\]}
    token=${token%\"}
    token=${token#\"}
    token=${token%\`}
    token=${token#\`}
    [[ -n "$token" ]] || continue
    [[ "$token" == "$PLAN_DIR"* ]] && continue
    [[ -e "$REPO_ROOT/$token" ]] || continue
    paths+=("$token")
  done < <(rg -o '([A-Za-z0-9_.-]+/)+[A-Za-z0-9_.@+-]+' "$plan_file" 2>/dev/null || true)

  {
    printf '\n## Slice Manifest\n\n'
    printf 'Use this manifest to constrain repository exploration. Start with these paths and nearest tests. Broaden only when the slice cannot be implemented from this set.\n\n'
    printf 'Current slice path: %s\n\n' "$(relpath "$plan_file")"
    printf 'Existing repo paths explicitly mentioned by the slice:\n'
  } >>"$out"

  if ((${#paths[@]} > 0)); then
    printf '%s\n' "${paths[@]}" | sort -u | while IFS= read -r path; do
      printf -- '- %s\n' "$path" >>"$out"
    done
  else
    printf -- '- none found; infer the smallest touch set from the slice text before reading broadly\n' >>"$out"
  fi
}

write_shared_context() {
  local out=$1

  {
    printf '## Repository Context\n\n'
    printf 'Current branch: %s\n\n' "$(git branch --show-current)"
    printf 'Use the current branch. Do not create or switch branches.\n'
    printf 'All changes must follow Wyrd-native vocabulary and repository conventions.\n'
    printf 'Do not reference commits, plan files, plan names, or automation context in code comments or docstrings.\n'
    printf 'Document code only when repo style calls for it, and document durable behavior or invariants.\n'
  } >"$out"

  append_reference_paths "$out"

  if ((${#NOTES[@]} > 0)); then
    {
      printf '\n## Additional Notes\n\n'
      local note
      for note in "${NOTES[@]}"; do
        printf -- '- %s\n' "$note"
      done
    } >>"$out"
  fi
}

write_implementer_prompt() {
  local out=$1
  local plan_file=$2
  local loop_index=$3
  local feedback_file=${4:-}

  write_shared_context "$out"
  write_slice_manifest "$out" "$plan_file"

  {
    printf '\n## Role\n\n'
    printf '$wyrd-rust-python\n\n'
    printf 'You are the implementation worker for one Wyrd plan slice.\n'
    printf 'Implement exactly the current plan slice. Do not implement later slices.\n'
    printf 'Prefer the repo'\''s existing patterns. Keep edits scoped and pragmatic.\n'
    printf 'Do not perform broad repository exploration. Inspect the slice, the manifest paths, and nearest tests first.\n'
    printf 'If the manifest is insufficient, use targeted rg queries for the missing symbol or owning module only.\n'
    printf 'If the slice is blocked by a missing design decision, stop and report the blocker instead of reading unrelated modules.\n'
    printf 'Use mise tasks for checks. Do not run the full pre-pr gate unless the prompt asks for it.\n'
    printf 'When finished, summarize changed behavior, tests/checks run, and any remaining blockers.\n'
    printf '\n## Current Plan Slice\n'
  } >>"$out"

  append_file_block "$out" "Plan Slice" "$plan_file"

  if [[ -n "$feedback_file" ]]; then
    {
      printf '\n## Required Follow-up\n\n'
      printf 'This is implementation loop %s. Address the feedback below and preserve all already-correct work.\n\n' "$loop_index"
      printf '```text\n'
      cat "$feedback_file"
      printf '\n```\n'
    } >>"$out"
  fi
}

write_reviewer_prompt() {
  local out=$1
  local plan_file=$2

  write_shared_context "$out"
  write_slice_manifest "$out" "$plan_file"

  {
    printf '\n## Role\n\n'
    printf 'You are the parent plan-conformance reviewer for this Wyrd implementation slice.\n'
    printf 'Use a condensed, modified review-and-plan review pass: security, bugs/performance, tests, maintainability/style, clean-code/SOLID, developer/agent experience, and Wyrd contract boundaries.\n'
    printf 'Do not run the full review-and-plan pipeline. Do not create .dev/review artifacts. Do not write an implementation plan.\n'
    printf 'Do not edit files. Inspect the current uncommitted diff, including untracked files, and validate it against the plan slice.\n'
    printf 'Focus on deviations from the plan, missing required behavior, missing tests, Wyrd boundary violations, unnecessary scope creep, legacy vocabulary, and likely correctness issues.\n'
    printf 'Only report findings you validate against source. A clean review with zero findings is acceptable.\n'
    printf 'Also provide a reviewer-readable commit title and body based on the actual code change and overall plan intent.\n'
    printf 'The commit title and body must not mention file paths, plan files, plan names, commit hashes, or automation context.\n'
    printf '\n## Current Plan Slice\n'
  } >>"$out"

  append_file_block "$out" "Plan Slice" "$plan_file"

  {
    printf '\n## Diff To Review\n\n'
    printf 'Review the working tree produced by:\n\n'
    printf '```bash\n'
    printf 'git status --short\n'
    printf 'git ls-files --others --exclude-standard\n'
    printf 'git diff HEAD\n'
    printf '```\n'
    printf '\nImportant: `git diff HEAD` omits untracked files. Use `git status --short` and `git ls-files --others --exclude-standard` to identify and inspect newly created files before approving.\n'
    printf '\nReturn only raw JSON matching this schema shape:\n\n'
    printf '```json\n'
    cat "$REVIEW_SCHEMA"
    printf '\n```\n'
  } >>"$out"
}

extract_json_object() {
  local raw_file=$1
  local json_file=$2

  python3 - "$raw_file" "$json_file" <<'PY'
import json
import sys

raw_path, out_path = sys.argv[1], sys.argv[2]
text = open(raw_path, encoding="utf-8").read()
decoder = json.JSONDecoder()
for index, char in enumerate(text):
    if char != "{":
        continue
    try:
        value, end = decoder.raw_decode(text[index:])
    except json.JSONDecodeError:
        continue
    with open(out_path, "w", encoding="utf-8") as out:
        json.dump(value, out, indent=2)
        out.write("\n")
    break
else:
    raise SystemExit("no JSON object found in reviewer output")
PY
}

validate_review_json() {
  local review_file=$1
  jq -e '
    (.status == "approved" or .status == "changes_required")
    and (.findings | type == "array")
    and (.recommended_checks | type == "array")
    and (.commit_title | type == "string")
    and (.commit_body | type == "string")
  ' "$review_file" >/dev/null
}

review_approved() {
  local review_file=$1
  jq -e '
    .status == "approved"
    and ([.findings[] | select(.severity == "critical" or .severity == "important")] | length == 0)
  ' "$review_file" >/dev/null
}

write_review_feedback() {
  local review_file=$1
  local feedback_file=$2

  {
    printf 'The plan-conformance review requires changes.\n\n'
    jq -r '
      "Status: \(.status)\nSummary: \(.summary)\n",
      (.findings[]? | "- [\(.severity)] \(.path):\(.line // "n/a") \(.issue)\n  Required change: \(.required_change)")
    ' "$review_file"
    printf '\nRecommended checks:\n'
    jq -r '.recommended_checks[]? | "- \(.)"' "$review_file"
  } >"$feedback_file"
}

validate_commit_message() {
  local message_file=$1
  local title
  title=$(sed -n '1p' "$message_file")

  [[ -n "$title" ]] || return 1
  [[ "$title" != *".dev/plan"* ]] || return 1
  [[ "$title" != *"Plan:"* ]] || return 1
  [[ "$title" != *".md"* ]] || return 1
  [[ "$title" != */* ]] || return 1

  ! rg -n '(^|[[:space:]])(\.dev/plan|Plan:|this plan|this phase|[[:alnum:]_.-]+\.md|[[:alnum:]_.-]+/[[:alnum:]_./-]+|[0-9]{2}[a-z]?[-_][[:alnum:]_-]+)([[:space:]]|$)' "$message_file" >/dev/null
}

write_commit_message() {
  local review_file=$1
  local message_file=$2

  jq -r '
    .commit_title,
    "",
    .commit_body
  ' "$review_file" >"$message_file"

  validate_commit_message "$message_file"
}

collect_plan_files() {
  ALL_MD_FILES=()
  while IFS= read -r file; do
    ALL_MD_FILES+=("$file")
  done < <(find "$PLAN_DIR" -maxdepth 1 -type f -name '*.md' | sort)

  PLAN_FILES=()
  CONTEXT_FILES=()
  local candidate_files=()

  local file base
  if ((${#ALL_MD_FILES[@]} > 0)); then
    for file in "${ALL_MD_FILES[@]}"; do
      base=${file##*/}
      if [[ "$base" =~ $EXCLUDE_REGEX ]]; then
        CONTEXT_FILES+=("$file")
        continue
      fi
      if [[ "$base" =~ $INCLUDE_REGEX ]]; then
        candidate_files+=("$file")
      else
        CONTEXT_FILES+=("$file")
      fi
    done
  fi

  local start_found=0
  local started=0
  if [[ -z "$START_AT" && -z "$START_AFTER" ]]; then
    started=1
  fi

  local skip_regex skip_file
  if ((${#candidate_files[@]} > 0)); then
    for file in "${candidate_files[@]}"; do
      base=${file##*/}

      if [[ -n "$START_AT" && "$started" == "0" ]]; then
        if selector_matches_file "$file" "$START_AT"; then
          started=1
          start_found=1
        else
          continue
        fi
      fi

      if [[ -n "$START_AFTER" && "$started" == "0" ]]; then
        if selector_matches_file "$file" "$START_AFTER"; then
          started=1
          start_found=1
        fi
        continue
      fi

      skip_file=0
      if ((${#SKIP_REGEXES[@]} > 0)); then
        for skip_regex in "${SKIP_REGEXES[@]}"; do
          if [[ "$base" =~ $skip_regex || "$(relpath "$file")" =~ $skip_regex ]]; then
            skip_file=1
            break
          fi
        done
      fi
      if [[ "$skip_file" == "1" ]]; then
        continue
      fi

      PLAN_FILES+=("$file")
    done
  fi

  if [[ -n "$START_AT" && "$start_found" == "0" ]]; then
    die "--start-at selector did not match an implementation plan: $START_AT"
  fi
  if [[ -n "$START_AFTER" && "$start_found" == "0" ]]; then
    die "--start-after selector did not match an implementation plan: $START_AFTER"
  fi

  local extra
  if ((${#EXTRA_CONTEXT_FILES[@]} > 0)); then
    for extra in "${EXTRA_CONTEXT_FILES[@]}"; do
      CONTEXT_FILES+=("$extra")
    done
  fi
}

validate_mise_check_commands() {
  local command_text

  for command_text in "${PER_PLAN_CHECKS[@]}"; do
    [[ "$command_text" == mise\ run\ * ]] || die "per-plan checks must use mise run: $command_text"
  done
  for command_text in "${FINAL_CHECKS[@]}"; do
    [[ "$command_text" == mise\ run\ * ]] || die "final checks must use mise run: $command_text"
  done
}

run_codex_prompt() {
  local prompt_file=$1
  local jsonl_file=$2
  shift 2

  info "codex: $(quote_cmd "$@")"
  "$@" <"$prompt_file" >"$jsonl_file"
}

run_claude_prompt() {
  local prompt_file=$1
  local raw_file=$2
  shift 2

  info "claude: $(quote_cmd "$@")"
  "$@" <"$prompt_file" >"$raw_file"
}

run_implementer() {
  local plan_file=$1
  local plan_run_dir=$2
  local loop_index=$3
  local model=$4
  local effort=$5
  local feedback_file=${6:-}
  local prompt_file="$plan_run_dir/implementer-loop-$loop_index.prompt.md"
  local final_file="$plan_run_dir/implementer-loop-$loop_index.final.md"
  local jsonl_file="$plan_run_dir/implementer-loop-$loop_index.jsonl"

  write_implementer_prompt "$prompt_file" "$plan_file" "$loop_index" "$feedback_file"
  build_codex_args "$model" "$effort" "danger-full-access" "$final_file"
  run_codex_prompt "$prompt_file" "$jsonl_file" codex "${CODEX_ARGS[@]}"
}

run_reviewer() {
  local plan_file=$1
  local plan_run_dir=$2
  local loop_index=$3
  local prompt_file="$plan_run_dir/reviewer-loop-$loop_index.prompt.md"
  local final_file="$plan_run_dir/reviewer-loop-$loop_index.json"
  local raw_file="$plan_run_dir/reviewer-loop-$loop_index.raw"

  write_reviewer_prompt "$prompt_file" "$plan_file"
  if [[ "$REVIEWER_ENGINE" == "claude" ]]; then
    run_claude_prompt \
      "$prompt_file" \
      "$raw_file" \
      claude \
      --print \
      --model "$REVIEWER_MODEL" \
      --effort "$REVIEWER_EFFORT" \
      --permission-mode dontAsk \
      --tools "Bash,Read,Grep,Glob" \
      --output-format text
    extract_json_object "$raw_file" "$final_file"
  else
    local jsonl_file="$plan_run_dir/reviewer-loop-$loop_index.jsonl"
    build_codex_args "$REVIEWER_MODEL" "$REVIEWER_EFFORT" "read-only" "$final_file" "$REVIEW_SCHEMA"
    run_codex_prompt "$prompt_file" "$jsonl_file" codex "${CODEX_ARGS[@]}"
  fi
  validate_review_json "$final_file"
}

run_checks_for_plan() {
  local plan_run_dir=$1
  local loop_index=$2
  local feedback_file=$3
  local failed=0
  local command_text log_file check_index

  : >"$feedback_file"

  for check_index in "${!PER_PLAN_CHECKS[@]}"; do
    command_text=${PER_PLAN_CHECKS[$check_index]}
    log_file="$plan_run_dir/check-loop-$loop_index-$check_index.log"
    if ! run_command_logged "per-plan check" "$command_text" "$log_file"; then
      failed=1
      {
        printf 'Per-plan check failed: %s\n' "$command_text"
        printf 'Log: %s\n\n' "$log_file"
        printf '```text\n'
        tail -n 240 "$log_file"
        printf '\n```\n\n'
      } >>"$feedback_file"
    fi
  done

  return "$failed"
}

commit_current_slice() {
  local review_file=$1
  local plan_run_dir=$2
  local message_file="$plan_run_dir/commit-message.txt"

  git add -A
  write_commit_message "$review_file" "$message_file" || die "generated commit message failed validation: $message_file"
  git commit -F "$message_file"
}

PLAN_DIR=""
INCLUDE_REGEX='^[0-9]{2}[a-z]?[-_].*\.md$'
EXCLUDE_REGEX='^(00.*|README)\.md$'
IMPLEMENTER_MODEL="gpt-5.4-mini"
IMPLEMENTER_EFFORT="medium"
REVIEWER_ENGINE="claude"
REVIEWER_MODEL="claude-opus-4-7"
REVIEWER_MODEL_SET=0
REVIEWER_EFFORT="high"
ESCALATION_MODEL="gpt-5.5"
ESCALATION_EFFORT="high"
MAX_REVIEW_LOOPS=2
FAST_MODE=0
PERSIST_CODEX_SESSIONS=0
DRY_RUN=0
START_AT=""
START_AFTER=""

EXTRA_CONTEXT_FILES=()
CONTEXT_FILES=()
NOTES=()
PER_PLAN_CHECKS=()
FINAL_CHECKS=()
SKIP_REGEXES=()

while (($# > 0)); do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --context)
      (($# >= 2)) || die "--context requires a file"
      EXTRA_CONTEXT_FILES+=("$2")
      shift 2
      ;;
    --note)
      (($# >= 2)) || die "--note requires text"
      NOTES+=("$2")
      shift 2
      ;;
    --include)
      (($# >= 2)) || die "--include requires a regex"
      INCLUDE_REGEX=$2
      shift 2
      ;;
    --exclude)
      (($# >= 2)) || die "--exclude requires a regex"
      EXCLUDE_REGEX=$2
      shift 2
      ;;
    --skip)
      (($# >= 2)) || die "--skip requires a regex"
      SKIP_REGEXES+=("$2")
      shift 2
      ;;
    --start-at)
      (($# >= 2)) || die "--start-at requires a selector"
      START_AT=$2
      shift 2
      ;;
    --start-after)
      (($# >= 2)) || die "--start-after requires a selector"
      START_AFTER=$2
      shift 2
      ;;
    --per-plan-check)
      (($# >= 2)) || die "--per-plan-check requires a command"
      PER_PLAN_CHECKS+=("$2")
      shift 2
      ;;
    --final-check)
      (($# >= 2)) || die "--final-check requires a command"
      FINAL_CHECKS+=("$2")
      shift 2
      ;;
    --implementer-model)
      (($# >= 2)) || die "--implementer-model requires a model"
      IMPLEMENTER_MODEL=$2
      shift 2
      ;;
    --implementer-effort)
      (($# >= 2)) || die "--implementer-effort requires an effort"
      IMPLEMENTER_EFFORT=$2
      shift 2
      ;;
    --reviewer-engine)
      (($# >= 2)) || die "--reviewer-engine requires an engine"
      [[ "$2" == "claude" || "$2" == "codex" ]] || die "--reviewer-engine must be claude or codex"
      REVIEWER_ENGINE=$2
      shift 2
      ;;
    --reviewer-model)
      (($# >= 2)) || die "--reviewer-model requires a model"
      REVIEWER_MODEL=$2
      REVIEWER_MODEL_SET=1
      shift 2
      ;;
    --reviewer-effort)
      (($# >= 2)) || die "--reviewer-effort requires an effort"
      REVIEWER_EFFORT=$2
      shift 2
      ;;
    --escalation-model)
      (($# >= 2)) || die "--escalation-model requires a model"
      ESCALATION_MODEL=$2
      shift 2
      ;;
    --escalation-effort)
      (($# >= 2)) || die "--escalation-effort requires an effort"
      ESCALATION_EFFORT=$2
      shift 2
      ;;
    --max-review-loops)
      (($# >= 2)) || die "--max-review-loops requires a number"
      [[ "$2" =~ ^[1-9][0-9]*$ ]] || die "--max-review-loops must be a positive integer"
      MAX_REVIEW_LOOPS=$2
      shift 2
      ;;
    --fast)
      FAST_MODE=1
      shift
      ;;
    --persist-codex-sessions)
      PERSIST_CODEX_SESSIONS=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --)
      shift
      break
      ;;
    -*)
      die "unknown option: $1"
      ;;
    *)
      [[ -z "$PLAN_DIR" ]] || die "only one PLAN_DIR is supported"
      PLAN_DIR=$1
      shift
      ;;
  esac
done

[[ -n "$PLAN_DIR" ]] || die "PLAN_DIR is required"
if [[ -n "$START_AT" && -n "$START_AFTER" ]]; then
  die "--start-at and --start-after are mutually exclusive"
fi
if [[ "$REVIEWER_ENGINE" == "codex" && "$REVIEWER_MODEL_SET" == "0" ]]; then
  REVIEWER_MODEL="gpt-5.5"
fi

require_command git
require_command codex
require_command mise
require_command jq
require_command rg
require_command python3
if [[ "$REVIEWER_ENGINE" == "claude" ]]; then
  require_command claude
fi

REPO_ROOT=$(repo_root)
[[ -n "$REPO_ROOT" ]] || die "not inside a git repository"
cd "$REPO_ROOT"

[[ -f "$REPO_ROOT/AGENTS.md" ]] || die "AGENTS.md not found at repo root"
[[ -f "$REPO_ROOT/architecture/wyrd-design.md" ]] || die "architecture/wyrd-design.md not found"
[[ -d "$PLAN_DIR" ]] || die "plan directory not found: $PLAN_DIR"
PLAN_DIR=$(python3 - "$PLAN_DIR" <<'PY'
import os
import sys

print(os.path.abspath(sys.argv[1]))
PY
)

if ((${#EXTRA_CONTEXT_FILES[@]} > 0)); then
  for context_file in "${EXTRA_CONTEXT_FILES[@]}"; do
    [[ -f "$context_file" ]] || die "context file not found: $context_file"
  done
fi

if ((${#PER_PLAN_CHECKS[@]} == 0)); then
  PER_PLAN_CHECKS=("mise run fmt")
fi

if ((${#FINAL_CHECKS[@]} == 0)); then
  FINAL_CHECKS=("mise run pre-pr")
fi

collect_plan_files
validate_mise_check_commands
((${#PLAN_FILES[@]} > 0)) || die "no implementation plan files matched in $PLAN_DIR"

if [[ "$DRY_RUN" == "1" ]]; then
  printf 'Plan directory: %s\n' "$(relpath "$PLAN_DIR")"
  printf 'Implementer: %s (%s)\n' "$IMPLEMENTER_MODEL" "$IMPLEMENTER_EFFORT"
  printf 'Reviewer: %s:%s (%s)\n' "$REVIEWER_ENGINE" "$REVIEWER_MODEL" "$REVIEWER_EFFORT"
  printf 'Escalation: %s (%s)\n' "$ESCALATION_MODEL" "$ESCALATION_EFFORT"
  printf 'Fast mode: %s\n' "$FAST_MODE"
  if [[ -n "$START_AT" ]]; then
    printf 'Start at: %s\n' "$START_AT"
  fi
  if [[ -n "$START_AFTER" ]]; then
    printf 'Start after: %s\n' "$START_AFTER"
  fi
  if ((${#SKIP_REGEXES[@]} > 0)); then
    printf 'Skip regexes:\n'
    for skip_regex in "${SKIP_REGEXES[@]}"; do
      printf '  - %s\n' "$skip_regex"
    done
  fi
  printf '\nContext files:\n'
  if ((${#CONTEXT_FILES[@]} > 0)); then
    for file in "${CONTEXT_FILES[@]}"; do
      printf '  - %s\n' "$(relpath "$file")"
    done
  fi
  printf '\nImplementation order:\n'
  for file in "${PLAN_FILES[@]}"; do
    printf '  - %s\n' "$(relpath "$file")"
  done
  printf '\nPer-plan checks:\n'
  for command_text in "${PER_PLAN_CHECKS[@]}"; do
    printf '  - %s\n' "$command_text"
  done
  printf '\nFinal checks:\n'
  for command_text in "${FINAL_CHECKS[@]}"; do
    printf '  - %s\n' "$command_text"
  done
  exit 0
fi

is_clean_worktree || die "worktree must be clean before starting"

RUN_ROOT="$REPO_ROOT/.git/wyrd-plan-runner/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$RUN_ROOT"
REVIEW_SCHEMA="$RUN_ROOT/review-schema.json"
write_review_schema "$REVIEW_SCHEMA"

info "run logs: $RUN_ROOT"

plan_file=""
for plan_file in "${PLAN_FILES[@]}"; do
  plan_slug=$(sanitize_name "$plan_file")
  plan_run_dir="$RUN_ROOT/$plan_slug"
  mkdir -p "$plan_run_dir"

  info "starting plan slice: $(relpath "$plan_file")"

  feedback_file=""
  approved_review_file=""
  loop=1
  while ((loop <= MAX_REVIEW_LOOPS)); do
    if ((loop == 1)); then
      model=$IMPLEMENTER_MODEL
      effort=$IMPLEMENTER_EFFORT
    else
      model=$ESCALATION_MODEL
      effort=$ESCALATION_EFFORT
    fi

    run_implementer "$plan_file" "$plan_run_dir" "$loop" "$model" "$effort" "$feedback_file"

    check_feedback="$plan_run_dir/check-feedback-loop-$loop.txt"
    if ! run_checks_for_plan "$plan_run_dir" "$loop" "$check_feedback"; then
      if ((loop >= MAX_REVIEW_LOOPS)); then
        die "per-plan checks failed after $loop loop(s); see $check_feedback"
      fi
      feedback_file=$check_feedback
      ((loop++))
      continue
    fi

    review_file="$plan_run_dir/reviewer-loop-$loop.json"
    if ! run_reviewer "$plan_file" "$plan_run_dir" "$loop"; then
      die "reviewer did not produce valid JSON; see $review_file"
    fi

    if review_approved "$review_file"; then
      approved_review_file=$review_file
      break
    fi

    review_feedback="$plan_run_dir/review-feedback-loop-$loop.txt"
    write_review_feedback "$review_file" "$review_feedback"
    if ((loop >= MAX_REVIEW_LOOPS)); then
      die "review still requires changes after $loop loop(s); see $review_feedback"
    fi
    feedback_file=$review_feedback
    ((loop++))
  done

  [[ -n "$approved_review_file" ]] || die "plan slice did not reach approval"
  commit_current_slice "$approved_review_file" "$plan_run_dir"
  is_clean_worktree || die "worktree is not clean after commit"
  info "committed plan slice: $(relpath "$plan_file")"
done

for check_index in "${!FINAL_CHECKS[@]}"; do
  command_text=${FINAL_CHECKS[$check_index]}
  log_file="$RUN_ROOT/final-check-$check_index.log"
  run_command_logged "final check" "$command_text" "$log_file" || die "final check failed: $command_text; see $log_file"
done

info "all plan slices completed"
