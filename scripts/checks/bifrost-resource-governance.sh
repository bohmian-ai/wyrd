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

scan_direct_root_construction() {
  local scan_root="$1"
  local allowed_owner="${2:-}"
  local violations
  violations="$(find "$scan_root" -type f -name '*.rs' ! -path "$allowed_owner" -print0 | xargs -0 -r perl -0777 -ne '
    s{/\*.*?\*/}{}gs;
    s{//[^\n]*}{}g;
    while (/\bBifrostResourceGovernor\s*::\s*from_snapshot\s*\(/g) {
      my $prefix = substr($_, 0, $-[0]);
      my $line = 1 + ($prefix =~ tr/\n//);
      print "$ARGV:$line:$&\n";
    }
  ' 2>/dev/null || true)"
  if [[ -n "$violations" ]]; then
    printf 'Direct Bifrost root construction found outside its runtime owner:\n%s\n' "$violations"
    return 1
  fi
}

scan_resources_owner() {
  local owner_file="$1"
  local violations
  violations="$(perl -0777 -ne '
    s{/\*.*?\*/}{}gs;
    s{//[^\n]*}{}g;
    my ($production, $tests) = split(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{/, $_, 2);
    my $expected = () = $production =~ /let\s+root\s*=\s*BifrostResourceGovernor\s*::\s*from_snapshot\s*\(/g;
    print "$ARGV:production:expected exactly one runtime-owned root construction, found $expected\n" if $expected != 1;
    $production =~ s/let\s+root\s*=\s*BifrostResourceGovernor\s*::\s*from_snapshot\s*\(/let root = permitted_runtime_constructor(/g;
    print "$ARGV:production:unexpected direct root construction\n" if $production =~ /\bBifrostResourceGovernor\s*::\s*from_snapshot\s*\(/;
    print "$ARGV:tests:direct root construction\n" if defined($tests) && $tests =~ /\bBifrostResourceGovernor\s*::\s*from_snapshot\s*\(/;
    print "$ARGV:tests:direct root acquisition\n" if defined($tests) && $tests =~ /\.try_acquire_(?:scribe_memory|oracle|oracle_memory)\s*\(/;
  ' "$owner_file" 2>/dev/null || true)"
  if [[ -n "$violations" ]]; then
    printf 'Invalid Bifrost root ownership inside resources.rs:\n%s\n' "$violations"
    return 1
  fi
}

if [[ "${BIFROST_RESOURCE_GOVERNANCE_SELF_TEST:-0}" == "1" ]]; then
  fixture_dir="$(mktemp -d)"
  trap 'rm -rf "$fixture_dir"' EXIT
  mkdir -p "$fixture_dir/negative" "$fixture_dir/positive" "$fixture_dir/owner-valid" "$fixture_dir/owner-invalid"
  printf '%s\n' '// BifrostMemoryGovernor was removed; MemoryLedger is historical vocabulary.' > "$fixture_dir/negative/comment.rs"
  scan_tree "$fixture_dir/negative"
  printf '%s\n' '// BifrostResourceGovernor::from_snapshot is discussed but never called.' > "$fixture_dir/negative/root_comment.rs"
  scan_direct_root_construction "$fixture_dir/negative"
  printf '%s\n' 'fn bypass(value: BifrostMemoryGovernor) { let _ = value.memory_ledger(); }' > "$fixture_dir/positive/bypass.rs"
  if scan_tree "$fixture_dir/positive" >/dev/null 2>&1; then
    printf 'Bifrost resource governance positive fixture was not rejected.\n'
    exit 1
  fi
  printf '%s\n' 'fn bypass(snapshot: Snapshot, policy: Policy) { let _ = BifrostResourceGovernor::from_snapshot(snapshot, policy); }' > "$fixture_dir/positive/root.rs"
  if scan_direct_root_construction "$fixture_dir/positive" >/dev/null 2>&1; then
    printf 'Bifrost direct-root positive fixture was not rejected.\n'
    exit 1
  fi
  printf '%s\n' 'fn from_snapshot(snapshot: Snapshot, policy: Policy) { let root = BifrostResourceGovernor::from_snapshot(snapshot, policy); } #[cfg(test)] mod tests { let request = ImmutableRequest; }' > "$fixture_dir/owner-valid/resources.rs"
  scan_resources_owner "$fixture_dir/owner-valid/resources.rs"
  printf '%s\n' 'fn from_snapshot(snapshot: Snapshot, policy: Policy) { let root = BifrostResourceGovernor::from_snapshot(snapshot, policy); } #[cfg(test)] mod tests { fn bypass(snapshot: Snapshot, policy: Policy) { let root = BifrostResourceGovernor::from_snapshot(snapshot, policy); root.try_acquire_oracle(1, 1); } }' > "$fixture_dir/owner-invalid/resources.rs"
  if scan_resources_owner "$fixture_dir/owner-invalid/resources.rs" >/dev/null 2>&1; then
    printf 'Bifrost resources.rs test-module fixture was not rejected.\n'
    exit 1
  fi
  printf 'Bifrost resource governance fixture coverage passed.\n'
  exit 0
fi

scan_tree "$root_dir/crates/vala/vala-bifrost-redux"
scan_tree "$root_dir/crates/wyrd/wyrd-server"
scan_tree "$root_dir/crates/wyrd/wyrd-testing"
scan_direct_root_construction \
  "$root_dir/crates/vala/vala-bifrost-redux" \
  "$root_dir/crates/vala/vala-bifrost-redux/src/resources.rs"
scan_resources_owner "$root_dir/crates/vala/vala-bifrost-redux/src/resources.rs"
scan_direct_root_construction "$root_dir/crates/wyrd/wyrd-server"
scan_direct_root_construction "$root_dir/crates/wyrd/wyrd-testing"
printf 'Bifrost resource governance check passed.\n'
