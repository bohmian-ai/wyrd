---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-073, REQ-080, REQ-146, AC-013]
depends_on: [TASK-005]
parent_task: TASK-005
remediates: [FIND-TASK-005-2, FIND-TASK-005-9, FIND-TASK-005-10]
---

# Close TASK-005 round 2 findings

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36.
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`.
- Prior review: `changes/active/verified-change-contract/review/TASK-005-r1`.
- This review: `changes/active/verified-change-contract/review/TASK-005-r2/findings-validation.md` and `verdict.md`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; reviewed candidate: `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`.

## Diagnoses and required outcomes

### FIND-TASK-005-2 — binding journey evidence

`AC-013` requires a real Service binding journey with two Services sharing one Trigger, separate runs per binding, and manual binding invocation through its Operator path. The existing Rust Drift journey at `sdks/wyrd-sdk-rust/tests/drift_verification.rs:463-547,974-1028` covers one Service and scheduled dispatch; `pg_verification_routes.rs:318-360` proves only manual enqueue. The three Drift SDK journeys do not prove the two-Service or manual-to-dispatch path. A projection or routing defect could therefore mix, omit, or duplicate results while current tests pass.

Extend the existing Rust SDK/server Drift journey with two active Services referencing the same Trigger and eligible at one occurrence. Prove separate binding and result identities and each binding's configured dispatches. Invoke one binding manually and prove its failed result dispatches; retain the existing direct-run no-dispatch assertion. Reuse the existing Trigger, Service, Operator, generic scheduler, and SDK journey harness. No new scheduler, public API, or test harness is needed. This closes the production path at the client→server→client tier required by `AC-013`.

### FIND-TASK-005-9 — fit work outlives cancellation

`REQ-146` requires shutdown drain then cancellation of remaining work, with its fenced lease released or expired. In `verification/fitter.rs:215-232,258-294`, timeout/shutdown cancels a token and awaits blocking fit before lease settlement. The token is checked during decode and once before `vala_drift::fit_baseline`; PSI/SPC work in `vala-drift/src/baseline/mod.rs`, `psi/mod.rs`, and `spc/mod.rs` can continue without observing it. The existing cancellation test stops before fitting. An expensive fit may exceed the execution timeout or shutdown drain; its lease may expire and be reclaimed while the first process still computes.

Make the existing `vala-drift` fit owner observe the fitter's cancellation signal during potentially long PSI/SPC work, including within a large feature. Preserve the ordinary fit API's output and profile semantics for uncancelled callers, decoded-data budget, shared permit, and SQL attempt fence. The server must await blocking work's cooperative stop before releasing or failing the lease. Do not detach work, add another worker/timeout framework, or move resource ownership.

### FIND-TASK-005-10 — stale ready task authority

The original task's frontmatter still says `status: ready`, `spec_revision: 35`; its outcome, owner/approach, and scenarios at `TASK-005-production-drift-verifier.md:1-43,85,118,158` direct local typed Oracle plans. Approved revision 36 requires a scoped SYSTEM token and fixed SQL through query service, Gate, and local or peer Oracle. The task's later evidence acknowledges that change, but an implementer following the active task would recreate the displaced boundary.

Align that same task's revision and governing prose with revision 36. Keep its original obligations and historical r1 evidence. The existing task is the owner; a replacement packet or implementation change adds nothing to this correction.

## Constraints and non-goals

- Preserve approved revision 36 token scope, audited Gate/query path, tenant isolation, exact subject/series/window SQL, and ordinary result publication.
- Preserve existing scorer results and the shared runtime's claim, permit, dispatch, and fenced settlement behavior.
- Do not add client aggregation, raw scorer reads, user SQL, a Drift scheduler, compatibility surfaces, or a new fit worker.
- Do not modify unrelated pre-existing `Oracle::query_plan` solely because it has no caller.

## Acceptance and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-005-2` | Two Services sharing one Trigger produce separately attributable Drift results and configured dispatches; a manual binding run dispatches, a direct run does not. | Extend and run the exact Rust `drift_verification` SDK journey through repository-managed Postgres; assert binding IDs, result IDs, and dispatch sets. |
| `FIND-TASK-005-9` | Cancellation after fit work starts stops its blocking computation before fenced release/timeout settlement; uncancelled profiles remain unchanged. | Add the smallest controlled focused fit cancellation check and run its exact `mise exec --` command, then the owning broader lane. |
| `FIND-TASK-005-10` | No active ready instruction names revision 35 or the displaced local typed-plan path. | Read the final task frontmatter, outcome, owner, approach, and scenarios against approved revision 36. No runtime test is needed. |

Use `mise.toml` to select the narrowest owning journey/fit tasks. Record the exact focused command for every named test, then run required format/lints and the touched-surface broader lanes under `mise`. Preserve the cumulative candidate and submit it for a fresh `$wyrd-task-review` against the same base.

## Remediation r2 Evidence

Candidate commits `95a8a827`..`7a497bb8` on `vcc/task-005`, same base
`f8811ac5`. Postgres-backed commands run inside
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && <command>"`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-005-2` two Services on one Trigger; manual binding dispatch | `drift_methods_fit_score_persist_and_dispatch` registers Trigger `drift-daily` and two Services binding `drift-custom` through it (two and one inline Operators). `SharedTrigger::assert_one_occurrence_runs_each_binding` makes both bindings due and asserts exactly one run per binding, distinct binding and result IDs, each result's own subject, failed verdicts, 2 and 1 dispatches, and no shared Operator. `assert_manual_binding_run_dispatches` starts a `binding` run through the SDK and asserts a new failed result, new dispatch IDs, and the same Operators as the scheduled run. The existing direct-run no-dispatch assertion in `complete` is unchanged | Focused journey command below | PASS |
| `FIND-TASK-005-9` fit ignores cancellation after decode | `vala_drift::fit_baseline_until(batch, spec, cancelled)` polls before each feature, between a feature's collect, edge, and binning phases, and every `CANCEL_CHECK_ROWS` (65,536) rows in PSI binning and categorical counting. It returns `DriftFitError::Cancelled`. `fit_baseline` and the PSI/SPC fitters delegate with a never-cancelled probe, so their output is unchanged. The server fitter passes `|| cancel.is_cancelled()` inside `spawn_blocking`, and `fit_next` still awaits that blocking task before release or fail. Permits, the decoded budget, and the SQL lease fence are untouched | `baseline::cancellation::cancellation_after_fit_starts_stops_inside_the_feature` (PSI numeric stops mid-binning, PSI categorical mid-count, SPC before limits; no poll after the stop; an uncancelled fit equals `fit_baseline`); `verification::fitter::tests` | PASS |
| `FIND-TASK-005-10` stale task authority | Frontmatter changed to `status: review` and `spec_revision: 36`. Outcome, owners, approach step 3, Scenario 3, acceptance criterion, and write set now name the SYSTEM read token and fixed SQL through the query service. The r1 evidence is kept | `grep -n "DataFusion\|typed Oracle\|logical-plan\|revision 35"` on the task finds only the r1 historical note | PASS |

Focused commands, each run alone in this session (1 passed):

```bash
mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=baseline::cancellation::cancellation_after_fit_starts_stops_inside_the_feature)'
# inside the Postgres wrapper above
mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=drift_methods_fit_score_persist_and_dispatch)'
```

Lanes run after the final code change, each exit 0: `mise run fmt`,
`mise run lints`, `mise run test:vala` (1308), `mise run test:wyrd` (2134),
`mise run test:bifrost` (9/9 lanes, including the Drift, Oracle, SDK, Python, and TypeScript journeys), and `git diff --check f8811ac5..HEAD`.

Non-goals held: no new scheduler, worker, timeout framework, public API, or
harness. `Oracle::query_plan` is untouched.

Material limit: one quantile sort within a feature cannot be interrupted. It
runs at most O(n log n) over a column held within the 256 MiB decoded budget.

### Failure diagnosis — Oracle capacity baseline (found while verifying)

- **Symptom:** the first `mise run test:bifrost` run failed at
  `wyrd-testing::oracle capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
  (`capacity.rs:126`). `spill_directories` read 3 at the baseline and 4 after
  settlement, while every admission, reservation, and spill-file counter was 0.
  The test passes when run alone.
- **Evidence:** each analytical query `RuntimeEnv` makes a DataFusion
  `datafusion-*` TempDir when it is built (`oracle/spill.rs:89-95`). Upstream
  worker task entries keep that `RuntimeEnv` through their `TaskContext` until
  coordinator end-of-stream or cache invalidation. `release_graph` returns the
  counters once reservations reach 0. The test read `ownership_baseline` once,
  right after `await_clean_nodes`.
- **Cause:** the empty directory lives until the upstream lease teardown,
  which can finish after graph release. `architecture/bifrost-design.md`
  permits this ("Query-owned leases remain alive until coordinator
  end-of-stream, cancellation, or cache invalidation"). The single read
  asserted an ordering the design does not promise.
- **Fix site:** the test's settled-baseline comparison (`capacity.rs`, commit
  `7a497bb8`). It now polls under the existing `CLEAN_NODE_POLLS` × 100 ms
  bound, following the pattern in `distributed.rs`. A directory that never goes
  away still fails with the last snapshot. `capacity.rs` is the only test that
  compares `spill_directories`. Other `await_clean_nodes` users check only
  graph and attempt state. No production change is needed.
- **Diagnostician:** a fresh read-only diagnostician was given the failing
  command, the trace, and the diff. It reported this cause and fix site. This
  diff does not touch Oracle spill or ownership code.
