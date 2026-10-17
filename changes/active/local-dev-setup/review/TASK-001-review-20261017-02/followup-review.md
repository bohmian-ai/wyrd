# Focused follow-up: retained local identity

**RESOLVED. Retain FIND-TASK-001-2 (INCORRECT), narrowed to incomplete pre-provision identity proof.** BEH-R2-1 and INV-R2 identify reachable states that the security/system/maintainer closure claims do not trace. This is source-backed discovery for independent validation, not a majority decision or final task verdict.

## Subject, inputs and scope

Base `bc3bf25e3f78b0c429110350a3de4d58290e632f` → candidate `38dbc023cd71292e1badd3abe90e87c69b76fa96`; HEAD matched. Original task/spec revision 1, prior verdict/validated ledger and R1 (including its focused passing evidence) were consulted. Used the cumulative identity/configuration/test changes, with the latest remediation range only as a locator. Read the conflicting current reports, existing shared client/authentication, saved-login writers/readers, protected settings writers, installed setup and tenant-directory owners. No CodeGraph index exists. Scope is only this identity conflict in this worktree; no other findings or siblings were reassessed.

Governing obligations: spec REQ-002 preserves unrelated tenant/deployment configuration and refuses adoption; REQ-008 requires validated overrides, preserved identity and recoverable one-time-key persistence; INV-003/004 and prior FIND-2/R1 require refusal before selection replacement or unsavable issuance. AGENTS client/server ownership and existing credential-resolution semantics constrain the correction.

## Producer-to-consumer evidence

### 1. Same-origin retained key for B, no `[dev]`, request absent A

This is an ordinary existing-client state, not corrupt storage: `GlobalConfig::save_local` writes `[default].api_key` and `[client]` endpoints without requiring a `[dev]` section (`crates/shared/wyrd-client/src/global_config.rs:213-257`). Existing login/config writers preserve it.

1. In `crates/wyrd/wyrd-cli/src/server/dev.rs:307-327`, a different requested slug is rejected only when a saved `[dev].tenant` exists. With none, the new origin guard rejects a different origin, but accepts the same loopback origin. No human login is needed for this case.
2. At `:535-549`, the retained B key triggers `saved_client(endpoint, grpc)`. Its complete body (`:1269-1304`) constructs an ordinary client, obtains a token and lists one Card. B's real admin key authenticates and reads B successfully. Neither operation asserts that B is requested A.
3. `run` then calls `save_settings` before `provision` (`:550-551`). `save_local_dev` (`global_config.rs:145-200`) writes the selected DSNs and A's development selection using the existing protected replacement owner. A successful unrelated identity proof has therefore already allowed configuration mutation.
4. Existing `crates/wyrd/wyrd-server/src/main.rs::setup` (`:261-302`) reads active tenants. A is absent, so with legitimate `WYRD_PLATFORM_CREDENTIAL` it invokes the ordinary authorized platform creation operation and receives A's one-time key. This is authorized creation, not an authz bypass.
5. `save_tenant` (`dev.rs:619-638`) passes saved B to `ProvisionedTenant::credential`; that method (`:916-933`) chooses the freshly issued A key **before** saved B. Its UUID-prefix check consequently succeeds for A.
6. Only `GlobalConfig::save_local` detects that existing B differs from candidate A (`global_config.rs:233-247`). Its correct no-overwrite refusal leaves B intact, but the new A key is dropped, A remains active without its key saved, and `[dev]` already selects A. With no platform credential, setup can instead refuse after step 3; that still violates selection preservation.

Thus server acceptance of some retained key is insufficient expected-tenant binding. The final conflict guard must remain, but cannot undo prior effects.

### 2. Same-origin/same-slug saved login for selected B masks retained A key

`SavedLogins::save` (`crates/shared/wyrd-client/src/saved_login.rs:196-202`) replaces only the same origin/slug login; its writer (`:405-429`) preserves unrelated default keys. `SavedLogin` stores origin and slug, not deployment/tenant UUID (`:42-54`). A user can log in to B at the reused local origin while retaining A's default key and old dev selection.

`validate_saved_login` (`dev.rs:364-383`) compares the selected login's slug only. For B with A's same slug this passes. `saved_client` leaves `ClientConfig.credential` unset and copies the normal client selector/configuration. `ClientConfig::resolve_credential` (`crates/shared/wyrd-client/src/config.rs:221-249`) correctly selects the saved human login ahead of the default key. A valid B login with Card-read permission therefore satisfies the supposed A-key preflight. `save_settings` changes the selected database; B's existing setup returns B's UUID; the saved A key is rejected by `ProvisionedTenant::credential` only afterward. A default key revoked/invalid on the selected deployment can similarly be masked by a valid human login.

This is one root cause: the preflight establishes neither use of the exact retained key nor its binding to the requested active tenant. Changing normal shared credential precedence would regress standalone clients and the required fresh-client behavior after setup.

## Why the existing passing proof does not close it

The extended `pg_tests::server_dev_repeat_and_partial_failure_preserve_state` switches from A into an empty database with no competing login. The exact default A key consequently fails exchange immediately; its assertions correctly prove that narrower case. The remote-origin cases exercise the new literal origin guard. Neither creates case 1's valid B credential/requested absent A, nor case 2's successful higher-priority B login. The listener-override proof establishes legitimate same-deployment behavior and must remain.

The closure reports accurately locate the preflight before writes, but infer “saved key accepted by selected server” from `saved_client`'s name and successful bounded read. Its actual credential resolver and missing expected-tenant input invalidate that inference.

## Retained proposal FUP-R2-IDENTITY / FIND-TASK-001-2

- **Classification:** INCORRECT; existing prior root cause, no new FIND number.
- **Location:** `dev.rs::DevServer::run` preflight and `saved_client`, with the expected identity produced by existing server `setup`/tenant-directory read. The protected-store conflict check is correct.
- **Consequence:** unrelated selection can still be persisted, and legitimate platform creation can still issue a key that the retained default-key conflict makes unsavable before final refusal.
- **Minimum safe correction:** complete both proofs before `save_settings` or a setup call capable of issuance: (a) authenticate the **exact** protected `GlobalConfig::local_api_key` against the selected listeners using existing `ClientConfig` explicit-credential support, no tenant selector and an in-memory token cache, through existing `WyrdClient` exchange/read behavior; (b) bind that authenticated key's tenant to the requested existing active tenant. A missing requested tenant with a retained unrelated key must refuse before create.

Existing owner comparison: `main.rs::setup` already performs the relevant side-effect-free `list_tenants(&OperatorPool)` read before root initialization/create; `WyrdPostgres::resolve_tenant_slug` (`crates/wyrd/wyrd-sql/src/postgres.rs:193-207`) is the shared active-tenant resolver. The directory belongs to the server; do not add SQL to the client tier or duplicate its queries. `Platform::list_tenants` is an existing callable API but requires additional platform authority, so requiring it for every restart would regress approved behavior. Existing `setup` has no read-only mode: invoking it before saves without further constraints can itself issue the unsavable key in case 1. Merely changing call order is therefore insufficient.

**Correction boundary uncertainty for validation:** the exact-key bypass has a concrete existing-interface correction (`ClientConfig` above). I did not establish an already-callable, side-effect-free, requested-tenant identity read available to dev without platform credentials. The existing directory lookup identifies the correct owner, but does not authorize inventing a new public CLI/HTTP operation, private phase protocol or platform-credential prerequisite. Independent validation must select a bounded correction within the approved existing interfaces, or explicitly identify any authority decision needed; this follow-up does not prescribe new setup plumbing as an approved solution. The reachable bad state and retained finding do not depend on that implementation choice.

Preserve ordinary fresh-client/SavedLogins precedence after setup, protected no-overwrite checks, key-before-endpoint partial-save resume, same-deployment listener/DSN overrides, setup's no-key first-use path, and explicit missing/revoked-key recovery guidance. A refusal alone must not rotate or discard a key. Do not accept a saved slug, origin, human login or unverified token payload as proof of the exact retained key/requested tenant binding. No new endpoint, auth semantics, store, token decoder or recovery mechanism is recommended.

## Focused closure proof

Extend existing P9, retaining its normal restart, partial-save, lost-key and listener-settlement assertions:

1. Prepare same-origin B default key/endpoints with no dev section; request absent A with genuine platform authority. Require byte-identical prior config/credential files, absent A/no A admin issuance, settled child, and B still usable. Also verify refusal preserves selection without platform authority.
2. Keep A default key/dev selection, save a valid B human login at the same origin and slug, select B's database. Require refusal before selection mutation/issuance despite the valid login. Include a missing/revoked default-key case so a human login cannot substitute for key proof; retain existing explicit recovery behavior.
3. Verify a valid exact key/requested tenant with the same deployment's changed listeners still resumes; after setup, ordinary fresh-client precedence continues to choose a saved login normally.

Use the existing exact managed-Postgres P9 selector and nearest focused shared client/setup checks if those owners change. No new harness, broad suite, real identity-provider dependency or paid call is needed. Existing saved-login fixture/token helpers can supply the competing supported state.

## Limits

Source audit only; no Cargo/build/test/check execution or runtime reproduction. No source, test, task/spec, staging or commit changes. This report resolves the source reachability conflict and forwards the retained proposal, existing exact-key correction and explicit tenant-lookup wiring uncertainty for independent Ponytail validation; it does not claim whole-task acceptance or four-target execution. Other findings and sibling repositories were deliberately not checked.
