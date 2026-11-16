---
id: TASK-013
status: proposed
approved_spec_revision: 16
requirements: [REQ-015, REQ-017, REQ-018]
acceptance: [AC-010, AC-012, AC-013]
invariants: [INV-001, INV-007]
depends_on: [TASK-012, TASK-012-R1]
---

# Prove queued fairness and measure its cost to direct verification

A flooding tenant must not put 99 quiet tenants behind its backlog. Prove claim
fairness, completion, drain and return of charged memory in a deterministic
Postgres integration test. Separately, give the benchmark reader a same-pace
direct-only control beside the existing L2 `competing` run, so the cost of queued
traffic to direct throughput and latency is visible on one `combined` server.

## Inputs, owners and flow

Execute only after [TASK-012-R1-allocation](012-R1-allocation.md) is complete.
Its revision-16 contract was explicitly approved on 2026-11-16. TASK-012 supplies the composed server
and ported Verification harness; TASK-012-R1 supplies the reviewed workload
manager, consumption accounting, preserved Bifrost memory/workspace behavior,
connected functional proof and clean L1 qualification records. Benchmark that
implementation, not the pre-allocation TASK-012 candidate. Preserve the older
compatible baseline identities for comparison and identify the applicable
post-allocation L1 record when calibrating canonical rates.

The selected split and inherited obligation dispositions
are recorded in [code-mode port / isolation proof and throughput qualification](../design/code-mode-port.md#isolation-proof-and-throughput-qualification).
Approved revision 16 REQ-015 and REQ-019–024 govern shared capacity; separate `direct` and
`worker` servers are not this comparison's topology. TASK-014 consumes this
task's functional regression and full L2 baseline, alongside the applicable L1
`direct-verify` record from TASK-012-R1; TASK-012 itself runs no benchmark.
This dependency changes the implementation measured, not this task's comparison
criteria, full-run timing or 100-tenant claim-fairness scope.

Extend the existing owners, without adding a command, format, renderer or
comparison pass/fail budget:

- `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs` owns
  `a_flooding_tenant_does_not_delay_another_tenants_run`, its real Postgres queue,
  tenant fixtures and held `EngineScript`. Scale that scenario to one flooding
  tenant plus 99 separately seeded quiet tenants, each submitting one run.
  Configure limits and hold engines to create saturation, rather than loading
  the host. With engines held, prove every quiet run is claimed in the first
  round behind at most one flood run, using durable claim order rather than
  elapsed-time thresholds. Release the engines and assert every submitted run
  completes, no queued or leased rows remain, and the process memory ledger
  returns to its starting value after the owned work settles. This is an
  integration invariant, not a benchmark observation or an RSS assertion.
- `crates/wyrd/wyrd-testing/src/bin/capacity/verification/{mod,run,workload}.rs`
  owns L2 `competing`: public Rust-client `observe().verify` and
  `observe().submit` together on one server with Scribe evidence writes. Today
  `run.rs::Op::arrival` sends `ASSERT_VERIFIER` for both streams; this exercises
  no Agents, model exchanges or `code.run`. Replace that workload on both
  streams with `workload.rs`'s existing `bench-canonical`: one Assert and two
  independent Agent checks, four model exchanges and two `code.run` operations
  per completed verification. Reuse L1's canonical binding, inputs, fixture
  provider and reconciliation owners. Reconcile correct verdicts and those
  exchange/code counts for completed direct calls and for drained queued work;
  submission receipts are not completions. Keep work completed during drain
  attributable to its run rather than dropping it at the measurement boundary.
  Add only the direct-only control beside `competing`, using that same canonical
  workload, identities, server/resource settings and direct arrival schedule
  with queued submissions disabled. Replace the assertion workload; do not
  retain it as a third step. Each repetition runs capacity → overload → recovery,
  then drains. Preserve existing L2 criteria and validity semantics; do not add
  a comparison pass/fail budget or force L2 into L1's closed-loop, 300-second,
  zero-error method.
- `crates/wyrd/wyrd-testing/src/experiment.rs` owns the standard record and
  report. Show both steps together in its existing tables: direct achieved
  throughput and public-call p50/p99, queued offered/accepted/completed/refused
  counts and backlog. The control explicitly records zero queued traffic.
  Retain drain, recovery, refusals and verdict evidence through the existing
  path; queued receipts alone do not count as completed work. Extend existing
  metrics where needed within the current record format.

Reuse the declaration, records, report and `--steps` / `--profile` / `--smoke`
behavior. Update the Verification section of `benchmarks/README.md` and the
owning `bench:l2:verification` description in `mise.toml` to state the canonical
workload, exactly two steps, their windows and total run time, and explain the
matched-rate comparison. Edit the description or generator at its source owner,
not generated output. Document Verification's timing exception beside the shared
L2 defaults; do not shorten Bifrost or Gateway L2.
No 100-tenant benchmark, Python/TypeScript mixed traffic, multi-process proof,
optimization or throughput target belongs in this task.

## Verification timing and canonical rates

The full `mise run bench:l2:verification` run must finish in **15–20 minutes**,
including warmups, measured windows and all drains. Use Verification's existing
`Declaration::timing` / `Timing` at the engine declaration owner to declare a
60-second warmup and three 120-second measured windows per step, with a
900–1,200-second full-run elapsed range. Preserve smoke's reduced timing and
existing selection/profile behavior. The shared `Level::Two` defaults and the
other engines' declarations remain unchanged. If Verification cannot express
its own windows and total timing within the existing declaration, stop and ask
the maintainer rather than adding benchmark machinery or shortening other L2s.

With the existing 5/12 → 2/12 → 5/12 schedule, a measured window is 50 seconds
capacity, 20 seconds overload at **4× both rates**, then 50 seconds recovery.
Recovery overhead p99 excludes the first 30 recovery seconds, leaving 20 seconds.
Its existing 1,000-sample floor therefore needs at least 50 completed direct
verifications/s in that interval. Confirm the actual qualifying sample count
in every measured repetition of both steps at the chosen rate; the offered rate
alone is not proof. Record the window rationale and elapsed-time accounting:
two steps' warmups/windows total 840 seconds before drains and other overhead.
Each drain retains its 60-second limit; account for warmup drains, both steps'
drains and remaining overhead within the whole-run bound rather than assuming
an approximately 17-minute worst case. Do not weaken the sample floor, omit
drains or lengthen a run to obtain more repetitions. If these constraints cannot
all be met, report the measurements and ask the maintainer.

The old 400 direct/s and 100 queued/s defaults were calibrated for assertions.
Derive `--direct-rate` and `--submit-rate` for canonical work from the existing
compatible L1 canonical record identified below. Record its identity, supporting
capacity observations, chosen rates and rationale in the controls. Capacity must
be sustainable with both streams, while 4× overload must actually saturate.
The control uses the identical direct rate and multiplier, with zero queued
traffic. Confirm sustainability, saturation and recovery sample sufficiency in
the canonical run; do not invent numeric defaults before that evidence exists.
During overload only `WYRD_VERIFICATION_429_STAGING_FULL` is declared admissible.
Report any other refusal code rather than widening the allowed set; preserve
existing no-unexpected-refusal rules outside overload. Freeze the resulting
rates, windows, schedule and resource controls in the baseline for TASK-014.

## Acceptance and verification

| Obligation | Trigger and expected observation |
|---|---|
| REQ-015/017; AC-010/012; INV-001 | The 100-tenant held-engine integration scenario proves first-round quiet claims, completion of every run, empty queued/leased state and return to the starting process memory charge. A fairness, drain or charge-release regression fails the existing CI integration lane. |
| REQ-015/018; AC-010/013; INV-007 | Both `competing` streams and its direct-only control execute `bench-canonical` through the existing bindings/provider on one `combined` server. Correct verdicts, four model exchanges and two code operations reconcile per completed direct or drained queued verification. Existing tables show direct throughput/p50/p99 and queued counts/backlog together, with explicit zero queued traffic in the control. The comparison measures interference; TASK-014 evaluates isolation after optimization. |
| REQ-018; AC-013 | Unit tests at the existing declaration and report owners prove exactly two canonical steps, control selection, Verification-specific timing and paired standard output; workload tests at the existing verification owners protect canonical stream selection and reconciliation. Selection, profiling and smoke retain existing semantics. The full clean unprofiled canonical L2 run finishes in 15–20 minutes, with sustainable capacity, actual 4× saturation, unchanged refusal rules and sufficient recovery p99 samples. Its frozen controls and record supply TASK-014's baseline. |

The integration target requires `test-support`. Existing focused commands are:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::a_flooding_tenant_does_not_delay_another_tenants_run)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once)'
```

The second is existing shared-Postgres replica-claim proof, not a two-process
capacity experiment. Preserve it rather than building another replica run.
The integration lane is `mise run test:bifrost:integration:server`. New or renamed
tests use actual focused selectors recorded after addition, before RED.
Declaration unit tests use the `wyrd-testing` `capacity` binary target; record
and report unit tests use its library target:

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
mise exec -- cargo nextest run --locked -p wyrd-testing --lib
mise run fmt
mise run lints
```

Record exact focused commands for the new declaration/report tests. Smoke the
control and competing through the existing L2 runner, then capture one **full,
clean, unprofiled** run with both declared steps included by default:

```bash
mise run bench:l2:verification
```

Retain its immutable record, standard report, source/build/host identities,
resource controls and invocation for TASK-014. Smoke, selected-step, profiled
or dirty diagnostics do not replace it; a clean candidate is supplied by the
maintainer, and this task authorizes no commit. Keep the existing L1
`direct-verify` record as the separate direct-only baseline, recording its path
and compatibility with the candidate. Missing or incompatible baseline evidence
must be reported rather than manufactured or relabelled. Outputs are the CI
regression, declared control/report tests, benchmark guide update, full L2
baseline and identified existing L1 baseline.
