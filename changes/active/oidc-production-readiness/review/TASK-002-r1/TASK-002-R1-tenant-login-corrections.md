---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-007, REQ-008, REQ-014, REQ-017, INV-002, INV-004, AC-006, AC-007]
depends_on: [TASK-002]
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5, FIND-TASK-002-6, FIND-TASK-002-7]
---

# Tenant login correctness and repository-boundary remediation

## Authority and immutable subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-002-r1/verdict.md`
- Validated findings: `changes/active/oidc-production-readiness/review/TASK-002-r1/findings-validation.md`
- Original base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`

## Outcome

Tenant human login must use immutable OIDC subject identity, fully bind ID tokens to the intended client and provider-advertised algorithm policy, audit provider-driven role changes atomically, and preserve the repository's RLS, SQL-capability, and secret-redaction boundaries. The existing state-bound callback, screened IO, issuance, refresh, workload, platform, and audit mechanisms remain the owners.

## Issue diagnoses and required corrections

### FIND-TASK-002-1 — Human identity mapping can replace OIDC `sub`

`ConnectionInput` accepts any nonempty human subject mapping; the shared mapper then feeds that value to the callback's durable `(issuer, subject)` lookup. A human connection configured with `subject: email` can merge distinct subjects at one issuer or split one subject when the claim changes. Existing journeys always configure `sub`, so their same-email proof does not exercise this path.

Require tenant human connections to use exact OIDC `sub` at both the public connection validation boundary and the stored connection decode/upgrade boundary. A pre-existing stored non-`sub` human connection must fail closed. Continue using the existing claim mapper and identity SQL; preserve independently configurable display email and groups. Workload issuer claim mappings remain unchanged. Do not add linking, a second human mapper, or a compatibility interpretation.

### FIND-TASK-002-2 — Human ID tokens omit OIDC authorized-party validation

The shared verifier proves that the client id occurs in `aud`, but the tenant callback does not inspect `azp`. A signed multi-audience token with missing or mismatched `azp` can therefore reach identity resolution and issuance.

At the tenant human callback boundary, after generic JWT verification and before identity resolution, require multiple audiences to carry string `azp` equal to the connection-bound client id; if `azp` is present for a single audience, require the same equality. Reuse verified raw claims and the bound client id. Do not apply ID-token-only `azp` semantics to workload assertions or silently change platform-login behavior.

### FIND-TASK-002-3 — Provider-driven role replacement has no role-change audit evidence

Every successful human callback replaces durable User roles, but issuance appends only the generic token-exchange event. Mapping or group changes can grant or revoke roles without canonical evidence distinguishing the mutation.

Have the existing role-assignment owner report whether set replacement changed durable membership. When it changed, append exactly one redacted canonical allowed event with stable operation `auth.user.roles.sync` and the User principal resource through the existing auth-audit append path before the callback transaction commits. Preserve the ordinary token-exchange event. An audit append failure must roll back the role update, refresh/token issuance, and completion together. Unchanged assignments must not claim a mutation. Do not add a sink, table, publisher, or role-history subsystem.

### FIND-TASK-002-4 — ID-token algorithm is selected from the untrusted header

The callback rediscovers the provider's supported ID-token algorithms, discards them, and the shared verifier selects any non-HMAC asymmetric header algorithm compatible with the JWKS key. This admits a provider-unadvertised algorithm.

Carry the freshly discovered supported ID-token algorithm set through the existing tenant callback verification and reject algorithms outside its supported asymmetric intersection before identity resolution. Reuse provider metadata and the current verifier/JWKS owner. Do not introduce administrator algorithm configuration, a parallel verifier, or changes to workload federation unless separately required by its authority.

### FIND-TASK-002-5 — TenantConn login-state queries duplicate forced RLS

Purge, consume, complete, and redeem run through `TenantConn` on a table with forced RLS, yet repeat `data_tenant_id` predicates and binds; a source-shape test requires the prohibited duplication.

Keep tenant id only as inserted row ownership data. Remove duplicate tenant-selection predicates and corresponding binds from TenantConn-backed transitions while preserving state hash, initiation binding, expiry, consumed, and completion conditions. Replace the source-shape assertion with real Postgres proof that tenant B cannot mutate or redeem tenant A's state and tenant A can perform each valid one-use transition. Do not add another manual tenant guard.

### FIND-TASK-002-6 — Cross-tenant state lookup exposes raw `PgPool`

The callback extracts `WyrdPostgres::app_pool()` and passes it into a public query function accepting any `PgPool`. The SQL operation is narrow, but its Rust type no longer encodes that only the runtime app-role pool may invoke it.

Expose the one state-hash-to-tenant lookup as a narrow inherent operation on the existing `WyrdPostgres` owner, using its private app pool internally. The callback supplies only the state hash. Preserve the current SECURITY DEFINER function, one-column result, least disclosure, and fail-closed unknown/expired/consumed behavior. Do not create a new wrapper, trait, repository abstraction, or broaden the unrelated slug resolver.

### FIND-TASK-002-7 — PKCE verifier is printable through derived `Debug`

`NewLoginState` and `ConsumedLoginState` publicly derive `Debug` while holding the live PKCE verifier as `String`; diagnostic formatting exposes the exchange possession secret.

Restore `SecretString` across those public state values and expose the verifier only at SQL binding and provider request boundaries. A private SQL decode row may use `String` if required and convert immediately at the boundary. Reuse the already-installed `secrecy` dependency; do not add a wrapper type or change stored bytes.

## Constraints and preserved behavior

- Preserve state entropy, hashing, one-use consumption committed before provider IO, initiation binding, fixed callback responses, sealed one-use completion, and no callback token/code disclosure.
- Preserve screened and pinned discovery/token/JWKS IO, issuer/signature/time/nonce checks, bounded key refresh, and exact redirect binding.
- Preserve five-minute access-token snapshots, refresh-family replay revocation, exact connection id/revision cutoff, and successor provenance.
- Preserve current platform-login and workload-federation contracts unless a correction explicitly names their boundary.
- Preserve tenant User zero-default authority, tenant-valid group mapping, machine-plane separation, and no email linking.
- Preserve forced RLS as the tenant boundary, caller-owned `TenantConn` transactions, canonical audit staging, and the single publisher.
- Do not restore the public authorization-code token grant, legacy CLI login, GET login route, or token-bearing callback response.
- Do not add compatibility paths, new public APIs, new persistence resources, new audit mechanisms, new dependencies, or speculative abstractions.
- Do not implement TASK-003 BFF or TASK-004 CLI handoff behavior in this remediation.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| Human connection authoring accepts exact `sub`, rejects other subject paths, and stored non-`sub` human connections fail closed before login. Same-issuer tokens with different signed `sub` and identical email create distinct Users without authority transfer. | `FIND-TASK-002-1` |
| Tenant callback rejects multi-audience tokens with missing/mismatched `azp`, rejects any present mismatched `azp`, accepts matching `azp`, and persists no User session/completion on rejection. | `FIND-TASK-002-2` |
| Changed provider-derived assignments emit exactly one canonical `auth.user.roles.sync` event plus token exchange; unchanged assignments emit no role-sync event; audit failure rolls back assignments and session state. | `FIND-TASK-002-3` |
| Tenant callback rejects a validly signed asymmetric token whose algorithm was not advertised for ID tokens, accepts an advertised supported algorithm, and rejects before User/session/completion persistence. | `FIND-TASK-002-4` |
| TenantConn state transitions contain no manual tenant-selection predicate; live Postgres proves tenant B cannot purge, consume, complete, or redeem tenant A's state and tenant A retains one-use behavior. | `FIND-TASK-002-5` |
| The callback and query library expose no new raw pool signature for state resolution; valid state resolves through `WyrdPostgres`, while unknown, expired, consumed, and cross-tenant cases fail closed. | `FIND-TASK-002-6` |
| Debug formatting of both public login-state values never contains a sentinel PKCE verifier, while SQL storage and provider exchange behavior remain byte-equivalent. | `FIND-TASK-002-7` |

## Focused proof and broader verification

Add the smallest tests at the existing owners and extend the real-server identity journey where the behavior crosses the HTTP/server boundary. Specifically prove the seven acceptance rows above; do not create a new harness. Every newly named Rust test must be recorded and run with its exact `mise exec -- cargo nextest run --locked -p <crate> --lib|--test <target> -E 'test(=<exact-name>)'` command and repository-managed Postgres/IdP wrapper when required.

Run the original four focused identity journey wrappers and the complete identity lane, then run:

- `mise run test:principals:integration`
- `mise run test:principals:unit`
- `mise run test:sql`
- `mise run codegen:check`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `mise run check:client-tier`
- `mise run fmt`
- `mise run lints`
- `git diff --check`

If a required correction changes an approved public behavior, identity/security decision, persistent-data contract, or workload/platform semantics beyond the boundaries above, stop for specification revision rather than choosing a new policy during implementation.

## Implementation evidence

Commits: `81e15241a` (RLS-only login state, sealed verifier, owner state lookup, callback id-token policy), `7679f1004` (callback Postgres proofs), `33da71e43` (human `sub` enforcement and journey proofs), `ce2e53fd5` (completion rewrap rationale), plus this evidence commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: exact `sub` only; stored non-`sub` fails closed; same email + distinct `sub` are distinct Users | `wyrd-spec/src/auth/human_connection.rs` (`HUMAN_SUBJECT_CLAIM`, `validate`); `wyrd-auth/src/pg_resolvers.rs` (`IssuerDecodeError::HumanSubject` before login) | `human_subject_must_be_exactly_sub`; `stored_human_connection_requires_the_sub_subject_claim`; `same_email_different_subjects_are_distinct_users`; `tenant_connection_admin_journey` (PUT `subject: email` → 400 `WYRD_SPEC_400_VALIDATION`) | PASS |
| FIND-2: `azp` required for multi-audience, equality when present, nothing persisted on refusal | `wyrd-auth/src/callback.rs::verify_authorized_party`, called after verify/nonce, before `bound_connection` | `verify_authorized_party_enforces_azp_for_the_client`; `a_multi_audience_token_needs_azp_naming_the_client`; `tenant_callback_refusal_journey` (missing/foreign `azp` → 401; final success uses multi-audience with matching `azp`) | PASS |
| FIND-3: one `auth.user.roles.sync` on change, none unchanged, audit failure rolls back all | `role_assignments.rs::replace_user_roles -> bool`; `callback.rs::roles_sync_event` via `append_auth_audit` in the callback transaction; `audit.rs::USER_ROLES_SYNC_OPERATION` | `changed_roles_are_audited_once_and_unchanged_roles_never`; `a_failed_role_sync_audit_rolls_back_the_whole_login` (`WYRD_AUDIT_503_UNAVAILABLE`, nothing persisted); `a_withdrawn_oidc_group_invalidates_the_roles_it_granted` (1, 1, 2 events) | PASS |
| FIND-4: unadvertised algorithm refused before persistence; advertised accepted | `callback.rs::verify_id_token_algorithm` over fresh `id_token_signing_alg_values_supported` minus HMAC | `an_unadvertised_id_token_algorithm_is_refused`; `tenant_callback_refusal_journey` (discovery advertises RS256, EdDSA token → 401, no completion); every success path uses an advertised EdDSA | PASS |
| FIND-5: no manual tenant predicate; B cannot purge/consume/complete/redeem A; A one-use | `wyrd-sql/src/queries/auth/login_state.rs` (no `data_tenant_id` in transitions; tenant only as inserted ownership); source-shape test removed | `pg_login_state::pg_tests::login_state_transitions_are_confined_to_the_owning_tenant`; `mise run check:tenant-isolation` | PASS |
| FIND-6: no raw pool signature; lookup via `WyrdPostgres` fails closed | `WyrdPostgres::login_state_tenant(&Sha256Hex)` on private app pool; `resolve_by_login_state_for_app` removed | `pg_login_state::pg_tests::login_state_tenant_names_only_the_owner_of_a_pending_state` (pending A/B, unknown, expired, consumed); `mise run check:from-pools-allowlist` | PASS |
| FIND-7: Debug never prints the verifier; storage/exchange byte-equivalent | One `LoginState` with `code_verifier: SecretString`, exposed only at SQL bind and provider exchange | `login_state_debug_redacts_the_pkce_verifier`; human/refusal/switch journeys exchange the same verifier with the mock provider | PASS |

Focused commands (all exit 0 in this session):

- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=auth::human_connection::tests::human_subject_must_be_exactly_sub)'`
- `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::login_state::tests::initiation_columns_round_trip_and_refuse_ambiguity) | test(=queries::auth::login_state::tests::login_state_debug_redacts_the_pkce_verifier)'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::callback::pg_tests::verify_authorized_party_enforces_azp_for_the_client) | test(=auth::callback::pg_tests::a_multi_audience_token_needs_azp_naming_the_client) | test(=auth::callback::pg_tests::an_unadvertised_id_token_algorithm_is_refused) | test(=auth::callback::pg_tests::same_email_different_subjects_are_distinct_users) | test(=auth::callback::pg_tests::changed_roles_are_audited_once_and_unchanged_roles_never) | test(=auth::callback::pg_tests::a_failed_role_sync_audit_rolls_back_the_whole_login)' && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=pg_resolvers::pg_tests::stored_human_connection_requires_the_sub_subject_claim)' && cargo nextest run --locked -p wyrd-sql --test pg_login_state -E 'test(=pg_tests::login_state_transitions_are_confined_to_the_owning_tenant) | test(=pg_tests::login_state_tenant_names_only_the_owner_of_a_pending_state)'"`
- `mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<journey> mise run test:identity:journey` for `tenant_human_login_journey`, `tenant_callback_refusal_journey`, `tenant_provider_switch_journey`, `tenant_machine_independence_journey`

Lanes (all exit 0 in this session): `mise run test:identity:journey` (27/27), `test:principals:integration`, `test:principals:unit`, `test:sql`, `codegen:check`, `check:tenant-isolation`, `check:from-pools-allowlist`, `check:client-tier`, `fmt`, `lints`, `git diff --check`.

Reuse-finding dispositions: applied 1–8 and 10 (single `LoginState`; column-derived initiation kind with a `num_nonnulls = 1` CHECK replacing `initiation_kind`; `Sha256Hex::digest`/`From<[u8; 32]>`; `HumanConnections::redeem_completion` replacing `LoginCompletions`; nil-principal code-failure audit; `require_keyring` shared by staging and redemption). Skipped 9 (reuse `DiscoveryFixture` in journeys): it fixes advertised algorithms to `["RS256","EdDSA"]` and owns its server, so it cannot drive the per-case unadvertised-algorithm refusal FIND-4 requires. Security notes: verifier restored to `SecretString`; `completion_sealed` is intentionally outside `SealedSecretRewrap` because completions are unreadable after the two-minute TTL, which is recorded in `wyrd-auth/src/sealing.rs`. The retained old key must outlive the verification pass by that TTL.

Non-goals held: no BFF `/login/complete` route, no CLI handoff, no new public API, persistence resource, audit mechanism, or dependency; workload and platform-login verification unchanged. TASK-002 forward pointers updated to `known_initiation` / `HumanConnections::redeem_completion`.
