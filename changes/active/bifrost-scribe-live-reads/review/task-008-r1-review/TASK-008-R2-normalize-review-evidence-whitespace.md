---
id: TASK-008-R2
title: Normalize committed review evidence whitespace
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-014, REQ-015, AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-7]
depends_on: [TASK-008-R1]
---

# TASK-008-R2: close the committed artifact whitespace gate

Route directly to `$wyrd-implement`. This is a static artifact correction.

## Subject and authority

- Approved specification: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20.
- Original tasks: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md` and `TASK-008-tenant-proven-per-file.md` in the same directory.
- Prior remediation: `changes/active/bifrost-scribe-live-reads/review/task-008-review/TASK-008-R1-close-review-gaps.md`.
- Reviewed candidate: `23eafa368bca19208faf8311eb7b5421e3660b38`; R1 parent: `6e7add054e33701ca5ecb52a5c859948b15161a3`; TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.
- Diagnosis: this directory's `findings-validation.md`, `standards-review.md`, `verification.md` and `verdict.md`; AGENTS §12 and the original/remediation task whitespace obligations.

## Diagnosis and selected correction

FIND-007-7 is a distinct static gate violation. The R1 commit adds ten Docker
progress lines with trailing spaces at
`changes/active/bifrost-scribe-live-reads/review/task-008-review/final-named.log:1-10`
and an extra blank line at EOF in that directory's
`task-review-invariants.md:55`. Both the R1 and cumulative committed-range
`git diff --check` commands exit 2. An empty working-tree check exits 0 but
does not inspect committed additions. The source/task/documentation portion
of the correction passes its scoped whitespace check.

The recorded test outcomes remain credible and no runtime regression is
alleged. The candidate still cannot satisfy its required final whitespace
gate. The fault originates in the captured evidence/report text; changing
production code, test selectors or gate configuration would not close it.

Normalize only the ten trailing spaces and report EOF at those existing
artifact owners. Preserve all substantive log content, selected tests,
outcomes and report conclusions. Reuse Git's existing whitespace check;
no new checker, log pipeline, dependency or test harness is required. Record
passing proof over the cumulative reviewed change, distinguishing committed
range checking from working-tree checking. No new architecture decision is
needed.

## Preserved behavior and non-goals

FIND-007-4/5/6 remain closed. Preserve all imports, panic docs, exact recipes,
assertions, tenant footer production/proof, schema deletion, shared scan,
partitions, native counters/terminal, remote protocol, leases and durable
behavior. FIND-007-3 remains accepted unchanged; Postgres tenant columns stay
excluded; the approved code remains `WYRD_VALA_500_QUERY_TENANT_INVARIANT`.

No source change, commit, Postgres wrapper, full mise lane or benchmark is
authorized by this task. Use `CARGO_TARGET_DIR=$PWD/target-review` and
`MISE_STATE_DIR=$PWD/.mise-review-state` if any permitted mise command is
needed. Do not weaken a gate or fabricate historical evidence. No RED runtime
test is appropriate for whitespace-only text correction.

## Acceptance and focused proof

FIND-007-7 closes when both artifacts contain no flagged whitespace, their
substantive content is unchanged, and the actual cumulative corrected diff
passes the existing check. With the correction still uncommitted, include
it by comparing the original base to the working tree:

```sh
git diff --check a7582db587c6170a290760f1741673125612b797
git diff --check 6e7add054e33701ca5ecb52a5c859948b15161a3
git diff --check
git diff --word-diff=porcelain -- changes/active/bifrost-scribe-live-reads/review/task-008-review/final-named.log changes/active/bifrost-scribe-live-reads/review/task-008-review/task-review-invariants.md
```

Require the three checks to exit 0 and inspect the final command for
whitespace-only changes. In a later immutable candidate, use
`git diff --check <original-base> <candidate>` as the confirmation proof.
Current independent runtime proof already passed 13 exact redux tests,
scoped formatting and scoped Clippy; do not rerun them for this text-only
correction. Full integration and capacity qualification remain caller-owned.
