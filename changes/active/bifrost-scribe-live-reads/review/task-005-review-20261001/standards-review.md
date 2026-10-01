# TASK-005 repository standards review

Subject: `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` (59 changed files). `1f1cbcf5f` is excluded. Review is read-only. Overall result: **FAIL**.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage |
| --- | --- | --- |
| Gate and OTLP HTTP/gRPC transport | `AGENTS.md` §§3–6, 9, 11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md` telemetry and query boundaries; references `architecture/patterns.md`, `languages/rust-core.md`, `domain/telemetry-observations.md`, `domain/vala-architecture.md` | Gate operation and stream instrumentation, transport error mapping, server metrics registration, test consumers |
| Scribe ingest, WAL, memory, staging, persistence | Same repository and Rust authorities; `architecture/bifrost-design.md` ingest/telemetry; references `domain/analytical-operations-reliability.md`, `domain/olap-serving.md`, `domain/telemetry-observations.md` | New row counter, backlog restoration, lane gauges, span changes, existing and new owner/journey tests |
| Oracle planning, admission, execution, peer streaming | Same repository and Rust authorities; `architecture/bifrost-design.md` query/telemetry; references `domain/datafusion.md`, `domain/iceberg.md`, `domain/olap-serving.md`, `domain/analytical-operations-reliability.md` | Scan facts, pruning, admission labels/gauges, query and peer stream spans, journey consumers |
| Shared storage and Forge/catalog | Same repository and Rust authorities; `architecture/bifrost-design.md` storage, maintenance, telemetry; references `domain/iceberg.md`, `domain/analytical-operations-reliability.md`, `domain/telemetry-observations.md` | Cache/request owner inspection, metric emission, settled task results and fallback durable reads |
| Test harness, benchmark binding, documentation, task evidence | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md`; references `languages/testing-workflows.md`, `languages/spec-driven-development.md`, `languages/maintainer-style.md`; applicable Bifrost design and docs authority | Focused tests, metric binding and captures, docs updates, reported command evidence |

No Python, TypeScript, public wire, generated schema, dependency manifest, or `mise.toml` change appears in the diff. Their specialized rules do not independently apply.

## Applicable rules

| Rule | Result | Evidence |
| --- | --- | --- |
| Correct owner and layer (`AGENTS.md` §§3, 5, 9; `architecture/agent-rules.md`; Bifrost design) | PASS | Production telemetry is changed in Gate/Scribe/Oracle/storage/Forge owners; server edits remain transport/metrics; no client-tier or `wyrd-spec` implementation moved. |
| Closed, bounded metric labels; owner-derived facts; separated request, stream, and durable boundaries (Bifrost design telemetry; telemetry reference) | PASS for repository-rule scope | Changed labels are closed operation/outcome/class/stage values; the architecture update documents attempts, restored backlog, Gate versus Oracle durations, and durable task authority. Task-acceptance correctness is for the implementation reviewers. |
| Rust documentation for materially changed items (`AGENTS.md` §16) | PASS on inspected new owners and methods | New `ForgeSettledAttempt`, stream span helper, storage owner inspections, staging backlog and tests carry intent docs; fallible changed methods inspected carry `# Errors` where applicable. |
| Module-scope imports only (`architecture/agent-rules.md`) | **FAIL** | Three newly added function-local ordinary imports appear at `scribe/tests/wal_closeout.rs:292`, `wyrd-testing/src/bifrost/scribe_workload.rs:728`, and `wyrd-testing/src/bifrost/telemetry.rs:3415`. See STD-001. |
| Tests at the owning runtime/tier and exact focused commands (`AGENTS.md` §11; agent rules; testing reference) | PASS for recorded focused runs | Scribe/Oracle/Forge scenarios use real-server `wyrd-testing` journey targets; restored staging uses Postgres in `pg_tests`; storage logic uses inline unit tests. Candidate evidence records exact selectors and passing exits. |
| Broad verification for intentionally cross-owner work (`AGENTS.md` §11) | **FAIL** | This diff crosses Scribe, Oracle, Forge, shared storage, server, and shared test infrastructure. The candidate evidence explicitly records that `mise run gate` was not run. See STD-002. |
| No gate circumvention or generated-artifact hand edits (agent rules; `AGENTS.md` §12) | PASS | Diff adds no `#[allow]`, `#[ignore]`, generated schemas, or stubs; no deleted test is used to conceal a red gate in the inspected diff. |
| Formatting and lint/documentation lanes (`AGENTS.md` §11) | PASS on supplied evidence | Candidate records passing `mise run fmt`, `mise run lints`, `mise run docs:check`, and `git diff --check`. |

## Material findings

### STD-001 — Newly added local `use` statements violate the module import rule

- **Rule:** `architecture/agent-rules.md` requires all ordinary `use` statements at the top of the module; its only function-local exception is `use TraitName as _` needed for a generic function.
- **Locations:** `crates/vala/vala-bifrost-redux/src/scribe/tests/wal_closeout.rs:292`; `crates/wyrd/wyrd-testing/src/bifrost/scribe_workload.rs:728`; `crates/wyrd/wyrd-testing/src/bifrost/telemetry.rs:3415`.
- **Evidence/consequence:** The first duplicates the already present module import at line 12. The other two hide enum dependencies inside methods and make the source module's dependency block incomplete. The candidate diff added each line; none meets the documented exception.
- **Testable correction:** Remove the duplicate, move the other imports to their module import blocks (or qualify the enum names), then run format and lint checks.

### STD-002 — Required broad repository gate has no result

- **Rule:** `AGENTS.md` §11 calls for `mise run gate` when a change intentionally crosses several ownership boundaries without a complete capability gate; §12 requires applicable checks to pass before completion.
- **Location:** TASK-005 candidate implementation evidence, lines 414–419, states the gate was not run; the diff spans `vala-bifrost-redux`, `wyrd-telemetry`, `wyrd-server`, `wyrd-testing`, and architecture/docs.
- **Consequence:** Focused scenarios and module unit tests do not establish whole-repository cross-owner compatibility. This is a verification finding, not an instruction to change production code.
- **Testable correction:** Run `mise run gate` on the immutable candidate in an isolated checkout/worktree and record the exit and any diagnosed failure. The user-supplied original task also requires whole journey lanes and the standard capacity benchmark; those are task-verification obligations for the implementation review, not additional repository-rule findings here.

## Verification limits

This standards review inspected source and the recorded results; it did not run gates. The user-supplied original TASK-005 at `/home/thorrester/Documents/GitHub/wyrd-pr-95/...` requires benchmark, whole journey, and broad gate verification, while the task file in the candidate repository explicitly omits those runs. The orchestrator should reconcile that authority difference against the user-designated original task. No conclusion here relies on the implementation evidence's `PASS` labels as proof of behavior.
