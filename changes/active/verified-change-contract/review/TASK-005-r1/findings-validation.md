# TASK-005 Wave 2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`
- `.codegraph/` is absent, so source and caller tracing used repository text and the complete Git diff.
- `HEAD` matched the candidate before and after validation.

## Wave 1 finding validation

| Wave 1 source ID | Disposition | Validated result |
|---|---|---|
| `TASKREV-001` | **CONFIRMED** | Retained as `FIND-TASK-005-1`. |
| `TASKREV-002` | **REVISED** | Deduplicated with `REPO-002` and `STAT-002`; retained as `FIND-TASK-005-2` with only the journeys explicitly required by the task, spec, and Drift authority. |
| `TASKREV-003` | **CONFIRMED** | Retained as `FIND-TASK-005-3`. |
| `TASKREV-004` | **CONFIRMED** | Retained as `FIND-TASK-005-4`. |
| `TASKREV-005` | **REVISED** | Deduplicated with `DATA-002` and `STAT-001`; retained as `FIND-TASK-005-5`. The correction is bounded by the parsed rule/trend state, not an assumed literal eight means, because the preserved rule accepts authored thresholds such as 16. |
| `REPO-001` | **REVISED** | Retained as `FIND-TASK-005-6`; source confirms the violation, but no approved internal query identity/capability exists, so implementation remediation would make a new security decision. |
| `REPO-002` | **REVISED** | Merged into `FIND-TASK-005-2`. |
| `REPO-003` | **CONFIRMED** | Retained as `FIND-TASK-005-7`. |
| `STAT-001` | **REVISED** | Merged into `FIND-TASK-005-5`. |
| `STAT-002` | **REVISED** | Merged into `FIND-TASK-005-2`. |
| `DATA-001` | **CONFIRMED** | Deduplicated with `SEC-TEN-001`; retained as `FIND-TASK-005-8`. |
| `DATA-002` | **REVISED** | Merged into `FIND-TASK-005-5`. |
| `SEC-TEN-001` | **CONFIRMED** | Merged into `FIND-TASK-005-8`. |
| `SEC-TEN-002` | **CONFIRMED** | Retained as `FIND-TASK-005-9`. |

No Wave 1 finding was rejected. Optional hardening and unrelated pre-existing debt remain excluded.

## Final validated finding ledger

### FIND-TASK-005-1 — CONFIRMED — INCORRECT: a never-created Custom observation table becomes a terminal engine error

- **Wave 1 sources:** `TASKREV-001`.
- **Violated obligation:** TASK-005 Scenario 5, `REQ-080`, `INV-004`, `AC-012`, and `architecture/logic/drift.md` require an empty Custom window to complete `inconclusive` with null details and zero feature rows.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/drift.rs:376-386,465-473,608-668`.
- **Caller and reachability trace:** `VerifierRunner::execute` dispatches a claimed Drift run to `DriftEngine::verify`, which calls `try_verify`. The Custom branch calls `aggregate` and then the sole production caller of `custom_mean`. `aggregate` converts Oracle `TableNotFound`—the normal state before a tenant's first observation write—to an empty batch vector. `custom_mean` has no production caller besides this branch and rejects that vector as malformed. The existing Rust empty-window journey first writes the physical table, so it does not cover this reachable state.
- **Evidence:** `aggregate` returns `Ok(Vec::new())` at line 633, while `custom_mean(&[])` returns `Err` at lines 378-381 and its unit test explicitly expects that error. `try_verify` maps it to terminal `drift_invalid`, producing no result rather than an inconclusive result.
- **Observable consequence:** a valid direct Custom run before the first observation write settles `errored` instead of `completed/inconclusive`.
- **Decision-complete correction:** reuse the existing `Drift(None)` pre-scoring path by treating the empty batch sequence as an empty Custom window. Preserve errors for a nonempty malformed aggregate (wrong row count, missing/wrong columns) and leave PSI/SPC behavior unchanged. No new abstraction or dependency is needed.
- **Focused closure proof:** in a fresh tenant, register a Custom Verifier, run it before any Drift write, and assert `completed/inconclusive`, null details, zero feature rows, and no dispatch; retain a focused malformed-nonempty aggregate test that still errors.

### FIND-TASK-005-2 — REVISED — MISSING: the mandatory production Drift journey matrix is incomplete

- **Wave 1 sources:** `TASKREV-002`, `REPO-002`, `STAT-002`.
- **Violated obligation:** TASK-005 Scenarios 3-6 and acceptance criteria, `AC-012`, `AC-013`, `AC-017`, `AC-028`, and `architecture/logic/drift.md:421-447` explicitly require real Rust, Python, and TypeScript SDK-to-server journeys for PSI numeric/categorical, SPC, Custom, activation, delivery, recovery, authorization, and tenancy behavior.
- **Exact location:** `sdks/wyrd-sdk-rust/tests/drift_verification.rs:433-741`; `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py:175-269`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/verification-run.test.ts:85-145`; `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:493-523`; `mise.toml:346-352,411-431`.
- **Caller and reachability trace:** the Rust and Python journeys start a bound `WyrdTestServer` with the production verification runtime; the Python constructor's `verification_runtime` flag routes directly to `WyrdTestServerBuilder::with_verification_runtime_for_test`. TypeScript's `startTestServer` calls `start_test_server_async`, whose full body exposes only `auditPublication` and starts the builder without that existing runtime toggle. Its `verification-run.test.ts` therefore proves enqueue/readback only, is absent from the canonical TypeScript Bifrost journey lane, and cannot reach baseline fitting or Drift execution. The Rust journey has one high-drift window for the methods and one already-created-table Custom empty window; the Python journey covers authoring paths and high-drift PSI/SPC. Inspection found no other production Drift journey owner.
- **Evidence:** the required cases absent from real production-path evidence include PSI fitted-edge equality, categorical unknown/zero bins and minimum-sample pass/inconclusive; SPC adaptive/authored sample sizes, ordered rule/zone/trend/threshold, pass, trailing and no-data; Custom threshold equality, unequal-batch weighted mean, invalid and never-written input; exact `[start,end)` exclusion; shared observations across bindings; inactive/unready/missed/no-backfill scheduling; shared Trigger isolation; manual binding dispatch; partial ACK/retry; Notify and sibling dispatch independence; and restart recovery. Unit scorer inputs and plan rendering do not exercise Oracle/DataFusion or client/server seams.
- **Observable consequence:** the candidate can regress required method semantics and cross-boundary activation/delivery behavior while all recorded lanes remain green; TypeScript has no production Drift execution proof.
- **Decision-complete correction:** extend the existing Rust, Python, and TypeScript journey owners and repository-managed server harness—no new harness or client-side scoring—to drive the explicitly required matrix through public SDKs, the production verification runtime, Oracle, Bifrost persistence, and status/result reads. Expose the existing test-server runtime toggle through the TypeScript testing binding and include its Drift target in the canonical TypeScript journey lane. Reuse existing generic TASK-004 fault, restart, and delivery controls rather than duplicating runtime machinery.
- **Focused closure proof:** run the three language journey targets against repository-managed Postgres and assert the method-specific scores/verdicts/features plus the activation, ACK/retry, delivery, restart, authorization, and tenant outcomes named above.

### FIND-TASK-005-3 — CONFIRMED — DRIFT: repository-wide skill policy changes are unrelated to TASK-005

- **Wave 1 sources:** `TASKREV-003`.
- **Violated obligation:** TASK-005's scoped production Drift implementation and the completion requirement that no unrelated change enter the cumulative candidate.
- **Exact location:** `.agents/skills/wyrd-implement/SKILL.md:56-76`; `.agents/skills/wyrd-task-review/SKILL.md:78-84`; mirrored `.claude/skills/...` files.
- **Caller and reachability trace:** these files are workflow instructions consumed by future implementation and review sessions and mirrored by the existing skill-sync mechanism; they are not called by, imported into, or required by any Drift registration, fitting, query, scoring, runtime, SDK, or test path. The complete commit range shows they entered through standalone commit `ff27b0eb`.
- **Evidence:** the additions impose repository-wide diagnostician and review policy. TASK-005 already records the two relevant failure diagnoses under existing repository testing rules; neither production behavior nor the diagnosed test fixes depend on changing these skills.
- **Observable consequence:** accepting TASK-005 would also change how unrelated future Wyrd work is implemented and reviewed, including the instructions used to review this candidate.
- **Decision-complete correction:** delete these four skill-file changes from the cumulative TASK-005 candidate. If wanted, submit the synchronized policy change under separately approved scope.
- **Focused closure proof:** `git diff base..candidate -- .agents/skills .claude/skills` is empty for TASK-005; the ordinary skill-sync check remains green.

### FIND-TASK-005-4 — CONFIRMED — VIOLATION: specifically named new tests lack exact focused-run evidence

- **Wave 1 sources:** `TASKREV-004`.
- **Violated obligation:** TASK-005 Verification and Evidence, `AGENTS.md` section 11, and `spec-driven-development.md:177-205` require an exact repository-native focused command for every specifically named Rust, Python, and TypeScript test.
- **Exact location:** the Implementation Evidence in `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md:181-255` and the named tests in `crates/vala/vala-drift/src/baseline/mod.rs`, `crates/wyrd/wyrd-sql/tests/pg_drift_baselines.rs`, `sdks/wyrd-sdk-rust/tests/drift_verification.rs`, and changed named CLI/runtime tests.
- **Caller and reachability trace:** this is an evidence obligation, not a callable implementation surface. The named statistical tests execute `fit_baseline`/aggregate scoring directly; the SQL tests execute the real `DriftBaselineQueue`; the Rust journey executes the full public SDK/server path. Their broad family lanes do not replace the required exact selectors and can hide a zero-selection or stale-target mistake.
- **Evidence:** the task records broad `mise` lanes, one exact Oracle planner selector, and one Python file command. It does not list exact selectors for the five new aggregate-input statistical tests, four new Postgres baseline tests, the Rust Drift journey test, or the changed specifically named CLI/runtime tests; “the focused commands above” cannot identify commands absent from the record.
- **Observable consequence:** the immutable evidence does not establish that each final named test target and selector actually ran.
- **Decision-complete correction:** run each named test with its exact repository-native command and record that command and result in the task evidence; include the repository-managed Postgres wrapper where needed. Do not replace these with another broad lane.
- **Focused closure proof:** every named test has one exact command whose selector is confirmed from the final target and whose recorded run exits successfully on the cumulative candidate.

### FIND-TASK-005-5 — REVISED — VIOLATION: production SPC retains every aggregate batch, mean, and zone

- **Wave 1 sources:** `TASKREV-005`, `DATA-002`, `STAT-001`.
- **Violated obligation:** `architecture/logic/drift.md:153-163` requires SPC aggregate rows to stream through bounded rule state instead of accumulating an unbounded vector; TASK-005 requires bounded server aggregation while preserving existing SPC semantics.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/drift.rs:342-367,514-532,608-668`; `crates/vala/vala-drift/src/spc/mod.rs:181-274`; `crates/vala/vala-drift/src/spc/weco.rs:100-243`.
- **Caller and reachability trace:** the SPC branch of `DriftEngine::try_verify` calls the shared `aggregate`, then the sole server caller of `spc_chunks`, then `score_spc_chunks`. The other `score_spc_chunks` production caller is raw-batch `score_spc`; tests call it for parity. Full bodies show three window-sized allocations on the server path: collected `Vec<RecordBatch>`, `SpcTargetChunks.means`, and the derived `Vec<i8>`. `evaluate` scans consecutive, alternating, and seven-point trend windows; its authored consecutive/alternating thresholds can exceed eight (the preserved default includes 16), so a literal fixed-eight buffer would change semantics.
- **Evidence:** retention grows with every subgroup in a valid manual window even though each decision can be updated from bounded recent rule/trend state plus counters. Oracle already provides ordered aggregate rows, so collecting them is not required for ordering.
- **Observable consequence:** a large valid retained window can exhaust verifier memory after Oracle has reduced raw observations, yielding retry/OOM rather than the required SPC judgment.
- **Decision-complete correction:** add an incremental aggregate-input scorer in the existing `vala-drift` SPC owner that retains only the parsed rule/trend lookback and accumulated row/violation state, and feed decoded ordered Oracle SPC batches to it as frames arrive. Preserve raw `score_spc`, public report shape, authored rule thresholds, exact consecutive/alternating/trend counting, alert-threshold filtering, trailing chunks, and inconclusive behavior. Keep PSI and Custom's already-small aggregate handling unchanged; add no dependency or second algorithm.
- **Focused closure proof:** parity tests compare raw/current and streaming reports for consecutive, alternating, trend, threshold, trailing, and inconclusive cases; a long multi-batch production-path test proves retained subgroup state is bounded by rule/trend lookback rather than total subgroup count.

### FIND-TASK-005-6 — REVISED — VIOLATION / SPEC REVISION REQUIRED: the Drift reader manufactures forbidden authority for the result-writer-only SYSTEM principal

- **Wave 1 sources:** `REPO-001`.
- **Violated obligation:** `architecture/wyrd-design.md:151-175` and `architecture/wyrd-security-posture.md:68-75,142-146` reserve tenant `SYSTEM` for canonical verification-result publication and fix its authority to exactly `bifrost_record:write` scoped to one Verifier. Internal request boundaries must carry sanctioned authenticated authority.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/drift.rs:564-605`.
- **Caller and reachability trace:** `DriftEngine::context` has one caller, `aggregate`; all Custom, PSI, and SPC production branches call `aggregate`. The full body loads the persisted SYSTEM UUID, constructs `PrincipalKind::System`, inserts `Permission::bifrost_query_read()` into its effective permissions, and presents the same permission to `AuthorizedQueryContext`. Oracle's context only checks tenant agreement; the public query path instead derives its context from an already authenticated `Caller`. `verifier_runs` retains a requester only for manual runs; scheduled runs have none. No existing internal Drift-query principal or fixed read capability was found.
- **Evidence:** the code creates the authority it then asks Oracle to authorize. This directly widens a principal whose approved permission set is exact and cannot cover scheduled reads under the existing public-caller pattern.
- **Observable consequence:** the reserved result writer gains undocumented, synthetic observation-read authority, defeating the fixed least-privilege identity contract and its revocation/grant model.
- **Decision-complete correction:** do not implement a guessed principal substitution or bypass. Revise and approve the specification/security authority to define the tenant-bound internal identity or capability that authorizes fixed verification observation reads, its exact object/Card scope, issuance or construction path, and audit semantics. Then route `DriftEngine` through that sanctioned mechanism and keep SYSTEM write-only. Reusing the manual requester alone is insufficient because scheduled runs are required and carry no requester.
- **Focused closure proof:** security tests prove SYSTEM cannot hold or exercise `bifrost_query:read`; production direct and scheduled Drift journeys prove the newly approved identity/capability can read only the exact tenant, observation table, subject, series, and frozen window, with the required audit decision.

### FIND-TASK-005-7 — CONFIRMED — VIOLATION: the new Drift test module lacks required rustdoc

- **Wave 1 sources:** `REPO-003`.
- **Violated obligation:** `AGENTS.md:702-718` and `architecture/agent-rules.md:35` require rustdoc on every new Rust item, explicitly including test modules.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/drift.rs:672-673`.
- **Caller and reachability trace:** this is a compile-time module item, so there is no runtime caller. Its contained helpers and four tests cover fixed plan construction and aggregate decoding, but neither an outer `///` nor inner `//!` documents the module.
- **Evidence:** the new item is `#[cfg(test)] mod tests { ... }` with imports immediately inside it.
- **Observable consequence:** the candidate violates a hard merge-blocking repository documentation rule.
- **Decision-complete correction:** add one concise module-level rustdoc comment describing its fixed-plan and aggregate-decoding proof. Add no helper, abstraction, or test.
- **Focused closure proof:** source inspection shows the module rustdoc and the normal format/lint lane passes.

### FIND-TASK-005-8 — CONFIRMED — VIOLATION: baseline fits bypass the shared Verifier/baseline permit ceiling

- **Wave 1 sources:** `DATA-001`, `SEC-TEN-001`.
- **Violated obligation:** `REQ-146` fixes one shared ceiling of 16 globally and 4 per tenant across Verifier and baseline executions, with permits acquired before durable claim; TASK-005 Scenario 2 explicitly requires reuse of the generic runtime permits.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/mod.rs:344-409`; `crates/wyrd/wyrd-server/src/verification/fitter.rs:63-213`; `crates/wyrd/wyrd-server/src/verification/runner.rs:80-307`; `crates/wyrd/wyrd-server/src/verification/permits.rs:27-87`.
- **Caller and reachability trace:** `VerificationRuntimeBuilder::build` constructs `BaselineFitter` first, then constructs a new `VerifierPermits` owned only by `VerifierRunner`. Runner `claim_round` acquires a tenant/global permit before `claim` and holds it through `process` settlement. Fitter `run` calls `pass`, which calls `fit_next`; `fit_next` claims and commits durable work before executing `fit`, with no permit field or argument. Repository search found no other `VerifierPermits` owner or fitter admission path.
- **Evidence:** one-at-a-time fitter iteration is a separate per-process serial limit and does not participate in the required shared global/per-tenant counts. A process may run 16 Verifiers plus a fit, or 4 Verifiers plus a fifth same-tenant fit.
- **Observable consequence:** the promised process ceiling and tenant fairness are false at the baseline CPU/memory boundary, and work can consume an attempt when no shared capacity existed.
- **Decision-complete correction:** construct one existing `VerifierPermits` owner in runtime composition and share it with runner and fitter. The fitter must acquire both tenant/global slots before claiming a baseline and hold them through fit and fenced settlement/release; unavailable capacity leaves the row unclaimed. Preserve the existing durable queues and do not add a second semaphore type.
- **Focused closure proof:** occupy all four slots for one tenant and show its due baseline remains unclaimed while another tenant progresses when global capacity remains; separately prove combined active Verifier runs plus fits never exceed 16 and that releasing a slot permits the same row to be claimed.

### FIND-TASK-005-9 — CONFIRMED — VIOLATION: compressed-size checking does not bound decoded baseline work or cancellation

- **Wave 1 sources:** `SEC-TEN-002`.
- **Violated obligation:** `REQ-115` requires all worker concurrency, engine calls, and shutdown drain to be bounded; `REQ-146` requires remaining work to cancel after drain; TASK-005 Scenario 2 requires a bounded fitter. Tenant-authored artifacts are a semi-trusted resource boundary.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/fitter.rs:47-51,171-253,297-315`; `crates/wyrd/wyrd-server/src/verification/mod.rs:361-367`.
- **Caller and reachability trace:** `BaselineFitter::run` calls `pass` → `fit_next` → the sole caller of `fit`. `fit` resolves one tenant-authored artifact, reads all compressed bytes, then its `spawn_blocking` closure collects every decoded Parquet batch into a vector, concatenates the full decoded table, and calls the sole production caller of `vala_drift::fit_baseline`. `fit_next` races the join future with shutdown; dropping that future releases the durable row but cannot stop the already-running blocking closure. `RuntimeLimits::execution_timeout` is passed to `DriftEngine`, not the fitter. Other `fit_baseline` callers are tests.
- **Evidence:** the only budget is registered compressed object metadata `<= 256 MiB`; Parquet expansion, decoded Arrow bytes, rows, fit CPU, and blocking-task lifetime have no bound. Cancellation can therefore detach live decode/fit work, and a reclaimed row can start another copy.
- **Observable consequence:** a normal authorized tenant can submit a highly compressible baseline that exhausts shared memory/CPU; shutdown or lease release can leave duplicate resource-consuming work after durable ownership was relinquished.
- **Decision-complete correction:** keep the existing fitter and Parquet reader, but enforce one explicit decoded-work budget while iterating bounded record batches before concatenation/fitting, and make the blocking decode/fit operation cooperatively observe runtime timeout/shutdown so work stops before the lease is released. Reuse existing runtime limits/cancellation and structured failed-baseline status; add no new worker, queue, artifact format, or dependency.
- **Focused closure proof:** a small-on-disk/high-expansion Parquet fixture exceeds the decoded budget and settles visibly failed without allocating beyond it; timeout/shutdown proof shows decode/fit stops before release and the same row cannot execute concurrently through reclaim.

## Ponytail result

The retained corrections reuse existing owners: `DriftEngine`, `vala-drift` SPC scoring, `VerifierPermits`, `BaselineFitter`, current SDK journey harnesses, and the existing evidence record. No finding justifies a new scheduler, queue, result path, client aggregator, test harness, dependency, or compatibility surface. `FIND-TASK-005-6` cannot be safely reduced to code because the repository currently has no approved identity/capability for required internal Drift reads.

## Verification limits

- This was a static acceptance validation under the review time budget; no broad or focused test command was rerun.
- The task records green broad lanes, but `FIND-TASK-005-4` identifies the missing selector-level evidence and `FIND-TASK-005-2` identifies required journeys that do not exist.
- No denial-of-service fixture or long-window SPC benchmark was executed; the unbounded allocations and detached blocking-task path are directly established by full-body and caller inspection.
- The complete Oracle distributed planner, object-store stack, and unrelated TASK-004 runtime behavior were not requalified beyond the TASK-005 paths necessary to validate the proposed findings.

## Recommended review verdict

**SPEC_REVISION_REQUIRED**

Eight findings are bounded implementation/evidence corrections, but `FIND-TASK-005-6` requires a new approved security decision defining authority for mandatory internal Drift observation reads. Prescribing that identity or capability in a remediation task would exceed the approved task and current architecture authority.
