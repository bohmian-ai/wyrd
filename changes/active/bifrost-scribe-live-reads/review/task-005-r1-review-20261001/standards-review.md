# TASK-005 R1 repository standards review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65` (immutable HEAD checked). **Result: FAIL.** The user explicitly deferred `mise run gate` to another branch; its absence is excluded from this verdict. `.codegraph/` is absent. This audit uses the complete cumulative diff, routed repository authorities, candidate source, and retained verification outputs; it does not use another R1 reviewer's conclusions.

## Authority coverage

| Changed surface | Applicable authority and coverage |
| --- | --- |
| Scribe ingest, staging, admission, WAL, persistence, and telemetry | `AGENTS.md` §§3–6, 10–12, 16; `architecture/agent-rules.md` (owner shape, rustdoc, import placement, tests, gate integrity); `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`; `architecture/references/{architecture/patterns.md,languages/rust-core.md,languages/testing-workflows.md,domain/telemetry-observations.md,domain/analytical-operations-reliability.md}`. Inspected cumulative diff, stage owner and recovery tests. |
| Oracle/Gate query, admission, pruning, and OTLP server edge | Same Rust and Bifrost authorities plus `architecture/wyrd-security-posture.md`, `architecture/references/domain/{olap-serving.md,datafusion.md}` and `languages/errors.md`. Inspected cumulative diff, typed errors, transport projection and journey selectors. |
| Forge maintenance, catalog, Iceberg, and shared storage | Same Rust and Bifrost authorities plus `architecture/references/domain/{iceberg.md,analytical-operations-reliability.md}`. Inspected cumulative diff, owner/consumer and Forge journey surfaces. |
| `wyrd-sql::tenant_conn::begin_bound` release-build repair | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `architecture/agent-rules.md` (TenantConn transaction boundary, tenancy, imports, async and docs); `architecture/wyrd-security-posture.md`; `architecture/references/{architecture/patterns.md,languages/rust-core.md,languages/testing-workflows.md}`. Inspected the entire six-line diff and surrounding transaction owner/call path as described by retained compiler diagnosis. |
| `wyrd-telemetry` test capture, `wyrd-testing` support and real-server journeys | `AGENTS.md` §§3–4, 11–12, 16; `architecture/agent-rules.md` (test tier/runtime and exact commands); `architecture/references/{languages/testing-workflows.md,domain/telemetry-observations.md}`. Inspected capture diff, new Scribe restart/concurrency tests, journey results and benchmark reports. |
| Architecture/docs, task evidence, review packet, backlog dashboard spec | `AGENTS.md` §§1–2, 11–12, 14, 16; `architecture/agent-rules.md`; `architecture/references/{languages/spec-driven-development.md,languages/implementation-execution.md,languages/testing-workflows.md}`; `architecture/wyrd-design.md`, `architecture/bifrost-design.md`. Inspected changed-file list, task evidence, review packet, benchmark report and `docs:check` record. |

## Applicable rule results

| Rule | Result and source evidence |
| --- | --- |
| Server owners retain durable behavior; typed tenant and SQL boundaries remain intact (`AGENTS.md` §§2–3, 9, 15; agent rules) | **PASS** on inspected changes. The R1 SQL fix only boxes the existing `pool.begin_with(AssertSqlSafe(statement))` future in `tenant_conn.rs:116`; it does not change its transaction statement, error mapping, role, or commit ownership. Telemetry remains under Vala/server and shared test capture remains behind `test-support`. |
| Struct-centered Rust, narrow async, no speculative new dependencies/features (`AGENTS.md` §§4–6, 15; agent rules) | **PASS** on inspected changes. Staged backlog publication lives on `ScribeStagingRuntime` and its assembler guard; no added Cargo feature or dependency appears in the cumulative diff. The boxed SQL future awaits real IO. |
| Module-top imports and bare types in signatures (agent rules) | **PASS.** R1 moved the previously identified local ordinary imports and changed fully qualified signature types to module imports and bare names; the new `pg_tests` imports are at that test module's top, which the rule expressly permits. |
| Rustdoc on changed items; fallible/cancellation contracts (`AGENTS.md` §16; agent rules) | **PASS** on inspected R1 symbols: `begin_bound` documents the release layout reason and `# Errors`; stage owner and recovery/concurrency test changes carry intent documentation. No new undocumented public or private R1 item found in inspected source. |
| Correct test tier and exact test commands (`AGENTS.md` §11; agent rules) | **PASS.** Concurrency is an inline owner test; Postgres recovery is in `pg_tests`; abrupt restart is an ignored real-server journey run through repository-managed setup. Retained output records `mise run test:bifrost:journey` with 147 passes and no failures, plus targeted SQL tests. |
| No weakened gate, generated hand edit, or hidden failure (`AGENTS.md` §12; agent rules) | **PASS** for source edits. The new journey uses the established environment `#[ignore = "requires Postgres and object storage"]` marker and is run with `--run-ignored=all`; no new production `#[allow]`, skipped assertion, generated stub, or schema edit appears. `git diff --check` passes. |
| Format, lints, docs and required scoped checks on the final tree (`AGENTS.md` §§11–12) | **FAIL** for final lint evidence. `mise run fmt`, `mise run lints`, and `mise run docs:check` are recorded through `6d5cad774`; `49f3b601d` then changed `wyrd-sql`, and only `cargo fmt -p wyrd-sql --check` is recorded after it. See STD-R1-002. The user override removes the broad `gate` requirement only. |
| Red verification gate blocks completion; diagnose its failure (`AGENTS.md` §12) | **FAIL.** `mise run bench:bifrost:query-capacity` exited 1 and its retained candidate report marks selective target FAIL, 9.3 ms p50 at 1 client versus 7 ms. The host-contention explanation is expressly unproven and no quiet rerun exists. See STD-R1-001. |

## Material findings

### STD-R1-001 — The required benchmark has a red target

- **Rule:** `AGENTS.md` §12: a red gate blocks completion, regardless of who turned it red; diagnose and fix the failure itself rather than attributing it elsewhere.
- **Location/evidence:** `changes/active/bifrost-scribe-live-reads/review/task-005-review-20261001/r1-outputs/bench-candidate/report.md`, selective target row; `TASK-005-R1-close-telemetry-proof.md` verification records exit 1 and says host contention is unproven. The earlier 6.8 ms sample was at `f0365f9ea`, not the specified base.
- **Consequence:** The candidate does not yet have credible proof that the approved query-capacity target survives the telemetry change. Passing journeys do not exercise this latency target.
- **Testable correction:** Run the existing standard benchmark once on a quiet, isolated host without changing threshold or harness; retain its report. If the target still fails, diagnose the changed query path and correct the cause, then rerun the same benchmark. No new benchmark matrix is required.

### STD-R1-002 — Final-tree lint check is not recorded

- **Rule:** `AGENTS.md` §11 requires `mise run lints` for Rust changes and §12 requires applicable checks to pass on the completed implementation.
- **Location/evidence:** The R1 packet records `mise run lints` passing only through `6d5cad774`. Commit `49f3b601d` then modified `crates/wyrd/wyrd-sql/src/tenant_conn.rs`; only `cargo fmt -p wyrd-sql --check` is recorded afterward. The latest commit is `1fc68f3b7`.
- **Consequence:** The final Rust source has no recorded workspace Clippy result.
- **Testable correction:** Run `mise run lints` on the cumulative candidate and record the exit; correct any resulting diagnostic without suppressing it.

## Limits

I did not rerun heavyweight lanes. The user-directed gate deferral is honored. This is a repository-rules audit; the task reviewers decide whether the extra backlog dashboard spec and SQL build repair violate task scope. The benchmark diagnosis remains an inference until a controlled run or source-backed alternative explains the failed target.
