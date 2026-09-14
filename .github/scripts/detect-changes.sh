#!/usr/bin/env bash
set -euo pipefail

base="${1:-${WYRD_CHANGE_BASE:-}}"
head="${2:-${WYRD_CHANGE_HEAD:-}}"
output_file="${GITHUB_OUTPUT:-/dev/stdout}"

# WYRD_CHANGED_FILES names a prepared file list so the self-tests can classify
# fixed scenarios without constructing commits.
if [[ -n "${WYRD_CHANGED_FILES:-}" ]]; then
  changed_files="$WYRD_CHANGED_FILES"
else
  changed_files="${RUNNER_TEMP:-/tmp}/wyrd-changed-files"
  if [[ -z "$base" || -z "$head" ]]; then
    echo "base and head SHAs are required" >&2
    echo "usage: $0 <base-sha> <head-sha>" >&2
    echo "or set WYRD_CHANGE_BASE and WYRD_CHANGE_HEAD" >&2
    exit 2
  fi
  git diff --name-only "$base" "$head" > "$changed_files"
fi

set_output() {
  local name="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$changed_files"; then
    echo "$name=true" >> "$output_file"
  else
    echo "$name=false" >> "$output_file"
  fi
}

matches_any() {
  grep -Eq "$1" "$changed_files"
}

set_output_all() {
  local name="$1"
  local pattern="$2"

  if [[ -s "$changed_files" ]] && ! grep -Evq "$pattern" "$changed_files"; then
    echo "$name=true" >> "$output_file"
  else
    echo "$name=false" >> "$output_file"
  fi
}

# ui also covers the cross-package generated token targets so the check:tokens
# drift lock fires on a hand-edit to any of them, not just on brand/ source edits.
# storage/identity gate their own heavy emulator + OIDC e2e workflows. Both boot
# the server, so a wyrd-server change re-runs both; docs/UI/python-only PRs skip.
# A Bifrost-only change can use the complete capability gate. Mixed, global,
# and unknown changes deliberately fall back to the repository gate.

rust_pattern='^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|deny\.toml|mise\.toml|crates/|sdks/wyrd-sdk-rust/|sdks/wyrd-sdk-ts/native/|sdks/wyrd-sdk-ts/native-testing/|sdks/wyrd-sdk-python/Cargo\.toml|sdks/wyrd-sdk-python/src/)'
python_pattern='^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/shared/wyrd-utils/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|sdks/wyrd-sdk-python/|examples/python/)'
docs_pattern='^(mise\.toml|docs/|openapi\.yaml|crates/wyrd-spec/schemas/|examples/)'
ui_pattern='^(mise\.toml|crates/wyrd/wyrd-server/wyrd-ui/|docs/src/styles/wyrd-tokens\.css|\.claude/skills/wyrd-ui/references/wyrd-theme\.css|\.agents/skills/wyrd-ui/references/wyrd-theme\.css)'
generated_pattern='^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/wyrd-spec/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|sdks/wyrd-sdk-python/)'
storage_pattern='^(mise\.toml|Cargo\.lock|crates/wyrd/wyrd-storage/|crates/wyrd/wyrd-server/|crates/wyrd/wyrd-sql/|crates/shared/wyrd-client/src/storage/|crates/wyrd-spec/src/storage/|\.github/workflows/storage-integration|\.github/scripts/detect-changes\.sh)'
identity_pattern='^(mise\.toml|Cargo\.lock|docker-compose\.yml|tests/fixtures/identity/|crates/shared/wyrd-auth|crates/shared/wyrd-client/|crates/wyrd/wyrd-auth/|crates/wyrd/wyrd-server/|crates/wyrd/wyrd-testing/|crates/wyrd-spec/src/security|\.github/workflows/identity-e2e\.yml|\.github/scripts/detect-changes\.sh)'
workflow_pattern='^(\.github/workflows/|\.github/scripts/)'
bifrost_only_pattern='^(architecture/bifrost-design\.md|architecture/references/domain/(olap-serving|iceberg|datafusion|arrow-analytical-interop|analytical-operations-reliability)\.md|crates/vala/vala-bifrost-redux/|crates/shared/wyrd-client/(src/bifrost/|tests/pg_bifrost_e2e\.rs)|crates/vala/vala-sql/(src/(queries|row_types)/(forge|oracle|file_list|maintenance|scribe)|tests/(oracle_admission|pg_(file_list|forge|maintenance|olap|oracle|stream)))|crates/wyrd/wyrd-testing/(src/bifrost/|tests/bifrost/)|crates/wyrd/wyrd-server/src/(bifrost/|oracle/|query/|grpc/(query|scribe_tail)\.rs)|crates/wyrd/wyrd-server/tests/(pg_eval_v1_protocol|pg_grpc_ingest_smoke|pg_grpc_smoke|pg_merge_http_protected|pg_router_smoke)\.rs|crates/wyrd/wyrd-mcp/(src/bifrost/|tests/bifrost/)|crates/wyrd-spec/src/vala/(api|assignment_authority|error|ids|managed_columns)\.rs|sdks/wyrd-sdk-python/(python/wyrd/bifrost/|tests/bifrost/|tests/test_bifrost\.py|tests/integration/test_bifrost_(e2e|query)\.py)|sdks/wyrd-sdk-ts/wyrd/(tests/unit/bifrost-query\.test\.ts|tests/integration/oracle-query\.test\.ts))'

set_output rust "${rust_pattern}"
set_output python "${python_pattern}"
set_output docs "${docs_pattern}"
set_output ui "${ui_pattern}"
set_output generated "${generated_pattern}"
set_output storage "${storage_pattern}"
set_output identity "${identity_pattern}"
set_output workflow "${workflow_pattern}"
set_output_all bifrost_only "$bifrost_only_pattern"

# Global paths configure every lane at once; they take the complete gate.
global_pattern='^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|deny\.toml|mise\.toml|\.github/)'
# Paths that select no verification of their own.
unverified_pattern='^(changes/|architecture/|[^/]+\.md$)'
known_pattern="$rust_pattern|$python_pattern|$docs_pattern|$ui_pattern|$generated_pattern|$storage_pattern|$identity_pattern|$workflow_pattern|$bifrost_only_pattern|$global_pattern|$unverified_pattern"

set_output global "$global_pattern"
if [[ -s "$changed_files" ]] && grep -Evq "$known_pattern" "$changed_files"; then
  unclassified=true
else
  unclassified=false
fi
echo "unclassified=$unclassified" >> "$output_file"

# Mixed means Bifrost and non-Bifrost Rust in one change. Mixed, global, and
# unclassified changes take the complete gate; every other change runs only the
# lanes its classifiers select.
bifrost_files="$(grep -Ec "$bifrost_only_pattern" "$changed_files" || true)"
other_rust_files="$(grep -Ev "$bifrost_only_pattern" "$changed_files" | grep -Ec "$rust_pattern" || true)"
if (( bifrost_files > 0 && other_rust_files > 0 )); then
  mixed=true
else
  mixed=false
fi
if matches_any "$global_pattern" || [[ "$unclassified" == true || "$mixed" == true ]]; then
  echo "full_gate=true" >> "$output_file"
else
  echo "full_gate=false" >> "$output_file"
fi

if [[ -s "$changed_files" ]]; then
  echo "any=true" >> "$output_file"
else
  echo "any=false" >> "$output_file"
fi

echo "Changed files:"
sed 's/^/- /' "$changed_files"
