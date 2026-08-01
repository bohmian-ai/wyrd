#!/usr/bin/env bash
# Enforce Wyrd's single-provider TLS dependency policy.
set -euo pipefail

graph=$(cargo tree --workspace --all-features -e features,no-dev)

if rg -q 'rustls feature "ring"' <<<"$graph"; then
  echo "FAIL: production graph enables a Ring-backed Rustls provider"
  printf '%s\n' "$graph" | rg 'rustls feature "ring"' || true
  exit 1
fi

if ! rg -q 'rustls feature "aws[-_]lc[-_]rs"' <<<"$graph"; then
  echo "FAIL: production graph does not enable the AWS-LC Rustls provider"
  exit 1
fi

if cargo metadata --format-version=1 --no-deps |
  jq -e '.packages[] | .dependencies[] | select(.name == "reqwest" and (.req | startswith("^0.12")))' \
    >/dev/null; then
  echo "FAIL: a workspace package directly depends on Reqwest 0.12"
  exit 1
fi

echo "OK: AWS-LC is the only production Rustls provider and workspace Reqwest is 0.13"
