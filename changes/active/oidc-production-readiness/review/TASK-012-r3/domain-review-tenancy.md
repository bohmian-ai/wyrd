# TASK-012 r3 tenancy domain review

## Result

**PASS**

No material tenancy, authority, or saved-login isolation finding remains in the
cumulative candidate `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447` relative to
base `adf349081077b3cfe0d56ab9a665cf01e2d86da4`.

## Reviewed boundary

This review traced only the tenant-selection and saved-login boundary required
by TASK-012:

1. `wyrd auth login` begins the device grant with a typed `TenantSlug`, derives
   the deployment key through the shared canonical-origin owner, and saves the
   returned access/refresh pair under `(origin, tenant_key)`
   (`crates/wyrd/wyrd-cli/src/auth/login.rs:56-80,100-139`).
2. `SavedLogin` persists exactly the canonical origin, route key, and token
   pair. `SavedLogins::save` replaces the same origin/key record and appends it
   as newest; `select` first scopes candidates to the exact origin, then either
   requires the requested key or takes the last record for that origin
   (`crates/shared/wyrd-client/src/saved_login.rs:40-52,72-89,178-187,200-235`).
3. `ClientConfig::resolve_credential` gives an explicit credential and the
   environment tiers precedence over saved logins. A saved login is considered
   only for the canonical origin of the configured HTTP server; an unmatched
   tenant fails with `tenant_mismatch` instead of falling through. A tenant
   selector beside a bearer or API key is refused because those credentials
   already name their tenant (`crates/shared/wyrd-client/src/config.rs:161-233`).
4. A selected login becomes a `SavedLoginSource` bound to that exact origin and
   route key. Every mint rereads that record under the file lock; renewal sends
   only its refresh token through the OAuth refresh grant and atomically saves
   the rotated pair under the same key
   (`crates/shared/wyrd-client/src/saved_login.rs:237-247,274-333,386-434`;
   `crates/shared/wyrd-client/src/auth.rs:373-394`).
5. Rust, Python, and TypeScript converge at the same constructor. The language
   bindings forward `server_url`, `credential`, and `tenant` to
   `client_from_options`, which fills one `ClientConfig` and calls
   `WyrdClient::with_config`; that constructor resolves one credential and
   shares its `AuthMiddleware` with the normalized HTTP transport
   (`crates/shared/wyrd-client/src/bifrost/facade.rs:940-977`;
   `sdks/wyrd-sdk-python/src/client.rs:34-58`;
   `sdks/wyrd-sdk-ts/native/src/client.rs:49-72`;
   `crates/shared/wyrd-client/src/client.rs:71-89`).

## Authority coverage

| Authority | Obligation checked | Result |
|---|---|---|
| Spec rev11 REQ-012 and AC-004 | Saved records are keyed by server and tenant; explicit credentials override; tenant selects one same-server record; omission selects that server's newest record; renewal stays on the selected record. | PASS |
| Spec rev11 INV-001 | A path, host spelling, browser value, or client selector cannot become effective tenant identity after login. | PASS |
| Spec rev11 INV-003 and INV-005 | Client selection cannot merge principal planes or create durable identity/permission authority. | PASS |
| Spec rev11 INV-007 | CLI and all SDKs share the Wyrd credential and one Rust resolution/renewal owner. | PASS |
| `architecture/wyrd-security-posture.md` | Tenant identity comes only from verified credentials; clients cannot assert durable identity; client-supplied tenant identity is not API authority. | PASS |
| `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx` | Durable identity and permission remain server-owned; Python and TypeScript project `wyrd-client` rather than implementing another resolver. | PASS |
| Original TASK-012 and R1/R2 remediation tasks | One conventional OAuth client owns device/refresh; one normalized, userinfo-free origin is shared by saved-login, auth, and HTTP transport; no language-specific token path is introduced. | PASS |

## Source conclusions

- **Canonical server identity.** `canonical_origin`, `TokenExchange::new`, and
  `HttpTransport::new` all consume `HttpConfig::validate`. That method parses
  once and returns `HttpsOrigin::of_url`, which uses the URL standard's origin
  serialization: scheme/host case and default-port aliases converge, and path,
  query, and fragment are discarded. The normalized origin is therefore both
  the saved-login namespace and the actual root used for auth and HTTP calls
  (`crates/shared/wyrd-client/src/transport/config.rs:204-246`;
  `crates/shared/wyrd-client/src/saved_login.rs:108-128`;
  `crates/shared/wyrd-client/src/auth.rs:221-278`;
  `crates/shared/wyrd-client/src/transport/http.rs:143-176`). This deliberate
  collision is between equivalent spellings of one deployment, not tenants.
- **Userinfo rejection.** `HttpsOrigin::of_url` refuses username or password
  before serializing an origin, and `HttpConfig::validate` does not echo the
  configured URL in that refusal (`crates/wyrd-spec/src/operator_connection.rs:97-126`;
  `crates/shared/wyrd-client/src/transport/config.rs:218-245`). No userinfo can
  become a saved key, endpoint, or diagnostic target.
- **Explicit override.** The explicit credential occupies tier zero. When it
  resolves, no saved-login lookup occurs. If a tenant selector is also present,
  bearer/API-key credentials fail locally rather than silently interpreting the
  selector; workload assertions remain the one conventional case where the
  route key is sent to the server for verified exchange
  (`crates/shared/wyrd-client/src/config.rs:163-203,206-233`;
  `crates/shared/wyrd-client/src/transport/credential.rs:146-177,242-276,308-327`).
- **No cross-tenant saved-login selection.** `select` cannot fall from a named
  tenant to another record and cannot consider a record under another origin.
  An omitted selector chooses only the newest candidate already scoped to the
  same origin. The tenant key remains local selection metadata: authenticated
  requests carry the selected access token, and refresh carries its refresh
  token. Neither operation sends the saved key as effective tenant identity.
  Editing or relabeling the user-owned file therefore cannot widen authority;
  it can only cause the client to present another credential already possessed
  by that local user, whose server-verified tenant and permissions remain
  authoritative.
- **Renewal binding.** `SavedLoginSource` retains `(origin, tenant_key)`, rereads
  that exact record while holding the exclusive file lock, and exchanges that
  record's refresh token. Rotation updates the same in-memory record before the
  atomic write. No tenant selector is consulted again during renewal and no
  sibling record supplies the refresh token.
- **Durable authority.** The cumulative runtime diff changes client OAuth,
  endpoint normalization, saved-login renewal plumbing, and browser launch;
  it changes no server identity, tenant resolution, principal, RBAC, token
  issuance, or persistence owner. The client continues to select a credential;
  the server continues to derive effective tenant and permissions from the
  verified credential.

## Focused verification

The following exact tests each selected one test and passed:

- `saved_login::tests::canonical_origin_is_the_url_origin`
- `saved_login::tests::selection_picks_the_named_or_newest_login`
- `config::tests::saved_login_ranks_between_env_and_credentials_file`
- `config::tests::tenant_selector_is_refused_beside_a_self_naming_credential`
- `transport::config::tests::https_and_loopback_http_are_accepted`
- `transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused`

All were run with `mise exec -- cargo nextest run --locked -p wyrd-client
--lib -E 'test(=...)'` against the immutable candidate.

## Verification limits

- I did not rerun the Keycloak/Postgres Rust, Python, or TypeScript identity
  journeys. Their source was inspected to confirm that each exercises the same
  shared resolver and asserts named/newest selection, explicit precedence,
  renewal, authorization denial, and revoked-refresh refusal. Those journeys
  require the repository-managed identity environment and are broader than
  this focused domain pass.
- I did not broaden into unrelated server-side tenant provisioning, OIDC
  connection tenancy, SQL RLS, Bifrost storage tenancy, or platform-plane
  authorization.
- I did not treat locally editable route-key labels as server authority. OAuth
  access and refresh tokens are conventionally opaque client credentials; the
  security boundary is the receiving server's verification and binding, not a
  second client-side tenant-claim parser.

## Material findings

None.

