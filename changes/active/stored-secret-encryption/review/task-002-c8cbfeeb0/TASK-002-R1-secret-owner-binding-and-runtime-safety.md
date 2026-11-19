---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-stored-secret-encryption
spec_revision: 1
parent_task: TASK-002
requirements: [REQ-003, REQ-008, REQ-009, REQ-012]
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4]
---

# Preserve secret ownership, maintenance progress and race winners

## Outcome and subject

Finish TASK-002's existing secret-owner cutover: substituted auth ciphertext is
refused, Operator maintenance progresses, key-service failures remain retryable
503, and concurrent replacement cannot overwrite a committed winner.

Approved [spec revision 1](../../spec.md), original
[TASK-002](../../tasks/TASK-002-standardize-secret-owners.md); reviewed base
`451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`, candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`.
[Validated findings](findings-validation.md) contain the diagnosed source paths,
authority, triggers and evidence; [verdict](verdict.md) fixes acceptance scope.
All four gaps are implementation defects, not demands for production churn to
repair missing evidence or historical TDD.

## Correction boundaries

**FIND-TASK-002-1 — REQ-003.** Active and Candidate human connections can share
issuer/client/method while having different connection IDs; their current AAD is
identical. Carry concrete trusted human connection identity through existing
ClientSecretOwner candidate preparation, persistence, decoder/open and maintenance
projection. Keep the common facade, existing candidate identity/CAS/revision/session
semantics, and remote encryption outside transactions. Reuse the auth/SQL owner;
no new schema, cipher, key source or authentication workflow.

**FIND-TASK-002-2 — REQ-009/012 and TASK-002 maintenance preservation.** Rewrap
currently prepares all tenant rows before its first commit. Healthy aggregate remote
work exceeding the tenant budget discards all progress on every pass. Preserve
bounded, resumable committed progress in OperatorSecrets and its existing SQL
selection/swap owner, retaining exact envelope/version fences, cancellation and
other-tenant delivery independence. Remote IO must stay outside transactions.
No job table, scheduler or future whole-store admin action is required.

**FIND-TASK-002-3 — REQ-009 and explicit approved outage behavior.** Completed
retryable preparation installs an unprepared credential whose failure becomes 502
at execution. Preserve transient preparation state through the existing Gateway
snapshot/resolution/error boundary so affected calls return the existing exact
service-unavailable 503. Preserve fallback/healthy credentials, background recovery,
generation/listener fencing and admitted immutable snapshots. Never carry old
opened material forward or add invocation-time KMS/PG waits, a TTL or another cache.

**FIND-TASK-002-4 — REQ-008.** Operator change's new reread/reprepare loop defeats
the correct SQL fence: a losing replacement can overwrite a newer committed one.
Return the existing conflict error on a lost observed-row fence at the service
owner. Keep remote IO outside transactions and preserve the winning envelope,
version and disable state. Correct the race-test expectation and TASK-002 evidence
claim that retry-success defines the contract. Do not weaken SQL fencing or add
coordination machinery. This restores settled behavior; no spec revision is needed.

## Ordered closure scenarios and proof

Add/select each regression and capture expected behavioral RED before production
changes, then GREEN and affected existing checks. Implementation records actual
exact selectors after tests exist; names below describe scenarios, not new function
names to copy. Historical scenarios 2/3 TDD violations stay recorded.

1. **Same-tenant auth substitution refuses — finding 1.** Use real server/Postgres
   and existing OIDC/token fixtures. Create distinct Active/Candidate owners sharing
   issuer/client/method with valid credentials; copy ciphertext between them and
   invoke actual candidate authentication. Assert refusal before secret use, intact
   owners authenticate, and existing cross-tenant refusal remains. Cover corrected
   maintenance reconstruction with the same owner identity. Owners: wyrd-auth
   resolver tests and wyrd-server identity integration; the consumer proof is the
   required journey tier, not only an isolated AAD unit.
2. **Completed Operator work survives pass limits — finding 2.** Through the
   production facade/local remote-service fixture and real repository Postgres,
   use individually successful delayed rows with a small tenant budget. Observe
   a committed subset and convergence across passes; race replacement/disable and
   verify existing exact-envelope fences preserve them. Assert cancellation and
   other-tenant delivery remain independent. Use existing Operator integration
   fixtures/time controls; no synthetic host load or benchmark.
3. **Completed Gateway preparation failure stays 503 and recovers — finding 3.**
   Real Gateway/server/Postgres journey: complete a retryable remote unwrap failure
   and install its snapshot, then invoke repeatedly. Assert exact existing 503 code,
   no provider dispatch, and unrelated healthy credential use. Restore the same key
   endpoint in the same server process and observe background recovery without
   resource edits/restart. Retain warm zero-extra-key-request assertions and existing
   snapshot lifecycle proof; an unfinished stall alone cannot close this finding.
4. **Lost replacement fence returns conflict — finding 4.** With delayed remote
   sealing, let disable commit first. Assert exact existing conflict code, unchanged
   winning status/envelope/version and no losing replacement write. Then let another
   credential replacement commit first; the loser conflicts, and the actual consumer
   uses the winner's value. Reuse the real server/Postgres Operator route/delivery
   fixtures and existing SQL conditional update; correct contradictory evidence.
   - Evidence: `service.rs` `change` drops the `CHANGE_ATTEMPTS` loop. A lost
     observed-row fence returns `OperatorConnectionConflict`.
     `pg_operator_connection_routes::rotation_racing_a_disable_conflicts_and_keeps_the_disable`
     returns 409 `WYRD_OPERATOR_409_CONNECTION_CONFLICT`. Transit sees 2 encrypts, with no
     re-seal. The envelope and `secret_version` are unchanged and the status is disabled.
     `pg_operator_delivery::racing_rotations_keep_the_first_commit_and_deliver_it` runs two
     delayed PATCHes: the first returns 200 and the second returns 409, and the next real
     delivery sends `Bearer` plus the winner's token. RED, before the fix: both tests failed
     with `left: (200, None)`, `right: (409, Some("WYRD_OPERATOR_409_CONNECTION_CONFLICT"))`.
     GREEN: `with-test-postgres.sh` with `db:migrate:all:inner` and `cargo nextest run --locked
     -p wyrd-server --features test-support --test integration --test-threads=1 -E
     'test(/^(pg_operator_connection_routes|pg_operator_delivery)::/)'` passed 19/19.
     The stale TASK-002 scenario 4 claim is corrected. TDD PASS.

## Runners and affected checks

Use repository-managed setup and the narrowest existing runners; coordinate Cargo
sequentially. Existing owners/runners are `test:identity:journey` for identity fixture
setup, `test:operators:integration` for route/delivery tests, and
`test:gateway:native:inner` for native Gateway journeys inside the Postgres wrapper.
Select only the changed tests using confirmed exact module-qualified nextest
selectors, with required features/profile/tools from their mise owner. Do not run
full multi-language journey aggregates for this bounded remediation; integrated
change review owns them. New/changed SQL logic gets focused real-Postgres owner
proof through test:sql setup, without requiring the whole SQL suite during iteration.

Run affected focused regressions plus format/lints, check:deps and
check:tenant-isolation. Run codegen:check only if generated contracts change; docs
checks only if public docs change. No migration is authorized or needed. Preserve
all excluded signing/hash/external-reference/storage behavior and existing typed
redacted responses. TASK-003/004 new administration, live-cloud credentials, Linux
startup proof, benchmarks and unrelated cleanup remain outside this remediation.

Handoff records exact RED/GREEN commands and results, added/moved wait inventory,
remaining accepted CI limits, and original TDD violations. Return implementation
evidence to the caller; do not automatically launch review.
