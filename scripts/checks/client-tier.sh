#!/usr/bin/env bash
# Foundation invariants: wyrd-spec server-tier-free; shared shells pyo3+sqlx-free;
# Skald keeps only locked Wyrd foundation edges.
set -eu

forbidden_spec=$(cargo tree -p wyrd-spec --no-default-features -e normal | \
  rg '(^|[ ─└├])(utoipa|pyo3|sqlx|axum|opentelemetry|reqwest|tokio)' || true)
if [ -n "$forbidden_spec" ]; then
  echo "FAIL: wyrd-spec pulls forbidden deps at --no-default-features:"
  echo "$forbidden_spec"
  exit 1
fi

for crate in wyrd-auth-verify; do
  forbidden_shared=$(cargo tree -p "$crate" --all-features -e normal | \
    rg '(^|[ ─└├])(pyo3|sqlx)' || true)
  if [ -n "$forbidden_shared" ]; then
    echo "FAIL: $crate pulls forbidden deps:"
    echo "$forbidden_shared"
    exit 1
  fi
done

# `tower-http` is intentionally absent: it is a non-optional transitive
# dependency of `reqwest` (client-side follow-redirect), the sanctioned async
# HTTP client for the auth exchange and HTTP transport. `axum` still flags the
# server HTTP stack.
forbidden_wyrd_client=$(cargo tree -p wyrd-client --all-features -e normal | \
  rg '(^|[ ─└├])(sqlx|tokio-postgres|postgres|deltalake|datafusion|axum|kube|aws-sdk|azure_|google-cloud|opendal|rdkafka|lapin|redis|deadpool-|vala-)' || true)
if [ -n "$forbidden_wyrd_client" ]; then
  echo "FAIL: wyrd-client pulls forbidden client-tier deps:"
  echo "$forbidden_wyrd_client"
  exit 1
fi

# S3.C7: wyrd-mcp must stay engine-free (no vala-bifrost, no sqlx, no datafusion, no axum).
# Mirror of the wyrd-client guard above. Dev-deps are excluded (-e normal).
forbidden_wyrd_mcp=$(cargo tree -p wyrd-mcp --all-features -e normal | \
  rg '(^|[ ─└├])(sqlx|datafusion|vala-|axum)' || true)
if [ -n "$forbidden_wyrd_mcp" ]; then
  echo "FAIL: wyrd-mcp pulls forbidden engine deps (sqlx|datafusion|vala-|axum):"
  echo "$forbidden_wyrd_mcp"
  exit 1
fi

forbidden_skald_prompt=$(cargo tree -p skald-prompt --all-features -e normal | \
  rg '(^|[ ─└├])(reqwest|tokio|hyper|axum|sqlx|opentelemetry|skald-runtime|skald-providers|skald-cache|skald-tool)' || true)
if [ -n "$forbidden_skald_prompt" ]; then
  echo "FAIL: skald-prompt pulls forbidden client-unsafe deps:"
  echo "$forbidden_skald_prompt"
  exit 1
fi

for crate in skald-agent skald-workflow skald-tool skald-runtime; do
  forbidden_skald_member=$(cargo tree -p "$crate" --all-features -e normal | \
    rg '(^|[ ─└├])(wyrd-|vala-)' | \
    rg -v '(^|[ ─└├])(wyrd-spec|wyrd-semver|wyrd-interfaces|wyrd-utils|wyrd-runtime|wyrd-error-derive|wyrd-observe)' || true)
  if [ -n "$forbidden_skald_member" ]; then
    echo "FAIL: $crate pulls forbidden Wyrd/Vala deps outside the locked foundation set:"
    echo "$forbidden_skald_member"
    exit 1
  fi
done

if rg -n 'wyrd_spec|wyrd-' crates/skald \
  --glob '!crates/skald/skald-prompt/**' \
  --glob '!crates/skald/skald-agent/**' \
  --glob '!crates/skald/skald-agent/Cargo.toml' \
  --glob '!crates/skald/skald-tool/Cargo.toml' \
  --glob '!crates/skald/skald-workflow/**'; then
  echo 'Skald engine crates must remain free of Wyrd references outside locked boundary crates'
  exit 1
fi

if rg -n 'wyrd_cards|wyrd-cards' crates/skald/skald-agent; then
  echo 'skald-agent must not depend on wyrd-cards'
  exit 1
fi

if ! rg -q 'wyrd-(spec|interfaces|utils)' crates/skald/skald-prompt/Cargo.toml; then
  echo 'skald-prompt must remain the documented Skald/Wyrd boundary crate'
  exit 1
fi
