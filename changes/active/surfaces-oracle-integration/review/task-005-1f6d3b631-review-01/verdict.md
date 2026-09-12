# TASK-005 task-review verdict

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 6 at candidate
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-005-audit-at-the-authorization-boundary.md`
- Base: `0af5eef72f83a95167f6ee3bdc873df9f9cc4254`
- Candidate: `1f6d3b6316bfd87c8a52e36b623b80d608304e19`
- Verdict: **SPEC_REVISION_REQUIRED**

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every actual permission evaluation appends exactly one allowed/denied event and fails closed | Several route Allows and denials do not append | Existing Bifrost test asserts no allowed list/describe row | FAIL |
| Canonical audit contains permission decisions and nothing else | Authentication, reconciliation, and storage lifecycle outcomes still append | Source inspection | FAIL |
| Gate remains ingest RBAC boundary and preserves dynamic permission | Gate audit owner and event projection | Focused Gate test | PASS |
| Scribe/Forge/reader-protection/publication transitions use lineage, not audit | Engine append sites removed; operational tables remain | SQL/server tests | PASS |
| Retained schema/hash/event time/daily layout are canonical | SQL, table definition, projection | SQL/codegen/Forge evidence | PASS |
| Watermark publication is replay-safe under tail growth and competing replicas | Publishers can read overlapping ranges with different upper bounds/batch IDs | Replay proof uses one unchanged range only | FAIL |
| Oracle commit-before-checkpoint replay stays within its allowed duplicate bound | No stable relay identity in staging | Test asserts only `count >= 2` | FAIL |
| No compatibility migration, second history owner, or new test | Greenfield edit and existing test files | Diff inspection | PASS |
| Repository standards | Independent audit | `standards-review.md` | FAIL |

## Material findings

### FIND-TASK-005-1 — MISSING: actual permission verdicts lack audit rows

`wyrd-server/src/bifrost/service.rs:31-55` appends only Deny. Allowed list/describe and non-create registration branches proceed without an event, and existing tests assert that absence. Other card, storage, and admin permission checks also lack one or both outcomes. This violates REQ-026 and the task's first criterion and makes audit failure fail-open for allowed reads. Every receiving authorization boundary must persist exactly one event before result/refusal, using the operation transaction where one exists.

### FIND-TASK-005-2 — INCORRECT: non-permission outcomes still enter canonical audit

`auth/login.rs:51-75`, card reconciliation/lifecycle paths in `components/cards/service.rs`, and `wyrd-storage/src/audit.rs` convert authentication, reconciliation, blob, or backend outcomes into authorization outcomes without evaluating a permission. This contradicts “authorization decisions and nothing else.” Preserve operational lineage/diagnostics but remove these canonical audit events.

### FIND-TASK-005-3 — INCORRECT: watermark-only publication duplicates overlapping retained ranges

`audit/publication.rs` lets publisher A read `1..N` and publisher B later read `1..N+1`; projection derives different batch IDs from those bounds, so Scribe cannot deduplicate the shared prefix. The candidate's revision-6 authority explicitly declares the watermark the only progress state. A safe frozen in-flight range requires changing that approved persistent-state decision, so this cannot be ordinary remediation under revision 6.

### FIND-TASK-005-4 — INCORRECT: repeated Oracle relay crashes create unbounded duplicates

`oracle/query_audit.rs:471-488` commits then checkpoints with no stable Postgres relay identity. Repeated commit-before-checkpoint crashes append A, B, C, and onward, while REQ-026A permits one duplicate. The existing test accepts `count >= 2`. Selecting durable deduplication identity and persistence semantics is another material persistent-data decision requiring specification authority.

### FIND-TASK-005-5 through FIND-TASK-005-10 — VIOLATION: repository standards fail

The independent audit's stable mapping is: `FIND-TASK-005-5` raw `PgConnection` (`STD-005-1`); `-6` redundant TenantConn predicates (`STD-005-2`); `-7` unearned dynamic Gate trait (`STD-005-3`); `-8` fully-qualified signature type (`STD-005-4`); `-9` stale/contradictory vocabulary and operations docs (`STD-005-5`); and `-10` dead tenant-isolation exemption (`STD-005-6`). Their exact locations, consequences, and testable corrections are preserved in `standards-review.md`.

## High-risk boundary review

The independent specialist confirmed incomplete/fail-open authorization audit and unbounded Oracle replay duplication. It passed Gate-before-Scribe ordering, TenantConn transaction coupling, WAL-before-row-return acceptance, per-tenant chain serialization, and the removal of engine-only audit events.

## Verification limits

Recorded green lanes cannot prove missing events or the changing-tail/repeated-crash interleavings. The candidate is older than the checkout; only immutable candidate blobs were used. Revision 7/TASK-006 later changes the frozen-range decision but is outside this TASK-005 subject and does not repair the Oracle or authorization findings.

## Prior-finding closure

No prior TASK-005 review verdict was supplied.
