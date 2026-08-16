#!/usr/bin/env bash
# Enforce Redux as the sole Bifrost catalog and server boot authority.
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root_dir"

if [[ -d crates/vala/vala-bifrost ]]; then
  echo "legacy vala-bifrost crate still exists" >&2
  exit 1
fi

legacy_matches="$({
  git grep -n -E 'vala_bifrost::|WyrdCatalog' -- '*.rs' 'Cargo.toml' || true
  git grep -n -E '^vala-bifrost[[:space:]]*=' -- 'Cargo.toml' || true
})"
if [[ -n "$legacy_matches" ]]; then
  echo "legacy Bifrost authority remains:" >&2
  echo "$legacy_matches" >&2
  exit 1
fi

state_owner_count="$(rg -c '^    pub bifrost: Arc<BifrostCatalog>,$' crates/wyrd/wyrd-server/src/state.rs)"
if [[ "$state_owner_count" != "1" ]]; then
  echo "AppState must retain exactly one required Redux catalog owner" >&2
  exit 1
fi

boot_owner_count="$(rg -c '^    let bifrost = Arc::new\($' crates/wyrd/wyrd-server/src/boot/mod.rs)"
if [[ "$boot_owner_count" != "1" ]]; then
  echo "production boot must construct exactly one Redux catalog" >&2
  exit 1
fi

echo "Redux is the sole Bifrost catalog and server boot authority"
