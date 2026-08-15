#!/usr/bin/env bash
set -euo pipefail

# Reject executable Rust ownership paths that can construct or recover a
# Bifrost capacity authority outside the composed role capabilities.
root_dir="${BIFROST_RESOURCE_GOVERNANCE_ROOT:-$(git rev-parse --show-toplevel)}"

scan_tree() {
  local scan_root="$1"
  local violations
  violations="$(find "$scan_root" -type f -name '*.rs' -print0 | xargs -0 -r perl -0777 -ne '
    s{/\*.*?\*/}{}gs;
    s{//[^\n]*}{}g;
    while (/\b(?:BifrostMemoryGovernor|ScribeMemoryBudget|OracleMemoryBudget|ParentMemoryReservation|BifrostDataFusionMemoryPool|MemoryLedger)\b|\.memory_ledger\s*\(|\.scribe_budget\s*\(|\.oracle_budget\s*\(|\.with_bifrost_memory_pool\s*\(|\.governor\s*\(/g) {
      my $prefix = substr($_, 0, $-[0]);
      my $line = 1 + ($prefix =~ tr/\n//);
      print "$ARGV:$line:$&\n";
    }
  ' 2>/dev/null || true)"
  if [[ -n "$violations" ]]; then
    printf 'Obsolete Bifrost resource ownership surface found:\n%s\n' "$violations"
    return 1
  fi
}

if [[ "${BIFROST_RESOURCE_GOVERNANCE_SELF_TEST:-0}" == "1" ]]; then
  fixture_dir="$(mktemp -d)"
  trap 'rm -rf "$fixture_dir"' EXIT
  mkdir -p "$fixture_dir/negative" "$fixture_dir/positive"
  printf '%s\n' '// BifrostMemoryGovernor was removed; MemoryLedger is historical vocabulary.' > "$fixture_dir/negative/comment.rs"
  scan_tree "$fixture_dir/negative"
  printf '%s\n' 'fn bypass(value: BifrostMemoryGovernor) { let _ = value.memory_ledger(); }' > "$fixture_dir/positive/bypass.rs"
  if scan_tree "$fixture_dir/positive" >/dev/null 2>&1; then
    printf 'Bifrost resource governance positive fixture was not rejected.\n'
    exit 1
  fi
  printf 'Bifrost resource governance fixture coverage passed.\n'
  exit 0
fi

scan_tree "$root_dir/crates/vala/vala-bifrost-redux"
scan_tree "$root_dir/crates/wyrd/wyrd-server"
scan_tree "$root_dir/crates/wyrd/wyrd-testing"
printf 'Bifrost resource governance check passed.\n'
