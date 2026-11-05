# Discovery claim comparison

All six independent discovery reports are complete. Proposed findings group into one cause, not four distinct defects:

| Proposed IDs | Shared violated behavior | Evidence |
|---|---|---|
| B027-1, I027-R4-01, SYS-027-1, INTERACTION-1 | Narrow ordering dereferences current-document placement for previous-generation loaded panel IDs during valid document replacement | Source state/writer traces; installed-runtime scheduling model (behavior); actual in-memory mounted candidate runtime probes (invariants, system, interaction), with wide control in interaction |

Repository standards and maintainer reports propose no material issue. Their PASS assessments of structure and steady-state geometry do not falsify the runtime transition. All six discovery reports close FIND-027-1..5 from current source. New replacement failure does not require retained edits or held gestures and is separate from FIND-027-5's reset dependency.

A fresh focused follow-up is required: discovery exposes a reachable cross-generation render path outside existing narrow test coverage, and the system report leaves the minimal synchronization mechanism open whereas implementation/domain reports select current-placement membership at the ordering boundary. Follow-up must independently establish positive scope, reproduce the actual candidate, and resolve whether the local projection correction preserves loading/error/cancellation behavior without moving IO lifecycle. No intended verdict is supplied. All discovery proposals and follow-up results will then go to a separate fresh Ponytail validator; matching reports are not proof by vote.

## Focused follow-up outcome

`followup-review.md` is RESOLVED. It independently reproduces the actual candidate and establishes positive direct-Svelte-host scope. Its fresh-process memory-only source variant demonstrates that current-placement membership at the narrow projection closes the failure while preserving current loading/error rows, abort/stale-result exclusion, same-ID content reload, layout-only load suppression, invalid-to-valid recovery and wide behavior. This is a single shared cause and no new finding. The source variant is probe evidence only; the candidate was not edited. Final disposition remains the independent validator's responsibility.
