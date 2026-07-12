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
#   1. tonic/prost dep declarations appear only in wyrd-tonic and Cargo.toml
#   2. No renamed-dep workaround (package = "tonic") bypasses the rule
#   3. Source imports use wyrd-tonic re-exports, not tonic:: directly
#   4. Workspace pin entries for all four tonic-family crates are present
#   5. wyrd-server depends on wyrd-tonic with the "server" feature
set -e

# 1. Manifest declarations must only appear in wyrd-tonic and workspace root
! rg -n --no-heading \
    -e '^[[:space:]]*(tonic|tonic-health|tonic-reflection|prost)[[:space:]]*=' \
    --glob '!crates/wyrd/wyrd-tonic/Cargo.toml' \
    --glob '!Cargo.toml' \
    crates/ python/

# 1a. Renamed-dep evasion (package = "tonic")
! rg -n --no-heading \
    -e 'package[[:space:]]*=[[:space:]]*"(tonic|tonic-health|tonic-reflection|prost)"' \
    --glob '!crates/wyrd/wyrd-tonic/Cargo.toml' \
    --glob '!Cargo.toml' \
    crates/ python/

# 2. Source-code imports must not bypass wyrd-tonic re-exports
! rg -n --no-heading \
    -e '^[[:space:]]*(pub(\([^)]+\))?[[:space:]]+)?use[[:space:]]+(tonic|tonic_health|tonic_reflection|prost)::' \
    --glob '!crates/wyrd/wyrd-tonic/src/**' \
    crates/ python/

# 2a. extern crate form
! rg -n --no-heading \
    -e '^[[:space:]]*extern[[:space:]]+crate[[:space:]]+(tonic|tonic_health|tonic_reflection|prost)\b' \
    --glob '!crates/wyrd/wyrd-tonic/src/**' \
    crates/ python/

# 3. Workspace pin sanity
rg -q -e '^[[:space:]]*tonic[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-health[[:space:]]*=' Cargo.toml
rg -q -e '^[[:space:]]*tonic-reflection[[:space:]]*=' Cargo.toml
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
