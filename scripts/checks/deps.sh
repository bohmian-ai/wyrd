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

# Wyrd has one verification vocabulary: no current path or content names the
# retired Eval model (identifiers, permissions, tables, routes, generated
# contracts, docs), abbreviated or as an expanded product name such as
# EvaluationRun or evaluation_runs. Descriptive "evaluation" prose, ordinary
# computation (evaluate_operator, is_evaluation) and the standard GenAI
# projection (GenAiEvaluationResult, evaluation_results) are not product names.
# History is exempt (changes/, applied migrations). Refusal tests and check
# inputs are exempt only as the exact current lines below (path:content), so a
# new or edited line is checked again; external syntax Wyrd does not own is
# stripped token by token, so a retired name beside it still fails.
eval_name='(^|[^A-Za-z])evals?([^a-z]|$)|(^|[^A-Z])Evals?([^a-z]|$)|(^|[^A-Z])EVALS?([^A-Z]|$)'
eval_noun='(Runs?|Suites?|Tasks?|Jobs?|Datasets?|Reports?|Records?|Workflows?)'
eval_lower='(runs?|suites?|tasks?|jobs?|datasets?|reports?|records?|workflows?)'
eval_upper='(RUNS?|SUITES?|TASKS?|JOBS?|DATASETS?|REPORTS?|RECORDS?|WORKFLOWS?)'
eval_product="(^|[^A-Za-z])evaluations?[_-]?$eval_lower([^a-z]|\$)|(^|[^A-Z])Evaluations?$eval_noun([^a-z]|\$)|(^|[^A-Z])EVALUATIONS?_?$eval_upper([^A-Z]|\$)|/evaluations([^a-z]|\$)|(^|[^A-Za-z])evaluations?:[a-z]|vala\.evaluations?([^a-z]|\$)"
eval_vocab="$eval_name|$eval_product"
eval_allowed=$(cat <<'EOF'
./crates/shared/wyrd-client/src/observe/tests.rs:        "vala.eval.observations",
./crates/shared/wyrd-loader/src/parse.rs:    fn parse_rejects_retired_drift_and_eval_card_kinds() {
./crates/shared/wyrd-loader/src/parse.rs:        for kind in ["Drift", "Eval"] {
./crates/shared/wyrd-loader/src/parse.rs:    /// Refuse the retired `Drift` and `Eval` Card kinds at the loader boundary.
./crates/shared/wyrd-runtime/src/permission.rs:                "any_of evals:{action}"
./crates/shared/wyrd-runtime/src/permission.rs:                "evals:{action}"
./crates/shared/wyrd-runtime/src/permission.rs:                format!("evals:{action}").parse::<Permission>().is_err(),
./crates/shared/wyrd-runtime/src/permission.rs:                    json!({"resource": "evals", "action": action, "scope": "all"})
./crates/shared/wyrd-runtime/src/permission.rs:    /// Proves the retired `evals` resource is an ordinary unknown resource in
./crates/shared/wyrd-runtime/src/permission.rs:                    "resource": {"any_of": ["operators", "evals"]},
./crates/shared/wyrd-runtime/src/permission.rs:                "structured evals:{action}"
./crates/vala/vala-bifrost-redux/src/tables/mod.rs:            ("eval", "assertions"),
./crates/vala/vala-bifrost-redux/src/tables/mod.rs:            ("eval", "observations"),
./crates/vala/vala-bifrost-redux/src/tables/mod.rs:            ("eval", "result_items"),
./crates/vala/vala-bifrost-redux/src/tables/mod.rs:            ("eval", "runs"),
./crates/vala/vala-sql/tests/integration/pg_migration.rs:            "a new vala.eval task is refused"
./crates/vala/vala-sql/tests/integration/pg_migration.rs:    /// Eval task survives, another task is lost, or `vala.eval` is admitted.
./crates/vala/vala-sql/tests/integration/pg_migration.rs:    /// holding Eval Forge tasks: it deletes only those tasks, keeps every
./crates/vala/vala-sql/tests/integration/pg_migration.rs:            insert("vala.eval").await.is_err(),
./crates/vala/vala-sql/tests/integration/pg_migration.rs:        let retired = insert("vala.eval").await.expect("old Eval task inserts");
./crates/vala/vala-sql/tests/integration/pg_migration.rs:            "../../migrations/20261016000000_retire_eval_namespace.sql"
./crates/vala/vala-sql/tests/integration/pg_migration.rs:            !namespaces.contains("vala.eval") && namespaces.contains("vala.verification"),
./crates/vala/vala-sql/tests/integration/pg_migration.rs:    /// other namespace's task, and then refuses a new `vala.eval` task.
./crates/vala/vala-sql/tests/integration/pg_migration.rs:    /// The forward migration retiring `vala.eval` upgrades a schema still
./crates/vala/vala-sql/tests/integration/pg_migration.rs:             ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval',\
./crates/wyrd-spec/src/card/mod.rs:        assert!(!enum_values.contains(&json!("Eval")));
./crates/wyrd-spec/src/card/mod.rs:    fn removed_drift_and_eval_card_kinds_fail_to_deserialize() {
./crates/wyrd-spec/src/card/mod.rs:        for kind in ["Drift", "Eval"] {
./crates/wyrd-spec/src/card/mod.rs:    /// Reject `kind: Drift` and `kind: Eval` envelopes; neither is a Card kind.
./crates/wyrd-spec/src/card/mod.rs:    /// replacing both `Drift` and `Eval` and no `Workflow`.
./crates/wyrd-spec/src/card/mod.rs:                    "spec:\n  implementation:\n    kind: eval\n    spec: {tasks: {}}\n  inputs:",
./crates/wyrd-spec/src/card/operator.rs:        for removed in ["{{run_id}}", "{{eval.total_tasks}}"] {
./crates/wyrd-spec/src/vala/trace/mod.rs:        assert!(wire.get("eval_results").is_none(), "no abbreviated alias");
./crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:        for kind in ["Drift", "Eval"] {
./crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:    /// `plan` and `apply` refuse the retired `kind: Drift` and `kind: Eval`
./crates/wyrd/wyrd-cli/tests/cli.rs:            .any(|line| line.trim_start().starts_with("eval")),
./crates/wyrd/wyrd-cli/tests/cli.rs:    for command in [&["eval"][..], &["eval", "run"][..]] {
./crates/wyrd/wyrd-cli/tests/cli.rs:        "help lists no eval command: {help}"
./crates/wyrd/wyrd-cli/tests/cli.rs:        let codes = ["vala.eval", "vala.absent"].map(|namespace| {
./crates/wyrd/wyrd-cli/tests/cli.rs:            stderr.contains("unrecognized subcommand 'eval'"),
./crates/wyrd/wyrd-cli/tests/cli.rs:/// than the usage code, lists `eval` in help, or connects to the listener.
./crates/wyrd/wyrd-cli/tests/cli.rs:/// The compiled `wyrd query` refuses the retired Eval tables exactly as it
./crates/wyrd/wyrd-cli/tests/cli.rs:/// The retired `wyrd eval` command tree is gone: the shipped binary refuses
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:                .all(|table| table["namespace"] != "vala.eval"),
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:                .any(|word| word == "eval" || word == "evals")),
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:    /// `evals` role cannot obtain a token, no tool names the retired Eval
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:            for namespace in ["vala.eval", "vala.absent"] {
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:            let role = format!("evals_{action}");
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:            "no tool names the retired Eval workflow"
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:             SELECT data_tenant_id,decode(md5(fqn),'hex'),'vala.eval.result_items',fingerprint,\
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:                    &[serde_json::json!({"resource": "evals", "action": action, "scope": "all"})],
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:    /// `vala.eval.result_items` is listed nowhere and refused for describe and
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:    /// `cards.get` as `kind: Verifier`, while `Drift` and `Eval` are refused
./crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:        for retired in ["Drift", "Eval"] {
./crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:    for kind in ["Drift", "Eval"] {
./crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:/// reference, this proves: the retired `kind: Drift` and `kind: Eval` Cards
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// bare `/evaluations` route segment. A word ends at a character that cannot
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// continue it: not lowercase for `eval`/`Eval`, not uppercase for `EVAL`.
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// Each case-folded `eval` occurrence must open a word: `eval` after no
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:                "eval" => !before.is_some_and(|c| c.is_ascii_alphabetic()),
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:                "Eval" | "EVAL" => !before.is_some_and(|c| c.is_ascii_uppercase()),
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// (`Eval…`, `…_EVAL_…`) nor an expanded product name (`EvaluationRun`,
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// `evaluation_runs`, `/evaluations`) — mirroring the `check:deps` vocabulary
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:                    if word == "EVAL" {
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// letter, `Eval`/`EVAL` after no uppercase letter (a camel boundary). It is
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:        .match_indices("eval")
./crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:/// product name (`EvaluationRun`, `evaluation_runs`), as opposed to prose or a
./crates/wyrd/wyrd-server/wyrd-ui/src/lib/features/observe/core/ObserveJourney.test.ts:  expect(html).not.toContain('/observe/evaluations');
./crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:    json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } })
./sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:    assert all(table["namespace"] != "vala.eval" for table in bifrost.list_tables())
./sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:        assert refusals("vala.eval", name) == refusals("vala.absent", name), name
./sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:            f"py_evals_{action}", [{"resource": "evals", "action": action, "scope": "all"}]
./sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:    assert not hasattr(Observe, "eval")
./sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:    emits, and the Eval media module are removed with no alias."""
./sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:        importlib.import_module("wyrd.eval")
./sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:    """The verify/submit types live only in ``wyrd.verify``; Judgment, drift/eval
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:            .expect_err("a stale evals grant is no grant");
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:                &format!("evals_{action}"),
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:                &[json!({ "resource": "evals", "action": action, "scope": "all" })],
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:        let retired = refusals("vala.eval", name).await;
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:        listed.iter().all(|table| table.namespace != "vala.eval"),
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:/// `server_error` for a role that no longer decodes — and the retired Eval
./sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:/// whose stored role still grants `evals:run` or `evals:invoke` is refused by
./sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:    const credential = server.scopedApiKey(`evals_${action}`, [{ resource: "evals", action, scope: "all" }]);
./sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:    const retired = await refusals("vala.eval", name);
./sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:  expect((await bifrost.listTables()).every((table) => table.namespace !== "vala.eval")).toBe(true);
./sdks/wyrd-sdk-ts/wyrd/tests/unit/observe.test.ts:    expect("eval" in Observe.prototype).toBe(false);
EOF
)
eval_scope=(--hidden -g '!.git' -g '!changes/**' -g '!**/migrations/**')
hits=$(rg --files "${eval_scope[@]}" . | rg "$eval_vocab" || true)
while IFS= read -r hit; do
  line=${hit#*:}
  record="${hit%%:*}:${line#*:}"
  rg -qxF -- "$record" <<<"$eval_allowed" && continue
  sed -E 's/eval "\$|literal_eval\(|eval_metric=//g' <<<"$record" | rg -q "$eval_vocab" &&
    hits+=$'\n'"$hit"
done < <(rg -n "${eval_scope[@]}" -g '!scripts/checks/deps.sh' "$eval_vocab" . || true)
if [[ -n "${hits#$'\n'}" ]]; then
  echo 'FAIL: retired Eval vocabulary; use verification terms:'
  echo "${hits#$'\n'}"
  exit 1
fi

# Inline secrets exist only under wyrd-spec's test-utils feature.
cargo nextest run --locked -p wyrd-spec --no-default-features --features server \
  --test security_inline_gate
