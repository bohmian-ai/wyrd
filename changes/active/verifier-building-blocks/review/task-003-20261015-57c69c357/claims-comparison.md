# Discovery claim comparison

All seven required discovery reports are complete and report FAIL. Claims are grouped by violated invariant, not line number. This is routing, not source validation or a finding ledger.

| Invariant / claim | Discovery source IDs |
|---|---|
| Exact Artifact evidence and Python code unsupported | BEH-01, INV-01 |
| Hard aggregate worker memory absent | BEH-02, INV-03, RES-01, SYS-003-1 |
| Cumulative writable and total scratch quotas reset / omitted | BEH-03, INV-02, RES-02, SEC-003-02, SYS-003-3 |
| Cumulative Python time overbooked / lost on cancellation or retry | BEH-04, RES-03, SYS-003-4 |
| DOCX semantic expansion exceeds bounded parsing | BEH-05, INV-04 |
| UTF-8 print truncation panics parent task | BEH-06, INV-06, RES-05, MNT-003-01, SYS-003-5 |
| Oversized read unit silently loses suffix | BEH-07, INV-05 |
| Required boundary regressions not authored | BEH-08; related matrix gaps in invariant report |
| Original-slot mapping unavailable in filesystem results | INV-07 |
| Existing empty/null JSON locations rejected | INV-08 |
| Cancelled workers leave stale PID kill authority | RES-04, SYS-003-2 |
| Result bounds applied after complete collection | RES-06 |
| Separate worker OS identity requirement | SEC-003-01 |
| Postgres tests leak into fast lane | REPO-003-01 |
| Mandatory changed-item rustdoc incomplete | REPO-003-02, MNT-003-02 |

Follow-up is required for two material ambiguities: BEH-08 authoring obligations versus TASK-005 composition ownership, and the interpretation of separate unprivileged identity under an unmapped user namespace. A fresh followup-rev will resolve those exact questions. All claims, including unique claims and any follow-up conclusions, proceed to fresh independent Ponytail validation. Agreement above does not establish truth.

Follow-up completed in followup-review.md with RESOLVED for both: the task requires authored gated authority regressions now while production composition remains TASK-005; an unmapped user namespace does not change inherited host identity, but the proposed closure must not require a different harmless primary GID. Both conclusions and their precise correction boundaries are now with the independent validator. No new findings were introduced by follow-up.

Root verification logs independently show 9 Agent, 21 hosted, and 5 actual-worker cases passing. Discovery reports do not claim these existing tests fail; claims concern source paths not protected by those cases. No destructive host-pressure reproduction is required or authorized by the review.
