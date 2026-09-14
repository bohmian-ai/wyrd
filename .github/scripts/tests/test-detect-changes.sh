#!/usr/bin/env bash
# Self-tests for .github/scripts/detect-changes.sh
#
# WHY THIS FILE EXISTS: detect-changes.sh classifies changed files into named
# output flags (rust, python, identity, storage, …) that gate expensive CI
# workflows. A wrong classifier silently skips the wrong workflow — the
# identity e2e suite won't run when it should, or will run when it shouldn't.
# These tests pin every classifier path so regressions are caught before merge.
#
# HOW IT WORKS: extracts each classifier's regex directly from detect-changes.sh
# (no duplication), then checks representative file paths against it. No git
# history required — pattern matching only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DETECT="$SCRIPT_DIR/../detect-changes.sh"

PASS=0
FAIL=0

_extract_pattern() {
  grep -m1 "^$1_pattern=" "$DETECT" | sed "s/^$1_pattern='//;s/'$//"
}

IDENTITY_PATTERN="$(_extract_pattern identity)"
BIFROST_PATTERN="$(_extract_pattern bifrost_only)"

check() {
  local label="$1"
  local file="$2"
  local want="$3"   # true | false

  if echo "$file" | grep -qE "$IDENTITY_PATTERN"; then
    got=true
  else
    got=false
  fi

  if [ "$got" = "$want" ]; then
    echo "OK  $label"
    PASS=$((PASS+1))
  else
    echo "FAIL $label: expected identity=$want for '$file', got $got"
    FAIL=$((FAIL+1))
  fi
}

# --- identity=true cases ---
check "mise.toml"                          "mise.toml"                                                       true
check "Cargo.lock"                         "Cargo.lock"                                                      true
check "docker-compose.yml"                 "docker-compose.yml"                                              true
check "tests/fixtures/identity/dex-config" "tests/fixtures/identity/dex-config.yaml"                       true
check "tests/fixtures/identity/keycloak"   "tests/fixtures/identity/keycloak-realm.json"                   true
check "wyrd-auth shared crate"             "crates/shared/wyrd-auth-oidc/src/lib.rs"                       true
check "wyrd-client auth"                   "crates/shared/wyrd-client/src/transport/credential.rs"         true
check "wyrd-client config"                 "crates/shared/wyrd-client/src/config.rs"                       true
check "wyrd-auth crate"                    "crates/wyrd/wyrd-auth/src/lib.rs"                              true
check "wyrd-server src"                    "crates/wyrd/wyrd-server/src/main.rs"                            true
check "wyrd-testing oidc_fixture"          "crates/wyrd/wyrd-testing/src/oidc_fixture.rs"                  true
check "wyrd-spec security"                 "crates/wyrd-spec/src/security/mod.rs"                          true
check "identity-e2e workflow"              ".github/workflows/identity-e2e.yml"                             true
check "detect-changes script itself"       ".github/scripts/detect-changes.sh"                             true

# --- identity=false cases (docs/UI/python-only changes must not trigger) ---
check "docs site change"                   "docs/src/index.mdx"                                             false
check "python example"                     "examples/python/demo.py"                                        false
# wyrd-server/ is in the identity pattern so any change under it, including UI, triggers it
check "wyrd-ui svelte (under wyrd-server)" "crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.svelte"        true
check "vala crate (no identity impact)"    "crates/vala/vala-eval/src/lib.rs"                               false
check "README"                             "README.md"                                                      false
check "storage crate (no identity impact)" "crates/wyrd/wyrd-storage/src/lib.rs"                           false

check_bifrost_only() {
  local label="$1"
  local want="$2"
  shift 2

  got=true
  for file in "$@"; do
    if ! echo "$file" | grep -qE "$BIFROST_PATTERN"; then
      got=false
    fi
  done

  if [ "$got" = "$want" ]; then
    echo "OK  $label"
    PASS=$((PASS+1))
  else
    echo "FAIL $label: expected bifrost_only=$want, got $got"
    FAIL=$((FAIL+1))
  fi
}

check_bifrost_only "Forge implementation" true \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs"
check_bifrost_only "Bifrost cross-crate change" true \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs" \
  "crates/wyrd/wyrd-server/src/oracle/peer_service.rs"
check_bifrost_only "mixed domain change" false \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs" \
  "crates/skald/skald-agent/src/lib.rs"
check_bifrost_only "global test configuration" false "mise.toml"

# check_selection runs the real classifier over a change and compares the
# outputs that decide between focused lanes and the complete gate.
check_selection() {
  local label="$1"
  local want="$2"   # space-separated name=value outputs
  shift 2

  local files outputs
  files="$(mktemp)"
  outputs="$(mktemp)"
  printf '%s\n' "$@" > "$files"
  WYRD_CHANGED_FILES="$files" GITHUB_OUTPUT="$outputs" bash "$DETECT" > /dev/null

  local expected got=ok
  for expected in $want; do
    if ! grep -qx "$expected" "$outputs"; then
      got="missing $expected in: $(tr '\n' ' ' < "$outputs")"
    fi
  done
  rm -f "$files" "$outputs"

  if [ "$got" = ok ]; then
    echo "OK  $label"
    PASS=$((PASS+1))
  else
    echo "FAIL $label: $got"
    FAIL=$((FAIL+1))
  fi
}

check_selection "generic Rust runs focused lanes" "rust=true bifrost_only=false full_gate=false" \
  "crates/skald/skald-agent/src/lib.rs"
check_selection "Bifrost-only runs the capability gate" "bifrost_only=true full_gate=false" \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs"
check_selection "mixed Bifrost and Rust takes the full gate" "bifrost_only=false full_gate=true" \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs" \
  "crates/skald/skald-agent/src/lib.rs"
check_selection "global configuration takes the full gate" "global=true full_gate=true" "Cargo.lock"
check_selection "unclassified path takes the full gate" "unclassified=true full_gate=true" \
  "scripts/postgres/with-test-postgres.sh"
check_selection "planning-only change selects nothing" "any=true unclassified=false full_gate=false rust=false" \
  "changes/active/example/spec.md" "README.md"
check_selection "client storage selects storage" "storage=true" \
  "crates/shared/wyrd-client/src/storage/mod.rs"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
