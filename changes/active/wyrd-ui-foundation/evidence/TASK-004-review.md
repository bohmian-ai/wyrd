# TASK-004 review — REMEDIATE

Original immutable review range:
`027fa942a16626fef2c3a0788bca7c1cf05ee7e8..5551695db9ba7656d30624efe47e0ddac5e1c0b2`.
Authority: approved UI foundation spec revision 6, TASK-004, AGENTS.md and the
accepted CRW/CRWM artboards. No earlier formal approval exists. This is a
self-review, not an independent review or final change approval.

## FIND-TASK-004-1 — MODERATE — draft metadata and history

`src/lib/server/changes/store.ts::MockChanges.save` initialized new drafts from
`summaries[2]` but did not replace team, service, activity, author, or creation
time and appended no revision event. Saving a checkout draft with a new team
therefore retained ledger metadata, disappeared from its own team filter, and
had no saved-revision entry in Timeline. This violates REQ-060, REQ-062,
REQ-065, REQ-099 and INV-009. Existing save/resume tests verified title and
subjects but could not detect these fields.

Closed through TASK-004-R1: draft-owned metadata replaces fixture metadata;
saves append revision events while prior snapshots remain intact. Extended
real HTTP draft journey failed on the missing revision event, then passed with
the event and submitted-team filter checks. No durable server contract changed.

## FIND-TASK-004-2 — MAJOR — narrow visual contract remains incomplete

`src/lib/features/changes/ChangeHeader.svelte`, `changes.css`, `Review.svelte`,
and the subject drilldown produce the supplied CRWM comparisons. In
`TASK-004-compare-review-mobile-light.png`, the implementation's first thread
ends near the bottom of the viewport; the accepted CRWM-02 fits two threads
and the Submit review surface. In the subject comparison, the diff begins near
600px rather than near 220px, leaving discussion below the initial viewport.
This is a concrete mismatch with TASK-004's locked major regions, hierarchy and
narrow action placement, REQ-082 and REQ-132. No horizontal overflow and reachable
controls are useful counterevidence, but do not satisfy the visual contract.

Required outcome: reproduce the accepted compact narrow header and workflow
regions with readable, operable controls; compare Review and Subjects at
390 × 844 in both themes and verify primary actions and discussion placement.
Route through wyrd-plan for bounded UI remediation. This finding remains open;
the task is not approved by this commit.

## FIND-TASK-004-3 — MODERATE — seeded draft resumes unrelated contents

`src/lib/server/changes/fixtures.ts::fixtureChange` constructs change_03's draft
by spreading `newDraft` and replacing only its title. The Change is the ledger
write-path change with ledger subjects, but Resume draft opens the checkout
intent, ranking subjects and checkout Claims. A save then replaces the ledger
record with those unrelated seeded values. This violates REQ-062/REQ-099 and
TASK-004's save/resume scenario. Newly created drafts round-trip correctly;
that existing coverage does not exercise the seeded draft.

Required outcome: seeded draft fields project the same owners, subjects,
intent and Claims shown in its detail view. Add a journey opening change_03,
resuming, and saving without edits, proving those fields remain consistent.
Route through wyrd-plan. This finding remains open.

## Verification and scope

Reviewed shared WyrdClient permission/mocking guards, load/action and error
boundaries, draft/run/review transitions, revision snapshots, retry behavior,
Markdown sanitization, source anchors, generated error examples, routes and
visual evidence. No CodeGraph index is present. Existing real HTTP tests cover
CSRF, cross-tenant rejection, list URL state, incomplete saves, run prerequisites
and retries, review conflicts and history. Supporting tests exercise permissions,
immutable comment revisions, safe GFM and override/closure separation.

The supplied full suite, build, codegen, token and format evidence was checked;
post-remediation focused HTTP journey passes (7 tests). Post-remediation full UI suite: 108 tests passed in 23 files; Svelte check:
0 errors and 0 warnings; production build: passed. Workspace Clippy is still limited
by the documented unchanged `wyrd-client/src/transport/http.rs:600` failure.
No lint or test was weakened. Commit authorization does not approve this task.
