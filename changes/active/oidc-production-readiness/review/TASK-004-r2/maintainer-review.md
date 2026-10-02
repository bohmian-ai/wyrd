# Maintainer Review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Binding authority: `changes/active/oidc-production-readiness/spec.md`
  revisions 8 and 9, plus
  `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md` and its addenda.
- Explicit exclusion: revision 10 `REQ-021` is owned by TASK-008; this review
  does not assess or raise the OAuth endpoint body/error wire format.
- Maintainer authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
  `architecture/wyrd-security-posture.md`, and
  `architecture/references/languages/maintainer-style.md`.

The candidate remained checked out at the stated commit throughout this pass.
The repository has no `.codegraph/` directory, so navigation used the
cumulative Git diff, `rg`, and direct source/caller inspection.

## Changed-surface coverage

| Surface | Owner, caller, declaration, and test coverage | Result |
|---|---|---|
| Device-grant wire contract | Read `wyrd-spec::auth::device`, the device variant of `TokenRequest`, module exports, the stable error catalog, generated schemas, server route registration, and the OpenAPI contract assertions. The contract uses the RFC 8628 names consistently and keeps tenant route context typed. Revision 10's later form/error-envelope requirement is excluded. | PASS |
| Durable device authorization | Read the migration and all queries in `device_authorizations.rs`, their `TenantConn` callers, login-state binding, callback completion use, migration coverage, and `CliLogins` Postgres tests. The table, expiry/poll ownership, and query names make the lifecycle followable without a second durable auth owner. | PASS |
| Server device workflow | Read `CliLogins::{authorize,approve,deny,redeem,end}`, its helpers and tests, the server adapter handlers, auth route dispatch, login callback consumer, and request-id/audit calls. `CliLogins` is a cohesive dependency-owning owner for this capability; the server adapter stays limited to HTTP extraction and presentation. | PASS |
| CLI workflow | Read `LoginFlow::{new,run,poll,save}`, `login`, `logout`, `status`, command registration, browser launch, unit parsing coverage, and the real CLI journey. The device flow is discoverable on one stateful owner, blocking credential-file work is isolated from Tokio, and terminal output remains token-free. | PASS |
| Shared credential file | Read `CredentialsFile` in full, its saved-login and API-key-cache callers, the retained `[default].api_key` reader, file-safety tests, and cache tests. The new owner centralizes locked writes, atomic replacement, mode/owner checks, and preservation of unrelated TOML content. No second file or custom timeout/state machine remains. | PASS |
| Saved-login selection and renewal | Read `SavedLogin`, `SavedLogins`, `SavedLoginSource`, all file operations and unit tests, `ClientConfig::resolve_credential`, `CredentialChain`, `AuthMiddleware` renewable-source paths, and the concurrent real-server journey. The current shape matches revision 8: newest-login default, tenant-key selection, one blocking lock, ordinary refresh/store, local-first logout, and no generation, pending marker, tombstone, lock deadline, or format version. | PASS |
| API-key access-token cache | Read `AuthMiddleware::{build,exchange_and_store,persist}`, `CredentialsFile::{cached_api_key_token,cache_api_key_token}`, config parsing, and tests proving same-key reuse, content preservation, and no renewable-token persistence. The existing credential file is reused rather than introducing another store. | PASS |
| Rust client consumers | Read changed `Cards`, `Bifrost`, global config, error, and client-construction paths and their Rust SDK journey call sites. The optional tenant key is carried through the shared `ClientConfig`; no language-specific auth implementation was introduced. | PASS |
| Python projection | Compared every changed PyO3 constructor with its source stub and assembled public stub, including client, Cards, Bifrost, state, gateway, operators, and verification, and read the Python journey/typecheck assertions. Round 1's missing `tenant` declarations are closed: runtime signatures, public declarations, and docs now agree. One test-harness method still describes the removed mechanism (`MAINT-TASK-004-R2-1`). | FAIL |
| TypeScript projection | Compared N-API entry points, public option objects, generated declarations, and the TypeScript journey. Runtime and declarations consistently expose `tenant` as the saved-login tenant route key. The testing binding and its generated declaration still describe the removed mechanism (`MAINT-TASK-004-R2-1`). | FAIL |
| Shared identity test harness and journeys | Read `HumanSso`'s device authorization, browser approval, provider callback, token redemption, save/expire/revoke helpers and the Rust/Python/TypeScript consumers. The shared Rust harness accurately implements and documents the device grant; two foreign-runtime wrappers retained stale handoff wording. | FAIL |
| Docs, manifests, and verification wiring | Read the changed authentication/client-configuration docs, dependency and feature changes, identity-target routing, production-wheel boundary, API error generation, and mechanically adjusted call sites. No additional maintainer defect was found. | PASS |

## Material finding

### MAINT-TASK-004-R2-1 — Python and TypeScript testing APIs still claim to use the deleted CLI handoff

- **Changed locations:**
  `sdks/wyrd-sdk-python/src/testing.rs:800` documents
  `save_human_login` as using “the CLI handoff”; the same stale contract is
  present on the TypeScript N-API source at
  `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:587` and is projected into the
  shipped declaration at `sdks/wyrd-sdk-ts/testing/index.d.ts:215`.
  Their actual shared owner, `crates/wyrd/wyrd-testing/src/human_login.rs:151`
  and `:219`, now authorizes, approves, and redeems the RFC 8628 device-code
  grant. A repository-wide production/test-surface search finds no remaining
  CLI handoff implementation.
- **Governing rule:** revision 8 deletes the custom handoff and requires the
  conventional RFC 8628 device grant. `AGENTS.md` section 16 requires
  materially changed Rust documentation to explain the real workflow, and the
  Maintainer Style guide requires foreign-language documentation and generated
  declarations to match runtime behavior. The standing human direction also
  rejects a mechanism that neither the approved standard nor comparable tools
  retain.
- **Concrete maintenance cost:** the public test helpers tell SDK maintainers
  and declaration consumers that the journeys exercise a server handoff which
  no longer exists. That obscures what auth boundary the first-class SDK
  journeys actually prove and can send a future maintainer looking for, or
  recreating, the deleted custom protocol. The TypeScript mismatch is shipped
  in the generated declaration, not confined to an internal comment.
- **Smallest testable correction:** replace “CLI handoff” with “CLI device
  login” in the two Rust source docs and regenerate the existing TypeScript
  declaration. Do not add a compatibility name, second flow, check, file, or
  test; the current shared `HumanSso` implementation and existing generation
  lane already cover the behavior.

## Prior-finding closure

| Prior finding | Current evidence | Result |
|---|---|---|
| Round-1 `MAINT-TASK-004-1`: Python public declarations omitted `tenant` | PyO3 constructors and source/public stubs now expose `tenant: str | None` across WyrdClient, Cards, Bifrost, state, gateway, operators, and verification; `test_every_public_constructor_accepts_and_forwards_tenant` and the recorded `py:typecheck` lane cover the projection. | CLOSED |

The other round-1 findings were not maintainer findings, but their remediation
changed maintainer-owned surfaces. This pass confirmed that the one-file
credential owner, tenant-key-only selector, TLS validation reuse, transactional
logout audit call site, conventional renewal, and device-grant replacement are
locally coherent and do not retain the superseded custom machinery.

## Verification assessment

- Independent static check: `git diff --check
  06f134dc14164c040c0e5014d21de29c240f4116..362878494ed80ca5c5533a4364f744bf92dd1e06`
  passed.
- The task records final exit `0` for format/lints, codegen and boundary
  checks, docs, shared/client/CLI/server tests, Python and TypeScript unit,
  typecheck and integration lanes, every focused identity target, and the
  unfiltered identity journey after Docker was restored.
- Those green lanes cannot establish documentation accuracy: generation
  faithfully carries the stale TypeScript source comment into
  `testing/index.d.ts`, and the Python wrapper's runtime behavior still works.
  Source comparison is therefore the direct proof for
  `MAINT-TASK-004-R2-1`.
- No finding or verification requirement in this report concerns revision 10
  `REQ-021`.

## Calibration notes

- No split of `CliLogins`, `SavedLogins`, or `CredentialsFile` is requested.
  Each has one cohesive stateful responsibility, and another module would add
  navigation without creating a new owner.
- The device verification page and its small HTML helper do not justify a UI
  framework or template abstraction.
- No preference-only suggestions are retained.

## Overall result

**FAIL**

The round-1 Python declaration gap is closed and the implementation is
maintainable across its production Rust, SQL, server, CLI, and first-class SDK
owners. The remaining bounded defect is documentation drift on two changed
test-runtime wrappers and the generated TypeScript declaration; it should be
corrected at those existing source docs without adding any mechanism.
