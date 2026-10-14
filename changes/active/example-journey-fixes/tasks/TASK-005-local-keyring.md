# TASK-005 Gateway managed secrets use the deployment sealing keyring

Spec: `../spec.md` REQ-007 (r2), INV-001, INV-002, AC-001..AC-003.

Revision note: rewritten per owner directive 2026-10-14: reuse the deployment
sealing keyring. The earlier per-tenant keyring with a restart on a new key is
withdrawn.

Seal and open gateway `managed_secret` credentials with the existing
deployment `SealingKeyring`, exactly as the other stored sealed secrets do:
same envelope, same tenant binding, same boot rotation and rewrap. Do not add
a key derivation, envelope format, or configuration. Delete the separate
gateway keyring: `gateway.managed_secret_keys`, `GatewayManagedSecretKeys`,
the gateway `ManagedSecretKeys`/`TenantKeyring` plumbing, and
`WyrdTestServer`'s keyring materialization. No compatibility path; add a new
migration for the schema change.

`wyrd server dev --tenant <slug>` must give a fresh deployment a sealing key
before first boot, through the existing `WYRD_SEALING_KEY_FILE` mechanism,
with no restart and no CLI copy of the server's config lookup. Never print a
key or a stored secret.

Evidence: a fresh CLI journey (`wyrd server dev`, quickstart-form
`wyrd gateway credential put`, a gateway call through a local mock provider
that receives the stored secret, a rerun that keeps working, the secret absent
from all output); gateway journeys showing managed secrets still work and stay
tenant-isolated; a rotation test through the existing sealing rewrap; docs
quickstart, setup, deployment configuration, and troubleshooting updated.

## Implementation evidence

Reuse map: the owner is `wyrd_crypt::SealingKeyring` (boot-built into
`AuthState::sealing_key`) with `wyrd_auth::sealing::SealedSecretRewrap` over
`SealedSecretTable::ALL`. The gap was only that the gateway table was not a
sealed-secret table; adding `SealedSecretTable::GatewayProviderCredentials`
puts it under the existing rewrap. Tenant separation rests on the row's RLS
plus `ManagedSecretBinding` (tenant, name, provider) inside the authenticated
payload, compared on open. `wyrd server dev` passes `WYRD_SEALING_KEY_FILE`
to the child server it already launches, pointing at an owner-only
`<config dir>/server/sealing.key` it creates once (unless the caller set a
sealing key) and reuses.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Managed secrets sealed/opened with the deployment keyring, bound to tenant/name/provider | `crates/wyrd/wyrd-gateway/src/managed.rs` (`ManagedSecretBinding::seal/open`), `credential.rs`, `components/gateway/service.rs` | `managed::tests::sealed_values_open_only_for_their_own_binding`; `pg_administration_tests::gateway_managed_secrets_persist_sealed_and_bound_envelopes` | PASS |
| Tenant isolation (INV-001) | RLS on `wyrd.gateway_provider_credentials`; binding compare on open | `pg_invocation_tests::gateway_managed_credentials_resolve_per_tenant_across_restart_and_rotation` (each tenant's own key dispatched; another tenant's envelope copied into a row refused before dispatch); `compatible::managed_secret_rotation_survives_server_restart` (cross-tenant 404 and invoke refusal through the real server); `check:tenant-isolation` | PASS |
| Rotation through the existing sealing rewrap | `SealedSecretTable::GatewayProviderCredentials` in `wyrd-sql/src/queries/auth/human_connections.rs` | same pg_invocation test: K2+K1 resolves, K2-only refuses before rewrap, `boot::rewrap_sealed_secrets` reseals, K2-only resolves both tenants; `managed::tests::deployment_rotation_rewraps_managed_secrets_under_the_new_write_key`; `boot::sealing_boot_pg_tests::keyless_boot_refuses_only_when_ciphertext_is_stored` | PASS |
| Separate gateway keyring deleted, no alias | `config.rs`, `state.rs`, `boot/mod.rs`, `wyrd-testing/src/server.rs`; migration `20261014000000_gateway_managed_secret_sealing_keyring.sql` drops `secret_key_version`, `secret_nonce` | `git grep` finds no `managed_secret_keys`, `TenantKeyring`, `ManagedSecretKeys`, `GatewayManagedSecretKeys`, `gateway_secret_keys`; `mise run lints` | PASS |
| `wyrd server dev` provides a sealing key before first boot, no restart | `crates/wyrd/wyrd-cli/src/server/dev.rs` (`local_sealing_key`) | `pg_tests::server_dev_keeps_the_gateway_managed_secret_key` | PASS |
| CLI journey: dev, quickstart put, gateway call delivers secret, rerun, no leak (INV-002) | `crates/wyrd/wyrd-cli/tests/pg_tests.rs` | same test (mock provider answers only with `Bearer <stored secret>`, `.expect(2)`); added to `test:local-dev:journey` | PASS |
| Docs updated | quickstart, setup, deploy-wyrd-for-a-team, troubleshoot-a-deployment; `architecture/wyrd-security-posture.md`; spec doc comment and regenerated schema | `mise run docs:check`, `mise run codegen:check` | PASS |

Commands:

- `mise run fmt`; `mise run lints`; `mise run check:deps`;
  `mise run check:tenant-isolation`; `mise run codegen:check`;
  `mise run docs:check` — pass.
- `mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(/^managed::tests::/)'` — 3 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::/) | test(/sealed|rewrap|sealing/)'` — 53 passed.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_keeps_the_gateway_managed_secret_key)'` — passed.
- `mise run test:gateway:gate` (Rust gateway, Vault, CLI, MCP journeys) — passed.
- `mise run test:local-dev:journey` — 4 Rust pg_tests passed (including the
  new journey) and 5 Python/TypeScript installed-CLI bootstraps passed.

Diagnosis (rotation test, first run): Symptom: after the rewrap, the K2-only
replica that had served the pre-rewrap refusal failed with "no provider
attempt completed". Evidence: `wyrd-gateway/src/health.rs` marks a deployment
unhealthy for 30 s per process after a failed attempt (`engine.rs:645`).
Cause: the test reused that replica, so its own earlier refusal held the
deployment in cooldown. Fix site: the test; the post-rewrap assertions now use
a freshly booted K2-only replica, as a rollout does.

Non-goals stayed excluded; no unrelated files changed. Material risk:
envelopes stored under the removed per-tenant keyring no longer open and must
be put again (pre-release only; a keyless boot refuses while they remain).
