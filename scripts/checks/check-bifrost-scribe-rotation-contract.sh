#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
design="$repository_root/architecture/wyrd-design.md"
rotation_contract="$(awk '/^\*\*Scribe ingress pressure sealing \(D83\)\.\*\*/{capture=1} capture{print} capture && /^Crash recovery/{exit}' "$design")"
rotation_contract_flat="$(tr '\n' ' ' <<<"$rotation_contract")"

require() {
  local pattern="$1"
  local description="$2"
  if ! grep -Eq "$pattern" <<<"$rotation_contract_flat"; then
    echo "Scribe rotation contract: missing $description" >&2
    exit 1
  fi
}

require '600-second shard-generation age' 'the 600-second shard-generation default'
require 'automatic rotation ORs projected non-empty WAL' 'the projected automatic OR predicate'
require 'closes the shard WAL and atomically freezes every non-empty' 'whole-shard pre-append rotation'
require 'before the incoming unit enters a fresh generation' 'fresh-generation admission ordering'
require 'selectively freeze younger largest writable buckets' 'selective younger pressure sealing'
require 'toward the low-water target without closing the shard WAL or resetting shard-generation age' 'pressure/automatic lifecycle separation'
require 'Forced and tenant-scoped sealing are also selective paths' 'forced and tenant path separation'
require 'never masquerade as the automatic whole-shard rotation contract' 'no selective/automatic conflation'

echo "Scribe pressure and automatic rotation contract is exact"
