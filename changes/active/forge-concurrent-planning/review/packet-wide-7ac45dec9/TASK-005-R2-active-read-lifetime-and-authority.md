---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: TASK-005-R1
remediates: [FIND-TASK-005-R1-1, FIND-TASK-005-R1-2, FIND-TASK-005-R1-3, FIND-TASK-005-R1-4, FIND-TASK-005-R1-5]
---

# Make active-read lifetime and destructive authority continuous

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Original task: `changes/active/forge-concurrent-planning/tasks/TASK-005-R1-active-table-reader-cut.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

## Diagnosis

Five revision-11 gaps share the active-read boundary. Dropping a stream spawns
claim deletion independently of analytical supervisor cancellation/join, so a
descendant can still perform IO after release. Acquisition and its permitted
retry reuse an early duration, letting PostgreSQL rebase `abandon_after` past
the one query deadline. Forge's three destructive paths commit their exclusive
authority check before external catalog/object effects, permitting a new reader
to commit the old cut in the gap. `production_closeout` still bans unrelated
sibling expiry/cleanup that revision 11 permits. Finally, Redux duplicates raw
`bifrost_tables` SQL instead of using the SQL-layer owner.

## Intended correction outcome

Claims outlive every analytical descendant, expire at exactly the existing
query deadline, and serialize continuously with snapshot expiration, expired
cleanup, and orphan cleanup. Tests permit independent sibling maintenance, and
durable SQL remains owned by `vala-sql`.

## Decision-complete recommendation

- Transfer the claim on analytical abandonment into the existing supervisor-
  owned graph settlement; cancel and join descendants before release while
  keeping caller drop nonblocking and deadline fallback intact. Direct spawned
  release may remain only where no analytical descendants exist.
- Retain the attempt's immutable local deadline and derive a positive remaining
  duration immediately before every SQL acquisition/reacquisition; PostgreSQL
  still stamps the row. Reject acquisition once no duration remains.
- Keep the existing per-table maintenance authority as the sole ordering
  mechanism, but hold its exclusive ownership continuously from the final
  reader check through the corresponding external effect's known outcome.
  Preserve uncertain-effect evidence and idempotent recovery. Do not recreate
  epochs, IO gates, advisory locks, per-query sessions, or a second protocol.
- Delete the tenant-wide sibling-strategy ban while preserving exact orphan
  identity, terminal phase, deletion, survivor, and query assertions.
- Replace the Redux raw registry query with the existing typed `vala-sql`
  catalog owner; retain `TableAuthority` as orchestration, not SQL ownership.

## Preserved behavior and non-goals

- Preserve tenant RLS, one-statement acquisition, explicit uncapped deadlines,
  promotion/non-destructive catalog movement, exact cut reconciliation, and
  drop's nonblocking caller behavior.
- Preserve immediate maintenance after final release and all real roots.
- Do not reintroduce any deletion claim prohibited by revision 11.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-005-R1-1` | A dropped analytical query retains active rows until every descendant is joined; Forge refuses destruction meanwhile. |
| `FIND-TASK-005-R1-2` | Initial acquisition and retry both expire at the original absolute query deadline. |
| `FIND-TASK-005-R1-3` | Each destructive path and acquisition have one deterministic ordering with no check/effect gap. |
| `FIND-TASK-005-R1-4` | Exact orphan evidence passes while independently eligible sibling expiry/cleanup is allowed. |
| `FIND-TASK-005-R1-5` | No raw `bifrost_tables` query remains outside its SQL owner. |

## Focused proof and broader verification

Hold a follower stopped-but-not-joined after stream drop and prove protection;
delay acquisition and forced reacquisition and assert PostgreSQL-time expiry at
the original deadline; pause each destructive path after its former last check
and race acquisition to prove one ordering; run closeout with an eligible
sibling; run tenant isolation and authority tests. Then run Oracle/Forge
journeys, `verify:bifrost`, principals integration, boundary checks, format,
lints, and diff check.

