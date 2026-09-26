#!/usr/bin/env bash
# Self-tests for .github/scripts/detect-changes.sh and the selector it runs.
#
# WHY THIS FILE EXISTS: the selector decides which verification lanes, SDK
# platform jobs, journeys, and release packages a change runs. A wrong
# selection silently skips proof, so every route — including the fallbacks that
# must take the complete gate — is pinned here against the real classifier and
# the real workspace dependency graph.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DETECT="$SCRIPT_DIR/../detect-changes.sh"
WORKFLOWS="$SCRIPT_DIR/../../workflows"

PASS=0
FAIL=0

ok() { echo "OK  $1"; PASS=$((PASS+1)); }
fail() { echo "FAIL $1"; FAIL=$((FAIL+1)); }

# expect <outputs-file> <token>...: `name=value` requires that exact output,
# `name~word` requires the word in that space-separated output, and `name!word`
# requires its absence.
expect() {
  local outputs="$1" token name value got
  shift
  for token in "$@"; do
    case "$token" in
      *~*) name="${token%%~*}"; value="${token#*~}" ;;
      *!*) name="${token%%!*}"; value="${token#*!}" ;;
      *) grep -qx -- "$token" "$outputs" || { echo "missing $token in: $(tr '\n' ' ' < "$outputs")"; return; }; continue ;;
    esac
    got=" $(grep -m1 "^$name=" "$outputs" | cut -d= -f2-) "
    if [[ "$token" == *~* && "$got" != *" $value "* ]]; then
      echo "$name lacks $value:$got"; return
    fi
    if [[ "$token" == *!* && "$got" == *" $value "* ]]; then
      echo "$name has $value:$got"; return
    fi
  done
  echo ok
}

# check_selection <label> <expected-tokens> <changed-file>...: runs the real
# classifier over a fixed change and compares its outputs.
check_selection() {
  local label="$1" want="$2" files outputs result
  shift 2
  files="$(mktemp)"
  outputs="$(mktemp)"
  printf '%s\n' "$@" > "$files"
  WYRD_CHANGED_FILES="$files" GITHUB_OUTPUT="$outputs" bash "$DETECT" > /dev/null
  # shellcheck disable=SC2086
  result="$(expect "$outputs" $want)"
  rm -f "$files" "$outputs"
  if [[ "$result" == ok ]]; then ok "$label"; else fail "$label: $result"; fi
}

# --- Scenario 1: classified Rust changes select their consumer closure -------

check_selection "leaf crate tests only its owner family" \
  "full_gate=false changed_packages=vala-drift rust_packages=vala-drift ci_lanes=test:vala" \
  "crates/vala/vala-drift/src/lib.rs"
check_selection "leaf crate omits unrelated families" \
  "full_gate=false changed_packages=wyrd-mcp ci_lanes~test:wyrd ci_lanes!test:skald ci_lanes!test:vala ci_lanes!test:shared ci_lanes!test:rust" \
  "crates/wyrd/wyrd-mcp/src/lib.rs"
check_selection "shared crate includes transitive consumers" \
  "full_gate=false rust_packages~skald-runtime rust_packages~skald-agent rust_packages~skald-workflow rust_packages~wyrd rust_packages~wyrd-server ci_lanes~test:skald ci_lanes~test:wyrd ci_lanes~test:vala" \
  "crates/skald/skald-cache/src/lib.rs"
check_selection "dev-dependency consumers are tested but not propagated" \
  "full_gate=false rust_packages~wyrd-sdk-rust rust_packages~wyrd-server rust_packages!wyrd-rust-examples" \
  "crates/wyrd/wyrd-testing/src/lib.rs"
check_selection "rename inside one crate keeps its closure" \
  "full_gate=false changed_packages=vala-drift ci_lanes=test:vala" \
  "crates/vala/vala-drift/src/old_name.rs" "crates/vala/vala-drift/src/new_name.rs"
check_selection "deleted crate path takes the full gate" \
  "full_gate=true" \
  "crates/wyrd/wyrd-retired/src/lib.rs"

metadata_failure="$(mktemp)"
printf '%s\n' "crates/vala/vala-drift/src/lib.rs" > "$metadata_failure.files"
WYRD_CARGO_METADATA=/nonexistent WYRD_CHANGED_FILES="$metadata_failure.files" \
  GITHUB_OUTPUT="$metadata_failure" bash "$DETECT" > /dev/null
if grep -qx 'full_gate=true' "$metadata_failure"; then
  ok "classifier failure takes the full gate"
else
  fail "classifier failure must take the full gate"
fi
rm -f "$metadata_failure" "$metadata_failure.files"

# A package the lane map does not own must not produce an empty test plan.
orphan="$(mktemp -d)"
cat > "$orphan/metadata.json" <<JSON
{"workspace_root": "$(git rev-parse --show-toplevel)",
 "workspace_members": ["orphan"],
 "packages": [{"id": "orphan", "name": "orphan",
   "manifest_path": "$(git rev-parse --show-toplevel)/crates/shared/orphan/Cargo.toml",
   "dependencies": []}]}
JSON
printf '%s\n' "crates/shared/orphan/src/lib.rs" > "$orphan/files"
WYRD_CARGO_METADATA="$orphan/metadata.json" WYRD_CHANGED_FILES="$orphan/files" \
  GITHUB_OUTPUT="$orphan/outputs" bash "$DETECT" > /dev/null
if grep -qx 'full_gate=true' "$orphan/outputs"; then
  ok "package without a test lane takes the full gate"
else
  fail "package without a test lane must take the full gate"
fi
rm -rf "$orphan"

# --- Scenario 2: cross-boundary changes and uncertainty preserve proof -------

check_selection "wire contract selects codegen, SDKs, and journeys" \
  "full_gate=false rust_client=true typescript=true python=true storage=true identity=true ci_lanes~codegen:check ci_lanes~py:test:integration ci_lanes~ts:test:integration ci_lanes~ts:napi:check ci_lanes~test:bifrost:gate ci_lanes~test:wyrd ci_lanes~test:shared" \
  "crates/wyrd-spec/src/lib.rs"
check_selection "shared client selects SDK platforms and journeys" \
  "full_gate=false rust_client=true typescript=true python=true identity=true ci_lanes~codegen:check ci_lanes~py:test:integration ci_lanes~ts:test:integration ci_lanes~test:bifrost:gate" \
  "crates/shared/wyrd-client/src/config.rs"
check_selection "server change selects identity and storage journeys" \
  "full_gate=false identity=true storage=true ci_lanes~test:wyrd ci_lanes~test:bifrost:gate" \
  "crates/wyrd/wyrd-server/src/state.rs"
check_selection "gateway engine selects the gateway journeys" \
  "full_gate=false changed_packages=wyrd-gateway rust_packages~wyrd-server rust_packages~wyrd-testing ci_lanes~test:wyrd ci_lanes~test:gateway:gate" \
  "crates/wyrd/wyrd-gateway/src/lib.rs"
check_selection "auth crate selects the identity journey" \
  "full_gate=false identity=true" \
  "crates/shared/wyrd-auth-oidc/src/lib.rs"
check_selection "storage crate selects the storage matrix" \
  "full_gate=false storage=true" \
  "crates/wyrd/wyrd-storage/src/lib.rs"
check_selection "client storage selects storage" "storage=true" \
  "crates/shared/wyrd-client/src/storage/mod.rs"
check_selection "Rust-only leaf skips SDK platforms and journeys" \
  "rust_client=false typescript=false python=false identity=false storage=false ci_lanes!py:test:integration" \
  "crates/vala/vala-drift/src/lib.rs"
check_selection "Bifrost-only runs the capability gate" \
  "bifrost_only=true full_gate=false ci_lanes=" \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs"
check_selection "Bifrost cross-crate change stays Bifrost-only" "bifrost_only=true full_gate=false" \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs" \
  "crates/wyrd/wyrd-server/src/oracle/peer_service.rs"
check_selection "mixed Bifrost and Rust takes the full gate" "bifrost_only=false full_gate=true rust_packages= ci_lanes=" \
  "crates/vala/vala-bifrost-redux/src/forge/worker.rs" \
  "crates/skald/skald-agent/src/lib.rs"
check_selection "global configuration takes the full gate" "full_gate=true" "Cargo.lock"
check_selection "workflow change takes the full gate" "full_gate=true" ".github/workflows/lints-test.yml"
check_selection "unclassified path takes the full gate" "full_gate=true" \
  "scripts/postgres/with-test-postgres.sh"
check_selection "planning-only change selects nothing" \
  "any=true full_gate=false ci_lanes= rust_client=false python=false typescript=false docs=false" \
  "changes/active/example/spec.md" "README.md"
check_selection "docs-only change runs docs checks" \
  "full_gate=false docs=true ui=false ci_lanes= rust_client=false identity=false" \
  "docs/src/index.mdx"
check_selection "UI-only change runs UI checks" \
  "full_gate=false ui=true ci_lanes= identity=false storage=false rust_client=false" \
  "crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.svelte"
check_selection "Python SDK change runs Python on both platforms and its journey" \
  "full_gate=false python=true typescript=false ci_lanes~py:test:integration ci_lanes~codegen:check" \
  "sdks/wyrd-sdk-python/python/wyrd/__init__.py"
check_selection "TypeScript SDK change runs TypeScript on both platforms and its journey" \
  "full_gate=false typescript=true python=false ci_lanes~ts:test:integration ci_lanes~ts:napi:check" \
  "sdks/wyrd-sdk-ts/wyrd/src/index.ts"
check_selection "Python example runs Python checks" "full_gate=false python=true" \
  "examples/python/demo.py"
check_selection "identity fixture selects the identity journey" "full_gate=false identity=true" \
  "tests/fixtures/identity/dex-config.yaml"

zero_outputs="$(mktemp)"
GITHUB_OUTPUT="$zero_outputs" bash "$DETECT" "$(printf '%040d' 0)" "$(git rev-parse HEAD)" > /dev/null
if grep -qx 'full_gate=true' "$zero_outputs" && grep -qx 'any=true' "$zero_outputs"; then
  ok "first push selects the full gate"
else
  fail "first push must select the full gate"
fi
rm -f "$zero_outputs"

# job <workflow> <job>: prints one job block of a workflow.
job() {
  awk -v name="  $2:" '$0==name{p=1;next} p&&/^  [a-z-]+:$/{p=0} p' "$WORKFLOWS/$1"
}

# The Linux ci job runs the gate for a full-gate change and otherwise runs
# check, boundary checks, and exactly the selected lanes; SDK platform jobs run
# on Linux and macOS, and the full gate also selects them.
ci_job="$(job lints-test.yml ci)"
if grep -q 'timed gate$' <<< "$ci_job" \
  && grep -q 'timed check$' <<< "$ci_job" \
  && grep -q 'timed check:client-tier$' <<< "$ci_job" \
  && grep -q 'timed check:pyo3-scope$' <<< "$ci_job" \
  && grep -q 'for lane in $CI_LANES' <<< "$ci_job" \
  && ! grep -q 'test:rust' <<< "$ci_job" \
  && grep -q 'WYRD_TEST_PACKAGES' <<< "$ci_job"; then
  ok "ci runs the gate or check plus the selected lanes"
else
  fail "ci must run the gate or check, boundary checks, and the selected lanes"
fi
for sdk_job in rust-client typescript python; do
  block="$(job lints-test.yml "$sdk_job")"
  if grep -q 'matrix-plan.outputs.os' <<< "$block" && grep -q 'outputs.full_gate' <<< "$block"; then
    ok "$sdk_job runs on both platforms and on the full gate"
  else
    fail "$sdk_job must run on both platforms and on the full gate"
  fi
done
if grep -q '"macos-15"' <<< "$(job lints-test.yml matrix-plan)"; then
  ok "SDK platform matrix includes macOS"
else
  fail "SDK platform matrix must include macOS"
fi
for workflow in lints-test.yml storage-integration-emulator.yml identity-e2e.yml; do
  if ! grep -q '\${{ *secrets\.' "$WORKFLOWS/$workflow"; then
    ok "$workflow uses no secrets"
  else
    fail "$workflow must not reach secrets from pull-request code"
  fi
done
if grep -q '^  push:' "$WORKFLOWS/storage-integration-cloud.yml" \
  && ! grep -q 'pull_request' "$WORKFLOWS/storage-integration-cloud.yml"; then
  ok "real-cloud storage stays push-to-main only"
else
  fail "real-cloud storage must stay push-to-main only"
fi

# --- Scenario 3: routine packaging is selective; release is full ------------

check_selection "leaf crate packages nothing" \
  "package_python=false package_typescript=false package_server=false package_crates=false" \
  "crates/vala/vala-drift/src/lib.rs"
check_selection "shared Skald change packages Python and server" \
  "package_python=true package_server=true package_typescript=false package_crates=false" \
  "crates/skald/skald-cache/src/lib.rs"
check_selection "wire contract packages every deliverable" \
  "package_python=true package_typescript=true package_server=true package_crates=true" \
  "crates/wyrd-spec/src/lib.rs"
check_selection "UI change packages the server bundle only" \
  "package_server=true package_python=false package_typescript=false" \
  "crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.svelte"
check_selection "TypeScript SDK change packages TypeScript only" \
  "package_typescript=true package_python=false package_server=false" \
  "sdks/wyrd-sdk-ts/wyrd/src/index.ts"
check_selection "docs change packages nothing" \
  "package_python=false package_typescript=false package_server=false package_crates=false" \
  "docs/src/index.mdx"
check_selection "unknown impact packages the complete matrix" \
  "package_python=true package_typescript=true package_server=true package_crates=true" \
  "scripts/postgres/with-test-postgres.sh"

release="$WORKFLOWS/release.yml"
release_builds_ok=true
for build in build-sdist build-linux build-macos build-typescript build-ui build-server package-crates; do
  block="$(job release.yml "$build")"
  grep -q "github.event_name == 'release'" <<< "$block" || release_builds_ok=false
  grep -q 'needs.changes.outputs.package_' <<< "$block" || release_builds_ok=false
done
if [[ "$release_builds_ok" == true ]]; then
  ok "release builds every deliverable; main builds affected deliverables"
else
  fail "each release build must run on release events and on its affected main push"
fi
publish_ok=true
for publish in publish-pypi publish-docker publish-binaries publish-crates; do
  block="$(job release.yml "$publish")"
  grep -q 'qualify' <<< "$block" || publish_ok=false
  grep -q 'verify-release-artifacts.sh' <<< "$block" || publish_ok=false
  grep -q 'actions/attest-build-provenance' <<< "$block" || publish_ok=false
done
if [[ "$publish_ok" == true ]]; then
  ok "publication waits for qualification and publishes attested verified digests"
else
  fail "every publish job must need qualification, verify recorded digests, and attest provenance"
fi
if grep -q 'uses: ./.github/workflows/nightly.yml' <<< "$(job release.yml qualify)" \
  && grep -q 'workflow_call' "$WORKFLOWS/nightly.yml"; then
  ok "release qualification runs the complete nightly gate"
else
  fail "release qualification must run the complete nightly gate"
fi

# verify-release-artifacts.sh accepts exactly the recorded, verified bytes.
VERIFY_ARTIFACTS="$SCRIPT_DIR/../verify-release-artifacts.sh"
artifacts="$(mktemp -d)"
(cd "$artifacts" && printf 'wheel' > a.whl && printf 'sdist' > b.tar.gz \
  && sha256sum a.whl > SHA256SUMS.one && sha256sum b.tar.gz > SHA256SUMS.two)
if bash "$VERIFY_ARTIFACTS" "$artifacts" > /dev/null; then
  ok "recorded artifacts verify"
else
  fail "recorded artifacts must verify"
fi
printf 'rebuilt' > "$artifacts/a.whl"
if ! bash "$VERIFY_ARTIFACTS" "$artifacts" > /dev/null 2>&1; then
  ok "rebuilt artifact bytes are rejected"
else
  fail "rebuilt artifact bytes must be rejected"
fi
(cd "$artifacts" && sha256sum a.whl > SHA256SUMS.one && printf 'extra' > c.whl)
if ! bash "$VERIFY_ARTIFACTS" "$artifacts" > /dev/null 2>&1; then
  ok "unrecorded artifact is rejected"
else
  fail "unrecorded artifact must be rejected"
fi
rm -f "$artifacts"/SHA256SUMS.* "$artifacts/c.whl"
if ! bash "$VERIFY_ARTIFACTS" "$artifacts" > /dev/null 2>&1; then
  ok "missing digest record is rejected"
else
  fail "missing digest record must be rejected"
fi
rm -rf "$artifacts"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
