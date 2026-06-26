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
  --validator-model MODEL        Finding validator and fix-plan model.
                                 Default: claude-sonnet-4-6
  --validator-effort EFFORT      Finding validator effort.
                                 Default: high
  --escalation-model MODEL       Retry implementer model after failures.
                                 Default: gpt-5.5
  --escalation-effort EFFORT     Retry implementer effort after failures.
                                 Default: high
  --max-review-loops N           Maximum implementation/review loops per plan.
                                 Default: 2
  --fast                         Request Codex fast service tier.
  --persist-codex-sessions       Do not pass --ephemeral to codex exec.
  --resume-current               Treat the current dirty worktree as the first
                                 selected plan slice and start at checks/review.
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
    },
    "review_dir": {
      "type": "string"
    },
    "implementation_plan_path": {
      "type": "string"
    }
  },
  "required": [
    "status",
    "summary",
    "findings",
    "recommended_checks",
    "commit_title",
    "commit_body",
    "review_dir",
    "implementation_plan_path"
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

create_review_dir() {
  local short
  short=$(git rev-parse --short HEAD)
  mkdir -p "$REPO_ROOT/.dev/review"
  mktemp -d "$REPO_ROOT/.dev/review/${short}-quick-XXXXXXXX"
}

prepare_review_packet() {
  local review_dir=$1
  local plan_file=$2
  local loop_index=$3

  {
    printf 'Review ID: %s\n' "$(basename "$review_dir")"
    printf 'Branch: %s\n' "$(git branch --show-current)"
    printf 'Plan slice: %s\n' "$(relpath "$plan_file")"
    printf 'Loop: %s\n' "$loop_index"
    printf 'Date: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$review_dir/setup.log"

  git status --short >"$review_dir/status.txt"
  git diff --stat HEAD >"$review_dir/stat.txt"
  git diff HEAD >"$review_dir/diff.patch"
  git ls-files --others --exclude-standard >"$review_dir/untracked.txt"
}

implementation_plan_path() {
  local review_dir=$1
  printf '%s/implementation-plan.md' "$review_dir"
}

implementation_plan_status() {
  local plan_file=$1

  if rg -n '^Status:[[:space:]]*clean[[:space:]]*$' "$plan_file" >/dev/null; then
    printf 'clean\n'
    return 0
  fi
  if rg -n '^Status:[[:space:]]*required_changes[[:space:]]*$' "$plan_file" >/dev/null; then
    printf 'required_changes\n'
    return 0
  fi
  return 1
}

write_shared_context() {
  local out=$1
  local preamble=${2:-}

  {
    if [[ -n "$preamble" ]]; then
      printf '%s\n\n' "$preamble"
    fi
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
      printf 'This is implementation loop %s. Address the validated implementation plan below and preserve all already-correct work.\n' "$loop_index"
      printf 'Treat this plan as the source of truth for immediate required changes. Do not implement optional polish or later plan slices.\n\n'
      printf '```text\n'
      cat "$feedback_file"
      printf '\n```\n'
    } >>"$out"
  fi
}

write_reviewer_prompt() {
  local out=$1
  local plan_file=$2
  local review_dir=$3
  local review_dir_rel
  local implementation_plan_rel

  review_dir_rel=$(relpath "$review_dir")
  implementation_plan_rel=$(relpath "$(implementation_plan_path "$review_dir")")

  write_shared_context "$out" "/review-and-plan-quick"
  write_slice_manifest "$out" "$plan_file"

  cat <<'TEXT' >>"$out"

## Role

You are the parent plan-conformance reviewer for this Wyrd implementation slice.
Use the condensed review-and-plan-quick contract: one reviewer, full review breadth, blocker-focused severity.

Do not run the full review-and-plan pipeline. Do not launch subagents. Do not edit source files.

Review the current logical slice only. Validate the uncommitted working tree against the current plan slice, Wyrd repo conventions, and the runner-provided manifest.
TEXT

  {
    printf '\n## Review Artifacts\n\n'
    printf 'Use this existing review directory: `%s`\n\n' "$review_dir_rel"
    printf 'The runner has already written the review packet:\n\n'
    printf -- '- `%s/setup.log`\n' "$review_dir_rel"
    printf -- '- `%s/status.txt`\n' "$review_dir_rel"
    printf -- '- `%s/stat.txt`\n' "$review_dir_rel"
    printf -- '- `%s/diff.patch`\n' "$review_dir_rel"
    printf -- '- `%s/untracked.txt`\n\n' "$review_dir_rel"
    printf 'Write these artifacts before returning JSON:\n\n'
    printf -- '- `%s/findings.md`\n' "$review_dir_rel"
    printf -- '- `%s`\n\n' "$implementation_plan_rel"
    printf '`implementation-plan.md` is the actionable output type for this review and the handoff to downstream agents.\n'
    printf 'It must contain the exact line `Status: clean` or `Status: required_changes` and enough detail for Codex to implement fixes without guessing.\n'
  } >>"$out"

  cat <<'TEXT' >>"$out"

## Calibration Sources

Apply the same lenses and finding bar as the installed `review-and-plan-quick` skill. If you need to calibrate a borderline issue, read the copy for the active review engine; otherwise use this prompt directly.

Skill roots:

- Claude: `~/.claude/skills/review-and-plan-quick/SKILL.md`
- Codex: `~/.codex/skills/review-and-plan-quick/SKILL.md`

Use the full `review-and-plan` prompt files in the same skill root only as calibration, not as a workflow to execute:

- `review-and-plan/review-security/review-security.md`
- `review-and-plan/review-bugs/review-bugs.md`
- `review-and-plan/review-tests/review-tests.md`
- `review-and-plan/review-style/review-style.md`
- `review-and-plan/review-clean-code/review-clean-code.md`
- `review-and-plan/review-developer-experience/review-developer-experience.md`
- `review-and-plan/review-frontend/review-frontend.md` when frontend files changed
- `review-and-plan/review-wyrd-ui-contracts/review-wyrd-ui-contracts.md` when UI/API/SDK/schema/docs surfaces changed

When Wyrd doctrine, contracts, APIs, SDKs, CLI, MCP, UI, docs, generated schemas, durable behavior, or public vocabulary are touched, apply `.codex/skills/review/SKILL.md` and read `docs/src/content/docs/concepts/core-doctrine.mdx` if needed.

## Review Lenses

Apply every relevant lens, but report only source-validated findings with a concrete failure, exploit, misuse, or maintenance path:

- Plan alignment and developer/agent experience.
- Security, compliance, tenant isolation, secrets, authn/authz, and audit.
- Bugs, correctness, data loss, async/concurrency, transactions, resource handling, and performance.
- Tests and verification that would catch realistic regressions.
- Maintainability, ergonomics, idioms, local conventions, clean code, SOLID, docs, comments, and simple local design.
- Frontend and Wyrd UI contracts when the slice changes Svelte, UI routes, request chains, schemas, SDKs, CLI, MCP, Python, docs, or API surfaces.
- Wyrd doctrine, Wyrd-native vocabulary, server-owned durable behavior, language-agnostic contracts, and agent-first/headless surfaces.

## High-Risk Calibration

Raise scrutiny for auth, tenant isolation, migrations, persistence, storage, secrets, audit, destructive operations, public contracts, generated schemas, HTTP/API, CLI, MCP, Python or Rust SDKs, UI request chains, async/concurrency, dependency manifests, and crate boundaries.

Always check dependency direction. Block foundational or owner crates depending on downstream support, fixture, test, UI, server, or integration crates that depend back on them. Dev-dependency cycles and type identity splits are real design issues, not test-only details.

## Blocking Bar

Block the slice only for:

- Plan contradiction or missing required stage work.
- Real bug, panic, data loss, security/compliance risk, tenant isolation risk, audit gap, or broken public contract.
- Developer or agent adoption failure on a changed public or semi-public surface.
- Missing test or verification that could let a realistic regression ship.
- Dependency direction drift, crate-boundary drift, or architecture drift that materially raises maintenance cost or makes the API awkward to use.

Do not block on broad branch-level concerns, speculative refactors, personal preferences, polish, unchanged existing debt, or findings better suited for the final full review.

Every finding must include a concrete source location, one plain-English issue sentence, a realistic failure path, evidence from source or local patterns, and the exact required fix or targeted verification gate. Consolidate duplicate findings under one root cause.

A clean review with zero findings is acceptable.

## Current Plan Slice
TEXT

  append_file_block "$out" "Plan Slice" "$plan_file"

  cat <<'TEXT' >>"$out"

## Diff To Review

Review the packet files and, when needed, the working tree produced by:

```bash
git status --short
git diff --stat HEAD
git diff HEAD
git ls-files --others --exclude-standard
```

Important: `git diff HEAD` omits untracked files. Use `git status --short` and `git ls-files --others --exclude-standard` to identify and inspect newly created files before approving.

Only run cheap targeted checks when they are required to confirm a finding. Recommended checks in the JSON must use `mise run ...` commands. If a surgical non-mise command is useful but no mise task exists, mention it in `summary` instead of `recommended_checks`.

## Implementation Plan Requirements

If blockers remain, `implementation-plan.md` must include numbered required changes. Each required change must include severity, source finding, files or symbols to inspect, the concrete problem, exact required implementation behavior, constraints, acceptance criteria, and verification commands.

If no blockers remain, `implementation-plan.md` must say `Status: clean`, list no required changes, and summarize relevant verification.

## JSON Output Contract

After writing review artifacts, return only raw JSON matching the schema shape below.

Use:

- `status: "changes_required"` only when a critical or important blocker remains.
- `status: "approved"` when there are no blockers. Minor findings may be included only as non-blocking notes.
- `severity: "critical"` for exploitable security, data loss, tenant isolation breakage, broken public contract, or direct plan contradiction.
- `severity: "important"` for any issue that should block this slice commit.
- `severity: "minor"` only for optional cleanup that should not block the commit.

Also provide a reviewer-readable commit title and body based on the actual code behavior and overall plan intent. The commit title and body must not mention file paths, plan files, plan names, commit hashes, or automation context.

Set `review_dir` and `implementation_plan_path` to the exact paths provided above.

```json
TEXT
  cat "$REVIEW_SCHEMA" >>"$out"
  cat <<'TEXT' >>"$out"

```
TEXT
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
    and (.review_dir | type == "string")
    and (.implementation_plan_path | type == "string")
  ' "$review_file" >/dev/null
}

review_approved() {
  local review_file=$1
  jq -e '
    .status == "approved"
    and ([.findings[] | select(.severity == "critical" or .severity == "important")] | length == 0)
  ' "$review_file" >/dev/null
}

materialize_review_artifacts_from_json() {
  local review_file=$1
  local review_dir=$2
  local plan_path=$3
  local status

  cp "$review_file" "$review_dir/review-result.json"

  if [[ ! -f "$review_dir/findings.md" ]]; then
    jq -r '
      "# Quick Review Findings\n",
      "Review ID: " + ($ARGS.named.review_id) + "\n",
      "Status: " + (if .status == "approved" then "clean" else "required_changes" end) + "\n",
      "## Findings\n",
      (if (.findings | length) == 0 then
        "No blocking findings.\n"
      else
        (.findings[] |
          "- [" + .severity + "] " + .path + ":" + ((.line // "n/a") | tostring) + " - " + .issue + "\n" +
          "  Required change: " + .required_change + "\n")
      end),
      "\n## Verification Gaps\n",
      (if (.recommended_checks | length) == 0 then
        "None.\n"
      else
        (.recommended_checks[] | "- " + .)
      end)
    ' --arg review_id "$(basename "$review_dir")" "$review_file" >"$review_dir/findings.md"
  fi

  if [[ ! -f "$plan_path" ]]; then
    if review_approved "$review_file"; then
      status=clean
    else
      status=required_changes
    fi

    {
      printf '# Immediate Implementation Plan\n\n'
      printf 'Status: %s\n' "$status"
      printf 'Review ID: %s\n\n' "$(basename "$review_dir")"
      printf '## Summary\n\n'
      jq -r '.summary' "$review_file"
      printf '\n## Required Changes\n\n'
      if [[ "$status" == "clean" ]]; then
        printf 'None.\n'
      else
        jq -r '
          [.findings[] | select(.severity == "critical" or .severity == "important")]
          | to_entries[]
          | "### \((.key + 1)). Address validated review finding\n\n" +
            "Severity: \(.value.severity)\n" +
            "Source finding: \(.value.path):\((.value.line // "n/a") | tostring)\n" +
            "Files or symbols to inspect:\n" +
            "- \(.value.path)\n\n" +
            "Problem:\n\(.value.issue)\n\n" +
            "Required implementation:\n- \(.value.required_change)\n- Preserve local Wyrd ownership boundaries and existing nearby patterns.\n- Do not implement optional polish or later plan slices.\n\n" +
            "Acceptance criteria:\n- The source finding no longer reproduces under the reviewed behavior.\n- Relevant tests or checks cover the changed behavior.\n\n" +
            "Verification:\n" +
            (if ($checks | length) == 0 then "- mise run fmt" else ($checks | map("- " + .) | join("\n")) end) +
            "\n"
        ' --argjson checks "$(jq '.recommended_checks' "$review_file")" "$review_file"
      fi
      printf '\n## Verification\n\n'
      jq -r '
        if (.recommended_checks | length) == 0 then
          "- mise run fmt"
        else
          .recommended_checks[] | "- " + .
        end
      ' "$review_file"
    } >"$plan_path"
  fi
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

write_validator_prompt() {
  local out=$1
  local plan_file=$2
  local review_dir=$3
  local review_dir_rel
  local implementation_plan_rel

  review_dir_rel=$(relpath "$review_dir")
  implementation_plan_rel=$(relpath "$(implementation_plan_path "$review_dir")")

  write_shared_context "$out"
  write_slice_manifest "$out" "$plan_file"

  {
    printf '\n## Role\n\n'
    printf 'You are the final validation and immediate-fix planning pass for one Wyrd implementation slice.\n'
    printf 'Use Claude Sonnet judgment to validate the quick-review findings against source, eliminate false positives, merge duplicates, and rewrite the implementation plan.\n'
    printf 'Do not edit source files. Only write `%s/validation.md` and update `%s`.\n\n' "$review_dir_rel" "$implementation_plan_rel"
    printf '## Review Directory\n\n'
    printf 'Read every relevant artifact in `%s`, including:\n\n' "$review_dir_rel"
    printf -- '- `setup.log`\n'
    printf -- '- `status.txt`\n'
    printf -- '- `stat.txt`\n'
    printf -- '- `diff.patch`\n'
    printf -- '- `untracked.txt`\n'
    printf -- '- `findings.md`\n'
    printf -- '- `review-result.json`\n'
    printf -- '- `implementation-plan.md`\n\n'
    printf 'Read changed source files and nearest local patterns only as needed to validate or reject findings.\n'
    printf '\n## Current Plan Slice\n'
  } >>"$out"

  append_file_block "$out" "Plan Slice" "$plan_file"

  cat <<'TEXT' >>"$out"

## Validation Rules

Validate every blocking finding before it remains in the plan.

- Keep a finding only when source evidence proves a realistic failure path, exploit path, tenant-isolation risk, public-contract break, missing required plan work, missing regression-catching test, or material maintainability issue.
- Eliminate false positives, speculative polish, broad branch concerns, unchanged existing debt, and issues better suited for the final full review.
- Merge duplicates under one root cause.
- Preserve only immediate required changes for this slice.
- Make each remaining required change implementation-ready for Codex.

## Required Outputs

TEXT

  printf 'Write `%s/validation.md` with:\n\n' "$review_dir_rel" >>"$out"

  cat <<'TEXT' >>"$out"

```markdown
# Final Validation

Status: clean|required_changes

## Confirmed Findings

- ...

## Eliminated Findings

- Finding:
  Reason eliminated:

## Plan Changes

- ...
```

TEXT

  printf 'Rewrite `%s` as the final source of truth.\n' "$implementation_plan_rel" >>"$out"

  cat <<'TEXT' >>"$out"

It must contain exactly one of these top-level status lines:

```markdown
Status: clean
```

or:

```markdown
Status: required_changes
```

When status is `required_changes`, each numbered required change must include:

- severity
- source finding
- files or symbols to inspect
- concrete problem and failure path
- exact required implementation behavior
- constraints and local patterns to preserve
- acceptance criteria
- verification commands, preferring `mise run ...`

When status is `clean`, list no required changes and summarize relevant verification.

Final response should only state the review directory, validation path, implementation plan path, and final status.
TEXT
}

run_reviewer() {
  local plan_file=$1
  local plan_run_dir=$2
  local loop_index=$3
  local review_dir=$4
  local prompt_file="$plan_run_dir/reviewer-loop-$loop_index.prompt.md"
  local final_file="$plan_run_dir/reviewer-loop-$loop_index.json"
  local raw_file="$plan_run_dir/reviewer-loop-$loop_index.raw"
  local plan_path

  plan_path=$(implementation_plan_path "$review_dir")

  write_reviewer_prompt "$prompt_file" "$plan_file" "$review_dir"
  if [[ "$REVIEWER_ENGINE" == "claude" ]]; then
    run_claude_prompt \
      "$prompt_file" \
      "$raw_file" \
      claude \
      --print \
      --model "$REVIEWER_MODEL" \
      --effort "$REVIEWER_EFFORT" \
      --permission-mode bypassPermissions \
      --tools "Bash,Read,Grep,Glob,Write,Edit" \
      --output-format text
    extract_json_object "$raw_file" "$final_file"
  else
    local jsonl_file="$plan_run_dir/reviewer-loop-$loop_index.jsonl"
    build_codex_args "$REVIEWER_MODEL" "$REVIEWER_EFFORT" "workspace-write" "$final_file" "$REVIEW_SCHEMA"
    run_codex_prompt "$prompt_file" "$jsonl_file" codex "${CODEX_ARGS[@]}"
  fi
  validate_review_json "$final_file"
  materialize_review_artifacts_from_json "$final_file" "$review_dir" "$plan_path"
  [[ -f "$review_dir/findings.md" ]] || die "reviewer did not write findings.md in $(relpath "$review_dir")"
  [[ -f "$plan_path" ]] || die "reviewer did not write implementation-plan.md in $(relpath "$review_dir")"
  implementation_plan_status "$plan_path" >/dev/null || die "implementation-plan.md is missing Status: clean|required_changes"
}

run_validator() {
  local plan_file=$1
  local review_dir=$2
  local plan_run_dir=$3
  local loop_index=$4
  local prompt_file="$plan_run_dir/validator-loop-$loop_index.prompt.md"
  local raw_file="$plan_run_dir/validator-loop-$loop_index.raw"
  local plan_path

  plan_path=$(implementation_plan_path "$review_dir")
  write_validator_prompt "$prompt_file" "$plan_file" "$review_dir"
  run_claude_prompt \
    "$prompt_file" \
    "$raw_file" \
    claude \
    --print \
    --model "$VALIDATOR_MODEL" \
    --effort "$VALIDATOR_EFFORT" \
    --permission-mode bypassPermissions \
    --tools "Bash,Read,Grep,Glob,Write,Edit" \
    --output-format text

  [[ -f "$review_dir/validation.md" ]] || die "validator did not write validation.md in $(relpath "$review_dir")"
  [[ -f "$plan_path" ]] || die "validator removed implementation-plan.md in $(relpath "$review_dir")"
  implementation_plan_status "$plan_path" >/dev/null || die "validated implementation-plan.md is missing Status: clean|required_changes"
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
VALIDATOR_MODEL="claude-sonnet-4-6"
VALIDATOR_EFFORT="high"
ESCALATION_MODEL="gpt-5.5"
ESCALATION_EFFORT="high"
MAX_REVIEW_LOOPS=2
FAST_MODE=0
PERSIST_CODEX_SESSIONS=0
RESUME_CURRENT=0
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
    --validator-model)
      (($# >= 2)) || die "--validator-model requires a model"
      VALIDATOR_MODEL=$2
      shift 2
      ;;
    --validator-effort)
      (($# >= 2)) || die "--validator-effort requires an effort"
      VALIDATOR_EFFORT=$2
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
    --resume-current)
      RESUME_CURRENT=1
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
require_command claude

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
  printf 'Validator: claude:%s (%s)\n' "$VALIDATOR_MODEL" "$VALIDATOR_EFFORT"
  printf 'Escalation: %s (%s)\n' "$ESCALATION_MODEL" "$ESCALATION_EFFORT"
  printf 'Fast mode: %s\n' "$FAST_MODE"
  printf 'Resume current: %s\n' "$RESUME_CURRENT"
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

if [[ "$RESUME_CURRENT" == "1" ]]; then
  [[ -n "$(git status --porcelain)" ]] || die "--resume-current requires a dirty worktree"
else
  is_clean_worktree || die "worktree must be clean before starting"
fi

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
  resume_this_plan=0
  if [[ "$RESUME_CURRENT" == "1" ]]; then
    resume_this_plan=1
    RESUME_CURRENT=0
    info "resuming current worktree for plan slice: $(relpath "$plan_file")"
  fi

  while ((loop <= MAX_REVIEW_LOOPS)); do
    if ((loop == 1)); then
      model=$IMPLEMENTER_MODEL
      effort=$IMPLEMENTER_EFFORT
    else
      model=$ESCALATION_MODEL
      effort=$ESCALATION_EFFORT
    fi

    if [[ "$resume_this_plan" == "1" && "$loop" == "1" && -z "$feedback_file" ]]; then
      info "skipping implementation; using existing worktree for review"
    else
      run_implementer "$plan_file" "$plan_run_dir" "$loop" "$model" "$effort" "$feedback_file"
    fi

    check_feedback="$plan_run_dir/check-feedback-loop-$loop.txt"
    if ! run_checks_for_plan "$plan_run_dir" "$loop" "$check_feedback"; then
      if ((loop >= MAX_REVIEW_LOOPS)); then
        die "per-plan checks failed after $loop loop(s); see $check_feedback"
      fi
      feedback_file=$check_feedback
      ((loop++))
      continue
    fi

    review_dir=$(create_review_dir)
    prepare_review_packet "$review_dir" "$plan_file" "$loop"
    implementation_plan=$(implementation_plan_path "$review_dir")

    review_file="$plan_run_dir/reviewer-loop-$loop.json"
    if ! run_reviewer "$plan_file" "$plan_run_dir" "$loop" "$review_dir"; then
      die "reviewer did not produce valid JSON; see $review_file"
    fi

    implementation_status=$(implementation_plan_status "$implementation_plan")
    if [[ "$implementation_status" == "clean" ]] && review_approved "$review_file"; then
      approved_review_file=$review_file
      break
    fi

    run_validator "$plan_file" "$review_dir" "$plan_run_dir" "$loop"
    implementation_status=$(implementation_plan_status "$implementation_plan")
    if [[ "$implementation_status" == "clean" ]]; then
      approved_review_file=$review_file
      break
    fi

    if ((loop >= MAX_REVIEW_LOOPS)); then
      die "review still requires changes after $loop loop(s); see $implementation_plan"
    fi
    feedback_file=$implementation_plan
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
