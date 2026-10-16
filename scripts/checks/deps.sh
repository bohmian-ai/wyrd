#!/usr/bin/env bash
# WHY THIS FILE EXISTS: Cargo compiles whatever a manifest declares, so a
# client, SDK, or contract crate can silently grow a server, database, PyO3,
# test-harness, or second-TLS-provider dependency. Each rule below is a crate
# boundary from AGENTS.md checked against the real resolved graph.
#
# WHAT IT CHECKS: forbidden crates in production (normal-edge) dependency
# cones, single versions of the Arrow/DataFusion stack, one Rustls provider,
# no test seams in the shipped server, and the production shape of
# wyrd-spec secrets.
set -euo pipefail

# deny <regex> <cargo tree args...>: fail when the normal-edge cone of the
# selected packages contains a crate whose name matches <regex>.
deny() {
  local pattern="$1" graph hits
  shift
  graph=$(cargo tree --locked -e normal --prefix none "$@")
  hits=$(rg "^($pattern) " <<<"$graph" | sort -u || true)
  if [[ -n "$hits" ]]; then
    echo "FAIL: cargo tree $* reaches forbidden crates:"
    echo "$hits"
    exit 1
  fi
}

# PyO3 only behind explicit `python` features; contracts never have one.
deny 'pyo3' --workspace
deny 'pyo3' -p wyrd-spec --all-features
deny 'pyo3' -p wyrd-storage --all-features
for crate in crates/shared/*/; do
  [[ "$(basename "$crate")" == wyrd-utils ]] && continue
  deny 'pyo3' -p "$(basename "$crate")" --all-features
done

# Test tooling never reaches a production package.
deny 'wyrd-testing|wyrd-bench|mockall|wiremock|mockito' --workspace \
  --exclude wyrd-testing --exclude wyrd-sdk-ts-testing --exclude wyrd-bench

# Client tier: contracts, clients, CLI, and SDKs stay free of server,
# database, engine, and cloud crates.
deny 'utoipa|sqlx|axum|opentelemetry|reqwest|tokio' -p wyrd-spec --no-default-features
deny 'sqlx' -p wyrd-auth-verify --all-features
deny 'sqlx|tokio-postgres|postgres|deltalake|datafusion|axum|kube|aws-sdk-.*|azure_.*|google-cloud-.*|opendal|rdkafka|lapin|redis|deadpool-.*|vala-.*|wyrd-server|wyrd-sql' \
  -p wyrd-client --all-features
deny 'sqlx|datafusion|vala-bifrost-redux|vala-sql|axum' -p wyrd-mcp --all-features
deny 'sqlx|wyrd-sql|vala-sql|wyrd-auth-issue|wyrd-server|wyrd-dev-fixtures' -p wyrd-cli -p wyrd-sdk-python
deny 'wyrd-sdk-python|wyrd-sdk-ts' -p wyrd-ai
deny 'wyrd-server' -p wyrd-dev-fixtures --all-features

# Vala's durable layer never reaches the Skald engine; skald-spec arrives only
# through wyrd-spec.
deny 'wyrd-skald-(agent|cache|prompt|providers|runtime|tool)' -p vala-sql --all-features

# Skald: the prompt boundary is runtime-free; engine crates reach Wyrd only
# through the locked foundation crates.
deny 'reqwest|tokio|hyper|axum|sqlx|opentelemetry|wyrd-skald-(runtime|providers|cache|tool)' \
  -p wyrd-skald-prompt --all-features
for crate in crates/skald/*/; do
  crate="wyrd-$(basename "$crate")"
  hits=$(cargo tree --locked -p "$crate" --all-features -e normal --prefix none |
    rg '^(wyrd|vala)-' |
    rg -v '^(wyrd-skald-[a-z]+|wyrd-spec|wyrd-semver|wyrd-interfaces|wyrd-utils|wyrd-runtime|wyrd-error-derive|wyrd-tls) ' |
    sort -u || true)
  if [[ -n "$hits" ]]; then
    echo "FAIL: $crate reaches Wyrd/Vala crates outside the Skald foundation set:"
    echo "$hits"
    exit 1
  fi
done

# Provider-credential mutation stays in the CLI and scoped MCP.
if rg -q '^[^/]*gateway_credential' sdks/wyrd-sdk-rust/src; then
  echo 'FAIL: wyrd-ai projects gateway_credential'
  exit 1
fi

# The Rust SDK projects only the parity surface: wyrd-client's `internal`
# feature (raw requests, writer internals, bundle introspection) is never
# enabled by its manifest, directly or through one of its own features, nor
# reached by its production graph. The test-only `testing` feature reaches it
# through the in-process CLI and is absent from production builds.
if rg -q 'wyrd-client/internal|^wyrd-client\s*=.*"internal"' sdks/wyrd-sdk-rust/Cargo.toml; then
  echo 'FAIL: wyrd-ai enables wyrd-client/internal'
  exit 1
fi
if cargo tree --locked -p wyrd-ai -e features,normal,build --prefix none |
  rg -q '^wyrd-client feature "internal"'; then
  echo 'FAIL: the wyrd-ai production graph enables wyrd-client/internal'
  exit 1
fi

# One version each of the tightly coupled Arrow/DataFusion stack.
duplicates=$(cargo tree --locked --workspace --all-features -e no-dev -d --prefix none |
  rg -o '^(object_store|datafusion|arrow|parquet) v\S+' | sort -u |
  awk '{n[$1]++; v[$1]=v[$1] " " $2} END {for (c in n) if (n[c] > 1) print c v[c]}')
if [[ -n "$duplicates" ]]; then
  echo "FAIL: multiple versions resolve:"
  echo "$duplicates"
  exit 1
fi

# AWS-LC is the only Rustls provider.
features=$(cargo tree --locked --workspace --all-features -e features,no-dev)
if rg -q 'rustls feature "ring"' <<<"$features"; then
  echo 'FAIL: the production graph enables the Ring Rustls provider'
  exit 1
fi

# The shipped server never enables a test seam such as WyrdPostgres::from_pools
# or SecretRef::Inline; those exist only behind test-support / test-utils.
for features in '' cloud; do
  seams=$(cargo tree --locked -p wyrd-server --features "$features" -e features,normal,build \
    --prefix none | rg -o '\S+ feature "test-(support|utils)"' | sort -u || true)
  if [[ -n "$seams" ]]; then
    echo "FAIL: wyrd-server (features: ${features:-default}) enables test seams:"
    echo "$seams"
    exit 1
  fi
done

# Wyrd has one verification vocabulary: no current surface names the retired
# Eval model (identifiers, permissions, tables, generated contracts, docs).
# History is exempt (changes/, applied migrations), as are refusal tests that
# prove a retired name stays unknown and external syntax Wyrd does not own.
# Production files with inline refusal tests are exempt by line, not by file.
eval_name='(^|[^A-Za-z])evals?([^a-z]|$)|(^|[^A-Z])Evals?([^a-z]|$)|(^|[^A-Z])EVALS?([^A-Z]|$)'
eval_exempt=(
  '^\./scripts/checks/deps\.sh:'
  '^\./crates/vala/vala-sql/tests/integration/pg_migration\.rs:'
  '^\./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/(query|verification)\.rs:'
  '^\./crates/wyrd/wyrd-cli/tests/(cli|card_lifecycle)\.rs:'
  '^\./crates/wyrd/wyrd-server/tests/integration/pg_(card_registration_route|openapi_contract)\.rs:'
  '^\./crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions\.rs:'
  '^\./crates/shared/wyrd-client/src/observe/tests\.rs:'
  '^\./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time\.rs:'
  '^\./sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time\.py:'
  '^\./sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface\.py:'
  '^\./sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time\.test\.ts:'
  '^\./sdks/wyrd-sdk-ts/wyrd/tests/unit/observe\.test\.ts:'
  '^\./crates/shared/wyrd-runtime/src/permission\.rs:[0-9]+:.*(retired `evals`|evals:\{action\}|"evals"[],}])'
  '^\./crates/wyrd-spec/src/card/mod\.rs:[0-9]+:.*(kind: eval\\n|`kind: Eval`|drift_and_eval_card_kinds|\["Drift", "Eval"\]|`Eval` and no|json!\("Eval"\))'
  '^\./crates/shared/wyrd-loader/src/parse\.rs:[0-9]+:.*(`Eval` Card kinds|drift_and_eval_card_kinds|\["Drift", "Eval"\])'
  '^\./crates/wyrd-spec/src/card/operator\.rs:[0-9]+:.*"\{\{eval\.total_tasks\}\}"'
  '^\./crates/wyrd-spec/src/vala/trace/mod\.rs:[0-9]+:.*get\("eval_results"\)\.is_none\(\)'
  '^\./crates/vala/vala-bifrost-redux/src/tables/mod\.rs:[0-9]+: *\("eval", "[a-z_]+"\),$'
  '^[^:]+:[0-9]+:.*(eval "\$|literal_eval\(|eval_metric=)'
)
hits=$(rg -n --hidden -g '!.git' -g '!changes/**' -g '!**/migrations/**' "$eval_name" . |
  rg -v "$(IFS='|'; echo "${eval_exempt[*]}")" || true)
if [[ -n "$hits" ]]; then
  echo 'FAIL: retired Eval vocabulary; use verification terms:'
  echo "$hits"
  exit 1
fi

# Inline secrets exist only under wyrd-spec's test-utils feature.
cargo nextest run --locked -p wyrd-spec --no-default-features --features server \
  --test security_inline_gate
