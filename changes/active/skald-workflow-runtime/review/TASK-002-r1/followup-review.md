# Focused follow-up: preflight/write external-reference replacement

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

`HEAD` still resolved to the candidate after inspection.

## Conflict investigated

The system review concluded that the write-time Active recheck and `FOR SHARE`
lock close the preflight/write race. The registry durability review proposed
`DUR-001`: preflight can validate UID/body A, then a completed delete and
re-registration at the same `(kind, space, name, version)` can make the write
transaction bind UID/body B without validating B.

## Source path inspected

- Request orchestration and transaction boundaries:
  `crates/wyrd/wyrd-server/src/components/cards/service.rs:574-584,995-1004,1055-1110`
- External body resolution and resolved graph validation:
  `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:46-82,388-467`
- UID rebinding at persistence:
  `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:602-675`
- Preflight identity lookup:
  `crates/wyrd/wyrd-sql/src/queries/cards/register.rs:400-455`
- Write-time recheck and relationship lock:
  `crates/wyrd/wyrd-sql/src/queries/cards/relationships.rs:17-54`
- Delete lifecycle and inbound-reference guard:
  `crates/wyrd/wyrd-sql/src/queries/cards/delete.rs:55-145`
- Identity reuse after terminal lifecycle:
  `crates/wyrd/wyrd-sql/migrations/20260601000014_card_registration_operations.sql:60-69`
- Existing concurrency proof:
  `crates/wyrd/wyrd-sql/tests/pg_cards_register.rs:591-721`

## Resolution

The transition described by `DUR-001` is reachable. The write-time lock starts
too late to preserve the UID/body that supplied preflight validation.

1. `resolve_card_references` resolves an external identity to UID A, loads A by
   UID, and supplies A's body to `WorkflowGraph::validate`
   (`resolve.rs:65-82,407-467`). The lookup is an ordinary Active identity read;
   neither it nor the by-UID load locks A.
2. `resolve_external` explicitly commits that tenant transaction before
   `write_registration` opens another transaction
   (`service.rs:995-1004`). No database lock or repeatable snapshot spans this
   boundary.
3. While no accepted Card yet points at A, `soft_delete_card_with_state` can
   lock and mark A deleted (`delete.rs:82-145`). The inbound-reference guard
   does not block deletion because the Workflow relationship has not been
   written.
4. After that delete commits, the partial identity index permits a new row B at
   the same exact identity because deleted, failed, and expired rows do not
   participate in uniqueness (`20260601000014_card_registration_operations.sql:66-69`).
   Waiting for B to become Active makes it eligible for the next lookup.
5. The write transaction discards the preflight UID values: it maps
   `plan.external_refs` down to identity-only `CardRef` values, calls
   `recheck_active_card_refs`, and replaces the plan with the returned pairs
   (`service.rs:1070-1075`). The SQL predicate contains identity plus
   `status = 'active'`, but no expected `card_uid` predicate or comparison
   (`relationships.rs:33-52`). Under the normal read-committed transaction it
   therefore finds and locks B.
6. `bind_card_references` looks up by identity and stamps the newly returned
   UID B into the submitted Workflow before hashing and persistence
   (`service.rs:1097-1104`; `resolve.rs:602-675`). Resolved graph validation is
   not rerun in the write transaction, so B's body and any different transitive
   Prompt graph did not supply the successful validation.

The `FOR SHARE` lock is still useful: once the write-time query has selected B,
it prevents B from being updated or deleted until registration commits. It
does not prove that B is A. The existing
`relationship_recheck_blocks_target_lifecycle_race` test starts at the
write-time recheck and verifies only this post-selection protection
(`pg_cards_register.rs:670-721`); it does not interleave replacement between
preflight and recheck.

This does not break SQL transaction atomicity: the write either commits all
rows or none. It breaks the semantic invariant that the durably bound graph is
the graph that passed resolved validation. A replacement Agent can therefore
make registration succeed while registered loading later rejects the stored
Workflow for Prompt binding or route/request-dialect incompatibility, contrary
to Revision 11 `REQ-014` and TASK-002 Scenario 2.

## Proposed finding resolution

`DUR-001` is **CONFIRMED** as proposed; no additional finding was discovered.

The minimum safe correction boundary is the existing write-time authority,
`recheck_active_card_refs`. Preserve the preflight `(CardRef, CardUid)` pairs
through that call and lock only the row matching both the exact identity and
the expected UID while requiring it to remain Active. Do not replace
`plan.external_refs` with a fresh identity-only resolution. If A was deleted or
replaced, return the existing unresolved-dependency refusal and roll back. This
keeps the current preflight validation owner, current `FOR SHARE` lifecycle
protection, and current transaction ordering; because the recheck precedes the
idempotency reservation, the caller can retry the same request/key and freshly
validate B.

Focused closure proof should preflight against A, commit deletion and an Active
B at the same identity before invoking the write-time authority, and assert
that the stale `(identity, A)` pair is refused with no registration operation,
Workflow row, or relationship committed. A fresh retry must validate B and
then either reject B's incompatible graph or accept it when valid. Retain the
existing test that proves a lifecycle mutation blocks after the expected UID is
successfully locked.

## Outcome

**RESOLVED** — the reviewers' claims are not both true across the full
preflight-to-write interval. The current lock closes lifecycle races only after
the second transaction chooses a row; it does not close the completed
identity-replacement window between the two transactions.
