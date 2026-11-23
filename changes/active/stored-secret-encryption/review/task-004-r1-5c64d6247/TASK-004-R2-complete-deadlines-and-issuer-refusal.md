---
id: TASK-004-R2
kind: remediation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 2
original_task: TASK-004-wrapping-key-lifecycle.md
findings: [FIND-TASK-004-1, FIND-TASK-004-2]
---

# Complete the shared deadline and preserve refused issuer envelopes

## Subject and outcome

Reviewed candidate `5c64d6247e8feea501df090593c9d9b64c9f3931`, prior candidate `576c5b35381c1c08c4668b8d5aedc4c39c6d73db`, original implementation base `45bc4ea9336a4a6ab1211c8ab34e9147ac752ffb`. Approved SPEC-stored-secret-encryption revision 2 and original TASK-004 remain authoritative. This follows R1; FIND-TASK-004-3 is closed and must remain so.

After this correction, waiting for an operator connection cannot consume the maintenance report's time budget, and restarting unchanged confidential issuer configuration cannot silently replace an old envelope that the deployment has refused.

## FIND-TASK-004-1: enforce the operation deadline at the SQL owner

**Root cause.** The pass owns a deadline, but the SQL boundary turns its remaining time into a new statement timeout. `OperatorPool::begin_bounded` first waits for acquisition and BEGIN without that deadline, then installs the original duration. Statement timeouts restart for each statement; they do not bound a complete operation. The scan/CAS helpers also await setup and transaction completion without the pass deadline. Ordinary pool contention can therefore still turn a bounded pass into an edge timeout. See [root-cause-deadline.md](root-cause-deadline.md) for the complete source trace.

**Correction boundary.** Keep one authoritative deadline, owned by the pass and enforced throughout the existing maintenance SQL/OperatorPool path: connection acquisition, transaction start/setup, statement execution/fetch, and transaction completion. All stages consume elapsed time from that same budget; none starts fresh work or grants a fresh duration after expiry. Carrying an absolute monotonic deadline or an equivalent non-resetting representation is implementation-owned. This is a local execution budget, not a new durable coordination clock.

Enforce the caller deadline for waits PostgreSQL cannot see, alongside PostgreSQL's own remaining statement bound and transaction ownership. Expiry returns the existing bounded, observation-only result rather than an unrelated pool error. Preserve cancellation of a blocked statement, rollback/discard hygiene for an abandoned uncommitted transaction, prior commits, tenant/row/exact-envelope CAS and remote crypto outside transactions. Never detach a write or leave a later COMMIT scheduled after abandoning its transaction. Keep the keyless boot presence check's existing fail-closed behavior.

Count only commits observed as successful and resume from committed envelopes when an acknowledgement was not observed. The existing blocked-uncommitted-swap guarantee remains: releasing its lock later must not cause that abandoned update to commit. Do not extend it into a promise that timing out an already-issued COMMIT can undo a commit PostgreSQL accepted. Existing observation-only counts and repeat-by-envelope semantics cover that acknowledgement uncertainty; no new response field, job or acknowledgement framework is authorized.

A timeout around acquisition alone, subtracting acquisition time without bounding acquisition/setup, raising the edge timeout, enlarging the pool, or suppressing pool errors does not close this cause. Correct both existing scan and swap paths at their shared SQL/OperatorPool boundary; do not add a general transaction framework or another connection abstraction. Private signatures and helper layout remain implementation-owned.

**Closure proof.** Use real repository Postgres and the existing maintenance/SQL fixtures:

- Occupy the operator connections while a pass reaches acquisition. Assert the existing bounded report arrives within its own budget, prior committed envelopes stand, and unobserved stores are not counted. Release capacity and repeat the same mode/destination to converge without rewriting completed envelopes.
- Release capacity near the original deadline, then hold the next statement on a real table/row lock. Assert acquisition time is not granted again to SQL. Establish ordering through held connections/locks and observable progress, not added sleeps.
- Keep R1's locked-discovery and blocked-uncommitted-swap tests green. After the abandoned swap's lock releases, prove it never lands later and retry safely. Inspect cancellation/ownership across setup and commit as well as the demonstrated contention paths; do not add disconnected mocks or a fixture per await.

**Evidence (commit `bb73d8ba0`).** `OperatorPool::begin_bounded(deadline: Option<Instant>)` runs acquisition, BEGIN and the transaction-local `statement_timeout` setup within the pass deadline. The timeout is the time left once setup finishes, so it is never the original duration. `stored_secrets` and `swap_stored_secret` run fetch, update and `commit` through the same `within(deadline, …)`. Expiry at any stage is `Ok(None)`, the existing bounded outcome. A cut future is dropped where it stands: SQLx queues the rollback and flushes it when the connection returns to the pool (`sqlx-core-0.9.0` `pool/connection.rs` `return_to_pool` pings), and an unsent COMMIT is never sent. `Some(true)` is returned only after an acknowledged commit. The pass's per-call `wait` computation is removed. The keyless boot presence check passes `None` and still fails closed. Remote crypto stays outside transactions.

- Starvation: `platform_admin_e2e::stored_secrets::a_pass_starved_of_connections_answers_within_its_budget`. A limit-2 pass commits 2. Then every serving-pool connection is held. The pass returns `(0,0,0,true)` in under 2.5s with every envelope unchanged. After release, `(2,3,0,false)`, and the 2 finished envelopes are byte-identical. RED on `5c64d6247`: `expect("the pass answers")` panicked on the pool's `SqlError` after the 5s acquire timeout. GREEN.
- Late capacity then lock: `…::a_pass_that_waited_for_a_connection_keeps_its_deadline_on_a_lock`. One held connection holds `LOCK TABLE wyrd.auth_human_connections`. The other is released at 3/4 of the 2s budget, which is the scenario's stimulus, not a synchronization wait. The pass answers `(0,0,0,true)` in under 2.5s with nothing moved, and a repeat after unlock gives `(0,5,0,false)`. RED on `5c64d6247`: "the pass took 3.525s of a 2s budget" (acquisition time granted again as a fresh statement timeout). GREEN.
- R1 regressions are green: `…::a_pass_stalled_on_a_locked_store_answers_within_its_budget` and `…::a_swap_stalled_on_a_locked_row_is_abandoned_and_resumed`. The latter still proves that the abandoned uncommitted swap never lands after its lock releases, and that the repeat is safe. The SQL seam test `wyrd-sql pg_stored_secrets::maintenance_lists_every_tenant_and_swaps_only_the_bytes_it_read` and `wyrd-server boot::kms_boot_pg_tests::keyless_boot_refuses_only_when_ciphertext_is_stored` are also green.
- Diagnosis (first test design). **Symptom:** both new tests still failed GREEN when driven over HTTP (`504`; 3.53s). **Evidence:** `components/auth/platform_extractor.rs:153` confirms the platform session through `state.postgres.operator_pool()` before the handler runs, and 3.53s ≈ 1.5s release + a full 2s budget. **Cause:** holding every connection blocked authentication, not the pass. The finding's starting state is an already-authorized pass. **Fix site:** the test only. It now drives `StoredSecretMaintenance::run` over the serving pool (`srv.state().postgres.operator_pool()`), as the existing configured-issuer test does. The first draft held the fixture's separate `srv.operator_pool()`, which never starved the server. `hold_every_connection` retries `PoolTimedOut` because background workers (Forge, sweeper) share the pool. No production code or assertion was weakened. No diagnostician was spawned: the cause is source-evident and only the new, unaccepted test changed.

## FIND-TASK-004-2: make credential replacement an explicit boot decision

**Root cause.** Boot reads a full trusted issuer row but retains only its ciphertext. The shared serializer then opens that ciphertext under the new configured binding and treats different plaintext, changed binding, and every decryption error as the same reason to seal configured plaintext. This loses the information needed to distinguish intentional configuration replacement from an unreadable unchanged credential. The facade already refuses correctly; boot reconciliation turns that refusal into a write. See [root-cause-issuer.md](root-cause-issuer.md) and the independent [failed-open validation](focused-issuer-validation.md).

**Correction boundary.** Reconcile the prior trusted row and configured issuer in the existing boot issuer owner, using existing persisted tenant/issuer/client/authentication metadata to establish binding changes. Keep cryptography/context construction in the existing shared secret owner and facade. Make the following distinction authoritative rather than adding KMS-error-specific fallback branches:

| Existing state / observation | Required existing behavior |
| --- | --- |
| New issuer or no existing secret | Preserve normal creation and its validation/KMS failure behavior. |
| A genuine binding change established from trusted prior and configured metadata | Preserve intentional configured replacement. Seal for the new binding; never attach old ciphertext to incompatible new client/method metadata. Preserve changes to secret-free authentication. |
| Same binding; open succeeds and plaintext equals configured secret | Retain the exact committed envelope; restart/key activation is not renewal. |
| Same binding; open succeeds and plaintext differs | A configured secret change is established; seal the configured replacement under the active key. |
| Same binding; open/comparison fails for any reason | Do not authorize sealing. Preserve the committed envelope and compatible binding, retaining ordinary refusal for this resource. Do not introduce a whole-server startup outage for this existing credential's local failure. |
| Explicit authorized owner replacement | Retain the existing typed replacement path, authorization and CAS without requiring old decryption. |

An error is an unavailable comparison, not proof of equality or changed secret. Preserve all such failures, including removed/denied key, provider outage, authentication failure and malformed ciphertext, rather than enumerating particular errors that should skip sealing. Redacted diagnostics may report that comparison was unavailable. Non-binding configuration fields retain their established update behavior where compatible; this task does not freeze the whole row.

Simply propagating the open error through boot would stop the server; blindly retaining old bytes under changed binding would break intentional replacement. Resolve both at the boot reconciliation owner with the existing trusted metadata and shared crypto leaf. Leave helper names, return types, serializer placement and control flow to the implementer. No fingerprint, schema change, extra secret copy, incident lock or new configuration is authorized.

If binding is unchanged and old material cannot open, a configured plaintext change cannot be established by this comparison. Controlled read restoration or the existing explicit owner replacement is the supported recovery. A demand to infer such a change from unreadable ciphertext requires a separate material decision; do not silently invent a mechanism or weaken containment.

**Closure proof.** Extend the existing configured-issuer real-server/Postgres journey and local OIDC/KMS fixtures:

- With unchanged tenant/issuer/client/method/secret under k1, restart with k2 active and k1 removed. Assert byte-identical committed envelope, no replacement seal, ordinary refusal for this issuer, and continued server/unrelated-operation availability. The removed-reference case is deterministic and needs no fixture delay. Also exercise old-decrypt refusal with active wrap still possible where the existing fixture supports it.
- Restore controlled k1 access: unchanged boot keeps the bytes, consuming use succeeds, and only explicit maintenance moves the envelope. Separately preserve explicit typed owner replacement while old access remains denied, with identity/CAS and no old-decrypt prerequisite intact.
- Keep readable equal/different-secret cases. Exercise genuine client-id/method changes, including a binding change with the old key absent, and prove the new envelope opens under the new binding. Preserve creation, secret-free configuration and discovery-failure restart behavior using existing focused tests where adequate.

**Evidence (commit `0e3308c57`).** `seed_trusted_issuers` now keeps the full prior `TrustedIssuerRow`. `reconcile_trusted_issuer` (`boot/issuer.rs`) is the single decision:
- A new issuer, no stored secret, or a changed `client_id`/`client_auth` versus the prior row is sealed afresh via `issuer_write_from_trusted`.
- Under the same binding, `trusted_issuer_secret_matches` returns `Some(true)` (keep the exact bytes), `Some(false)` (reseal the configured secret) or `None` for any open failure (keep the bytes, with a redacted warn). The issuer then keeps refusing, and the server keeps serving.

The crypto stays in `ClientSecretOwner::opens_to`. `keep_or_seal` is deleted. `issuer_write_from_trusted` lost its `stored` parameter and always seals. All 12 callers were traced: HTTP create, jwt_bearer and the tests already passed `None`. `issuer_write_with_envelope` serializes a kept envelope. The explicit replacement route is unchanged. There is no schema, fingerprint or configuration change.

- `platform_admin_e2e::stored_secrets::a_restart_keeps_a_configured_issuer_secret_it_cannot_open`:
  - k1 seal, then restart with k2 active and k1 removed: the envelope is byte-identical, Transit `encrypt` count is unchanged, `trusted_issuers` refuses, and an unrelated Service API-key sign-in still works.
  - Restore k1: the same bytes, `resolved_auth == "post configured-1"`, and only an explicit pass moves them (`changed 1`).
  - k2 removed: the bytes are kept, and the typed `PUT /v1/admin/trusted-issuers/client-secret` returns `200` without an old decrypt, giving a k3 envelope that resolves `replaced-1`.
  - k3 removed and the client id changed: sealed under k4 for the new binding, and it opens.
  - RED on `bb73d8ba0`: "a restart resealed an envelope it could not open" (k1→k2 bytes). GREEN.
- Fixture limit: `TransitFixture` cannot deny decrypt while allowing encrypt on one key; the removed-reference case already has the active wrap available while the old decrypt is refused.
- Readable equal, different-secret and method-change cases stay green: `…::activation_keeps_an_unchanged_configured_issuer_secret`. Boot issuer, trusted-issuer and jwt_bearer lib tests pass (`wyrd-server --lib -E 'test(/trusted_issuer/) | test(/^boot::issuer::/) | test(/jwt_bearer/)'`, 21). Creation, secret-free and discovery-failure restart behaviour are covered there. `wyrd-auth --lib` is 130/130.
- Diagnosis (test probe). The first availability probe used `/health` (`401`, authenticated) and then `/readyz` (`503`, initial warmup snapshot in `components/health/mod.rs`). Neither is related to the issuer. The probe is now an unrelated Service sign-in.

## Remove superseded logic at the existing owners

These corrections replace the faulty decisions; they must not add alternate paths around them. Replace the original-duration/resetting behavior in `OperatorPool::begin_bounded` and its maintenance callers with the complete-operation deadline contract. Retain server-side statement cancellation as part of that contract; do not retain an independently refreshed statement budget or a second maintenance transaction workflow.

Remove the failed-open-means-reseal fallback from `ClientSecretOwner::keep_or_seal`. Replace or narrow that helper and its serializer contract as needed so boot reconciliation owns the trusted-state decision exactly once, and explicit creation/replacement remains intentional. Update every affected caller and remove parameters/helpers made unused by moving that decision; do not leave the old fallback callable behind a new guard or wrapper. Existing crypto/context primitives, SQL fences and legitimate explicit replacement are retained.

Final source verification must trace all callers of the changed owners and confirm the superseded behavior is gone, not merely bypassed by the new tests. This is bounded removal of logic superseded by these two fixes, not permission for unrelated cleanup.

## Verification and preserved scope

Record exact focused names/selectors and expected behavioral RED/GREEN. Use mise and existing integration targets/runners: platform maintenance journeys under wyrd-server --test integration, owning boot/Gateway module tests under wyrd-server --lib, and SQL seam tests under wyrd-sql --test integration, wrapped by scripts/postgres/with-test-postgres.sh with repository migrations. Select only the relevant new/changed cases and affected existing regression tests; do not add standalone binaries or repeat broad owner suites for bounded remediation.

Run format/lints and affected tenant/dependency checks. Rerun codegen/docs checks only if their owners change. Keep CLI/report repeat semantics, all five stores, routine/recovery distinctions, authorization/audit, CAS/revocation, closed Gateway warming, notification/generation/listener behavior and invocation without added decrypt/catalog waits. No migration, dependency, benchmark, cloud experiment, SDK/MCP/UI expansion, incident framework, new job or architecture change is assigned. Preserve original recorded TDD history; no retroactive repair is required. Implementation returns evidence without launching review.

**Verification record.** Every Postgres test ran as `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked <selector>"`. Results:
- `wyrd-server --test integration -E 'test(/^platform_admin_e2e::stored_secrets::/)'`: 13/13 pass.
- `mise run lints`, `check:tenant-isolation`, `check:deps`, `fmt` and `git diff --check` pass.
- Codegen and docs owners are unchanged. `reference/stored-secret-encryption.md` already states that a removed key refuses its values until it is restored.
- Superseded logic is gone: there is no `keep_or_seal`, no duration-based `begin_bounded`, and no per-call `wait` in the pass.
- Behavioural status: PASS for both findings. TDD: FIND-2 PASS (RED recorded before the production change). FIND-1 VIOLATED: its production edit preceded its tests, and RED was then shown by running the tests against the stashed prior code.
- Final rerun at HEAD `0e3308c57` with a clean source tree:
  - `platform_admin_e2e::stored_secrets::` 13/13.
  - `wyrd-server --lib -E 'test(/trusted_issuer/) | test(/^boot::issuer::/) | test(/jwt_bearer/) | test(keyless_boot_refuses_only_when_ciphertext_is_stored)'` 22/22.
  - `wyrd-sql pg_stored_secrets::maintenance_lists_every_tenant_and_swaps_only_the_bytes_it_read` 1/1.
  - `wyrd-auth --lib` 130/130.
  - `fmt`, `lints`, `check:tenant-isolation`, `check:deps`, `codegen:check`, `docs:check` and `git diff --check` all exit 0.

**Correction.** The record above overstated verification and missed an ownership violation. It never ran `mise run test:sql`, which includes the `wyrd-sql` unit tests. `tests::tenant_scoped_query_modules_do_not_issue_transaction_control_sql` (`wyrd-sql/src/lib.rs:420`) failed on `dc0d6d62b`: `stored_secrets` and `swap_stored_secret` received an open transaction from `OperatorPool::begin_bounded` and committed it inside `queries/auth/`. **Fix:** `OperatorPool::fetch_all_bounded` and `OperatorPool::execute_bounded` (crate-private, `wyrd-sql/src/operator_pool.rs`) now own each complete bounded operation: acquisition, `BEGIN`, timeout setup, the statement, and commit, or rollback by drop. `begin_bounded` is removed. The slots keep their named operations, closed `StoredSecretTable` statements, and tenant/envelope CAS fences, and only build statements. One deadline still spans the whole operation, and `query_canceled` still maps to `None`. Neither gate changed. The behaviour is unchanged, so the existing tests are the proof. RED: the structural test above, on `dc0d6d62b`.
- `mise run test:sql`: 237/237, including the structural test. `check:tenant-isolation`, `lints`, `fmt` and `git diff --check` pass.
- `wyrd-sql --test integration -E 'test(/^pg_stored_secrets::/)'`: 1/1 (tenant CAS).
- `wyrd-server --test integration -E 'test(/^platform_admin_e2e::stored_secrets::/)'`: 13/13, including the starvation, late-capacity-then-lock, locked-store, abandoned-swap rollback, concurrent-owner and consuming-owner cases.
- `wyrd-cli --test cli` `stored_secret_journey::administrator_rotates_and_recovers_stored_secrets_through_the_cli`: 1/1.
- `identity_e2e::tenant_connection_rotation_journey` against real Keycloak and Dex: 1/1.
