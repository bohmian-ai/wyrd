---
id: TASK-003-R1
kind: remediation
status: implemented
spec: SPEC-stored-secret-encryption
spec_revision: 1
original_task: TASK-003-resource-rotate-and-revoke.md
findings: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5]
candidate: d3db737e5
---

# Complete stored-secret resource administration and its proof

Review subject: base `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`, candidate `c93287b54bf944c8c8cc9323f81c12ba5174369c`. Original task: `../../tasks/TASK-003-resource-rotate-and-revoke.md`. Diagnosis and retained evidence: `findings-validation.md`; verdict: `verdict.md`; executed results: `verification.md`.

## Outcome and correction boundary

An administrator can replace a bound trusted issuer's secret without destroying its references, renew through the shipped real surfaces with working consumers, and rely on the documented response and audit behavior. Use existing owners, facade, SQL capability/transaction boundaries, outbox and journey fixtures. No new authentication integration or maintenance framework is authorized.

1. **Trusted-issuer replacement — FIND-TASK-003-1; REQ-006/008, original scenario 2.** Existing create is INSERT-only; delete refuses bound issuers or cascade deletes references. Extend existing trusted-issuer administration through typed HTTP/CLI to accept the replacement, seal without opening old bytes, preserve issuer identity and bindings, and fence against relevant current secret/authority state. Reuse the existing service-account permission, client-secret owner and SQL mechanisms; do not implement replacement as destructive delete/add or boot-only upsert. Failure before commit preserves old committed state, and a losing replacement must not restore revocation.
2. **Composed proof — FIND-TASK-003-2; REQ-006/009, scenarios 1/5 and AGENTS Testing 1.** Existing human/issuer tests call handlers and open secrets directly; issuer CLI tests wiremock; Gateway prepares material but never dispatches after renewal. Extend existing server/identity and CLI journeys, rather than duplicate their full lower-tier test matrices. Observe actual authentication and retained human-session use across renewal, and actual Gateway provider credential use after renewal.
3. **Served contract — FIND-TASK-003-3; HTTP/OpenAPI authority.** Update the existing exact Operator operation-set test at `pg_openapi_contract.rs:2110` for the sixth renewal operation, with bodyless POST, redacted typed success and UUID locator coverage. Preserve exact-set checking. Its current failure is reproduced, not hypothetical.
4. **Public reference — FIND-TASK-003-4; REQ-012.** Correct `reference/stored-secret-encryption.md` to describe owner-specific disabled/inactive refusal behavior and unchanged semantic fields while permitting mutation metadata updates. Document the corrected trusted-issuer replacement. Regenerate `docs/public/llms-full.txt` through its owner; do not edit generated output by hand.
5. **Audit failure path — FIND-TASK-003-5; REQ-008/009 and audit authority.** The new human renewal passes an unstaged allowed event through successful KMS preparation to `begin_locked`, which stages only after DB acquisition and locking. Stage the known decision once through the existing process outbox before fallible operation work; prevent duplicate later staging. Reuse the owner boundary and preserve other callers. No blocking audit persistence, new audit store or success-only accounting.

## Observable closure

| Finding / requirement | Scenario and observations | Narrow proof owner |
| --- | --- | --- |
| 1; REQ-006/008 | Existing confidential issuer has live workload bindings. Replace through authorized HTTP/CLI, retain identity/bindings, then actual exchange/resolver consuming flow uses the replacement. Repeat with unreadable old envelope. Invalid input/provider preparation failure leaves committed state intact; deterministic concurrent revocation wins and losing replacement conflicts. | Existing trusted-issuer server/CLI identity journey and SQL owner seams; real server/Postgres with existing provider fixture. |
| 2; REQ-006/009 | A live human connection has a usable existing session; renew over HTTP, retain semantic revision, use that session, and complete subsequent confidential login with the unchanged secret. Trusted CLI renewal reaches the real server and subsequent authentication uses the same secret/bindings. Gateway renewal is followed by provider dispatch observing the unchanged credential. | Existing identity journey, trusted CLI journey and Gateway server journey. Existing lower-tier outage/race assertions stay. |
| 3; served contract | The existing exact-set test passes with six operations and asserts the renewal request/success/locator contract. | `pg_openapi_contract` exact nextest selector below. |
| 4; REQ-012 | Reference matches disabled Operator successful renewal, inactive human 404, per-owner conflict conditions and modification metadata. Generated projection matches source. | Content review, docs generation/check. |
| 5; audit authority | Authorize renewal; preparation succeeds; write acquisition or slot lock fails. Envelope remains unchanged and exactly one allowed decision was staged. Success and KMS refusal also stage one allowed decision; denial stages one denied decision. Audit unavailability cannot refuse the operation. | Focused existing recording-outbox/real-Postgres human owner or HTTP fixture; no new durable audit machinery. |

## Runners and affected checks

Find actual test names from source/nextest, record their exact selectors before RED, and execute every named new/changed test. Reuse the environment-owning tasks in `mise.toml`: focused `test:identity:journey` setup for identity/provider fixtures and existing Gateway/CLI journey setup. Do not run whole journey suites solely to obtain a narrow remediation result.

For Postgres-only proof, use `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <focused command>'`. Cargo commands are serialized across shared `target/`; no positional filter that can pass with zero selected tests. Existing exact failed command to close:

```sh
cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_openapi_contract::operator_connection_contract_publishes_five_typed_operations)'
```

If the test is renamed to describe six operations, record and execute the actual new exact selector. CLI journeys first build the current binaries through `mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server --bins`, then use the existing CLI integration target with `WYRD_CLI_E2E=1` and its actual selector under managed Postgres.

Run affected format/lints, `codegen:check`, `check:deps`, `check:tenant-isolation`, and docs generation/check as required by touched owners. Full combined journeys belong to integrated change review. No migrations, SDK/MCP/UI additions, new provider credential acquisition, session policy changes or benchmarks are in this task. Preserve nonblocking audit, CAS safety, KMS outside DB transactions, existing refresh/propagation and admitted-call semantics.

The recorded TDD violation remains history; do not fabricate historical RED or require production churn to repair it. New remediation behavior follows the applicable test-first execution discipline.

## Evidence

`PG` = `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && <cmd>'`. `ID` = the same wrapper with `db:migrate:inner`, then `scripts/identity/with-identity-providers.sh -- bash -lc "cargo build --locked -p wyrd-cli --bins --tests && <cmd>"` with `WYRD_KEYCLOAK_ISSUER`/`WYRD_DEX_ISSUER` from `mise.toml`.

| Finding | Result | Proof |
| --- | --- | --- |
| 1 | PASS | `components::admin::routes::pg_tests::replacing_issuer_secret_keeps_its_bindings_and_loses_to_deletion` (`--lib`, `PG`): a bound `SecretPost` issuer is replaced in place; the view is unchanged, the envelope changes, `PgIssuerResolver` opens the new secret, and the binding is still listed; replacement succeeds over a corrupted (`\x00`) envelope; an empty secret (400), a reader (403), a Transit outage (503, envelope unchanged), and a `PrivateKeyJwt` issuer (409) change nothing; a replacement racing a cascade delete (Transit delay, encrypt-count barrier) loses with a safe 409 and the issuer stays absent; a later call is 404; seven allowances and one denial staged. RED: the stubbed route returned `Internal` ("replacement succeeds" panicked). GREEN: `test(/components::admin::routes::pg_tests::/)` 19/19, rerun 22/22 with identity owner tests. CLI: `identity_e2e::renewing_and_replacing_auth_secrets_keeps_clients_working` (`ID`) runs `wyrd auth trusted-issuer replace-secret` against the real server: no secret source refuses before any request; with `WYRD_ISSUER_CLIENT_SECRET` it succeeds without echoing the secret, the stored envelope changes, and the Keycloak workload still exchanges through the unchanged binding. Server: route `PUT /v1/admin/trusted-issuers/client-secret`, type `ReplaceTrustedIssuerSecretRequest`, sealing `seal_trusted_issuer_replacement`, CAS reuse `swap_trusted_issuer_secret` (renamed from the renewal-only name). |
| 2 | PASS | Same identity journey (`ID`, nextest `2f0237eb-ebe3-4f4d-849e-28497b3837c3`, PASS): a confidential Keycloak connection is activated and alice signs in; HTTP renewal keeps id, revision, and state and redacts the secret; the existing session's access token reaches `/v1` and its refresh token refreshes; a fresh sign-in, whose code exchange opens the renewed secret at Keycloak, completes. `wyrd auth trusted-issuer renew-encryption` against the real server changes the envelope and the workload exchanges afterwards. Gateway: `compatible::managed_secret_opens_through_vault_transit_only_while_preparing` (`PG` + `cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all`) renews over HTTP after the rotation, waits for the snapshot reload (one more Transit decrypt), dispatches, and the upstream receives the unchanged rotated key; warm calls make no Transit request. Existing lower-tier race and outage tests are unchanged. No RED: these observe already-shipped behavior. |
| 3 | PASS | Renamed `pg_openapi_contract::operator_connection_contract_publishes_six_typed_operations`: exact set includes `(…/renew-encryption, post)`, no request body, 200 `OperatorConnectionView`, UUID `connection_id`. `PG` + `cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_openapi_contract::operator_connection_contract_publishes_six_typed_operations)'` PASS (nextest `f5a4733e-c5de-46b3-adee-9924ac5d2f83`); whole `pg_openapi_contract::` 25/25. |
| 4 | PASS | `docs/src/content/docs/reference/stored-secret-encryption.md`: renewal keeps identity, settings, state, revision, and sessions, with only `updated_at` (and Operator `updated_by`) changing; per-resource `404`/`409` table (disabled Operator renews and stays disabled; inactive human connection is 404); trusted-issuer replacement row and secret sources. `mise run docs:generate` regenerated `docs/public/llms-full.txt`; `mise run docs:check` passed. Content reviewed against each route's `utoipa` responses. |
| 5 | PASS | `HumanConnections::begin_locked` (`wyrd-auth/src/connections.rs`) now stages the decision before acquiring the transaction and slot lock, so every caller records its allowance once even when acquisition fails; nothing else stages it. `components::admin::identity::pg_tests::renewing_a_connection_secret_keeps_its_revision_and_login` closes the owner's pool while Transit reseals: 503 `WYRD_AUTH_503_VERIFY_UNAVAILABLE`, candidate envelope unchanged, and the renewal decision count is six allowances and one denial (success, wrong tenant, outage, lost race, retired, store failure; one denial). RED: `[("allowed", 5), ("denied", 1)]`. GREEN: identity owner tests 3/3. |

Checks: `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run docs:check`, `cargo nextest run --locked -p wyrd-cli --lib -E 'test(/auth::trusted_issuer::/)'` 16/16. No migrations, SDK, MCP, UI, or benchmarks. Behavioral verification: PASS. TDD: findings 1, 3 and 5 had an executed behavioral RED before production code; findings 2 and 4 are proof and documentation of shipped behavior, so `NOT_APPLICABLE`. The original TASK-003 violation stays recorded.
