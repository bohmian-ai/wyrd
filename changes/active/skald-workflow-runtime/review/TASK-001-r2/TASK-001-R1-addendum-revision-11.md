# TASK-001-R1 addendum: spec Revision 11 deletes the Observer system

Applies to `TASK-001-R1-close-validated-runtime-gaps.md`. The user approved spec
Revision 11 (REQ-053) on 2026-10-02 after that remediation task was written.
Where they conflict, this addendum and REQ-053 win.

## Required change

1. **Delete first.** Remove the Skald Observer plugin system entirely, with no
   compatibility alias: the `crates/skald/skald-observer` crate (workspace
   member, dependency edges, `Cargo.lock`), every observer hook and
   `current()`/`with_observer` scope in `skald-agent` and `skald-workflow`,
   `skald-workflow/src/observe.rs` (including `StepResultCeiling`), Workflow and
   Agent `observers`/`with_observers` surfaces, `crates/wyrd/wyrd` re-exports and
   tests, the Python `Observer`/`OtelObserver`/`observers=` exports, PyO3
   wrappers, stubs, stub-assembly entries, tests and examples, the Rust observer
   examples, and observer references in docs, architecture references, and
   boundary-check scripts. Do not touch the unrelated `wyrd.observe` (Vala
   observation / Bifrost) package.
2. **Replace with `tracing`.** Emit the REQ-053 span/event set synchronously
   from the Workflow engine and Agent loop, with no payloads. Prove it with
   `wyrd_telemetry`'s existing span-capture test helper; do not add a new
   capture harness or a local telemetry-init API.
3. **Enforce limits directly.** The step-result ceiling (FIND-TASK-001-5) is
   enforced by the engine where the result is admitted, not by an observer
   wrapper.
4. **Keep Agent timeout coverage.** Rewrite the timeout cases in
   `skald-agent/tests/observer_timeout.rs` against results/spans rather than
   deleting that coverage.

## Effect on the validated findings

- FIND-TASK-001-5, -6, -12: close by deletion plus item 3. The committed
  callback-boundary work for FIND-TASK-001-6 is removed with the system.
- FIND-TASK-001-10, -13: apply only the parts that survive deletion.
- All other findings: unchanged; complete them as the remediation task states.

Run the remediation task's verification lanes plus `mise run check:client-tier`,
`mise run check:pyo3-scope`, `mise run docs:check`, and `mise run check:examples`
after the deletion.
