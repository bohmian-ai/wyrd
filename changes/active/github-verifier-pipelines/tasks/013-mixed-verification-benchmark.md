---
id: TASK-013
status: proposed
approved_spec_revision: 12
requirements: [REQ-015, REQ-017, REQ-018]
acceptance: [AC-010, AC-013]
invariants: [INV-001, INV-007]
depends_on: [TASK-012]
---

# Compare direct verification with and without queued pressure

A benchmark reader can see whether queued work changed direct throughput or
client round-trip latency. Add one standard mixed run: Rust SDK clients offer
canonical direct verifications at a fixed declared pace while a queued canonical
flood runs. The mixed run uses 100 active authenticated tenants in total.
Submit the same canonical Verifier through the public queued path. Compare with
a direct-only control at the same pace on the same host and build.

## Inputs and existing owner

Execute after TASK-012 and before TASK-014. TASK-012 supplies the composed
server tree and A's ported Verification benchmark harness: canonical closed-loop
direct c1/c64 from Rust, Python and TypeScript, paced steps, profiles, standard
records and `capacity report` summary tables. That canonical direct run already
exists; this task only adds the mixed run and its control. The approach follows
TASK-012's Capacity boundary and architecture handoff, the finalized
resource-scaling benchmark remediation supplement, and the maintainer's
mixed-run decisions. TASK-014 consumes the new record as part of its baseline
and reuses this run for queued-isolation proof.

Extend the ported harness's Verification declaration, workload and report owners
under `crates/wyrd/wyrd-testing/src/bin/capacity/`, including
`verification/{mod,run,workload,driver}.rs`. Standard record and report behavior
lives in `crates/wyrd/wyrd-testing/src/experiment.rs`. Add the comparison as
declared step(s) of the existing Verification benchmark command. Reuse its
record format, profiles, integrity/validity rules, `--profile` and `--steps`
behavior, and report tables; do not introduce a benchmark command, collector,
record format or report renderer. Update the Verification section of
`benchmarks/README.md` to explain the new step(s) and their output.

## Recorded comparison

Use Rust SDK clients for both direct and queued traffic. The implementer chooses
the direct pace and queued offered rate, records and justifies both, and keeps
the direct pace fixed between control and mixed run. Record the tenant layout
(which tenants send direct versus queued), queued offered rate and resource
settings alongside the direct pace in the record's controls. Keep the same
direct tenants, identities and resource configuration in the control, with
queued traffic disabled. Provision tenant-scoped canonical bindings, inputs and
accepted execution authority through the existing fixture setup; relabelling
clients does not create tenants. Queued submissions must reach actual execution
and reconcile terminal evidence after drain; submission receipts are not
completions. Keep the canonical Assert/two-Agent workload, four model exchanges
and two code operations per completed verification. Record direct offered,
accepted, completed and refused work, achieved concurrency and arrival lateness
so driver underload cannot hide interference. The fixed offered pace is a
measurement control, never a server cap on direct work.

The standard summary tables show the control and mixed results together: direct
achieved throughput and client round-trip p50/p99, plus queued
offered/accepted/completed/refused counts and backlog for both, with terminal
evidence reconciliation and drain outcome retained in the standard record. The
control identifies its absence of queued traffic explicitly. These observations
expose the effect of queued work without inventing a throughput target or
pass/fail budget.

## Acceptance and verification

| Obligation | Trigger and expected observation |
|---|---|
| REQ-015/017 / AC-010; INV-001 | Run the declared mixed step(s) through real public Rust SDK direct and queued paths across 100 authenticated tenants, then compare with the same-pace direct-only control on the same host/build. Controls identify traffic placement and resource settings; standard tables expose direct throughput/latency and queued counts/backlog so interference is visible. |
| REQ-018 / AC-013; INV-007 | Select the new step(s), profile them and render their records through the existing harness. They retain standard selection, profiling and validity semantics; the existing report shows the comparison without another format or renderer. The Verification documentation explains how to run and read it. |

Add inline unit tests at the harness's existing declaration and report owners,
following their existing tests: prove the new step(s) are declared/selectable
and that a standard record renders both control and mixed metrics correctly. Use
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` for
declaration/workload tests and `mise exec -- cargo nextest run --locked -p
wyrd-testing --lib` for record/report tests (add exact filters after naming the
new tests); after adding tests, record their actual focused commands and
selectors rather than inventing names here. Run the affected Rust format/lint
checks through `mise run fmt` and `mise run lints`.

Run a smoke of the new step(s) with the existing Verification command's
`--smoke` / `--steps` support and retain its standard record and report. Then
capture one full clean, unprofiled Verification record including the new step(s)
and control on TASK-012's composed tree, with only this task's benchmark
additions, before TASK-014 optimizes it. Use `mise run bench:l1:verification`
with full default declared coverage, inherited direct validity rules and
separately reconciled queued overload outcomes; retain exact source/build/host
identities, controls and invocation. Smoke, profiled or selected-step
diagnostics do not replace that full clean baseline record.

Deliver the declared steps, standard record/profile/report integration,
Verification documentation, focused test and smoke evidence, and full clean
record to TASK-014. Python/TypeScript mixed traffic, two-replica proof,
optimization and pass/fail throughput budgets are outside this task.
