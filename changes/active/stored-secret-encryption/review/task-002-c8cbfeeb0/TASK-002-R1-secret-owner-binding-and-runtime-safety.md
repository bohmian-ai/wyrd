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
   - Evidence: `ClientSecretOwner` gains `connection_id`. A human connection's envelope
     authority is now `[client_id, method, connection_id]`, built at three points: in
     `stage`, which seals to the observed candidate's id or a fresh UUIDv7; in
     `human_connection_trusted_issuer`, which opens with `row.connection_id`; and in
     maintenance `SealedSecretTable::HumanConnections` (`ARRAY[client_id, client_auth,
     connection_id::text]`). `insert_human_candidate` takes that id.
     `put_candidate` conflicts unless the locked candidate is still the one `stage` read.
     Remote sealing stays outside transactions. No schema change.
     `identity_e2e::tenant_connection_refuses_a_secret_copied_between_connections` copies
     the Active row's valid secret onto a same-issuer/client/method Candidate. Its test
     sign-in is refused with 503 `WYRD_AUTH_503_VERIFY_UNAVAILABLE` and the Candidate
     stays untested. The Active connection still signs alice in, and a re-staged
     Candidate tests. RED, before the fix: `left: 200, right: 503`, because the copied
     secret completed the test sign-in. GREEN: the identity lane wrapper ran `cargo
     nextest run --locked -p wyrd-server --test integration --run-ignored=all
     --test-threads=1 -E
     'test(/^identity_e2e::(tenant_connection_|tenant_human_login|tenant_callback)/)'`,
     8/8 passed. `tenant_connection_rotation_journey` covers maintenance rewrap of the
     corrected authority, then sign-in. `-p wyrd-auth --lib --all-features -E
     'test(/pg_tests::/)'` passed 106/106, including the cross-tenant copy refusal.
     `-p wyrd-server --lib --features test-support -E 'test(/auth::callback::/)'`
     passed 19/19. TDD PASS.
2. **Completed Operator work survives pass limits — finding 2.** Through the
   production facade/local remote-service fixture and real repository Postgres,
   use individually successful delayed rows with a small tenant budget. Observe
   a committed subset and convergence across passes; race replacement/disable and
   verify existing exact-envelope fences preserve them. Assert cancellation and
   other-tenant delivery remain independent. Use existing Operator integration
   fixtures/time controls; no synthetic host load or benchmark.
   - Evidence: `secrets.rs` `rewrap_tenant` now runs the remote rewrap of one row with
     no transaction open, then commits that row's fenced `rewrap_connection` swap in its
     own short transaction before starting the next row. `moved` is counted through
     `&mut`, so a tenant cut off by its budget still reports the rows it committed. The
     existing SQL exact-envelope/version swap is reused; there is no job table or
     scheduler.
     `pg_operator_delivery::rewrap_commits_progress_across_tenant_budgets` holds every
     Transit answer for 200 ms with a 600 ms tenant budget and 3 stale rows. Pass 1,
     raced by a Slack rotation and a PagerDuty disable, commits 1–2 rows, and repeated
     passes converge on `[true; 3]`. The rotated token opens through `OperatorSecrets::open`
     and the disable is kept. RED, before the fix: `one pass commits the rows it finished
     within its budget: 0`. GREEN: the operator integration selector from scenario 4
     plus `-E 'test(/^pg_operator_delivery::(rewrap|slow_rewrap|stalled_rewrap|next_attempt)/)'`
     passed 4/4. `slow_rewrap_never_holds_back_another_tenants_delivery` still proves the
     other tenant's delivery, the cancelled shutdown and the rollback of a stalled pass;
     the full Operator suite passed 19/19 apart from this test, which passed on rerun after
     its plaintext assertion was corrected. TDD PASS.
3. **Completed Gateway preparation failure stays 503 and recovers — finding 3.**
   Real Gateway/server/Postgres journey: complete a retryable remote unwrap failure
   and install its snapshot, then invoke repeatedly. Assert exact existing 503 code,
   no provider dispatch, and unrelated healthy credential use. Restore the same key
   endpoint in the same server process and observe background recovery without
   resource edits/restart. Retain warm zero-extra-key-request assertions and existing
   snapshot lifecycle proof; an unfinished stall alone cannot close this finding.
   - Evidence: `GatewayCredentialSource::ManagedSecret` gains `pending`, set from the
     load's `KmsError::Unavailable`. The resolver maps it to `CredentialError::Pending`,
     and the engine to `FailureClass::CredentialPending`. That class is not billable,
     does not count toward the breaker, and continues to fallback.
     `CallExecution.credential_pending` holds when no attempt was billable;
     `respond` maps it to the existing `ServiceUnavailable` and never to
     `GatewayUpstreamUnavailable`. There is no new cache, TTL or invocation-time KMS/PG
     wait: resolution still reads only the admitted snapshot. `TransitFixture` gains
     `set_available`. `compatible::failed_managed_secret_preparation_stays_retryable_and_recovers`
     puts Transit into 500s and triggers a reload with an unrelated deployment write. Two
     calls return 503 `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, with no upstream dispatch;
     no-credential `acme-first` answers 200. Once the same endpoint is back, the call is
     served with the submitted key, with no edit or restart. RED, before the fix:
     `left: 502, right: 503`. GREEN: `WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream`
     with `with-test-postgres.sh`, `db:migrate:all:inner` and `cargo nextest run --locked
     -p wyrd-testing --test gateway -P journey --run-ignored=all -E
     'test(/^compatible::(failed_managed|cold_call|managed_secret)/) | test(/^resilience::/)'`
     passed 12/12, including the warm zero-Transit-request journey. `-p wyrd-gateway --lib`
     passed 78/78. `-p wyrd-server --lib --features test-support -E
     'test(/components::gateway::/)'` passed 87/87. TDD PASS.
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

## Handoff

- Commits: `6360b5f4d` (scenario 4), `d725e6e11` (scenario 1), `d4bc8d8d0`
  (scenario 3) and `e8306fee8` (scenario 2). The RED/GREEN commands and results are
  under each scenario. Every scenario is behaviorally PASS and TDD PASS.
- Wait inventory: no hot-path wait was added or moved.
  - The Gateway resolution still reads only the admitted snapshot; the pending flag is
    set by the existing background reload.
  - Operator rewrap is background maintenance. It now issues one short read, then one
    fenced swap transaction per row, with no transaction held across a key-service call.
  - Operator `change` and human-connection `stage` keep sealing outside transactions.
- Final checks pass: `mise run fmt`, `mise run lints`, `check:deps`,
  `check:tenant-isolation`, `codegen:check` (the `OperatorConnectionConflict` rustdoc
  changed) and `git diff --check`. No migration was added; docs did not change.
- Accepted CI limits remain: the Linux startup-prod lane and the
  `kms-integration-cloud` live-cloud keys, both per the TASK-002 evidence.
- The original TASK-002 TDD violations for scenarios 2 and 3 stay recorded there.
- Review is left to the maintainer.
