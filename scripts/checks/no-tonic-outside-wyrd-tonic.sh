#!/usr/bin/env bash
# WHY THIS FILE EXISTS: tonic, prost, and companion crates must be versioned
# exactly once in the workspace. Multiple declarations at different versions
# cause linker errors, duplicate generated types, and gRPC codec mismatches
# where a message type from one prost version is silently incompatible with
# the codec registered from another. wyrd-tonic is the single re-export
# boundary; all other crates import gRPC types through it so version changes
# are a one-line workspace edit rather than a multi-crate update.
#
# WHAT IT CHECKS:
#   1. tonic/prost dep declarations appear only in wyrd-tonic, the workspace
#      root, and the generated workspace-hack manifest
#   2. No renamed-dep workaround (package = "tonic") bypasses the rule
#   3. Source imports use wyrd-tonic re-exports, not tonic:: directly
#   4. Workspace pin entries for all four tonic-family crates are present
#   5. wyrd-server depends on wyrd-tonic with the "server" feature
set -e

# Fail the check when rg finds a forbidden match or cannot search (exit 2, e.g.
# a missing path). `! rg` would pass on a search error, and under `set -e` a
# negated command that fails does not stop the script, so only the last one
# ever decided the exit status.
forbid() {
    local status=0
    rg -n --no-heading "$@" || status=$?
    if [ "$status" -ne 1 ]; then
        echo "forbidden pattern found or search failed (rg exit $status)" >&2
        exit 1
    fi
}

# 1. Manifest declarations must only appear in wyrd-tonic and workspace root
forbid \
    -e '^[[:space:]]*(tonic|tonic-health|tonic-reflection|tonic-types|tonic-build|tonic-prost|tonic-prost-build|prost)[[:space:]]*=' \
    --glob '!crates/wyrd/wyrd-tonic/Cargo.toml' \
    --glob '!crates/shared/workspace-hack/Cargo.toml' \
    crates/

# 1a. Renamed-dep evasion (package = "tonic")
forbid \
    -e 'package[[:space:]]*=[[:space:]]*"(tonic|tonic-health|tonic-reflection|tonic-types|tonic-build|tonic-prost|tonic-prost-build|prost)"' \
    --glob '!crates/wyrd/wyrd-tonic/Cargo.toml' \
    --glob '!crates/shared/workspace-hack/Cargo.toml' \
    crates/

# 2. Source-code imports must not bypass wyrd-tonic re-exports
forbid \
    -e '^[[:space:]]*(pub(\([^)]+\))?[[:space:]]+)?use[[:space:]]+(tonic|tonic_health|tonic_reflection|prost)::' \
    --glob '!crates/wyrd/wyrd-tonic/src/**' \
    crates/

# 2a. extern crate form
forbid \
    -e '^[[:space:]]*extern[[:space:]]+crate[[:space:]]+(tonic|tonic_health|tonic_reflection|prost)\b' \
    --glob '!crates/wyrd/wyrd-tonic/src/**' \
    crates/

# 3. Workspace pin sanity
rg -q -e '^[[:space:]]*tonic[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-health[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-reflection[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-types[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-prost[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-prost-build[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*prost[[:space:]]*=' Cargo.toml

# 4. wyrd-server consumes wyrd-tonic with features = ["server"]
if command -v cargo >/dev/null && command -v jq >/dev/null; then
    server_features=$(cargo metadata --format-version=1 --no-deps |
        jq -r '.packages[] | select(.name=="wyrd-server")
               | .dependencies[]
               | select(.name=="wyrd-tonic")
               | .features[]')
    echo "$server_features" | rg -q '^server$'
else
    rg -q -U \
        -e 'wyrd-tonic[[:space:]]*=.*features[^\n]*"server"' \
        crates/wyrd/wyrd-server/Cargo.toml || \
    rg -q -U \
        -e '\[dependencies\.wyrd-tonic\][^\[]*features[^\n]*"server"' \
        crates/wyrd/wyrd-server/Cargo.toml
fi
