---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-001, REQ-005, REQ-018, REQ-021, INV-001, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: [TASK-009, TASK-010, TASK-011, TASK-012]
---

# Docs and architecture authority for the standard flows

## Outcome and Value

Operators and SDK users read documentation and architecture authority that
match the shipped standard flows: OIDC relying party on `openidconnect`, Wyrd
as the OAuth authorization server for the code, device, refresh, revocation,
token-exchange, and jwt-bearer grants, the BFF as a confidential client with
an encrypted cookie session, and the shared client on `oauth2`. This is report
item T5 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md).

## Owners, Scope, Consumers, and Prohibited Changes

Own public self-hosted and hosted docs, CLI help text, source documentation
that feeds generated declarations, and architecture authority
(`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
`architecture/wyrd-doctrine.mdx` where they conflict). Consume the completed
journeys of TASK-001–004 and TASK-009–012. Do not add hosted signup, social
login, a commercial stub, provider credentials, a provider-specific code path,
or a certified-provider list. Do not document SAML or SCIM; both are deferred.

Delete: every passage about the login handoff, the private BFF channel
(`/internal/bff/v1/*`, `x-wyrd-bff-key`), server-side browser sessions, sealed
login completion, and the keyring protecting session or login credentials.

## Approach

1. Reconcile public API, generated schemas, UI, CLI help, and docs against
   spec revision 11.
2. Document operator and IdP responsibilities, setup inputs and exact
   callback, one active connection, role mapping, CLI device login, saved-login
   selection (newest login by default, `tenant` to select), independent
   machine identity, and recovery. The keyring is required only when a
   provider secret is stored (REQ-005).
3. Document the OAuth surface: the authorize, token, platform token,
   revocation, and device authorization endpoints, RFC 8414 metadata, the
   `wyrd-ui` confidential and `wyrd-cli` public clients, refresh rotation for
   public clients only, the RFC 6749 §5.2 error mapping, and the one sanctioned
   exception to `WyrdError` problem+json for these endpoints.
4. In the security posture, change "rotated on every successful use" for
   access and refresh tokens to "rotated on every use for public clients".
5. Keep the generic provider setup page plus short examples for Okta,
   Microsoft Entra ID, Google, Auth0, and Keycloak.

## Proof Strategy

This task changes documentation and verifies already implemented behavior. It
adds no production executable logic, so a manufactured RED does not apply.
Use static contract comparison, the existing real-server journeys, and
generated-document checks. If closing a gap needs executable changes, route
it to the owning task (TASK-009–012); if it changes approved behavior, return
to the spec.

## Acceptance Criteria

- AC-001–009 have recorded owner evidence across self-hosted, hosted, UI, CLI,
  Rust/Python/TypeScript SDK, and machine paths.
- FIND-TASK-004-8: shared-client, Python, and TypeScript configuration docs,
  test-wrapper docs, and the generated TypeScript declaration describe
  newest-login selection, selected-tenant mismatch behavior, and RFC 8628
  device login only. Generated output is regenerated from its source owner,
  never hand-edited.
- No doc, help text, or architecture passage describes the handoff, the BFF
  channel, browser-session rows, sealed completion, or session sealing keys.
- Docs never promise instant access-token revocation or advertise production
  SSO before the production journey passes. No provider-specific code exists.

## Expected Write Set and Consumer Closure

`docs/`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
`architecture/wyrd-doctrine.mdx` where it conflicts, CLI help text, source
docs in `crates/shared/wyrd-client` and `sdks/*`, and generated artifacts
from their sources.

## Verification and Evidence

List each acceptance criterion's owning journey selection without running the
suite:
`mise exec -- cargo nextest list --locked -p wyrd-server --test identity_e2e --run-ignored=all`
and the equivalent CLI, UI Vitest, Python pytest collection, and TS Vitest
listings. Record each acceptance criterion's owner test and its result from
the owning task's evidence; the unfiltered journeys run once at change review.

Then run only the lanes covering this write set: `mise run docs:check`,
`mise run codegen:check` and `mise run ts:napi:check` (generated artifacts
and the TypeScript declaration), `mise run fmt` and `mise run lints` (CLI help
text and Rust source docs), `mise run py:format` and `mise run py:lints` when
Python source docs change, and
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` for UI text
changes.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if a
public contract differs from the approved spec, a standard provider would
need a provider-specific branch, or hosted signup becomes required in the
open-source server.

## Authority Links

[Approved spec](../spec.md);
[research report](../research/auth-standards-recommendation.md);
[routing](../review/TASK-004-r2/lead-direction-routing.md);
[AGENTS.md](../../../../AGENTS.md);
[Wyrd design](../../../../architecture/wyrd-design.md);
[security posture](../../../../architecture/wyrd-security-posture.md);
[testing workflow](../../../../architecture/references/languages/testing-workflows.md).

## Implementation Evidence

Commits: `bc88ea9d5` (architecture), `74b2ad6b6` (self-hosting SSO guide,
authentication, configuration, Kubernetes), `8e73561eb` (CLI reference, client
configuration, CLI help, server and SQL rustdoc, UI README, error-catalog
intro), `6409da059` (Python stub source, PyO3, TS source and napi rustdoc;
regenerated `client/__init__.pyi`, `index.d.ts`, `index.d.cts`), and the
follow-up remediation-doc and shared-client config commits.

Owner journeys were listed, not run (task contract); results are the owning
tasks' recorded evidence.

| Acceptance criterion | Implementation evidence (docs) | Owner journey (listed) | Result |
|---|---|---|---|
| AC-001 OIDC-off deployment, UI and SDK with an existing credential | `self-hosting/sso-and-oidc.svx` (OIDC optional; Operator recovery), `concepts/authentication.svx` (recovery page) | UI `OIDC-off credential UI` (`production_ui_bff_journey`); TASK-003/011 evidence | PASS |
| AC-002 self-hosted provider, exact callback, UI sign-in, role, allowed and denied call | `sso-and-oidc.svx` Deployment setup, Registering Wyrd at your IdP, Role mapping, Signing in | `tenant_human_login_journey`, UI `production SSO crosses replicas`; TASK-009/011 evidence | PASS |
| AC-003 hosted two tenants, two providers, wrong-tenant and same-issuer refusal | `sso-and-oidc.svx` Who operates what (one active connection per tenant, independent tenants) | UI `production multi-provider tenant switch`, `tenant_callback_refusal_journey`, `tenant_callback_issuer_binding_journey`, `same_issuer_two_tenant_isolation_keycloak`; TASK-002/011 evidence | PASS |
| AC-004 Rust/Python/TS use CLI login, renew, reject revoked; tenant selection | `get-started/client-configuration.svx` (chain, Saved user logins, mismatch), `reference/cli.svx` (`auth login`/`logout`/`status`), SDK docs and regenerated declarations | `cli_device_login_journey`, `saved_user_auth_journey`, `test_saved_user_auth_journey`, `saved user auth journey`, `concurrent_saved_renewal`, `human_oidc_login_journey`, `device_grant_refusal_journey`; TASK-010/012 evidence | PASS |
| AC-005 machine identity independent of SSO; exact workload binding | `sso-and-oidc.svx` Signing in (machines unchanged), workload sections; jwt-bearer curl now form-encoded (RFC 7523 §2.1) | `tenant_machine_independence_journey`, `workload_jwt_bearer_journey_keycloak`, `federated_cloud_journey_cli_authored_keycloak`; TASK-002/012 evidence | PASS |
| AC-006 provider switch with test sign-in | `sso-and-oidc.svx` lifecycle, testing, activation (kept) | `tenant_provider_switch_journey`, `tenant_connection_test_sign_in_journey`, UI `production provider replacement settings`; TASK-002/011 evidence | PASS |
| AC-007 fault and security behavior | `sso-and-oidc.svx` Login failures, Deployment setup (secret rotation, ingress rate limit), `self-hosting/authentication.svx` | `tenant_callback_refusal_journey`, `device_grant_refusal_journey`, `tenant_connection_rotation_journey`, `tenant_connection_session_cutoff_journey`, `a_withdrawn_oidc_group_invalidates_the_roles_it_granted`, `revoking_a_human_kills_the_session_refresh_authority`, `key_rotation_keycloak_admin_api`; TASK-009/010 evidence | PASS |
| AC-008 provider agnostic | `sso-and-oidc.svx` (standard OIDC only; generic registration plus Okta, Entra ID, Google, Auth0, Keycloak notes; no certified list) | Keycloak and Dex journeys above; no provider branch (TASK-009 evidence) | PASS |
| AC-009 contracts, CLI help, UI, docs, schemas agree | Architecture (`wyrd-design.md` Human sign-in, `wyrd-security-posture.md` OAuth authorization server, public-client rotation), concept and self-hosting docs, `wyrd auth` help, errors intro | `mise run docs:check`, `mise run codegen:check`, `mise run ts:napi:check` | PASS |
| FIND-TASK-004-8 SDK, test-wrapper docs, TS declaration | `wyrd-client/src/config.rs` `tenant`, `stubs/client.pyi`, `wyrd-sdk-python/src/client.rs`, `wyrd-sdk-ts/wyrd/src/index.ts`, `wyrd-sdk-ts/native/src/client.rs`; generated outputs regenerated by `codegen:stubs` and `ts:build`; testing wrapper already states RFC 8628 | `codegen:check`, `ts:napi:check` | PASS |
| No handoff, BFF channel, browser-session, sealed completion, or session keyring passage | Rewritten security posture, design, concepts, boot rewrap rustdoc and error text | `git grep` audit over `docs/src`, `architecture`, UI README: no match | PASS |
| No instant-revocation promise; no provider-specific code | Docs state access tokens live up to five minutes after logout or revocation | Docs-only diff; no executable change | PASS |

Listings: `cargo nextest list --locked -p wyrd-server --test identity_e2e
--test identity_ui_e2e --run-ignored=all` (32 tests),
`-p wyrd-cli --test cli` (`cli_device_login_journey`),
`-p wyrd-sdk-rust --test cards_state` (`saved_user_auth_journey`),
`-p wyrd-client` (`concurrent_saved_renewal`), pytest `--collect-only`
(`test_saved_user_auth_journey`), SDK Vitest `list` (`saved user auth journey`),
UI Vitest `list` with `WYRD_UI_INTEGRATION=1` (the four production tests).

Verification (all exit 0): `mise run docs:check`, `mise run codegen:check`,
`mise run ts:napi:check`, `mise run fmt`, `mise run lints`,
`mise run py:format`, `mise run py:lints`, `git diff --check`. The UI `pnpm
check` was not run: the only UI-tree change is `README.md`.

Non-goals stayed excluded: no hosted signup, social login, provider code,
certified-provider list, SAML, or SCIM documentation.
