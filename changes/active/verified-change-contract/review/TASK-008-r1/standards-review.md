# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Range: `ce5c09ef3..852894689`
- Approved authority consulted: `changes/active/verified-change-contract/spec.md` revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry
- Task evidence consulted: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- CodeGraph: not applicable; the repository has no `.codegraph/` directory.

This report audits repository standards only. It does not decide whether the implementation satisfies the task acceptance criteria and does not perform a Ponytail audit.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Active task record and revision-57 implementation evidence | `AGENTS.md` §§12, 14; `architecture/references/languages/spec-driven-development.md` (authority, task contract, task status, test-command precision); `architecture/references/languages/implementation-execution.md` (authority, execution record, focused verification, completion standard) | **FAIL** — the appended revision-57 implementation record is governed by frontmatter that still declares revision 49, `proposed`, and `SPEC_REVISION_REQUIRED`; see STD-001. The final evidence otherwise records focused tests, broader lint/format proof, a smoke run, risks, and non-goals. |
| Consolidated `capacity` benchmark binary (`main.rs`, `load.rs`, `step.rs`, `evidence.rs`, `report.rs`, `fixture.rs`, `judge.rs`, `profile.rs`) | `AGENTS.md` §§4–6, 11, 15–16; `architecture/agent-rules.md` (struct-centered Rust, rustdoc, async boundary, imports); `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/bifrost-design.md` (ingest/query/admission/telemetry); `architecture/operations/deployment-and-release.md`; `architecture/operations/reliability-and-recovery.md` | **FAIL** — imports, concrete domain enums/records, bounded driver concurrency, typed clients, public-client paths, structured errors in a binary, and test/module placement conform. The top-level benchmark lifecycle remains free-function orchestration rather than owned behavior (STD-002), and cancellation/partial-progress documentation is absent from material async workflows (STD-003). |
| Release-process server harness (`release_server.rs`) | `AGENTS.md` §§6, 11, 16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md`; `architecture/operations/deployment-and-release.md`; `architecture/operations/reliability-and-recovery.md` | **PASS with verification limit** — joined-replica ports are explicitly owned by the harness, readiness preserves the last body for diagnosis, abnormal drop retains logs, and the process remains bounded by the existing cgroup/lifecycle owner. The supplied evidence contains a reduced benchmark smoke run, not an isolated regression test for the new joined-port mapping or retained-log behavior. |
| `wyrd-testing/Cargo.toml` and `mise.toml` benchmark task consolidation | `AGENTS.md` §§1, 4, 11–12, 15; `architecture/agent-rules.md` (features/tasks, gate integrity); `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | **PASS** — no dependency or Cargo feature was added, the two superseded targets/tasks were removed, the replacement uses pinned Cargo commands inside the repository-managed Postgres/storage environment, and the benchmark remains opt-in rather than being smuggled into a fast lane. |
| Queue default documentation (`wyrd-queue/src/config.rs`) | `AGENTS.md` §§4, 16; `architecture/agent-rules.md` rustdoc rule; `architecture/references/languages/rust-core.md` | **PASS** — the edit only updates the named owning benchmark and preserves the documented queue behavior and default. |
| Server/Postgres runtime integration tests (`pg_verification_runtime.rs`) | `AGENTS.md` §11; `architecture/agent-rules.md` external-test and Postgres-lane rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/maintainer-style.md` | **PASS** — the external target is earned by a real Postgres-backed runtime seam, test rustdoc explains why a process journey cannot observe the claimed invariants, and exact setup-wrapped commands are recorded. No test was weakened or ignored. |
| Rust SDK journey changes (`drift_verification.rs`, `observe_run.rs`) | `AGENTS.md` §§8, 11–12, 16; `architecture/wyrd-design.md` doctrine 20 and client model; `architecture/wyrd-doctrine.mdx` public-surface boundary; `architecture/references/languages/testing-workflows.md`; `architecture/references/domain/olap-serving.md` | **PASS** — the tests drive the public Rust SDK against a real server, retain the owning language runtime, cover the added failed judge case and durable ingest/read-back behavior, and use the repository's established ignored-journey lane rather than weakening a required test. |
| Deletion of the prior verification/ingest benchmark sources | `AGENTS.md` §§12, 15; `architecture/references/languages/implementation-execution.md` (diff audit and no gate weakening); `architecture/references/languages/testing-workflows.md` | **PASS** — deleted benchmark-only assertions are replaced by focused unit/integration/journey proof in the candidate, and no production contract, generated artifact, or boundary gate was removed. |

## Rule-by-rule results

| Rule | Evidence | Result |
|---|---|---|
| A spec-driven implementation task names the approved spec revision, mapped obligations, and current lifecycle status. | `task-008-closeout.md:4-9` remains `status: proposed`, `planning_result: SPEC_REVISION_REQUIRED`, `spec_revision: 49`, while `task-008-closeout.md:1485-1571` records a revision-57 implementation and claims `IMPLEMENTED`. | **FAIL** (STD-001) |
| Stateful, multi-step workflows and dependency-backed orchestration have one concrete owning struct and inherent methods. | `capacity/main.rs:113-279` is a free `benchmark` workflow that owns setup, credentials, servers, tenants, deployment mutation, step sequencing, teardown, and report persistence; `capacity/main.rs:289-300` further mutates `Deployment` through a free helper. | **FAIL** (STD-002) |
| Every new or materially modified async/durable Rust operation documents cancellation and partial progress when relevant. | `capacity/main.rs:106-113`, `capacity/fixture.rs:203-209`, `capacity/load.rs:240-249`, and `capacity/step.rs:138-142` document errors but not cancellation despite process creation, durable registration/emission, spawned requests, flushing, and profiling side effects. | **FAIL** (STD-003) |
| Async is limited to IO/composition that awaits IO. | The candidate's async methods await HTTP, Postgres, filesystem-backed server lifecycle, client flush/shutdown, timers, or spawned request completion; pure report/math helpers remain synchronous. | **PASS** |
| Fallible Rust functions document errors; panic-bearing tests document panics. | New fallible functions carry `# Errors`; new tests in `capacity`, `pg_verification_runtime.rs`, and `observe_run.rs` carry `# Panics`. | **PASS**, subject to STD-003's separate cancellation requirement. |
| External tests earn their binaries and environment-dependent tests use the repository-managed lanes. | Server tests compose real Postgres-backed runtimes; SDK tests start a real `WyrdTestServer`; task evidence records Postgres wrappers and exact `mise exec -- cargo nextest` expressions. | **PASS** |
| No gate is weakened or hidden with `allow`, deleted assertions, or inappropriate ignores. | No new `allow`; the new SDK journey uses the existing `#[ignore = "requires the repository-managed Postgres journey lifecycle"]` convention and has a recorded `--run-ignored=all` command; removed benchmark correctness checks have focused replacements. | **PASS** |
| Test and build commands use the repository-pinned toolchain and appropriate feature policy. | `task-008-closeout.md:1502-1542` records exact Postgres-wrapped named tests, `mise exec` Clippy/nextest commands, and `mise run fmt`/`lints`; focused journey tests use `-P journey`, not `--all-features`. | **PASS** |
| Client code uses the shared public client instead of reaching into server/Vala owners. | The benchmark uses `Verification`, `WyrdState`, and `Bifrost`; the SDK journeys import through the Rust SDK/client surface. Direct SQL is confined to benchmark evidence collection, not workload generation. | **PASS** |
| Capacity work preserves bounded concurrency/backpressure and production-shaped topology. | `load.rs:40` fixes driver permits; `Lane::drive` uses `try_acquire_owned` and counts missed arrivals; the harness uses release servers, shared Postgres/object storage, peer TLS, and default client queues. | **PASS** |
| No new dependency, Cargo feature, public/wire contract, migration, generated artifact, or compatibility surface entered the diff. | Manifest changes only replace bin targets; no dependency or feature line was added; changed files contain no migrations or generated artifacts. | **PASS** |
| Secrets and diagnostic output remain bounded and redacted. | Credentials use `SecretString`; the shared signing key is written to the temporary benchmark directory; reports store binary/envelope/resource evidence, not credentials; retained abnormal logs print only their path. | **PASS** |

## Material findings

### STD-001 — Task authority was not advanced to the revision being implemented

- Governing rule: `architecture/references/languages/spec-driven-development.md` “Task contract” and task-status lifecycle; `AGENTS.md` §14.
- Location: `changes/active/verified-change-contract/tasks/task-008-closeout.md:4-9`, conflicting with the appended revision-57 record at `:1485-1571`.
- Evidence: the task still declares `status: proposed`, `planning_result: SPEC_REVISION_REQUIRED`, `spec_revision: 49`, and omits REQ-171/AC-040 from its mapped requirements, even though the same artifact now records “bench:capacity implementation (revision 57)” and ends `IMPLEMENTED`.
- Consequence: an implementation or reviewer following the required authority order cannot tell from the task contract that revision 57 authorized this work or that it reached review; automation and later remediation also inherit the wrong requirement mapping.
- Testable correction: update this task's contract to the approved revision and current review status, map REQ-171/AC-040/AC-041 explicitly, and retire the obsolete `SPEC_REVISION_REQUIRED` state while preserving the historical narrative in the body. Static inspection must show one unambiguous revision-57 authority before review resumes.

### STD-002 — The benchmark lifecycle is free-function orchestration instead of owned behavior

- Governing rule: `AGENTS.md` §5 Required Struct-Centered Rust Style; `architecture/agent-rules.md` struct-centered hard criterion; `architecture/references/languages/rust-core.md` Required Structural Style.
- Location: `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:113-279` and `:289-300`.
- Evidence: `benchmark` coordinates configuration, temporary resources, TLS/judge/signing dependencies, server and tenant lifecycle, mutable deployment/client state, step sequencing, shutdown, and report persistence as a free function. The free `connect` helper repeatedly consumes and mutates the natural `Deployment` owner.
- Consequence: lifecycle invariants and cleanup/partial-progress behavior are split between module-level functions and `Deployment`; a maintainer changing replica connection or teardown must reconstruct which function owns each phase, contrary to the repository's mandatory discoverable-method shape.
- Testable correction: place the benchmark lifecycle on one meaningful concrete owner that holds its configuration and lifecycle dependencies, and move client reconnection onto the existing `Deployment` owner. Keep `main` as the thin process boundary. Existing capacity unit tests and the smoke command must remain green.

### STD-003 — Material async workflows omit required cancellation and partial-progress rustdoc

- Governing rule: `AGENTS.md` §16 and `architecture/agent-rules.md` Rust documentation hard criterion; `architecture/references/languages/rust-core.md` Documentation.
- Locations: representative workflow boundaries are `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:106-113`, `capacity/fixture.rs:203-209`, `capacity/load.rs:240-249`, and `capacity/step.rs:138-142`.
- Evidence: these docs contain `# Errors` but no cancellation/partial-progress contract. Cancellation can occur after server processes start, after some Cards and keys are durably registered, while `JoinSet` requests may already have reached the server, after client queues admit observations, or while profiles and step evidence are only partly collected.
- Consequence: maintainers cannot safely determine what cleanup runs, which external effects may survive, whether a cancelled step can be retried as the same measurement, or whether partial results are intentionally discarded. This is especially material for a benchmark whose verdict depends on complete request accounting.
- Testable correction: add substantive cancellation/partial-progress sections to every new or materially modified async workflow where external effects can precede cancellation, including the listed owners and their shutdown/reconnect boundaries. State what is aborted, what is cleaned by owned drops, what durable server state may remain, and whether a partial step/report is usable. Rustdoc/lints and the focused capacity tests must remain green.

## Verification reviewed and limits

Reviewed recorded evidence:

- `mise run fmt` — clean.
- `mise run lints` — clean.
- `mise exec -- cargo clippy --locked -p wyrd-testing --all-targets --all-features -- -D warnings` — clean.
- `mise exec -- cargo clippy --locked -p wyrd-sdk-rust --all-targets --all-features -- -D warnings` — clean.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` — 6 passed.
- Exact repository-managed Postgres commands for the two runtime integration tests and two Rust SDK journeys — recorded PASS.
- Reduced-duration `mise run bench:capacity` smoke run — exercised the full step topology and exited 1 for the documented short-window sample/traffic reasons.
- `git diff --check ce5c09ef3..852894689` — clean during this review.

Limits:

- The default 30-minute benchmark was not run for this candidate; only the reduced smoke run is recorded. Whether that is sufficient acceptance evidence belongs to the task-acceptance reviewers, not this standards audit.
- No isolated regression test is recorded for joined-replica low-port selection or abnormal `LocalServer` log retention; the smoke run supplies integration coverage for joining and clean shutdown only.
- This review did not rerun Cargo-backed checks, to avoid overlapping shared-target work during the delegated review.

## Overall result

**FAIL**

The candidate has three material repository-standard violations: an ambiguous/stale task authority contract (STD-001), noncompliant free-function lifecycle orchestration (STD-002), and missing required cancellation/partial-progress documentation on material async workflows (STD-003).
