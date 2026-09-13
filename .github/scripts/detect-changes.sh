#!/usr/bin/env bash
set -euo pipefail

base="${1:-${WYRD_CHANGE_BASE:-}}"
head="${2:-${WYRD_CHANGE_HEAD:-}}"
output_file="${GITHUB_OUTPUT:-/dev/stdout}"
changed_files="${RUNNER_TEMP:-/tmp}/wyrd-changed-files"

if [[ -z "$base" || -z "$head" ]]; then
  echo "base and head SHAs are required" >&2
  echo "usage: $0 <base-sha> <head-sha>" >&2
  echo "or set WYRD_CHANGE_BASE and WYRD_CHANGE_HEAD" >&2
  exit 2
fi

git diff --name-only "$base" "$head" > "$changed_files"

set_output() {
  local name="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$changed_files"; then
    echo "$name=true" >> "$output_file"
  else
    echo "$name=false" >> "$output_file"
  fi
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

set_output rust '^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|deny\.toml|mise\.toml|crates/|python/py-wyrd/Cargo\.toml|python/py-wyrd/src/)'
set_output python '^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/shared/wyrd-utils/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|python/py-wyrd/|examples/python/)'
set_output docs '^(mise\.toml|docs/|openapi\.yaml|crates/wyrd-spec/schemas/|examples/)'
# ui also covers the cross-package generated token targets so the check:tokens
# drift lock fires on a hand-edit to any of them, not just on brand/ source edits.
set_output ui '^(mise\.toml|crates/wyrd/wyrd-server/wyrd-ui/|docs/src/styles/wyrd-tokens\.css|\.claude/skills/wyrd-ui/references/wyrd-theme\.css|\.codex/skills/wyrd-ui/references/wyrd-theme\.css)'
set_output generated '^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/wyrd-spec/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|python/py-wyrd/)'
# storage/identity gate their own heavy emulator + OIDC e2e workflows. Both boot
# the server, so a wyrd-server change re-runs both; docs/UI/python-only PRs skip.
set_output storage '^(mise\.toml|Cargo\.lock|crates/wyrd/wyrd-storage/|crates/wyrd/wyrd-server/|crates/wyrd/wyrd-sql/|crates/wyrd/wyrd-client/src/artifacts/|crates/wyrd-spec/src/storage/|\.github/workflows/storage-integration|\.github/scripts/detect-changes\.sh)'
set_output identity '^(mise\.toml|Cargo\.lock|docker-compose\.yml|tests/fixtures/identity/|crates/shared/wyrd-auth|crates/shared/wyrd-client/|crates/wyrd/wyrd-auth/|crates/wyrd/wyrd-server/|crates/wyrd/wyrd-testing/|crates/wyrd-spec/src/security|\.github/workflows/identity-e2e\.yml|\.github/scripts/detect-changes\.sh)'
set_output workflow '^(\.github/workflows/|\.github/scripts/)'
# A Bifrost-only change can use the complete capability gate. Mixed, global,
# and unknown changes deliberately fall back to the repository gate.
set_output_all bifrost_only '^(architecture/bifrost-design\.md|architecture/references/domain/(olap-serving|iceberg|datafusion|arrow-analytical-interop|analytical-operations-reliability)\.md|crates/vala/vala-bifrost-redux/|crates/shared/wyrd-client/(src/bifrost/|tests/pg_bifrost_e2e\.rs)|crates/vala/vala-sql/(src/(queries|row_types)/(forge|oracle|file_list|maintenance|scribe)|tests/(oracle_admission|pg_(file_list|forge|maintenance|olap|oracle|stream)))|crates/wyrd/wyrd-testing/(src/bifrost/|tests/bifrost/)|crates/wyrd/wyrd-server/src/(bifrost/|oracle/|query/|grpc/(query|scribe_tail)\.rs)|crates/wyrd/wyrd-server/tests/(pg_eval_v1_protocol|pg_grpc_ingest_smoke|pg_grpc_smoke|pg_merge_http_protected|pg_router_smoke)\.rs|crates/wyrd/wyrd-mcp/(src/bifrost/|tests/bifrost/)|crates/wyrd-spec/src/vala/(api|assignment_authority|error|ids|managed_columns)\.rs|python/py-wyrd/(python/wyrd/bifrost/|tests/bifrost/|tests/test_bifrost\.py|tests/integration/test_bifrost_(e2e|query)\.py)|typescript/wyrd/(tests/unit/bifrost-query\.test\.ts|tests/integration/oracle-query\.test\.ts))'

if [[ -s "$changed_files" ]]; then
  echo "any=true" >> "$output_file"
else
  echo "any=false" >> "$output_file"
fi

echo "Changed files:"
sed 's/^/- /' "$changed_files"
