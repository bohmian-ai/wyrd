---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-008, REQ-009, REQ-012]
depends_on: [TASK-001]
---

# Standardize every existing secret owner

## Outcome and flow

Make the standardized system usable: real initialization and owner-specific
credential writes → Task 1's facade/common envelope → existing Postgres resource
rows → actual auth, Gateway, and Operator use through the same facade. A local
administrator can initialize, create an Operator connection, and receive a
failed-Verifier notification without another encryption setup, including after
restart. This is the consuming end-to-end proof of Task 1, not another crypto
implementation.

Use the [approved spec](../spec.md), design
[One credential write and read](../design/notes.md#one-credential-write-and-read),
[Rotation, failure, and performance](../design/notes.md#rotation-failure-and-performance),
and [journey 1](../design/journey-scenarios.md#1-get-started-and-use-stored-credentials).
Task 1 supplies the common owner, context/envelope operations, backends, and
typed configuration. Tasks 3–4 subsequently add the requested admin workflows.
This task also supplies small live provider checks, organized and run like
the existing storage integration tests.

## Existing owners and scope

Keep the existing REST/HTTP provider adapters behind the facade, reusing
installed HTTP clients and credential/signing code. No gRPC rewrite, new
emulator service, or vendor SDK is part of this task. Do not add an external
dependency where the existing APIs and mechanisms suffice.

This is an encryption cutover. Auth-owned client secrets are inputs to the
common envelope, not authorization to change login, token exchange, workload
identity, OIDC protocols, or session policy. Reuse the provider credentials and
refresh mechanisms already used by Task 1. Do not add federation, a new
authentication stack, or a vendor SDK to complete this task. If an existing
mechanism cannot satisfy a named encryption requirement, report the specific
gap and proposed dependency cost to the maintainer before expanding the work.

Connect server composition/configuration and CLI `init.rs` to the common owner.
Reuse init's atomic protected-key persistence and restart lifecycle. Convert
auth `pg_resolvers.rs`, Gateway `managed.rs`/`credential.rs` and server snapshot
preparation, and Operator service/delivery to consume it. Expected context comes
from their trusted metadata. Platform OIDC remains platform-authorized. Reuse
existing Gateway snapshots and auth candidate/activation semantics.

Carry the existing secondary callers with their owners: auth `platform_login.rs`
and server `boot/issuer.rs`, auth/boot state injection, and the
`wyrd-testing/src/server.rs` builder. Replace that builder's sealing-key hook
and deterministic default with the common configuration/capability; test setup
must not retain an independent key path that hides a broken production init.

Make only the initial schema/format changes explicitly named in the spec's
**Persistent and authority changes** section: existing auth/platform/Gateway
columns carry the common envelope; consolidate Operator ciphertext into its
existing column, remove the named split crypto columns/index, retain semantic
version and lineage/RLS. The explicit unshipped exception permits direct initial
definition edits. No data migration, legacy reader, new registry, or new table.

Delete direct sealing, unversioned readers, Operator raw-key/Vault KV ownership,
and old configuration/env projections once callers move. Update fixtures and
schema comments. Existing maintenance must use the common crypto path; retain
its safety behavior while Task 4 consolidates orchestration and exposes actions.
Do not remove maintenance safeguards in anticipation of that task. Keep the
shared Vault KV reader for external Gateway references and all excluded crypto.

Remote crypto for existing writes must run outside held DB locks/transactions
and use existing version/byte fences for atomic commits. This includes the
Operator update path, which currently waits on key operations with a row lock.
The same constraint applies to maintenance adapted during this cutover; its
existing Operator batch also fetches keys while holding locked rows. Extend the
existing SQL update/swap owners to check the observed envelope and relevant
authority metadata when releasing those locks, rather than adding coordination
machinery or assuming secret-version equality alone protects every state change.
Prepare Gateway decryption material in the existing background snapshot-loading
owner; warm calls use memory and add no remote unwrap/per-request DB check.
Bind prepared material to exact envelope/revision and existing cache lifetimes.
The current `load_snapshot` is shared by background `reload` and caller-waited
`load_through` on a cache miss. Trace both consumers: adding remote unwrap to
that shared function would also add a caller-waited KMS dependency. Keep remote
preparation in the existing background owner and account for cold/invalidation
paths under the spec and sanctioned-wait rules, preserving the existing cache
and retryable-unavailable mechanisms.

## Ordered acceptance scenarios and evidence

Record focused commands and actual RED/GREEN/assertion evidence in these entries.

1. **Initialize and create a connection** — REQ-001–005, REQ-012.
   Given a fresh local init profile, run the real CLI initialization and create
   an Operator connection through its public client path without separate key
   settings. Verify protected persistent key reuse and a common encrypted row;
   views remain redacted. Extend the CLI local-development/server integration
   owners. RED is absent common init injection, not a test-supplied Operator KEK.
   - Evidence (`4cba5f06b`, `786ab94f0`): `crates/wyrd/wyrd-cli/tests/pg_tests.rs:1866`
     `init_keeps_one_stored_secret_key_for_every_owner`. RED: panicked at
     `pg_tests.rs:1894` "init keeps a local key: NotFound" (init wrote only the old
     `sealing.key`). GREEN: passes via `-p wyrd-cli --test pg_tests -E
     'test(=init_keeps_one_stored_secret_key_for_every_owner)'` under the PG wrapper.
     Asserts one protected file `[kms]` key reused across init reruns, a common
     envelope row, and redacted views. PASS.
2. **Deliver after restart** — REQ-001, REQ-005, REQ-009, REQ-012.
   Starting from that persisted deployment/connection, trigger a failed Verifier
   and observe delivery to the local provider fixture. Restart with the saved
   configuration and repeat. Exercise the real server and SDK/CLI journey using
   the connection from setup; do not substitute an independently seeded fixture.
   RED is the missing common-key consuming flow. Make the parked guide runnable
   from this proof with repository journey-tested examples.
   - Evidence (`4f46916c0`): `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:1381`
     `test_failed_verification_pages_on_call_across_restart`, run by the `mise.toml:175`
     local-dev journey line over the Python, TypeScript and Rust
     (`examples/.../failed_verification_dispatches`) examples: init → create the
     connection → failed Verifier → delivery to the fixture → restart → delivery again.
     GREEN on all three languages; `mise run check:examples` and `mise run docs:check`
     pass. The guide `respond-to-a-failed-verification.md` embeds those files and stays
     `draft: true` for the maintainer's publication call. RED was not captured before
     production code: TDD VIOLATED for this scenario. PASS (behavior).
3. **Use every existing secret owner** — REQ-001–005, REQ-008.
   Create tenant human-auth, workload-issuer, platform OIDC, Gateway managed,
   and Operator credentials through existing owner APIs, then perform their real
   authentication/exchange/provider/delivery operations. Existing login and
   exchange flows are regression consumers, not new authentication work. Reuse
   Task 1 backend fixtures to vary wrapping backends, not credential-acquisition
   methods or identity products. Use the real server and repository Postgres
   with local OIDC/provider fixtures; a redacted GET alone is insufficient.
   Verify the common envelopes are actually consumed, including on restart.
   RED is an owner still using the old crypto/configuration.
   - Evidence (`4cba5f06b`, `6722a5f54`): auth human connection, workload issuer and CLI
     SecretPost through Vault Transit across `restart_bound`:
     `identity_e2e.rs:5391` `auth_client_secrets_open_through_vault_transit_across_restart`
     (identity lane, PASS, 3 identity tests). Platform OIDC:
     `platform_admin_e2e.rs:1888`
     `platform_sign_in_opens_its_client_secret_through_vault_transit_across_restart`
     (asserts `client_secret=` reaches `/token`, PASS). Gateway:
     `wyrd-testing/tests/gateway/compatible.rs:936` (scenario 5). Operator:
     `pg_operator_delivery` 12/12 with `TransitFixture`. Owner units:
     `wyrd-gateway/src/managed.rs:118`, `wyrd-auth` `pg_resolvers::pg_tests` 7/7.
     Production code preceded these tests: TDD VIOLATED. PASS (behavior).
4. **Enforce scope and failure safety through consumers** — REQ-003–004,
   REQ-008. With two tenants and distinct owners, copy/tamper encrypted values
   or deny the configured key provider, then invoke actual use/write paths.
   Assert refusal, no cross-tenant success or secret leakage, and no partial
   overwrite of committed state. Race an existing replacement with revoke/update
   and assert fences preserve the winner. Server/Postgres integration tests cover
   RLS and owner metadata, beyond Task 1's isolated AAD checks. RED is missing
   consumer binding or unsafe remote-IO/commit integration.
   - Evidence (`e67392101`, verification-only; the behavior already existed, so there is no RED):
     `pg_operator_delivery::copied_or_altered_envelopes_fail_closed_at_delivery` copies an
     envelope across connections and across two tenants' same-named connections, and
     alters one byte. All three fail with `credential_invalid`, the intact one delivers,
     and the mock sees only `/hook/hooks-intact`.
     `pg_resolvers::pg_tests::resolver_refuses_an_envelope_copied_from_another_tenant`
     shows the jwt-bearer resolver refuses a cross-tenant copy while the source tenant
     still resolves its own secret.
     `pg_operator_connection_routes::rotation_racing_a_disable_keeps_the_disable` delays
     Transit 500 ms and lands the disable mid-encrypt. The disable stays, the rotation is
     fenced out, sealed again (3 encrypts) and commits once (`secret_version + 1`), and
     the connection is never re-enabled. This test's first draft expected a 409; the
     documented `change` re-prepare loop (`service.rs` `CHANGE_ATTEMPTS`) is the
     contract, so that was an assertion error, not a product defect.
     Existing proof, cited: `pg_invocation_tests.rs:900` (Gateway cross-tenant copy),
     `wyrd-sql` `updates_rotate_in_place_and_are_fenced_on_the_observed_row`,
     `gateway_concurrent_replacements_are_last_committed_and_family_independent`,
     `missing_key_refuses_only_credential_writes`, and the key-provider-denied refusals in
     `pg_operator_delivery`. Commands: `-p wyrd-server --test integration -E
     'test(/^pg_operator_connection_routes::/)'` 5/5, `pg_operator_delivery` 12/12, `-p
     wyrd-auth --lib -E 'test(/pg_resolvers::pg_tests::/)'` 7/7. TDD NOT_APPLICABLE. PASS.
5. **Preserve Gateway admission and preparation** — REQ-009.
   Load a managed credential through the common facade, warm a Gateway snapshot,
   then invoke the actual Gateway with a controlled provider fixture. Assert warm
   calls cause no wrapping-service requests or extra DB checks. Change/revoke the
   credential and exercise existing notification reload, generation fencing,
   listener loss and admitted-snapshot behavior. Integration and Gateway journey
   owners distinguish background preparation, warm admission, and the caller's
   cold-miss/invalidation path; inspect external waits on each path. RED is remote
   decryption on an attempt or stale prepared material surviving owner refresh;
   already-correct lifecycle cases need regression proof, not artificial churn.
   - Evidence (`6722a5f54`): `compatible.rs` `managed_secret_opens_through_vault_transit_only_while_preparing`
     checks exact Transit encrypt and decrypt counts across deploy, rotation and restart,
     with `settled()` waits. Warm calls add 0 requests.
     `cold_call_waits_retryably_while_a_managed_secret_is_prepared`: RED with the
     `load_through` fix reverted was 502 instead of the expected 503. GREEN: two cold calls
     against a stalled Transit return 503 `WYRD_SERVER_503_SERVICE_UNAVAILABLE` with no
     upstream call, then succeed once Transit answers. Gateway journeys 19/19 and
     `wyrd-server` gateway lib 87/87 pass. Waits added or moved:
     (a) remote unwrap only in the background `reload` and the writing server's `warm`,
     not a hot path;
     (b) the cold `load_through` keeps its one sanctioned snapshot statement and now
     returns retryable 503 instead of waiting on KMS;
     (c) warm admission adds no KMS or PostgreSQL wait.
     Contract (maintainer decision, 2026-11-19): a reload that meets a retryable KMS
     outage installs the credential unprepared and fails closed with a retryable 503
     until a reload prepares it. Previously opened material is never carried forward, so
     disabling the wrapping key stops serving at the next reload. PASS.
6. **Remove competing formats and setup** — REQ-012.
   On a fresh repository database, run the revised initial schema and fixture
   lifecycle, and inspect the resolved production caller/configuration graph.
   All recoverable owners use the facade, old crypto readers/configuration are
   absent, and excluded external-secret/signing paths still work. SQL integration
   and source/config checks provide static proof; do not add a test duplicating
   compiler guarantees. Update setup/team-deployment/provider references for all
   backends, including actual Vault Transit provisioning/auth/policy steps.
   Document the existing supported authentication setup; do not implement new
   login flows or IAM administration to make a provider setup example work.
   - Evidence (`6c53c3b23`): `mise run test:sql` 88 + 7 + 3 pass on a fresh schema.
     A static sweep (`git grep -i -E
     'sealing[_ ]?key|SEALING_|OPERATOR_KEK|kek_dir|SealingKey|secret_nonce|secret_key_version'`
     outside `changes/`) finds only applied Gateway migration history (dropped by
     `20261014000000`) and an unrelated queue "sealing headroom". The startup image
     scripts now set `WYRD_KMS_*`. The team deployment, troubleshooting and Add people
     pages name `[kms]`. `reference/stored-secret-encryption.md` is published with
     file, Vault Transit (mount, key, policy, token file, rotation), Cloud KMS and AWS
     KMS steps. `mise run docs:check`, `codegen:check`, `check:deps` and
     `check:tenant-isolation` pass. Limit: `test:server:startup:prod` builds its image
     only on Linux ("a Linux bundle builds on Linux"), so it was not run on this macOS
     host. TDD NOT_APPLICABLE. PASS; the maintainer accepted the startup-lane run as a
     CI-verified limit (2026-11-19).

7. **Exercise the real wrapping-provider APIs** — REQ-003–005.
   Using pre-provisioned symmetric test keys and existing supported deployment
   credentials, add one small live test each for GCP and AWS. Load the normal
   KMS configuration and construct the production facade with its existing
   credential loader, not injected mock credentials or an emulator endpoint.
   Seal a fixed test value, construct another facade with the same
   configuration, and open the resulting envelope. Change the owner context
   and assert refusal. These integration tests prove real credential
   acceptance, AWS signing, provider request/response compatibility, and
   context binding; no server or Postgres is needed for this provider boundary.
   Missing setup in an explicitly selected cloud test fails clearly rather
   than silently skipping.
   Preserve the existing real dev Vault Transit test and its rotation/refusal
   assertions. Reuse the existing API clients; do not add SDKs, authentication
   features, key provisioning/destruction, or benchmark work. If the existing
   adapter already passes, record verification-only evidence; do not manufacture
   RED or rewrite working code. An unavailable account or missing key is setup
   failure, not evidence of missing encryption behavior. Fix any demonstrated
   in-scope protocol defect at its existing adapter owner.

   Organize these tests like `wyrd-storage`: one `tests/integration/main.rs`
   target collecting `integration_gcp.rs`, `integration_aws.rs`, and
   `integration_vault.rs`. Move the existing `tests/vault_transit.rs` into
   that target, update its Cargo registration and runner, and retain a single
   compile of each test. Its current focused command is
   `mise exec -- cargo nextest run --locked -p wyrd-kms --test vault_transit --run-ignored=all -E 'test(=transit_wraps_rotates_and_refuses_through_the_facade)'`;
   record the new module-qualified selector after the move.
   Keep ordinary lanes credential-free; explicitly selected integration lanes
   own their external-service requirements. Do not invent new test function
   names or selectors before implementation records them.

   Add `test:kms:gcp:cloud` and `test:kms:aws:cloud` mise tasks, following
   `test:storage:<provider>:cloud` naming and exact provider-test selection.
   Keep `test:kms:vault` as the real local Vault runner, updated to select the
   moved module. Local cloud credentials belong in the same untracked
   configuration mechanism as storage tests, using existing `WYRD_KMS_*`
   settings and supported credential inputs. No Postgres wrapper is needed.

   Add `.github/workflows/kms-integration-cloud.yml` following the existing
   `storage-integration-cloud.yml`: provider jobs on pushes to `main`, the
   same setup action and concurrency convention, and the same mise commands
   used locally. Use a `kms-integration` environment for configured test-key
   references and credentials. Keys are provisioned before test execution;
   cloud tests need only encrypt/decrypt access. Reuse the existing CI
   credential-setup pattern, making GCP credentials available in the file form
   the existing KMS loader supports rather than adding a new credential mode.
   Cloud tests do not run
   on PRs or in credential-free gates. Keep the real dev Vault test in its
   existing regular CI lane and update selectors for the new test target.
   Document the required provisioned keys, encrypt/decrypt access and local/CI
   setup; report any unavailable live account as a verification limit.
   - Evidence (`35c6e26b8`): `crates/wyrd/wyrd-kms/tests/integration/main.rs` collects
     `integration_vault`, `integration_gcp` and `integration_aws`. Vault selector:
     `integration_vault::transit_wraps_rotates_and_refuses_through_the_facade`, and
     `mise run test:kms:vault` passes 1/1. Cloud selectors:
     `integration_gcp::gcp_seals_reopens_and_refuses_another_owner` and
     `integration_aws::aws_seals_reopens_and_refuses_another_owner`. Each loads
     `WYRD_KMS_*`, builds a production facade, seals, reopens with a second facade, and
     expects `KmsError::Authentication` for another owner. An unconfigured `mise run
     test:kms:aws:cloud` fails at `main.rs:31` with "WYRD_KMS_BACKEND must select
     aws_kms…", so missing setup does not skip silently. `cargo nextest run -p wyrd-kms`
     runs 17 and skips the 3 provider checks with no credentials.
     `.github/workflows/kms-integration-cloud.yml` follows
     `storage-integration-cloud.yml`: pushes to `main` only, the `kms-integration`
     environment, AWS OIDC, and the GCP key JSON written to a 0600 file for
     `GOOGLE_APPLICATION_CREDENTIALS`. `TESTING.md` lists the lanes and their setup, and
     `.github/scripts/tests` pass. Limit: no provisioned Cloud KMS or AWS KMS key or
     account was available here, so both live checks are unrun, and the first run on
     `main` proves them. The maintainer accepted this as the task's unavailable-account
     verification limit (2026-11-19). TDD NOT_APPLICABLE (verification-only). PASS.

## Verification and handoff

Test owners: `wyrd-cli` local init/CLI journeys; `wyrd-server` integration modules
for Operator routes/delivery, identity, and platform; `wyrd-auth` PG tests;
`wyrd-testing` Gateway journeys; SDK/MCP consuming journeys. Existing runners:
`test:local-dev:journey`, `test:operators:integration`, `test:identity:journey`,
`test:platform:journey`, `test:gateway:journey`, `test:wyrdstate:journey`, and
`test:sql`, all via `mise run`. `test:kms:vault` already exists; the GCP/AWS
cloud tasks and credentialed CI workflow in scenario 7 are work to add here.
Select focused new/changed tests during execution using their owning lane's
setup; the provider-only checks need no Postgres. Integrated review owns
their full combined runs and each shipped language surface. Record real focused
selectors before RED rather than guessing new test names here.

Run affected owner/SQL tests, `mise run check:tenant-isolation`,
`mise run check:deps`, `mise run fmt`, `mise run lints`, and applicable
`mise run codegen:check` / `mise run docs:check` / `mise run check:examples`.
List every added/moved external wait and its authority. Keep functional proof
focused on the changed encryption consumers. There is no benchmark requirement.

Output: all owners, setup, persisted formats, fixtures, and existing runtime
lifecycle use the common system. Tasks 3–4 consume those rows and owner services;
neither must translate an old encryption format or duplicate initialization.
