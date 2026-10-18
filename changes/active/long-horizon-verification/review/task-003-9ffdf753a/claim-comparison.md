# Discovery claim comparison

All seven required discovery reports are complete. This comparison groups claims; it does not validate them.

| Shared invariant / behavior | Discovery proposals | Other evidence / uncertainty | Routing |
|---|---|---|---|
| Existing concurrent shutdown cleanup ownership | B-001, INV-REV-001, MAINT-001 | Concurrency reviewer excludes hypothetical concurrency because production shutdown is sequential; implementation reviewers cite explicit existing shared-owner contract and task preservation | Fresh followup-rev required and completed; FOLLOW-001 resolves contract scope without claiming an observed production race. Send to independent validation. |
| Progress while A keeps offering calls | B-002, INV-REV-002 | Concurrency report accepts B with active held A; distinction between active A and continuing offered traffic materially affects acceptance | Fresh followup-rev required and completed; FOLLOW-002a distinguishes evidence. Send to validation. |
| Same enqueue key recovered after pressure and replayed once | B-002 | Unique proposal; calls on same Run may mint new keys | FOLLOW-002b traces shared client and existing HTTP seam. Send to validation. |
| Complete changed Rust error/panic documentation | STD-003-01 | Maintainer report treats local error-list omissions as nonblocking; repository has explicit rustdoc exception to general style threshold | Send all locations to validation against exact authority. |
| Normative finite-outbox audit/operational contract | STD-003-02 | Unique standards proposal | Send to validation; prose correction must preserve approved existing durability boundary. |
| Diagnostician evidence for failure-driven adaptations | STD-003-03 | Task records causes/confirmation, but no inspectable reports supplied; evidence-only proposal | Send to validation; no production defect presumed. |

System and durability discovery found no material runtime defects. No proposals are discarded by majority or agreement. Follow-up was limited to two material conflicts and their shared producer/evidence paths; no extra pass was requested to obtain agreement. The independent structured Ponytail validator receives every report and the actual cumulative range, with no intended verdict.

The requested approved revision 13 artifact remains unavailable: both immutable commits hold revision 14/draft. Caller clarification was requested. This is an authority limit separate from implementation findings; it cannot be resolved by reviewer votes.

## Superseding owner authorization — 2026-10-17

Steven explicitly authorized current spec.md TASK-003 text and excluded revision 14's draft pre-release schema addition. The prior authority blocker is resolved. See updated verdict.md and evidence-reassessment.md; historical discovery reports remain preserved. Supplied Reports A/B were copied verbatim to diagnostician-reports.md and close FIND-6 diagnoses 3/5, leaving only 1/2/4. One remediation task is issued in TASK-003-R1-close-admission-review.md.
