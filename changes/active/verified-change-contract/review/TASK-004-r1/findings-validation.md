# TASK-004 R1 Structured Ponytail Validation

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Wave 1 inputs: `task-review.md`, `standards-review.md`, `domain-review-security-tenancy-audit.md`, `domain-review-persistence-concurrency.md`, and `domain-review-bifrost-publication.md`

The candidate remained `49ad24707de47378b9df51764034c49a129c6b8b` throughout validation. `.codegraph/` is absent, so caller and implementation tracing used repository search and direct source inspection.

## Wave 1 disposition

| Wave 1 source ID | Disposition | Independent validation |
|---|---|---|
| Task review: empty proposed ledger | **REVISED** | The acceptance matrix is substantially supported, but the independently reachable repository-rule, security, shutdown, and required-proof gaps below prevent an empty final ledger. |
| `STD-004-R1-001` | **CONFIRMED** | `VerificationRuntimeBuilder::build` is the only production constructor and copies one app `PgPool` into `VerificationScheduler`, `ResultPublisher`, and `RunnerPools`; their tenant operations call `TenantConn::acquire` directly. The fixture repeats the same propagation. This is exactly the raw-pool field/signature shape prohibited by `architecture/agent-rules.md`. |
| `STD-004-R1-002` | **CONFIRMED** | All three result builders create anonymous positional `Vec<ArrayRef>` values; `finish` independently obtains `T::arrow_fields()` and joins the vectors only by index. `RecordBatch::try_new` checks count/type, not semantic field identity, so a same-typed reorder is reachable silent evidence corruption. |
| `STD-004-R1-003` | **CONFIRMED** | The cited changed fields, signatures, return types, and bounds use qualified paths despite the explicit top-level-import/bare-name rule. These are compile-valid but repository-invalid source shapes; no runtime redesign is required. |
| `SEC-001` | **CONFIRMED** | Native gRPC calls `Gate::authorize_record_write` before Scribe decodes the Arrow frame. Gate checks permission/kind/table only and records `allowed`. `validate_card_scope` then expressly admits an absent column and null rows. Reserved SYSTEM result writes can therefore persist with null `card_uid`, contrary to `REQ-086` and `AC-023`, and an out-of-scope value receives the wrong canonical audit outcome even when Scribe later refuses it. |
| `PERSIST-001` | **REVISED** | The scheduler awaits a whole pass before observing cancellation, and the runner checks cancellation before, but not across, the awaited tenant claim and commit. Both paths can commit and the runner can spawn work after shutdown admission closed. The finding is retained, but its correction is narrowed to cancellation at the existing transaction boundary plus fenced release/refund only when commit wins the race; no new claim protocol is needed. |
| `BIFROST-PUB-001` | **CONFIRMED** | The only role-separated runtime journey enqueues a direct unscored Drift result, emits no detail batch, selects only `verdict`, and cannot prove the exact SYSTEM/Verifier/subject/owner/binding identity contract explicitly required by `AC-023`. |
| `BIFROST-PUB-002` | **CONFIRMED** | The summary-refusal test is a clean pre-send error, while the runner-crash test crashes before publication. No test combines a durable detail ACK with loss of the in-flight summary publisher and verifies fenced run/dispatch state across reclaim, although TASK-004 Scenario 5 explicitly requires that crash proof. |

No Wave 1 material finding was rejected. The retained issues are independent: the two Bifrost findings are missing acceptance proofs, while the security and positional-mapping findings are implementation defects those proofs would help expose.

## Final deduplicated finding ledger

### FIND-TASK-004-1 — Raw application pools escape the connection owner

- **Wave 1 source IDs:** `STD-004-R1-001`
- **Status:** **CONFIRMED**
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` permits tenant work through `TenantConn` and cross-tenant work through `OperatorPool`, and keeps application-pool ownership/acquisition at the existing Postgres owner.
- **Exact locations:** `crates/wyrd/wyrd-server/src/verification/mod.rs:338-370`; `crates/wyrd/wyrd-server/src/verification/scheduler.rs:33-68,138-149`; `crates/wyrd/wyrd-server/src/verification/runner.rs:55-62,83-127,260-270,360-378,486-503`; `crates/wyrd/wyrd-server/src/verification/publisher.rs:63-101,184-197`; `crates/wyrd/wyrd-testing/src/verification.rs:63-91`.
- **Evidence and reachability:** `VerificationRuntime::builder` is called by server composition and runtime journeys; `build` is the sole constructor of the scheduler, runner, and publisher. It clones `state.postgres.app_pool()` into every owner. Their complete tenant-operation bodies use that stored pool for schedule, claim, Card load, settlement, and token minting. `VerificationFixture::provision` stores another clone used by every fixture operation. These are active production and shared test-library paths, not dormant seams.
- **Observable consequence:** tenant-role acquisition is no longer centralized on the repository's Postgres owner, and each service exposes an unrestricted pool to future methods at a tenant-sensitive boundary.
- **Decision-complete minimum correction:** delete the one-use `RunnerPools` wrapper. Retain `OperatorPool` only for the existing cross-tenant reads, and give the scheduler, runner, publisher, and verification fixture the narrow existing Wyrd Postgres owner used by their composition root; open every tenant transaction through that owner's `tenant_conn` method. Do not add a connection trait, factory, callback abstraction, or new pool wrapper, and do not move commits into queue methods.
- **Focused closure proof:** source inspection/`rg` must show no `PgPool` field or constructor parameter in the changed verification runtime or fixture modules. Run the existing SQL/runtime and role-separated journey lanes that exercise scheduling, claims, settlement, minting, and fixture seeding; no new permanent source check is warranted.

### FIND-TASK-004-2 — Result payload values are attached to schemas by position

- **Wave 1 source IDs:** `STD-004-R1-002`
- **Status:** **CONFIRMED**
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` and `architecture/references/domain/arrow-analytical-interop.md` require name-based mapping whenever schemas can diverge.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/results.rs:233-262,273-310,322-394`.
- **Evidence and reachability:** `ResultPayloadBuilder::build`, called by every completed runner publication, reaches `summary`, `drift_features`, or `eval_items`; each produces anonymous ordered arrays. `finish` separately reads the table owner's field vector and gives both to Arrow positionally. The three table declarations contain adjacent same-typed fields, so a field reorder can silently relabel data while still passing Arrow validation.
- **Observable consequence:** exact Verifier evidence can be stored under the wrong field name after an otherwise valid schema reorder or insertion.
- **Decision-complete minimum correction:** make each authored array carry its field name, then have the existing `finish` boundary validate uniqueness/completeness and order arrays from `T::arrow_fields()` by name before appending the three named correlation columns. Reject missing, duplicate, and unexpected authored names with the existing result-payload error surface. Do not introduce a second schema declaration or a generic mapping framework.
- **Focused closure proof:** add one focused unit test using an independently reordered table schema and assert values remain bound to their names, plus one table-driven negative assertion covering missing/duplicate/unexpected names. Retain the existing result mapping and publication tests.

### FIND-TASK-004-3 — Changed Rust signatures bypass the required import manifest

- **Wave 1 source IDs:** `STD-004-R1-003`
- **Status:** **CONFIRMED**
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires top-of-module imports and bare type names in fields, parameters, returns, trait bounds, and `where` clauses.
- **Exact locations:** `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:305,719,747,796,827,931,979,1008,1035,1077,1124,1165,1191,1214,1257,1296,1321,1340,1399,1459,1546,1578`; `crates/wyrd/wyrd-server/src/verification/clock.rs:77`; `crates/wyrd/wyrd-server/src/verification/mod.rs:98`; `crates/wyrd/wyrd-server/src/verification/publisher.rs:211-217,319,326,338-339,363`; `crates/wyrd/wyrd-server/src/verification/results.rs:55,505`; `crates/wyrd/wyrd-server/src/verification/runner.rs:533,584,588`; `crates/wyrd/wyrd-testing/src/verification.rs:36,39`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:91`; `crates/wyrd/wyrd-sql/tests/pg_verifier_runs.rs:77`; `sdks/wyrd-sdk-ts/native/src/verification.rs:23`.
- **Evidence and reachability:** every cited item was introduced in the candidate and is compiled by an owning crate/test target. The correction changes spelling only; none of the full function bodies or their callers needs behavioral modification.
- **Observable consequence:** the changed code violates a mandatory repository source-shape rule and obscures module dependency manifests.
- **Decision-complete minimum correction:** import the cited types, including collision-resolving aliases where necessary, in each module's existing top-level `use` block and use bare names in the changed fields/signatures/bounds. Do not reorganize modules or introduce aliases that hide domain ownership.
- **Focused closure proof:** inspect the changed Rust diff for qualified types in fields/signatures/bounds, then run `mise run fmt`, `mise run lints`, and the affected crate/typecheck lanes. No behavior test or new lint/check is needed for this mechanical correction.

### FIND-TASK-004-4 — Reserved SYSTEM writes do not require exact Verifier correlation at the canonical decision

- **Wave 1 source IDs:** `SEC-001`
- **Status:** **CONFIRMED**
- **Classification:** INCORRECT / VIOLATION
- **Violated obligation:** `REQ-086`, `REQ-145`, `INV-007`, `AC-023`, and `AC-030` require every reserved result row to carry and be authorized against the token's exact signed Verifier, with one canonical Gate allow/deny audit decision.
- **Exact locations:** `crates/vala/vala-bifrost-redux/src/gate/mod.rs:446-484,864-917`; `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:526-574,715-752`.
- **Evidence and reachability:** public `insert_batch` always reaches `dispatch_native_frame`; after table resolution it audits `allowed` based only on permission/kind/table, then sends opaque Arrow IPC to Scribe. `decode_rows` calls `validate_card_scope`, whose complete body returns success when `card_ref` is absent and skips null rows. The result tables' source fingerprint excludes the optional correlation field, so both shapes are admissible and stamp null `card_uid`; a foreign value is refused only after Gate already recorded `allowed`.
- **Observable consequence:** a live tenant SYSTEM bearer can create unattributed canonical verification evidence by omitting/nulling `card_ref`, and denied scope forgery is recorded as an allowed canonical write decision.
- **Decision-complete minimum correction:** extend the existing native Gate/Scribe admission seam so the reserved-result-table decision validates the decoded batch before appending the one Gate audit row: require exactly one UTF-8 `card_ref` field, require every row non-null, and authorize every parsed value against the SYSTEM principal's sole signed UID-bearing Verifier scope. Feed that result into the existing combined permission/kind/table decision so malformed, absent, null, or foreign scope records one denied decision and never reaches durable admission; exact scope records one allowed decision. Preserve Scribe's existing scope resolution and trusted UID stamping as defense in depth. Do not add another issuer, permission, audit event, or result-specific transport.
- **Focused closure proof:** add authenticated real-gRPC cases for absent, null, malformed/foreign, and exact-scope `card_ref`. Each refusal must produce no durable row and exactly one denied `bifrost_record:write` audit event; the valid case must stamp the signed Verifier UID and produce exactly one allowed event.

### FIND-TASK-004-5 — Shutdown cancellation does not close durable claim admission

- **Wave 1 source IDs:** `PERSIST-001`
- **Status:** **REVISED**
- **Classification:** INCORRECT
- **Violated obligation:** `REQ-146`, `AC-030`, and TASK-004 Scenario 7 require shutdown to stop new scheduler and runner claims immediately and drain only work already admitted.
- **Exact locations:** `crates/wyrd/wyrd-server/src/verification/scheduler.rs:89-105,117-157`; `crates/wyrd/wyrd-server/src/verification/runner.rs:157-201,214-270`.
- **Evidence and reachability:** `VerificationRuntime` is the sole caller of both `run` loops. Scheduler cancellation is first selected only after `report_depth` and all tenant schedule transactions in `pass`. Runner checks `stop` before calling `claim`, but `TenantConn` acquisition, queue claim, and commit can block; after they return the run is unconditionally spawned. Existing shutdown tests begin only after execution entered, so they prove draining, not admission closure.
- **Observable consequence:** cancellation can be followed by a newly committed scheduled run/cursor advance or a newly leased and executing run, consuming the shutdown drain with work admitted after closure.
- **Decision-complete minimum correction:** make the existing cancellation token participate at the durable transaction boundary. In the scheduler, cancellation must drop/roll back an in-progress occurrence transaction and prevent later tenant iterations. In the runner, cancellation must drop/roll back an uncommitted claim; if commit wins the race, immediately apply the existing fenced release/refund transition and do not spawn execution. Preserve schedule insert/cursor atomicity, permit-before-claim ordering, token fencing, and the existing 30-second drain for work spawned before cancellation. Do not add a shutdown table, registry, or second claim protocol.
- **Focused closure proof:** add two real-Postgres lock-controlled tests. Hold the due-binding row across cancellation and prove no run/cursor commit after release; hold a runnable row across cancellation and prove no lease/execution remains after release (or a commit-race claim is fenced-released with its attempt refunded). Keep the existing in-flight drain/release tests.

### FIND-TASK-004-6 — The role-separated journey omits the required result/detail identity proof

- **Wave 1 source IDs:** `BIFROST-PUB-001`
- **Status:** **CONFIRMED**
- **Classification:** MISSING
- **Violated obligation:** `AC-023` and TASK-004 Scenario 5 require the multi-server no-local-Scribe journey to prove exact result and detail identities through remote Gate/Scribe publication and Oracle query.
- **Exact location:** `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:34-120`.
- **Evidence and reachability:** this is the only role-separated verification-runtime journey. It scripts `Drift(None)`, enqueues a direct run, emits no detail batch, queries only `verdict`, and never asserts SYSTEM `principal_id`, Verifier `card_uid`, subject, owner, binding, result/detail join, or shared event time.
- **Observable consequence:** the required production-shaped proof passes even if the remote path misattributes writer/Verifier identity, loses the binding identity split, or cannot publish details.
- **Decision-complete minimum correction:** extend this existing journey and fixture path—no new cluster or harness—to execute one non-empty binding-created Drift result. Query the summary and matching feature rows through Oracle and assert the exact tenant SYSTEM principal, Verifier UID, subject UID, owner UID, binding ID, run ID, result ID join, and common event time while retaining the assertion that the runner node has no local Scribe.
- **Focused closure proof:** the amended ignored role-separated journey itself is the focused proof; run it through its existing `mise` journey lane and retain the lower-level result-mapping/Gate tests.

### FIND-TASK-004-7 — No crash proof crosses detail ACK and unknown summary state

- **Wave 1 source IDs:** `BIFROST-PUB-002`
- **Status:** **CONFIRMED**
- **Classification:** MISSING
- **Violated obligation:** TASK-004 Scenario 5 explicitly requires a crash test for the `REQ-086`/`REQ-087` boundary where partial rows may remain but cannot authorize completion or Operator dispatch.
- **Exact locations:** `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:489-603,800-843`; existing fault seam at `crates/wyrd/wyrd-server/src/verification/publisher.rs:209-249`.
- **Evidence and reachability:** `unacknowledged_summary_retries_with_a_fresh_result` fails before summary send without process loss. `crashed_runner_restarts_and_reclaims_without_duplicates` crashes while the engine is held, before any batch ACK. The already-installed `PublicationFault::hang_next` can stop the summary after the preceding detail ACK but has no caller, leaving the required crash interleaving untested.
- **Observable consequence:** a regression that completes or dispatches from detail-only partial state, or mishandles the leased run when the in-memory summary payload is lost, is not caught.
- **Decision-complete minimum correction:** use the existing runtime, Scribe, `hang_next`, and capability-crash controls to acknowledge a non-empty detail batch, block the summary, crash the runner capability, and inspect the durable run and dispatch tables before and after lease reclaim. Prove the partial detail never completes the run or creates dispatch, the same run identity is reclaimed under the existing attempt budget, and completion/dispatch occurs only after a later attempt receives every required ACK. Do not add a recovery coordinator or cross-table atomicity mechanism.
- **Focused closure proof:** one focused real-Postgres runtime test covering that interleaving, followed by the existing publication replay/fresh-attempt and crash/restart tests.

## Specification decision

No retained correction needs a material product, public API, architecture, security-policy, compatibility, resource-ownership, concurrency-semantics, or persistent-data decision beyond the approved specification and repository authorities. `SPEC_REVISION_REQUIRED` does not apply. The validated ledger is non-empty and contains `FIND-TASK-004-1` through `FIND-TASK-004-7`.
