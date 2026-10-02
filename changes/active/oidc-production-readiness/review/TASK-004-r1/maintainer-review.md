# Maintainer Review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Binding human direction:
  - `review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `review/TASK-003-r2/human-direction-connection-test.md`
  - `review/TASK-003-r5/human-direction-FIND-TASK-003-18.md`
- Maintainer authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/references/languages/maintainer-style.md`

The candidate remained checked out at the stated commit while this review was
performed.

## Changed-surface coverage

| Surface | Owner and caller/test coverage | Result |
|---|---|---|
| CLI handoff wire contract and errors | Read `wyrd-spec::auth::cli_handoff`, its module export and catalog error, the server handlers, OpenAPI registration, and the OpenAPI contract test. The request/response names, typed fields, error projection, and generated TypeScript error projection follow the existing contract shape. | PASS |
| Durable handoff and refresh-chain operations | Read the migration, `wyrd_sql::queries::auth::cli_handoffs`, the existing refresh-token query owner it reuses, and `CliLogins::{begin,claim,cancel,end}` with their server callers and Postgres tests. SQL remains behind `TenantConn`; the workflow is discoverable on the cohesive `CliLogins` owner; the logout path reuses the existing refresh-chain mechanism and documents the per-login scope required by the binding human direction. | PASS |
| Shared saved-login authority | Read `saved_login.rs` in full, `ClientConfig::resolve_credential`, `CredentialChain` changes, `TokenExchange` calls, the Bifrost/Card client construction doors, their callers, unit tests, and the concurrent real-server journey. `SavedLogins` owns the filesystem/locking lifecycle, `SavedLoginSource` owns renewal through the existing token-source capability, and callers do not duplicate persistence or renewal. Names and state transitions are locally traceable despite the necessarily substantial module. | PASS |
| CLI user workflow | Read `LoginFlow`, `login`, `logout`, `status`, browser launch, the command registration, CLI unit coverage, and `cli_login_journey`. The stateful login workflow has a concrete owner, blocking filesystem work is visibly isolated, and output helpers make the no-token contract apparent. | PASS |
| First-class Rust and TypeScript projections | Read the constructor changes in the shared Rust handles, Rust SDK journeys, N-API entry points, public TypeScript option objects, generated declarations, and the TypeScript journey. `tenant` is named consistently and delegates to the shared client; the TypeScript declarations match the runtime entry points. | PASS |
| First-class Python projection | Read every materially changed PyO3 constructor, the public/source stubs, generated package stubs, Python journey, testing wrapper, and typecheck lane selection. Runtime constructors delegate to the shared client, but their public declarations do not match them. | FAIL (`MAINT-TASK-004-1`) |
| Shared identity test harnesses and journeys | Read `HumanSso`, Rust/Python/TypeScript harness projections, the CLI/Rust/client/Python/TypeScript journeys, and the identity-lane target routing. The helpers have a coherent test-only owner and avoid language-specific durable auth implementations. | PASS |
| Production-wheel boundary check | Compared `feac127a0` with its parent and inspected the Python feature graph. The old command imported from the `py:setup` testing-enabled development environment, so it did not inspect the production wheel. The new command builds the default wheel (whose maturin feature set contains `python` but not `testing`), installs that exact wheel in an isolated no-project environment, proves `import wyrd` succeeds as a positive control, then refuses `import wyrd.testing`. Isolation prevents the preceding testing-enabled development install from satisfying either import. This materially strengthens the check and does not broaden an allowlist, skip a test, or remove an assertion. | PASS; strengthened, not weakened |
| Ancillary manifests, docs generator, and mechanically updated tests | Read the dependency/feature changes, affected test call sites, API-doc allowlist change, and task evidence update. No separate maintainer defect was found. | PASS |

## Material finding

### MAINT-TASK-004-1 — Python public declarations omit the new `tenant` option

- **Changed locations:** the runtime signatures added `tenant` at
  `sdks/wyrd-sdk-python/src/client.rs:48`,
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:185`,
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:307`,
  `sdks/wyrd-sdk-python/src/state/mod.rs:159`,
  `sdks/wyrd-sdk-python/src/state/mod.rs:1596`,
  `sdks/wyrd-sdk-python/src/gateway.rs:131`,
  `sdks/wyrd-sdk-python/src/operators.rs:68`, and
  `sdks/wyrd-sdk-python/src/verification.rs:40`; the corresponding source
  declarations still omit it at
  `python/wyrd/stubs/client.pyi:15`,
  `python/wyrd/stubs/bifrost.pyi:164`,
  `python/wyrd/stubs/bifrost.pyi:230`,
  `python/wyrd/stubs/state.pyi:151`,
  `python/wyrd/stubs/cards.pyi:245`,
  `python/wyrd/stubs/gateway.pyi:244`,
  `python/wyrd/stubs/operators.pyi:152`, and
  `python/wyrd/stubs/verification.pyi:14`. The assembled public package stubs
  reproduce the same omissions.
- **Governing rule:** `AGENTS.md` sections 8 and 12 and Maintainer Style
  “Python and TypeScript: document the typed contract” require public
  parameters, generated declarations, and runtime behavior to agree. TASK-004
  explicitly requires Python constructors to expose the tenant selector and
  generated/public types to be updated from source.
- **Concrete maintenance cost:** valid calls such as the candidate's own
  `Cards(server_url=url, tenant=FIXTURE_TENANT)` journey are accepted at
  runtime but rejected by an editor or type checker using the shipped `.pyi`.
  The generated docs also hide the selector and several docstrings still
  describe the older environment-to-credentials-file chain. A maintainer can
  therefore change or consume the public Python API only by reading the Rust
  binding rather than its declared contract. `codegen:check` remains green
  because it assembles these stale source declarations; it cannot infer the
  PyO3 signature. `py:typecheck` does not include the new identity journey, so
  it also does not exercise the mismatch.
- **Smallest testable correction:** update the existing Python source-stub
  declarations and their constructor documentation for the already-added
  `tenant: str | None` parameter, then regenerate the package stubs. Add the
  smallest existing typing-surface assertion that constructs at least the
  shared `WyrdClient` and `Cards` entry points with `tenant`; the other
  declarations are mechanically checked by regeneration. Do not add a second
  stub generator or a language-specific credential path.

## Verification assessment

The task records successful focused journeys and broader lanes, including
`codegen:check`, `py:typecheck`, `ts:napi:check`, and
`check:py-wheel-no-testing`. Source inspection explains why the first two do
not catch `MAINT-TASK-004-1`: generation begins from the stale hand-authored
source stubs, and the typecheck input list excludes the new integration
journey. I started an independent `mise run check:py-wheel-no-testing`; its
setup reached the wheel build but was waiting on the shared Cargo build lock,
so I interrupted it rather than compete with concurrent review verification.
The exact `feac127a0` command and feature graph nevertheless establish that
the check is strictly stronger as described above; the recorded completed run
remains the execution evidence.

## Calibration notes

No preference-only concerns are retained. In particular, splitting the
filesystem, lifecycle, and locking portions of `saved_login.rs` would add
navigation without identifying a separate owner, and the `CliLogins` owner is
cohesive across begin, claim, cancel, and end for the one CLI-login capability.

## Overall result

**FAIL**

The implementation is maintainable across its Rust, server, CLI, SQL, and
TypeScript owners, and commit `feac127a0` strengthens the production-wheel
boundary. The shipped Python typed contract is materially incomplete until
`MAINT-TASK-004-1` is corrected.
