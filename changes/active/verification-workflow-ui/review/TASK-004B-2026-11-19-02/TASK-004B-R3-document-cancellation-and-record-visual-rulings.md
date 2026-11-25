---
id: TASK-004B-R3
kind: remediation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 6
supporting_spec: SPEC-wyrd-ui-main-integration
supporting_spec_revision: 28
requirements: [REQ-014, REQ-018]
parent_task: TASK-004B
depends_on: [TASK-004B-R2]
remediates: [FIND-TASK-004B-8]
---

# Finish cancellation documentation and record visual acceptance

The functional R2 corrections are accepted within the inspected implementation
evidence. This task closes two missing Rust documentation blocks and the
already-required visual decisions. It does not reopen cancellation, validation,
analytical SQL or the completed loading behavior.

## Immutable context and authority

- Candidate: `7cf5bc4abb1462e5783df67ff6d0a2a5f2a06b14`.
- R2 implementation baseline: `41f2419c8`, committing the previously reviewed
  R1 working-tree tests. Prior review HEAD was `79a4ef661`.
- Approved contract: SPEC-verification-workflow-ui revision 6, supporting UI
  revision 28; original TASK-004B and R1's approved Revision 1 browser criteria.
- Predecessor: `../TASK-004B-2026-11-19-01/TASK-004B-R2-complete-support-lifecycle.md`.
- Review: `verdict.md`, `findings-validation.md`, `task-review-cancellation.md`.

Root AGENTS requires rustdoc for every new/changed Rust item and explicitly
blocks merge when it is missing. `agent_docs/rust.md` applies the requirement
to fields and regardless of visibility. Approved browser criteria separately
require the maintainer's ruling for visible differences; a requested ruling
is not an approval.

## FIND-TASK-004B-8 — minimum correction

New `NativeAbort.aborted` at `sdks/wyrd-sdk-ts/native/src/lib.rs:324` and
`Default::default` at line 346 lack rustdoc. Both were introduced by R2's
native cancellation change. Add focused rustdoc at those existing items:
explain the field's shared persistent cancellation state and the default's
fresh un-aborted state. Match actual invariants and adjacent style. Do not
rewrite logic, add helpers/options or expand the public API for this correction.

Closure is source inspection showing both documented items plus appropriate
format/diff checks. No runtime test, mutation exercise, journey-suite repetition
or production churn is required for these two documentation additions. Use
existing mise formatting/check owners and record actual results.

## Outstanding visual decisions

The twenty lifecycle images already exist at
`../TASK-004B-2026-11-19-01/captures/` with the recorded fixed viewport and
light/dark variants. Retain those artifacts and their execution provenance.
The maintainer still needs to rule on:

1. The half-dimmed Updating treatment with an overlaid label; the facet-rail
   label overlaps the existing click-to-filter hint.
2. The boxed loading state inside Observe's overview/timeline panel.
3. The existing dark full-page background ending at the viewport height.

Present the existing concrete captures and record each actual ruling in this
task's evidence, including acceptance/deferment or a specific requested
correction. Do not infer approval from this task, from passing tests, or from
the statement that styling predates R2. No particular new appearance or
background repair is authorized by this review finding.

If the maintainer requests a visual correction, change only the specified
existing owner and rerun the affected focused UI/capture checks; preserve
explicit pending states, readable controls, usable list, stable Back layout,
honest totals and no same-view Live flicker. If the current appearance is
approved, recording the ruling is sufficient; do not change it or rerun
unchanged runtime scenarios merely to produce another execution record.

## Preservation and handoff

FIND-TASK-004B-4 is closed for client-owned pre-header SQL/Card request
cancellation. Server computation termination is not newly promised or proven.
FIND-7 is resolved under the confirmed once-per-host-request ruling. Preserve
the earlier closed findings and accepted R2 behavior. Any further server-side
cancellation guarantee would need its own authority, not scope added here.

Record final candidate identity and evidence applicability. Preserve original,
R1 and R2 TDD violations; documentation does not repair test-first history.
Do not edit prior reports/evidence, stage, commit, push or launch another review
without instruction. No migrations, dependencies, new storage/trust mechanism,
benchmark run or broad regression suite is requested by this bounded task.

## R3 evidence

Candidate base: `7cf5bc4ab`; the change is the uncommitted working tree on top
of it, touching only `sdks/wyrd-sdk-ts/native/src/lib.rs` and this file.

### FIND-TASK-004B-8 — PASS

`NativeAbort.aborted` now documents the shared one-way cancellation state:
false until abort, never reset, and watched through a receiver per call, so one
abort stops every call, including one started afterwards
(`watch::Receiver::wait_for` checks the current value first).
`impl Default for NativeAbort` now documents the fresh, un-aborted signal it
gives. No logic, signature or public API changed. Proof is source inspection
of `sdks/wyrd-sdk-ts/native/src/lib.rs` (`NativeAbort` and its `Default`
impl), plus:

```bash
mise run fmt                                             # pass
(cd sdks/wyrd-sdk-ts/native && mise exec -- cargo clippy --locked -q)  # pass, no warnings
mise run ts:napi:check                                   # pass; generated index.d.ts unchanged
git diff --check                                         # pass
```

### Visual rulings (maintainer, 2026-11-19)

The maintainer was shown the existing captures in
`../TASK-004B-2026-11-19-01/captures/` (`lifecycle-*`). They were not retaken.

1. **Updating treatment** (50% dim plus the "● Updating" label, which overlaps
   the facet rail's click-to-filter hint): **accepted as is.**
2. **Boxed loading state** in Observe's overview and timeline panels:
   **accepted as is.**
3. **Dark background ending at the viewport height:** **accepted with no
   change. It is a capture artifact, not a defect.**
   - The band is the sidebar, not the page background. The sidebar is
     `position: sticky; top: 0; height: 100vh`
     (`src/routes/t/[tenantKey]/+layout.svelte`). In a live browser it stays
     pinned and fills the window at every scroll position.
   - The captures use `fullPage: true` (`src/lib/ui-pages.integration.test.ts`),
     which renders the whole document from the top in one frame, so the
     sidebar appears only within the first viewport height.
   - The maintainer first chose "fix now". After this explanation, they chose to
     accept it with no change.

No UI source changed, so no runtime or capture checks were rerun.

### Preservation

FIND-1 through FIND-7 and the accepted R2 behavior are untouched. The original,
R1 and R2 TDD violations stand. R3 is documentation only, so TDD is
NOT_APPLICABLE. No migration, dependency, benchmark or review was started.
Nothing has been staged or committed.
