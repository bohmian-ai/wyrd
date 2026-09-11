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
  grep -m1 "set_output $1 " "$DETECT" | sed "s/.*set_output $1 '//;s/'$//"
}

IDENTITY_PATTERN="$(_extract_pattern identity)"
BIFROST_PATTERN="$(grep -m1 "set_output_all bifrost_only " "$DETECT" | sed "s/.*set_output_all bifrost_only '//;s/'$//")"

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

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
