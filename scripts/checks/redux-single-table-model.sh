#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "Redux single-table model checker requires rg; refusing to run without it" >&2
  exit 1
fi

active_paths=(
  crates/vala/vala-bifrost-redux/src
  crates/vala/vala-bifrost-redux/tests
  crates/vala/vala-sql/src
  crates/vala/vala-sql/migrations
  crates/wyrd-spec/src/vala
  crates/wyrd-spec/schemas
  crates/wyrd-spec/tests/schemas
  docs/src/content/docs/bifrost
  crates/wyrd/wyrd-testing/benches
)

forbidden=(
  'SystemShared'
  'TenantOwned'
  'TableScope'
  'TableScopeWire'
  'system_shared'
  'tenant_owned'
  'control_bind'
  'tenant_bucket'
  'SystemTableWriteDenied'
  'MissingTenantColumn'
  'UnexpectedTenantColumn'
  'wyrd-system-owner'
)

for token in "${forbidden[@]}"; do
  if match="$(rg -n -F "$token" "${active_paths[@]}" 2>/dev/null)"; then
    printf 'Redux single-table model violation: %s\n%s\n' "$token" "$match" >&2
    exit 1
  fi
done

if rg -n -F 'DataTenantId::SYSTEM_OWNER' crates/vala/vala-bifrost-redux/src crates/wyrd/wyrd-testing/benches \
  | rg -v 'crates/vala/vala-bifrost-redux/src/lib\.rs:[0-9]+:' >/dev/null; then
  echo "Redux single-table model violation: sentinel data tenant" >&2
  exit 1
fi

if rg -n -e 'INSERT INTO platform\.tenants|INSERT INTO wyrd\.tenants' crates/vala/vala-sql/migrations | rg -n -e '00000000|system-owner' >/dev/null; then
  echo "Redux single-table model violation: nil or sentinel tenant migration" >&2
  exit 1
fi

if ! rg -n -F 'data_tenant_id' crates/vala/vala-bifrost-redux/src/tables >/dev/null; then
  echo "Redux single-table model check could not find mandatory physical tenant columns" >&2
  exit 1
fi

if rg -n -F 'vala.bifrost.' crates/wyrd/wyrd-testing/benches >/dev/null; then
  echo "Redux single-table model violation: synthetic vala.bifrost benchmark table" >&2
  exit 1
fi

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

for token in "${forbidden[@]}"; do
  fixture="$fixture_dir/${token//[^[:alnum:]]/_}.txt"
  printf '%s\n' "$token" > "$fixture"
  if ! rg -n -F "$token" "$fixture" >/dev/null; then
    echo "single-table model negative self-test failed for $token" >&2
    exit 1
  fi
done

printf '%s\n' 'DataTenantId::SYSTEM_OWNER' > "$fixture_dir/sentinel.rs"
if ! rg -n -F 'DataTenantId::SYSTEM_OWNER' "$fixture_dir/sentinel.rs" >/dev/null; then
  echo "single-table model negative self-test failed for sentinel tenant" >&2
  exit 1
fi

printf '%s\n' 'BifrostTableDescription { scope: TableScopeWire::Tenant }' > "$fixture_dir/contract.rs"
if ! rg -n -e 'BifrostTableDescription.*scope|TableScopeWire' "$fixture_dir/contract.rs" >/dev/null; then
  echo "single-table model negative self-test failed for scope contract" >&2
  exit 1
fi

printf '%s\n' 'table: "vala.bifrost.events"' > "$fixture_dir/benchmark.rs"
if ! rg -n -F 'vala.bifrost.' "$fixture_dir/benchmark.rs" >/dev/null; then
  echo "single-table model negative self-test failed for benchmark namespace" >&2
  exit 1
fi

echo "Redux single-table model passed"
