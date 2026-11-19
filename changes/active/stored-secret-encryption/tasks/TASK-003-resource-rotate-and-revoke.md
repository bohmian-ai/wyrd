---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-002, REQ-006, REQ-007, REQ-008, REQ-009, REQ-012]
depends_on: [TASK-002]
---

# Deliver resource Rotate and Revoke

## Outcome and flow

An administrator selects an existing resource and renews encryption, supplies
a replacement credential, or revokes its use. Existing resource identity →
owner-authorized action → common facade and atomic owner update → existing
runtime refresh → unchanged client reference. The operator need not handle
DEKs or database rows. Actual provider-token replacement/revocation remains
an issuer responsibility unless an existing supported integration performs it.

Use the [approved spec](../spec.md) and
[administrator journey](../design/admin-journey.md), particularly selected-secret
rotation, resource revocation, and retained propagation/in-flight behavior.
Task 2 supplies common encrypted rows, injected facade, and existing owner
consuming paths. This task adds/extends their administration, not a new registry.

## Existing owners and scope

Use the provider APIs behind the existing facade/adapters, reusing installed
HTTP clients and credential/signing code. Do not add a provider SDK or other
external dependency where those APIs and existing mechanisms suffice.

The new behavior is administration of stored secrets under existing resource
identities. A replacement credential is supplied to Wyrd, not minted by a new
authentication integration. Reuse Task 2 encryption and the existing owner
permissions, validation, activation, disable/revoke, and session behavior.
Do not add login methods, federation, grants, token issuance/exchange protocols,
or provider credential acquisition. Public resource actions are in scope;
a redesign of authentication is not. No new vendor SDK or authentication
stack is required. If one appears necessary, identify the exact approved
requirement and reuse limitation for the maintainer before adding it.

Extend `OperatorConnectionControl` update/disable, `GatewayAdministration`
credential replacement/revoke, auth `HumanConnections` candidate/test/activate,
issuer configuration owners, and platform identity administration. Existing
permissions and resource lifecycle semantics remain authoritative. Reuse current
actions where they already deliver replacement/revocation; add encryption renewal
through the same resource owners. Keep auth credential replacement distinct from
resealing unchanged plaintext. Do not make auth replacement bypass validation or
promise session preservation that its lifecycle does not provide.

Project the owner actions through typed `wyrd-spec` contracts/errors, server
HTTP/OpenAPI and the CLI. SDKs and MCP do not expose them. No UI or public raw
encrypt/decrypt endpoint. No wrapping-key administration in tenant actions.

Use the common facade outside DB locks, commit against current authority/secret
state, and trigger existing owner refresh. Preserve Gateway propagation and
admitted-call snapshots, Operator next-attempt checks, and staged nonblocking
audit. Document the actual action/mode and effects at its public owner surface.

## Ordered acceptance scenarios and evidence

Add actual assertion locations, exact focused commands, RED/GREEN, and limits
in these entries; test variant owners/surfaces under the same scenarios.

1. **Renew a selected secret's encryption** — REQ-002, REQ-006, REQ-008–009.
   Given a working resource created through Task 2's public path, an authorized
   admin chooses renewal. Assert unchanged plaintext and identity, fresh DEK/
   payload, redacted response, and successful actual downstream use without
   client reconfiguration. Auth renewal preserves semantic revision and session
   behavior. Begin with the smallest owner action in real-server/Postgres
   integration; RED is missing renewal through that owner and facade.
2. **Replace a leaked/expired credential** — REQ-002, REQ-006, REQ-008–009.
   Given a resource whose old credential expired or whose old envelope cannot
   decrypt, supply a valid replacement through its typed owner operation.
   Actual downstream use receives the replacement under the same identity with
   a fresh DEK. Auth replacement follows candidate/test/activate and its existing
   session effects; extend the existing flow only to use the common facade and
   expose the approved action. Do not replace its authentication machinery.
   Provider failure/invalid input leaves committed state intact.
   Server journeys plus OIDC/provider fixtures assert the consuming effect;
   RED is absent action/mode or replacement incorrectly depending on old decrypt.
3. **Revoke without resurrection** — REQ-007–009.
   Given an in-use resource, revoke it and exercise subsequent consuming use
   after existing owner refresh, including a second replica and restart. It is
   refused; a different resource still works. A concurrent renewal/replacement
   cannot undo revocation; an explicit existing authorized re-enable remains
   separate where supported. Deterministic server/Postgres integration observes
   races and admitted-work boundaries without asserting instantaneous global
   cancellation. Existing correct disable/revoke behavior needs regression proof;
   RED for new behavior is a missing projection or unsafe encryption action.
4. **Preserve permissions, commit safety, and redaction** — REQ-008–009.
   Attempt the actions as a wrong tenant and an unprivileged principal. Inject provider refusal/timeout and a competing resource
   update between read and commit. Observe permission refusal or conflict, no
   partial/losing write, and no keys/plaintext in responses, logs, errors or audit.
   Exercise actual owner services and existing snapshot invalidation; do not
   replace permission or persistence behavior with facade mocks. RED is a newly
   missing boundary/atomicity case; reuse existing proven cases.
5. **Operate through HTTP and the CLI** — REQ-006–008, REQ-012.
   Drive the previous operations through HTTP and CLI callers against the real
   server/Postgres. Verify typed
   requests/modes, stable resource references, errors, and actual downstream
   use/refusal. Document upstream credential revocation separately from Wyrd
   revoke and encryption renewal. RED is missing consumer exposure or divergent
   projection semantics; binding-only construction tests cannot replace journeys.

   **Evidence (permissions, commit safety, redaction).**
   - Wrong tenant:
     - Operator: 404, in the routes test and the CLI foreign-tenant loop.
     - Human: `WYRD_SPEC_404_NOT_FOUND` via `seed_additional_tenant`.
     - Trusted issuer: `WYRD_AUTH_404_ADMIN_NOT_FOUND`.
   - Unprivileged caller:
     - Operator, human, and trusted issuer: 403 with a denied audit row. Decision rows are `[("allowed",5),("denied",1)]` for human connections and `[("allowed",3),("denied",1)]` for trusted issuers.
   - Provider outage:
     - Gateway, Operator, and human: Transit `set_available(false)` returns 503 with the envelope unchanged.
     - Trusted issuer: `admin::routes::pg_tests::issuer_renewal_survives_outage_and_loses_to_deletion`. Platform: `platform_admin_e2e::renewing_platform_connection_encryption_keeps_its_sign_in`. Both return 503 with the envelope unchanged and sign-in still working.
   - Competing update between read and commit:
     - Gateway and human: deterministic races using the Transit delay and the encrypt-count barrier.
     - Operator: a disable racing a rotation.
     - Platform and trusted issuer: a deletion racing the renewal (same two tests, Transit delay and encrypt-count barrier) wins; the renewal returns 409 and the resource stays deleted.
   - Redaction:
     - Every response is the existing redacted view, and the tests assert no plaintext in responses or CLI output (Gateway CLI secret-leak loop).
     - Errors carry no secret or key text.
   - RED: part of the RED runs above; the trusted-issuer and platform outage and race cases were added after their production code and passed on first run.

   **Evidence (revocation without resurrection).**
   - Each renewal compares and swaps on the exact observed envelope plus the live state, so it cannot undo a revoke.
   - Operator: `pg_operator_connection_routes::renewal_keeps_revocation_and_refuses_without_authority` (a disabled connection stays disabled after renewal) and `pg_operator_delivery::revoked_connection_fails_closed_and_key_outage_retries`.
   - Gateway: the race test above (renewal racing a revoke returns `GatewayResourceConflict`; renewing a revoked credential returns a conflict with state and envelope unchanged), and `pg_invocation_tests::gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation` (a revoked credential is refused on a restarted replica before dispatch).
   - Human: the race against `deactivate_active` returns `WYRD_AUTH_409_CONNECTION_CONFLICT` and leaves no active connection; a retired connection returns 404.
   - Trusted issuer: a deleted issuer returns 404 and the row stays absent.
   - Platform: a removed connection returns 404 and the GET stays 404.
   - Second-replica and restart regression: `identity_e2e::auth_client_secrets_open_through_vault_transit_across_restart`, `platform_admin_e2e::platform_sign_in_opens_its_client_secret_through_vault_transit_across_restart`, and the Operator replica test.
   - Limit: no new cross-replica *renewal* test. Renewal changes no lifecycle state, so existing owner refresh applies unchanged.

   **Evidence (replacement: existing owner actions, reused, not re-implemented).** Each existing replacement already uses the Task 2 facade and supplies a fresh DEK. Regression proof:
   - Operator: `pg_operator_delivery::next_attempt_on_another_replica_uses_the_rotated_credential`, `racing_rotations_keep_the_first_commit_and_deliver_it`, and `pg_operator_connection_routes::rotation_racing_a_disable_conflicts_and_keeps_the_disable`.
   - Gateway: `gateway_credential_lifecycle_rotates_revokes_and_redacts`; the renewal-vs-replacement race in the Gateway test above (the replacement wins and the snapshot prepares REPLACED).
   - Human connections: `identity_e2e::tenant_connection_rotation_journey` covers candidate, test, and activate with its existing session effects.
   - Platform: `platform_admin_e2e::an_operator_configures_and_removes_federated_platform_sign_in` (PUT upsert).
   - Limit: trusted issuers have no in-place replace route. Replacement is delete-and-add, or the boot seed's upsert. No new replace route was added; the task forbids authentication redesign and the spec names none. These are existing proofs, so there is no RED.

   **Evidence (renewal).** Common prefix `PG` = `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner; <cmd>"`.
   - Operator: `wyrd-server/tests/integration/pg_operator_delivery.rs::renewed_encryption_keeps_the_credential_and_delivers_it` asserts the same id and plaintext, a changed envelope, a redacted view, and actual delivery with the unchanged credential. `PG` + `cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=pg_operator_delivery::renewed_encryption_keeps_the_credential_and_delivers_it)'`. RED: 404 "Wyrd route not found" for `/operator-connections/{id}/renew-encryption` (behavioral). GREEN: 22/22 with `pg_operator_connection_routes`.
   - Gateway: `wyrd-server/src/components/gateway/pg_administration_tests.rs::gateway_managed_secret_renewal_keeps_the_key_and_loses_races` (Transit delayed 300 ms) asserts the same name, state, and `rotated_at`, a changed envelope with no plaintext at rest, and that a fresh `load_snapshot` prepares the submitted key. `PG` + `cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/gateway::pg_administration_tests::/)'`. RED: compile-only (E0599, missing `renew_credential_encryption`). GREEN.
   - Human connection: `wyrd-server/src/components/admin/identity.rs::pg_tests::renewing_a_connection_secret_keeps_its_revision_and_login` asserts the same id, revision 1, Active, and `tested_revision`, a changed envelope, and that `active_connection` opens the same `SecretBasic` secret at the same revision (login and sessions untouched). Selector `test(/components::admin::identity::pg_tests::/)` with `--lib`. RED: compile-only (E0425). GREEN.
   - Trusted issuer: `wyrd-server/src/components/admin/routes.rs::pg_tests::renewing_issuer_encryption_keeps_the_secret_and_respects_deletion` asserts an unchanged view, a changed envelope, and that `PgIssuerResolver` opens the same `SecretPost` secret. Selector `test(/components::admin::routes::pg_tests::/)` with `--lib`. RED: compile-only (E0425/E0422). GREEN: 36/36 in that lib run.
   - Platform: `wyrd-server/tests/integration/platform_admin_e2e.rs::renewing_platform_connection_encryption_keeps_its_sign_in` (Vault Transit fixture) asserts an unchanged redacted view, exactly one more Transit encrypt for the renewal, and a real confidential sign-in afterwards whose token exchange carries the unchanged secret. `--test integration -E 'test(=platform_admin_e2e::renewing_platform_connection_encryption_keeps_its_sign_in)'`. RED: 401 `WYRD_AUTH_401_UNAUTHENTICATED` (route missing) for HTTP (behavioral). GREEN; `platform_admin_e2e::` 42/42.

   **Evidence (projections).** Renewal ships through HTTP and the CLI only. The Operator handle's `renew_encryption` is gated on wyrd-client's `internal` feature, which the CLI enables and `wyrd-ai` never does (`check:deps`); Gateway mutation stays on the non-SDK `CredentialWriter`.
   - HTTP: every owner test above.
   - CLI:
     - Operator: `card_lifecycle::pg_tests::cli_manages_redacted_operator_connections`.
     - Gateway: `gateway_server_journey::gateway_commands_manage_redacted_tenant_configuration`.
     - Trusted issuer: `wyrd-cli/tests/secret_sources.rs::trusted_issuer_renew_encryption_sends_no_secret`. RED: "unrecognized subcommand 'renew-encryption'" (behavioral). GREEN: `secret_sources::` 5/5.
     - Human and platform connections have no CLI surface; they renew over HTTP.
   - CLI command: `mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server --bins`, then `PG` + `cargo nextest run --locked -p wyrd-cli --test cli -E '<selector>'`.
   - Docs: `docs/src/content/docs/reference/stored-secret-encryption.md` § "Renew, replace, or revoke a stored secret" separates encryption renewal, replacement, Wyrd revoke, and upstream revocation.

## Verification and handoff

**Final regression (after fmt):** server lib 36/36; admin routes 18/18; integration (Operator routes and delivery, identity rotation and restart, platform) 13/13; CLI 7/7; unchanged MCP 8/8 and Rust SDK 10/10 after the projections were removed.

**Checks run:** `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`, `py:lints`, `py:typecheck`, `ts:typecheck`, `ts:napi:check` (SDK surfaces unchanged), `docs:generate`, `docs:check`, and `git diff --check`; all exit 0. No hot-path code changed. Renewal runs only on admin request paths, and its one remote wait is the KMS renew, which runs outside any transaction. No migration was added.

**TDD compliance: VIOLATED.**
- Production code came before its executed RED for:
  - the Gateway CLI verb (its first RED run used the wrong selector);
  - the trusted-issuer and platform outage and race tests.
- Gateway owner, human connection, and trusted issuer RED were compile-only.
- Each case has a GREEN behavioral test.


Reuse `wyrd-server` integration and `wyrd-auth` PG tests for owner state/races;
CLI journey targets for projections. Existing module/file owners include Operator
connections, Gateway administration, identity, and platform journeys. Discover
the actual new test selectors and record focused setup-aware commands before
RED. Available environment-owning runners are `mise run test:operators:integration`,
`mise run test:wyrdstate:journey`, `mise run test:gateway:journey`,
`mise run test:identity:journey`, and `mise run test:platform:journey`.
Combined journey runs belong to integrated review.

Run affected owner/projection tests, `mise run codegen:check`,
`mise run check:deps`, `mise run check:tenant-isolation`, Rust format/lints,
and `mise run docs:check`.
If runtime hot-path code changes, enumerate added/moved external waits and
their authority. Benchmarks are not assigned by this task and require an
explicit maintainer request for the current change.

Output: tested, documented resource actions over HTTP and the CLI. Task 4
is independent after Task 2; selected-secret actions do not depend on a new
deployment maintenance framework or provider key-admin privileges.
