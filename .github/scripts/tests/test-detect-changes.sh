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

check_selection "MCP client selects its consumers and journey lanes" \
  "full_gate=false changed_packages=wyrd-mcp rust_packages~wyrd-mcp rust_packages~wyrd-cli ci_lanes~test:wyrd ci_lanes~test:bifrost:gate ci_lanes~test:gateway:gate" \
  "crates/wyrd/wyrd-mcp/src/lib.rs"
check_selection "leaf crate omits unrelated families" \
  "full_gate=false changed_packages=wyrd ci_lanes~test:wyrd ci_lanes!test:skald ci_lanes!test:vala ci_lanes!test:shared ci_lanes!test:rust" \
  "crates/wyrd/wyrd/src/lib.rs"
check_selection "shared crate includes transitive consumers" \
  "full_gate=false rust_packages~wyrd-skald-runtime rust_packages~wyrd-skald-agent rust_packages~wyrd rust_packages~wyrd-server ci_lanes~test:skald ci_lanes~test:wyrd ci_lanes~test:vala" \
  "crates/skald/skald-cache/src/lib.rs"
check_selection "dev-dependency consumers are tested but not propagated" \
  "full_gate=false rust_packages~wyrd-cli rust_packages~wyrd-server rust_packages!wyrd-sdk-ts" \
  "crates/wyrd/wyrd-testing/src/lib.rs"
check_selection "rename inside one crate keeps its closure" \
  "full_gate=false changed_packages=wyrd rust_packages=wyrd ci_lanes~test:wyrd" \
  "crates/wyrd/wyrd/src/old_name.rs" "crates/wyrd/wyrd/src/new_name.rs"
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

# A package outside every family directory must not produce an empty test plan.
orphan="$(mktemp -d)"
cat > "$orphan/metadata.json" <<JSON
{"workspace_root": "$(git rev-parse --show-toplevel)",
 "workspace_members": ["orphan"],
 "packages": [{"id": "orphan", "name": "orphan",
   "manifest_path": "$(git rev-parse --show-toplevel)/crates/orphan/Cargo.toml",
   "dependencies": []}]}
JSON
printf '%s\n' "crates/orphan/src/lib.rs" > "$orphan/files"
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
  "full_gate=false rust_client=true typescript=true python=true storage=true identity=true docs=true ci_lanes~codegen:check ci_lanes~py:test:integration ci_lanes~ts:test:integration ci_lanes~ts:napi:check ci_lanes~test:bifrost:gate ci_lanes~test:wyrd ci_lanes~test:shared" \
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
check_selection "KMS crate selects the Vault Transit check" \
  "full_gate=false changed_packages=wyrd-kms ci_lanes~test:wyrd ci_lanes~test:kms:vault" \
  "crates/wyrd/wyrd-kms/src/lib.rs"
check_selection "auth crate selects the identity journey" \
  "full_gate=false identity=true" \
  "crates/shared/wyrd-auth-oidc/src/lib.rs"
check_selection "storage crate selects the storage matrix" \
  "full_gate=false storage=true" \
  "crates/wyrd/wyrd-storage/src/lib.rs"
check_selection "client storage selects storage" "storage=true" \
  "crates/shared/wyrd-client/src/storage/mod.rs"
check_selection "Rust-only leaf skips SDK platforms and unrelated journeys" \
  "rust_client=false typescript=false python=false identity=false storage=false ci_lanes!py:test:integration" \
  "crates/wyrd/wyrd/src/lib.rs"
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
check_selection "TypeScript SDK change runs TypeScript on both platforms, its journey, and the UI" \
  "full_gate=false typescript=true ui=true python=false ci_lanes~ts:test:integration ci_lanes~ts:napi:check" \
  "sdks/wyrd-sdk-ts/wyrd/src/index.ts"
check_selection "Python example runs Python checks" "full_gate=false python=true" \
  "examples/python/demo.py"
check_selection "docs example Card runs every language's journeys" \
  "full_gate=false docs=true python=true typescript=true ci_lanes~test:examples ci_lanes~py:test:integration ci_lanes~ts:test:integration" \
  "examples/setup-check/no-refund-promise.yaml"
check_selection "docs example Rust program runs the examples journeys only" \
  "full_gate=false python=false typescript=false ci_lanes~test:examples ci_lanes~check:examples" \
  "examples/setup-check/check.rs"
check_selection "crate error source regenerates the error reference" "full_gate=false docs=true" \
  "crates/shared/wyrd-client/src/error.rs"
check_selection "SDK error source regenerates the error reference" "full_gate=false docs=true" \
  "sdks/wyrd-sdk-python/src/lib.rs"
check_selection "Rust test source leaves the docs alone" "full_gate=false docs=false" \
  "crates/shared/wyrd-client/tests/integration/transport/http.rs"
check_selection "Rust SDK change selects its story journeys" \
  "full_gate=false ci_lanes~test:rust-sdk:journey ci_lanes~test:gateway:gate" \
  "sdks/wyrd-sdk-rust/src/lib.rs"
check_selection "identity fixture selects the identity journey" "full_gate=false identity=true" \
  "tests/fixtures/identity/dex-config.yaml"

# describe_jobs: reads a ci_jobs matrix on stdin and prints one line per job:
#   <name>: <lanes> (cache <cache>[, saves cache][, on RunsOn])
describe_jobs() {
  python3 -c '
import json, sys
for job in json.load(sys.stdin):
    notes = ["cache " + job["cache"]]
    if job.get("save"):
        notes.append("saves cache")
    if job.get("runs_on"):
        notes.append("on RunsOn")
    print("{}: {} ({})".format(job["name"], job["lanes"], ", ".join(notes)))
'
}

# jobs_for <changed-file>...: the described ci matrix for a fixed change.
jobs_for() {
  local files outputs
  files="$(mktemp)"
  outputs="$(mktemp)"
  printf '%s\n' "$@" > "$files"
  WYRD_CHANGED_FILES="$files" GITHUB_OUTPUT="$outputs" bash "$DETECT" > /dev/null
  grep -m1 '^ci_jobs=' "$outputs" | cut -d= -f2- | describe_jobs
  rm -f "$files" "$outputs"
}

# expect_jobs <label> <expected> <actual>: the described matrices match exactly.
expect_jobs() {
  if [[ "$3" == "$2" ]]; then
    ok "$1"
  else
    fail "$1"
    printf 'expected:\n%s\nactual:\n%s\n' "$2" "$3"
  fi
}

expect_jobs "docs-only change runs no ci job" "" "$(jobs_for docs/src/index.mdx)"
expect_jobs "planning-only change runs no ci job" "" "$(jobs_for changes/active/example/spec.md)"

expect_jobs "Bifrost-only change runs the Bifrost gate, which saves the test cache" \
  "bifrost: verify:bifrost (cache test, saves cache)" \
  "$(jobs_for crates/vala/vala-bifrost-redux/src/forge/worker.rs)"

expect_jobs "Rust-only leaf runs no RunsOn job" \
  "checks: check check:deps (cache clippy, saves cache)
rust:families: test:wyrd (cache test, saves cache)" \
  "$(jobs_for crates/wyrd/wyrd/src/lib.rs)"

expect_jobs "identity fixture runs only the identity journey, on RunsOn" \
  "rust:journeys: test:identity:journey (cache journeys-typescript, saves cache, on RunsOn)" \
  "$(jobs_for tests/fixtures/identity/dex-config.yaml)"

expect_jobs "TypeScript SDK change runs its lanes, then the identity journey, in one RunsOn job" \
  "checks: check check:deps codegen:check (cache clippy, saves cache)
typescript+rust:journeys: ts:napi:check ts:test:integration test:identity:journey (cache journeys-typescript, saves cache, on RunsOn)" \
  "$(jobs_for sdks/wyrd-sdk-ts/wyrd/src/index.ts)"

expect_jobs "server change runs hosted shard jobs, one RunsOn job, and one writer per cache" \
  "checks: check check:deps check:examples codegen:check (cache clippy, saves cache)
rust:families: test:wyrd test:shared (cache test, saves cache)
rust:gateway: test:gateway:gate (cache test)
rust:bifrost: test:bifrost:gate (cache test)
storage: test:storage:matrix (cache test)
python: py:test:integration (cache test)
typescript+rust:journeys: ts:napi:check ts:test:integration test:identity:journey test:rust-sdk:journey test:examples (cache journeys-typescript, saves cache, on RunsOn)" \
  "$(jobs_for crates/wyrd/wyrd-server/src/state.rs)"

# The complete gate runs every gate shard in mise.toml exactly once: six hosted
# jobs and one RunsOn job for the TypeScript then Rust journey shards.
full_gate_jobs="gate:checks: gate:checks (cache clippy, saves cache)
gate:rust:families: gate:rust:families (cache test, saves cache)
gate:rust:gateway: gate:rust:gateway (cache test)
gate:rust:bifrost: gate:rust:bifrost (cache test)
gate:storage: gate:storage (cache test)
gate:python: gate:python (cache test)
gate:typescript+gate:rust:journeys: gate:typescript gate:rust:journeys (cache journeys-typescript, saves cache, on RunsOn)"
expect_jobs "global change runs the full gate as seven jobs" \
  "$full_gate_jobs" "$(jobs_for Cargo.lock)"
expect_jobs "unknown path falls back to the full gate's jobs" \
  "$full_gate_jobs" "$(jobs_for scripts/postgres/with-test-postgres.sh)"
expect_jobs "nightly runs the full gate's jobs" "$full_gate_jobs" \
  "$(cd "$SCRIPT_DIR/../../.." && GITHUB_OUTPUT=/dev/stdout python3 .github/scripts/select-ci.py --gate \
    | cut -d= -f2- | describe_jobs)"

# No real change selects TypeScript lanes without the identity journey, so the
# planner is called directly: the lanes still run alone in the RunsOn job.
typescript_only="$(cd "$SCRIPT_DIR/.." && python3 -c '
import importlib.util, json
spec = importlib.util.spec_from_file_location("select_ci", "select-ci.py")
select_ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(select_ci)
print(json.dumps(select_ci.plan_jobs([("gate:typescript", "typescript", ["ts:test:unit"])])))
' | describe_jobs)"
expect_jobs "TypeScript-only selection runs as the RunsOn job" \
  "typescript: ts:test:unit (cache journeys-typescript, saves cache, on RunsOn)" \
  "$typescript_only"


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
if grep -q 'fromJSON(needs.changes.outputs.ci_jobs)' <<< "$ci_job" \
  && grep -q "for lane in \$CI_LANES" <<< "$ci_job" \
  && grep -q 'WYRD_TEST_PACKAGES' <<< "$ci_job"; then
  ok "ci runs each selected job's lanes in parallel"
else
  fail "ci must run the ci_jobs matrix and each job's lanes"
fi
# Only the planned RunsOn job takes a paid runner; every job starts after
# selection, with no warm-up job, and the required check aggregates them.
for workflow in lints-test.yml:ci nightly.yml:gate; do
  block="$(job "${workflow%%:*}" "${workflow#*:}")"
  if grep -q "runs-on: \${{ matrix.job.runs_on && format('runs-on={0}/runner=8cpu-linux-x64/ram=16/volume=100gb/extras=s3-cache', github.run_id) || 'ubuntu-24.04' }}" <<< "$block" \
    && grep -q 'if: matrix.job.runs_on' <<< "$block" \
    && grep -q "for lane in \$CI_LANES" <<< "$block"; then
    ok "$workflow runs only the planned RunsOn job on RunsOn, lane by lane"
  else
    fail "$workflow must run only the planned RunsOn job on RunsOn, lane by lane"
  fi
done
ci_complete="$(job lints-test.yml ci-complete)"
if ! grep -q '^  deps:' "$WORKFLOWS/lints-test.yml" "$WORKFLOWS/nightly.yml" \
  && ! grep -q 'needs\.deps' <<< "$ci_complete" \
  && grep -q 'needs: \[changes, matrix-plan, ci, rust-client, typescript, python, docs, ui, image\]' <<< "$ci_complete"; then
  ok "no warm-up job; ci complete aggregates every job"
else
  fail "ci complete must aggregate every job and no warm-up job may remain"
fi
for sdk_job in rust-client typescript python; do
  block="$(job lints-test.yml "$sdk_job")"
  if grep -q 'matrix-plan.outputs.\(os\|python\)' <<< "$block" && grep -q 'outputs.full_gate' <<< "$block"; then
    ok "$sdk_job runs on the planned platforms and on the full gate"
  else
    fail "$sdk_job must run on the planned platforms and on the full gate"
  fi
done
if grep -q '"macos-15"' <<< "$(job lints-test.yml matrix-plan)"; then
  ok "SDK platform matrix includes macOS"
else
  fail "SDK platform matrix must include macOS"
fi
if ! grep -q "\\\${{ *secrets\\." "$WORKFLOWS/lints-test.yml"; then
  ok "lints-test.yml uses no secrets"
else
  fail "lints-test.yml must not reach secrets from pull-request code"
fi
if grep -q '^  push:' "$WORKFLOWS/storage-integration-cloud.yml" \
  && ! grep -q 'pull_request' "$WORKFLOWS/storage-integration-cloud.yml"; then
  ok "real-cloud storage stays push-to-main only"
else
  fail "real-cloud storage must stay push-to-main only"
fi

# --- Scenario 3: the server image lane follows the server binary -------------

check_selection "leaf crate leaves the server image alone" \
  "package_server=false" \
  "crates/wyrd/wyrd/src/lib.rs"
check_selection "shared Skald change builds the server image" \
  "package_server=true" \
  "crates/skald/skald-cache/src/lib.rs"
check_selection "wire contract builds the server image" \
  "package_server=true" \
  "crates/wyrd-spec/src/lib.rs"
check_selection "UI change leaves the server image alone" \
  "package_server=false" \
  "crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.svelte"
check_selection "TypeScript SDK change leaves the server image alone" \
  "package_server=false" \
  "sdks/wyrd-sdk-ts/wyrd/src/index.ts"
check_selection "docs change leaves the server image alone" \
  "package_server=false" \
  "docs/src/index.mdx"
check_selection "unknown impact builds the server image" \
  "package_server=true" \
  "scripts/postgres/with-test-postgres.sh"

publish_ok=true
for publish in publish-pypi publish-docker publish-binaries publish-crates; do
  block="$(job release.yml "$publish")"
  grep -q 'verify-release-artifacts.sh' <<< "$block" || publish_ok=false
  grep -q 'actions/attest-build-provenance' <<< "$block" || publish_ok=false
done
if [[ "$publish_ok" == true ]]; then
  ok "every publication is attested and verified"
else
  fail "every publish job must verify recorded digests and attest provenance"
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
