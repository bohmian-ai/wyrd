# PostgreSQL audit claim comparison

This report routes independent discovery proposals; it does not validate them.
Candidate and user authority are fixed in [subject.md](subject.md).

| Cause / authority question | Discovery sources | Validation scope |
| --- | --- | --- |
| Rejected per-execution SQL writer/checkpoint-head authority | USER-PG-001 in all three reports | Human-directed removal, not a new FIND. Validate complete production/recovery/transport/WAL/migration/test dependency closure; moving writes is insufficient. Existing generic Bifrost batch/catalog/source SQL remains allowed. |
| SQL remains sole full terminal/error custody and public result-selection authority | PG-AUDIT-001, ES-01 | Reconcile with existing FIND-TASK-001-17 and affected part of 16; preserve queue status/retry/completion and independent group/Operator bookkeeping. Do not require removing every SQL status/error cache or result reference merely because it is JSON/a UUID. |
| Historical evidence reads rely on live Cards or a queue row | PG-AUDIT-002, ES-02 | Reconcile with existing FIND-TASK-001-16; use recorded identity and canonical Bifrost resolution with current scoped authorization, not new tombstones/direct queue rows. |
| Recovery after removing SQL head | Bifrost-data removal consequence | Existing durable batch/file-list/source-cut/WAL owners can be extended for table-scoped authoritative recovery coverage; current ordinary latest-visible query is insufficient. Missing approved owner extension is not by itself a new material approval decision. Preserve typed unavailable/ambiguous outcomes and accepted residual worker risk. |

Prior FIND-TASK-001-18 is withdrawn by the direct fast-return decision.
Prior FIND-TASK-001-20's proposed new authority-handoff hardening is superseded
by the user's accepted rare timing gap. Prior FIND-TASK-001-25's relocation-only
correction is superseded by removal, not revived as a second SQL protocol.
Prior finding24's rejected-head tuple code should be deleted with that owner,
not separately repaired to preserve the rejected table. Other prior findings
remain separately recorded unless the validated deletion actually subsumes
them; no automatic closure or comprehensive re-review is claimed.

No follow-up discovery is selected: the three reports agree on the permitted
owner boundary and present no unresolved source/authority conflict requiring a
new pass. Agreement is not proof; the fresh validator must independently trace
all retained claims, source coverage and minimum correction burden, including
whether this limited selection needs any further targeted specialist coverage.
