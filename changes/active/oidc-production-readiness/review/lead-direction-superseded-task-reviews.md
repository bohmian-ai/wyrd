# Lead direction: TASK-001, TASK-003 and TASK-005 close through the change review

Decided by: the wyrd-run lead, 2026-10-03, at the direction of Steven Forrester
(human), following spec revision 11 (approved 2026-10-02).

`review/change-r1/verdict.md` returned `BLOCKED` only because TASK-001,
TASK-003 and TASK-005 have no task-review `PASS` on their final commits. No
further task review will run for them.

## TASK-001 and TASK-003 are superseded

Both were built on the design that spec revision 11 replaced with standard
libraries and conventions. The same reasoning routed TASK-004's open findings
(`TASK-004-r2/lead-direction-routing.md`):

- TASK-003's private BFF channel, server-side browser session rows, the flow
  cookie and the custom CSRF were deleted by TASK-010 and TASK-011, which use
  `openid-client` with a `jose`-encrypted cookie and a stateless API server.
- TASK-001's relying-party and connection code was reworked by TASK-009 onto
  `openidconnect`.

Re-reviewing their pre-revision-11 candidates would audit replaced code. Their
last fix commits remain in the integrated candidate and are judged there:

- TASK-001 r4: `FIND-TASK-001-24` and `-25`, fixed in `11b5ffa66` (reuse
  cleanup `e39bbb216`).
- TASK-003 r6: `FIND-TASK-003-19`, a test-harness shutdown ordering fix in
  `a14896e34`.

## TASK-005

The human authorized one final remediation (`TASK-005-r3`, fixed in
`9224aa768`) and directed that it go straight to the change review with no
further task review.

## Required change-review behavior

Do not require task-review `PASS` verdicts for TASK-001, TASK-003 or TASK-005.
Judge their surviving code and documentation directly in the integrated
candidate, including the commits named above, against the approved
specification revision 11. Classify any real defect normally.
