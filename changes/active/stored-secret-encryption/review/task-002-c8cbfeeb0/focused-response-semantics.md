# Focused independent check: Operator race response semantics

Immutable subject: base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a` →
candidate `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`; SPEC-stored-secret-encryption
approved revision 1; TASK-002. Source-only focused review; no production changes,
no Cargo execution, and no validator conclusions used as evidence.

## Conclusion

The conflict promise applies to the current Operator credential-replacement path
being changed in TASK-002. It is not postponed exclusively to newly named TASK-003
actions. This is a settled implementation/acceptance-evidence defect, not genuinely
ambiguous approved behavior and not a requirement for a specification revision.

The smallest disposition is an INCORRECT finding against the Operator change owner:
when preparing a replacement against a row and losing its commit fence, return the
existing conflict error rather than reapply the original replacement against the
winning row. Correct the conflicting test assertions and implementation-evidence
claim. Preserve no remote IO in transactions, the exact-row fence, redaction,
disabled status, and separate explicit re-enable authority. No new contract is needed.

## Authority and task scope

1. Spec REQ-006 at `spec.md:142` expressly includes existing replacement operations
   in the common-facade cutover. REQ-008 at `:155–159` requires a complete envelope
   committed only while relevant secret/authority state has not changed and says
   a racing replacement/revocation returns conflict. The wording does not restrict
   this promise to an endpoint called Rotate or to work delivered in TASK-003.
2. TASK-002 explicitly assigns REQ-008 and its scenario 4 at `:146–153` says to
   race an **existing replacement** with revoke/update and assert fences preserve
   the winner. Its scope at `:73–81` requires remote crypto for existing writes
   outside transactions with atomic envelope/authority fences. These assignments
   remove any plausible task-boundary argument for deferring this race behavior.
3. TASK-003 describes adding/extending administration and explicitly says to reuse
   current actions where they already deliver replacement/revocation. It extends
   OperatorConnectionControl update/disable rather than creating a second
   competing durable owner. Nothing there exempts TASK-002's modified existing
   operations from their already-assigned REQ-008 obligation.
4. Finalized administrator design `design/admin-journey.md:60–75` first preserves
   existing replacement operations, then gives the resource flow: release the read
   transaction, prepare through the facade, CAS the envelope against observed
   metadata, and return conflict if concurrent replacement/revocation wins.
   This corroborates the spec rather than introducing a preferred response.
5. Operator architecture retains reversible disable and next-attempt behavior;
   it supplies no authority for silently retrying a losing credential replacement
   over a subsequently committed value. Data safety alone does not waive the
   explicit approved response semantics.

## Source and material behavior

Candidate `components/operators/service.rs:259–313` authorizes once, reads and
commits the current row, seals outside the transaction, calls update_connection
with the observed row, then commits. The SQL fence correctly returns None when
status/config/version/envelope changed. However the outer CHANGE_ATTEMPTS loop
then rereads current state, reapplies the original cloned request, and seals and
commits again. Conflict is returned only after every bounded attempt loses.

The named integration test `pg_operator_connection_routes.rs:190–237` stalls
Transit encryption, commits DELETE/disable, and expects the original replacement
request to return 200 after a third encrypt call; secret_version increases once
and Disabled is preserved. That proves the loop, not compliance with the conflict
response. The safe status preservation is real and should remain protected.

A second concrete supported race demonstrates why this is more than wording:
replacement A reads the old secret and waits on KMS; replacement B commits a new
credential first. A loses its first fence, rereads B's envelope, then resubmits A's
original secret and can return success. B's committed replacement is overwritten
by the stale request instead of A surfacing the required conflict. The existing
exact-row SQL fence is defeated at the service orchestration level, so changing
SQL fencing is unnecessary. No arbitrary input or unsupported deployment is
needed; concurrent credential replacement is expressly covered by REQ-008 and
TASK-002 scenario 4.

Base service.rs had no CHANGE_ATTEMPTS retry: it held FOR UPDATE through prepare
and update. The candidate had to release that lock to support remote KMS safely;
introducing reread/reprepare success semantics was a new decision. Claiming the
loop is an unchanged legacy contract is therefore incorrect.

## Why this is not BLOCKED or SPEC_REVISION_REQUIRED

TASK-002's scenario mechanics require preserving the race winner; they do not
require a successful 200 reprepare. The contradictory passage is the implementation
**evidence** at `TASK-002-standardize-secret-owners.md:162–167`, which calls the
service loop “the contract” and calls the original 409 assertion erroneous. An
implementation ledger and test cannot override the approved specification under
changes/spec-driven-development.md Authority. Correcting those assertions/notes
and the responsible service behavior satisfies the original scenario and the
spec simultaneously. Thus no contradictory mandatory task mechanics prevent a
settled fix, and no new human contract decision is necessary.

The current user summary promises that racing rotation leaves the connection
disabled; returning a conflict preserves exactly that outcome. It does not
explicitly authorize success after reread/reprepare or authorize overwriting a
concurrent replacement. The review must not infer such a waiver.

## Minimum observable closure

Keep the existing delayed-Transit real-server/Postgres race, but assert the
replacement loses with the stable OperatorConnectionConflict error, the disable
remains committed, and no replacement envelope/version is written. Add a focused
competing-replacement variant where the first commit's credential remains and the
loser returns conflict; prove the stored/current consumer uses the winner. Reuse
the existing service, SQL conditional update and provider fixture. No new action,
schema, lock held over KMS, or broad concurrency framework is required.
