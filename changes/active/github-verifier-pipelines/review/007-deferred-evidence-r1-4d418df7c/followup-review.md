# Focused follow-up: ignored deferred-evidence journey routing

Result: **RESOLVED**. BEH-002's missing owning-lane claim is supported. The standards review's claim that these ignores follow dedicated journey-lane conventions does not establish an actual lane and is not supported for these four tests.

Immutable subject: base `16ea9694c34a5ec1867306bd5e34df62f0888611`, candidate `4d418df7c23ee03a976f89349b74debdddee72c6`. HEAD still identifies that candidate. First-round focused discovery only; no other acceptance paths reopened. CodeGraph exploration preceded configuration/source inspection. Read `scope.md`, the original task's connected proof/obligations, applicable root Testing rules, test registration, nextest profiles, task runners and CI routing.

## Scope and source evidence

The four functions in `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs` are each newly marked `#[ignore = "requires the repository-managed Postgres journey lifecycle"]` immediately before their definitions: running-consumer at 499–500, restart at 549–550, compute-before-ACK at 598–599, and admission deadline at 674–675. The module is registered only in the `wyrd-server` integration target (`tests/integration/main.rs:14`). No re-export, include or second target executes these function bodies.

| Possible owning route | Actual selection | Outcome |
|---|---|---|
| `test:wyrd` / family aggregate | `mise.toml:75–84` provisions repository Postgres, then `scripts/run-family-tests.sh:44` invokes `cargo nextest run --locked` on family packages without `--run-ignored` | Includes the binary, skips these four tests |
| `test:bifrost:gate` | `mise.toml:323–340` runs ignored tests, but only selects wyrd-client, wyrd-testing, wyrd-mcp and vala-bifrost-redux packages/targets | Does not select wyrd-server |
| `test:bifrost:integration:server` | `mise.toml:380–383` selects only pg_grpc_ingest_smoke, pg_grpc_smoke, pg_merge_http_protected, pg_router_smoke and pg_verification_runtime; no `--run-ignored` | Excludes pg_deferred_evidence and would skip its ignored tests regardless |
| `test:bifrost:journey:server` | `mise.toml:623–630` runs `wyrd-testing --test server -P journey --run-ignored=all` | Different package/binary, not this module |
| Rust SDK cards/journey lanes | `mise.toml:106–108` selects `wyrd-ai --test integration`, including its separate deferred_evidence module | Exercises SDK stories, not these server lifecycle tests |
| Other ignored wyrd-server routes | Live Operator smoke, identity journey and identity UI selectors (`mise.toml:399,732,768`) explicitly select unrelated tests/modules | Do not select these four tests |
| Nextest profiles | `.config/nextest.toml` default/journey settings configure timeouts, concurrency and grouping; no setting admits ignored tests automatically | Does not change selection |
| Full gate / CI | `mise.toml:1519–1561` composes family, Bifrost and journey routes above; nightly and lints workflows consume these task shards | No additional ignored server test route |

Also inspected `scripts/run-bifrost-tests.sh` and its journey runner: the standalone complete Bifrost path delegates to the same explicit integration and journey owners and does not supply an omitted pg_deferred_evidence selector. Searches of task/script/workflow registration found no additional selector for this module or these function names. A one-off implementation-time invocation with `--run-ignored` is evidence of that run, not an owning normal verification lane.

## Material proposal and correction boundary

**FOLLOW-007-01 / BEH-002 — VIOLATION.** Root Testing forbids skipping a gate with `#[ignore]`; task AC-006/007 requires the named running/restart/resource/deadline outcomes. The candidate's normal family and aggregate lanes can report success while silently skipping all four new server-level proofs. Their exact tested behavior is therefore unprotected in ordinary verification. This is a concrete routing omission for explicit acceptance proof, not a request for additional crash cases or exhaustive recovery tests.

The smallest correction is removal of these four new ignores: the existing `test:wyrd` lane already owns the repository-managed Postgres lifecycle cited as the ignore reason and selects this package's integration binary. Preserve assertions, existing timeout/deadline semantics and the approved ordinary-restart journey. If a demonstrated environment requirement necessitates a dedicated route, wire the existing environment-owning route to these exact tests instead; no new harness, broad suite or test-only hook is needed. Source inspection of routing plus focused execution of the four existing tests under repository Postgres proves closure; full change suites belong to change review.

## Verification limits

This is independent static source validation of the conflicting routing claim, not final findings validation. No Cargo, Postgres or CI run was executed; no source/Git changes were made. Only this report was written. The parent recorded Docker denial, so actual runtime outcomes remain unverified in this review environment.
