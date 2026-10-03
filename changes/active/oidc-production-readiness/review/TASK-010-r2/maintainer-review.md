# Maintainer Review — TASK-010 R2

## Immutable subject

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Prior candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`

`FIND-TASK-010-1` is routed to TASK-011 by
`review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`; this review does not
reopen it.

## Changed-surface coverage

| Changed surface | Owners and symbols inspected | Callers, declarations, and proof inspected | Result |
|---|---|---|---|
| OAuth wire contracts | `CallbackQuery::response`, `ProviderResponse`, `OAuthErrorCode::InvalidTarget`, `TokenRequest`, generated callback/token schemas | callback extraction, token form decoding, shared-client callback projection, schema generator and checked-in schema parity | PASS |
| Authorization and provider callback | `authorize`, `unique_param`, `callback`, `exchange_authorization_code`, `provider_refusal`, `fence_bound_connection` | router registration, registered redirect checks, provider success/error paths, connection-slot owner, callback PG tests and filtered identity journeys | PASS |
| Device approval and redemption | `CliLogins::approve`, device login-state insertion, approval race tests, token redemption | SQL unique-index blocking path, denial and expiry/delete writers, no-authority assertions, existing exactly-once redemption test | PASS |
| Refresh containment and tenancy | `RefreshTokens::execute`, `active_refresh`, `revoke_refresh_chain`, `ACTIVE_REFRESH_SQL` | rotated-predecessor and inactive-row consumers, `TenantConn` RLS behavior, cross-tenant/expired/revoked tests | PASS |
| Audit | `LOGIN_OPERATION`, callback `login_event`, canonical `append_auth_audit` call | unchanged-role, role-changing, device, failure/rollback, and redemption audit paths | PASS |
| OAuth client authentication and served OpenAPI | `OAuthClients::identify`, `ClientForm<T>`, `SecurityAddon`, token/device/revoke operation declarations | Basic scheme variants, public `client_id`, served document assertions and anonymous-versus-Basic security requirements | PASS — prior `MAINT-010-1` is closed |
| Gateway admission and dependencies | removal of `tower_governor`/`governor`; NGINX `map`, `limit_req_zone`, and `/auth/device` limit | official NGINX route, same-client burst, other-route and other-client assertions, workspace/server manifests and lockfile | PASS for the device-limit owner; the startup proof mutation has the finding below |
| Official-image startup proof | `test:server:startup`, `serving_env`, key volume setup, development boot, production restart | `WyrdServerConfig`, `OperatorKeysConfig::validate`, `verify_operator_keys`, deployment doctrine, lane history and recorded failure diagnosis | **FAIL — `MAINT-010-R2-1`** |
| Tests and documentation | materially changed Rust tests/helpers, OpenAPI contract tests, startup shell assertions, task evidence | mandatory rustdoc, exact named tests, generated declaration parity, recorded narrow owner lanes | PASS apart from `MAINT-010-R2-1`; placement, naming, structure, and wording observations do not block |

The cumulative base-to-candidate review also retained the original task's
authorization-code, device, refresh, revocation, token-exchange, deletion,
persistence, and client-consumer seams. The R1 changes are localized in their
existing owners and do not introduce another durable authority or custom
protocol mechanism.

## Material finding

### MAINT-010-R2-1 — The startup lane changes deployment mode to bypass the multi-tenant production gate

- **Classification:** VIOLATION / REGRESSION.
- **Changed location:** `scripts/server/test-startup.sh:53-63,116-125,228-231`.
- **Governing rule:** `AGENTS.md` section 12 forbids weakening a check to make
  it pass and requires the diagnosed failure itself to be fixed. The active
  design states that multi-tenant production requires per-tenant KEKs from
  Vault over HTTPS (`architecture/wyrd-design.md:1323-1338`). The production
  code enforces that contract when no implicit tenant is configured
  (`crates/wyrd/wyrd-server/src/config.rs:1726-1732,1781-1811` and
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1421-1453`).
- **Evidence:** Before R1, the official-image lane restarted with
  `APP_ENV=production` and no `WYRD_SERVER_TENANT_SLUG`, which selects the
  server's multi-tenant production path. R1 records that this restart failed
  because the lane supplied no required Vault-backed Operator KEKs. Instead of
  provisioning that existing production dependency, the candidate sets
  `WYRD_SERVER_TENANT_SLUG=acme`, `WYRD_OPERATOR_KEK_SOURCE=file`, and an
  owner-only file key. Those settings are valid for a single-tenant deployment,
  but they cause both `OperatorKeysConfig::validate` and
  `verify_operator_keys` to skip the multi-tenant Vault requirements. The same
  edit is explicitly described as making the formerly red lane pass.
- **Concrete maintenance and proof consequence:** `mise run
  test:server:startup` can now be green while the official image cannot start
  in its repository-defined multi-tenant production topology, cannot read the
  active tenant key set from Vault, or fails the fail-closed key verification
  performed before readiness. Maintainers therefore lose the only official
  image/restart proof of that production configuration. This is not a wording
  or test-placement concern; the asserted runtime mode changed.
- **Smallest testable correction:** Preserve the owner-only signing-key volume
  fix and the focused NGINX device-limit assertions, but restore the startup
  lane's no-implicit-tenant production restart. Configure the lane through the
  existing `OperatorKeySource::Vault`/`wyrd-vault` KV-v2 path over HTTPS and
  seed the active tenant key needed by the already-created tenant before the
  production restart. Do not add a new key source, public option, custom secret
  mechanism, or second startup harness. Closure is the existing `mise run
  test:server:startup` showing the same development write, production restart,
  persistence verification, and gateway assertions while the production
  process has no `WYRD_SERVER_TENANT_SLUG` and uses Vault.

## Prior-finding closure

- Prior maintainer finding `MAINT-010-1` is closed. `ClientForm<T>` publishes
  the public client's optional `client_id`; `SecurityAddon` publishes ordinary
  RFC 7617 Basic; token, device-authorization, and revocation operations declare
  the alternatives; and `tenant_login_operations_publish_their_contract`
  verifies the served document.
- The four R1 rustdoc omissions are closed at their cited items. No new or
  materially modified Rust item inspected has a blocking documentation gap.
- `FIND-TASK-010-1` remains routed to TASK-011 and is not a maintainer finding
  in this round.

## Non-blocking calibration

- The file-backed KEK setup is a conventional and supported configuration for
  an explicitly single-tenant production deployment. It is not itself bad
  code; it is the wrong replacement for this lane's existing multi-tenant
  production proof.
- The generic OpenAPI-only `ClientForm<T>` adds no runtime parsing path and is
  the smallest local way to make the generated operation describe the actual
  standard OAuth form. No additional wrapper or abstraction is warranted.
- No placement, naming, structure, or wording observation is promoted to a
  finding.

## Verification evidence and limits

The candidate task records passing focused auth, SQL, OpenAPI, codegen,
boundary, format, lint, identity, CLI, and startup lanes. This maintainer pass
performed source/diff inspection and `git diff --check`; it did not rerun the
costly recorded lanes. Narrow task verification is appropriate here, with full
journeys deferred to change review. A green `test:server:startup` does not
close `MAINT-010-R2-1` because its production deployment mode is the changed
subject of the finding.

## Overall result

**FAIL** — `MAINT-010-R2-1` is one bounded gate-regression finding. The
single-tenant file-KEK configuration is legitimate in isolation, but changing
the established production restart to that mode is a weakened gate under
`AGENTS.md` section 12.
