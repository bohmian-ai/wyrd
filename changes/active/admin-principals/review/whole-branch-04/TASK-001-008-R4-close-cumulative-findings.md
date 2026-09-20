# TASK-001-008-R4 — Close cumulative admin-principals findings

## Route

Implement this packet with `$wyrd-implement`. Make the smallest cohesive
correction that closes every finding below. A later `$wyrd-task-review` must
review the complete cumulative candidate, not only this remediation diff.

## Authority and reviewed candidate

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  10, status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-04/findings-validation.md`.

`FIND-TASK-001-10` remains waived in full. Do not rewrite history, remove AI
trailers, or alter author/committer identity. The bundled
verified-change-contract content is approved cumulative scope; preserve its
substance.

Post-review owner correction: no version containing the pre-credential audit
schema has shipped. `FIND-admin-principals-R3-5` is withdrawn; there is no
historical deployment to migrate or compatibility state to prove. The
correction for `FIND-admin-principals-R4-7` is therefore deletion of the
pre-release compatibility machinery, not completion of it.

## Resume handoff

Start from the existing worktree. Do not reset, stash, overwrite, or clean it.
`README.md`, the revision-10 spec, and the whole-branch-03 verdict were already
dirty owner state before this review. Preserve them. Preserve this complete
whole-branch-04 review directory as the review record. Product source is clean
at the reviewed candidate; no half-implemented Rust edit needs salvaging.

Close the security and audit roots first because later journey proofs depend on
their final behavior. Then close durability/schema roots, public contract and
client projection, and finally documentation/rustdoc/evidence hygiene. Run one
focused proof at each checkpoint and the full required proof only on the final
tree. A green aggregate is not a substitute for a missing named scenario.

## Outcome

Finish the approved two-plane administrative capability without adding another
identity, audit, transport, OpenAPI, schema-migration, or test framework. Reuse
the existing verifier, refresh-family, authorization-epoch, canonical audit,
`TenantConn`, `OperatorPool`, catalog, `utoipa`, `wyrd-client`, CLI credential,
and journey owners. Delete duplicate knowledge where the candidate added it.

The R3 fixes remain valid and must be preserved: machine grants return no
refresh token, human refresh rotates and replay revokes its family, one
`vala.audit_staging`/`AuditPublisher` path exists, `/openapi.json` remains the
one OpenAPI endpoint, and duplicate YAML/snapshot/generator/release-digest
machinery stays deleted. The unshipped legacy-audit compatibility code is not a
required R3 fix and must be removed as described below.

## Validated diagnosis and decision-complete correction

### `FIND-admin-principals-R4-1` — revocation uncertainty fails open

Both cache-hit and cache-miss branches of
`crates/shared/wyrd-auth-verify/src/lib.rs:475-547` return a verified token when
the production `SqlRevocationCheck` cannot read tenant admission or the
authorization epoch. A revoked or suspended principal can therefore keep using
an otherwise-valid bearer and its cached permissions during a database or pool
outage, contrary to `INV-011` and `INV-013`.

Change the existing verifier branches to invalidate any cached positive result
and return the existing `AuthError::VerifyUnavailable`. Reuse the current
stable retryable 503 mapping. Delete the fail-open behavior and its affirmative
tests; add no fallback cache, stale-allow mode, or second admission service.
Prove cache-hit and cache-miss failure plus one protected served route, while
preserving known-current and known-revoked outcomes.

### `FIND-admin-principals-R4-2` — User revocation leaves refresh authority live

The served principal-revoke path reaches `wyrd-auth/src/revoke.rs:35-40`, whose
User arm advances only `auth_users.tokens_not_before`. An active User refresh
row survives. `RefreshTokens::execute` can consume it, mint a successor newer
than the epoch, and restore access.

In the existing User revocation branch and the same `TenantConn`, reuse the
current refresh-family revocation owner together with the epoch update. Both
must commit or roll back together. Do not add a revocation service, status
cache, or alternate token blacklist. Drive the served human flow through
login, access use, User revocation, refused old access, refused refresh, and
absence of a successor refresh row.

### `FIND-admin-principals-R4-3` — changed OIDC roles do not revoke old claims

Tenant OIDC callback replaces `auth_user_roles` and issues a successor in one
transaction, but role removal does not advance the User epoch. Old access
tokens retain role names in signed claims, and permission resolution uses those
names rather than rereading current bindings.

Make the existing role-replacement owner report whether the persisted set
actually changed. On a real change, advance the same User authorization epoch
in that transaction and issue the successor at an `iat` admitted by the new
epoch. An unchanged login must not invalidate sessions. Reuse the existing
epoch write and token claims; add no role cache or new authority source. Prove
old-token refusal, reduced successor authority, and unchanged-login stability
through the served OIDC journey.

### `FIND-admin-principals-R4-4` — replay audit omits the consumed refresh row

The replay branch at `wyrd-auth/src/refresh.rs:162-189` already holds
`stale.id`, revokes the family, and appends the canonical
`auth.refresh.revoke_family` event, but leaves `credential_id` empty. Successful
rotation already uses this field.

Attach `stale.id` to that existing event through the existing audit builder.
Do not add a second event, detail payload, or identifier. Extend the committed
replay proof to assert the exact UUID while preserving separate-transaction and
served-request proof that the successor family is unusable.

### `FIND-admin-principals-R4-5` — access-token/session grants are unaudited on both planes

Tenant API-key exchange signs and returns for Card-free principals without an
audit row; Card-bound exchange writes only `auth.card_scope.mint` and does not
attribute the API-key id. Platform credential exchange and platform federated
issuance also sign/return outside a canonical audit transaction. These are
reachable `/auth/token`, `/auth/platform/token`, and platform OIDC callback
paths. Audit unavailability cannot fail the grant closed.

For tenant exchange, append exactly one existing `auth.token.exchange` event
on the current `TenantConn` for both Card-free and Card-bound grants, naming
the API-key row id. Keep the distinct Card-scope event because it records a
different decision. For platform credential and federated issuance, reuse
`OperatorPool::begin_platform_audited` and the existing transaction-scoped
platform queries so authentication/touch or identity pin, canonical append,
and commit are one grant boundary. Credential exchange names its credential;
federated issuance names the resolved principal and no credential. Add no
platform audit table, alternate publisher, or shared abstraction that forces
the two planes into one service. Prove one committed attributed row per grant
and that injected append failure returns no token and commits no grant-side
effect.

### `FIND-admin-principals-R4-6` — initialization may persist an undisclosed root credential

`initialize_platform_root` commits before `main::init` performs several
fallible stdout writes, two of them before the secret. Closed stdout can leave
the unique root and verifier durable, the plaintext undisclosed, and retry
refused as already initialized.

Use `std::io::Write` to make the one credential disclosure an explicit
fallible step in the existing initialization workflow while its transaction is
still open. Flush successfully, then commit; writer failure drops the
transaction and leaves initialization retryable. Do not add an output service,
queue, temp secret file, or recovery exception. This deliberately selects the
only attainable cross-system ordering: a later database commit failure may
leave the operator with an unusable disclosed secret, but the deployment stays
uninitialized and retryable. Prove writer failure leaves zero root, grant, and
credential rows and that the next initialization succeeds once.

### `FIND-admin-principals-R4-7` — unshipped audit compatibility weakened generic schema validation

The fallback at
`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1490-1550`
compares all top-level fields by stable id after positional comparison fails.
Every existing physical table reaches it, so reordered log, trace, and other
canonical schemas now pass. The legacy fingerprint exception, additive Iceberg
column mutation, half-applied reconciliation, special old field-id allocation,
conditional legacy hash preimage, and their upgrade fixtures exist only to
support a schema that has never shipped.

Delete that compatibility path completely:

- restore strict positional matching as the only generic schema comparison and
  remove the stable-id fallback;
- remove the recognized legacy fingerprint, additive `audit_log` Iceberg
  evolution, half-applied reconciliation, special legacy field-id mapping, and
  their upgrade-only tests and helpers;
- make the current `audit_log` schema use the ordinary canonical schema path;
- fold nullable `credential_id` into the original `vala.audit_staging` creation
  migration and delete the branch-only additive credential migration; and
- encode the optional credential field in every current hash preimage, including
  `None`, and remove the legacy-preimage special case and proof.

Do not replace any of this with a migration framework or another compatibility
branch. Prove a reordered canonical table returns `MetadataMismatch`, a fresh
current audit table registers normally, credential-free and credential-bearing
current rows each reproduce their stored hash, and normal audit publication
still succeeds.

### `FIND-admin-principals-R4-8` — provisioning failure proof covers only one stage

The current real journey injects a trigger only on
`wyrd.auth_service_accounts`. It does not force failure at directory
claim/audit commit, role seed, grant, credential, tenant commit, or active
promotion, although `AC-007` explicitly requires each durable stage.

Parameterize the existing real Postgres journey over the actual durable
boundaries. Reuse its SQL trigger/failpoint pattern; do not add production
failure injection or another harness. Every case must leave the directory
failed/non-admitted and no usable credential, then retry to the original tenant
id with exactly one tenant-admin principal, grant, and live credential and no
orphan.

### `FIND-admin-principals-13` — the runtime OpenAPI contract is incomplete and self-certified

`WyrdApiDoc` omits mounted storage upload/local, evaluation, authorization, and
OTLP operations. Administrative annotations omit reachable principal-not-found,
audit-unavailable, and verifier-unavailable branches. The tests scan only two
source files, validate only codes already listed, and inspect router source
instead of requesting it. `ANONYMOUS_PATHS` is a second hand-written route
list. Green tests therefore certify an incomplete restatement.

Keep `utoipa`, `WyrdApiDoc`, and `/openapi.json`. Add the missing handler
annotations and register every mounted public operation. Declare anonymous
security on the owning operations and delete `ANONYMOUS_PATHS`; trace each
handler and declare every reachable stable error/problem response. Derive any
served-versus-documented closure proof from the same owning declarations rather
than another path/error list. Request the assembled server's `/openapi.json`,
validate its media/document shape, and prove `/openapi.yaml` is unrouted.
Inject representative audit and store failures and assert that the runtime
codes appear on the owning operations. Do not restore a checked-in snapshot,
YAML route, generator, drift task, release digest, or independent catalog.

### `FIND-admin-principals-R4-9` — first-party MCP owns an incomplete second HTTP path

`wyrd-mcp/src/client.rs` stores a caller-supplied raw `reqwest::Client`, writes
Wyrd headers, and explicitly omits reactive 401 renewal. All five `rmcp`
operations use it, and the real MCP journey constructs the raw client directly.

Keep `rmcp` framing in `wyrd-mcp`, but obtain HTTP/auth behavior from the
existing configured `WyrdClient`/`HttpTransport` capability. The adapter must
use the shared bounded 401 classification, durable-credential
`force_refresh`, and one replay; a second refusal is terminal. Do not add an
MCP facade, third transport, or another auth middleware. Prove exactly one
exchange and replay after the first 401 and no retry after the second, and
leave no independent Wyrd header/raw-client construction in the MCP crate.

### `FIND-admin-principals-R4-10` — architecture and public docs describe the removed model

`architecture/wyrd-design.md` still says machine API keys are exchanged once
and refreshed. Public authentication, identity, authorization, self-hosting,
and Bifrost pages still describe three principal kinds, mandatory Card binding,
or API-key refresh tokens. The shipped enum has five kinds and machine clients
re-exchange durable credentials; refresh is human-only.

Update the existing active design and every affected public page found by a
focused identity/refresh search. Describe the five kinds, platform/tenant
planes, optional Service Card binding and required Agent Card binding, machine
durable-credential re-exchange, and human OIDC refresh rotation. Regenerate
the existing LLM indexes. Add no compatibility vocabulary or new guide.

### `FIND-admin-principals-R4-11` — the operator journey bypasses the returned credential

After tenant creation, `operator_journey.rs` exchanges the printed
`admin_credential` through an in-process test helper and gives the CLI a bearer.
The operator page likewise jumps to an unexplained access token. The shipped
CLI already accepts one explicit credential input, classifies API key versus
bearer, and routes API-key exchange through `wyrd-client`.

Pass the once-returned tenant credential directly through that existing CLI
credential input for issuer configuration and restricted-principal creation.
Delete the fixture exchange from this journey and document the same handoff.
Do not add a `WYRD_API_KEY`-specific admin option or another credential parser.

### `FIND-admin-principals-R4-12` — cumulative touched Rust items lack mandatory Rustdoc

Confirmed examples in `wyrd-auth-issue`, `wyrd-client`, `wyrd-auth`, and
`wyrd-spec` include new/materially changed private and test functions with no
Rustdoc and undocumented panic paths. The recorded strict `wyrd-sql` public-doc
command cannot inspect them.

Perform a diff-based audit over the complete base-to-candidate-plus-remediation
range. Document every new or materially modified Rust item and its real
workflow role, invariants, side effects, errors, panics, cancellation, or
partial progress where applicable. Do not add lint suppression, a utility
trait, or a permanent repository scanner for this one change.

### `FIND-admin-principals-R4-13` — the cumulative diff fails whitespace hygiene

`git diff --check` reports one extra EOF blank line in three admin review files
and four approved verified-change-contract task files, including the already
dirty whole-branch-03 verdict.

Delete only the seven extra EOF blank lines listed in
`findings-validation.md`. Preserve every word and all existing owner edits,
especially in the dirty prior verdict. Do not reformat those documents.

### `FIND-admin-principals-R3-6` — exact named command evidence is incomplete

The R3 evidence table says several tests passed inside a larger run or gives a
bare name. That does not prove a nonzero exact selection through the required
environment.

Run every still-bare existing proof with package, target, exact
`test(=...)` selector, repository wrapper/environment, and result count. Append
the literal commands and results to this packet's implementation evidence.
This is evidence repair only; do not change tests merely to make selection
easier.

## Constraints and non-goals

- Preserve the two administrative planes, closed principal kinds, Card-free
  administrative principals, existing permission vocabulary, canonical
  `vala.audit_staging` append, and sole `AuditPublisher`.
- Preserve machine durable-credential re-exchange and human-only rotating
  refresh. Eventual UI/BFF refresh automation remains out of scope.
- Preserve independently durable denials and same-plane atomic
  allowance/effect transactions.
- Preserve `TenantConn`/`OperatorPool` boundaries, tenant isolation,
  fixed-cost invalid-key verification, last-admin protection, idempotent
  provisioning, and root recovery.
- Preserve `utoipa`, `/openapi.json`, OpenAPI user documentation, JSON Schemas,
  Python `.pyi`, TypeScript `.d.ts`, MCP, and `check:client-tier`. Do not restore
  any deleted OpenAPI snapshot/YAML/generator/drift/release-digest machinery.
- Do not retain or replace pre-release audit compatibility. Keep strict current
  fingerprint, field-order, hash-chain, replay, and dedup checks.
- Do not add a new database pool wrapper, audit table/sink, route catalog,
  transport, token service, role engine, cache, migration framework, failure-
  injection framework, or permanent documentation scanner.
- Do not implement UI state, browser token storage, platform-human CLI
  expansion, Card-name schema changes, the excluded base-red auth-cache test,
  the stale testing comment, or rustdoc-lane policy widening.
- Do not rewrite history or touch Git identity/provenance.

## Acceptance criteria

| Finding | Acceptance |
|---|---|
| `FIND-admin-principals-R4-1` | Revocation-store failure refuses cache-hit and cache-miss tokens with the stable 503; no stale permission survives. |
| `FIND-admin-principals-R4-2` | User revocation atomically retires its active refresh authority; neither old access nor refresh restores a session. |
| `FIND-admin-principals-R4-3` | A changed OIDC role set advances the User epoch and invalidates the old token; unchanged login does not. |
| `FIND-admin-principals-R4-4` | The committed replay-containment row names the exact consumed refresh UUID and the successor remains unusable. |
| `FIND-admin-principals-R4-5` | Tenant Card-free/Card-bound, platform credential, and platform federated grants each commit one canonical exchange audit; append failure returns no token and no grant-side effect. |
| `FIND-admin-principals-R4-6` | Failed credential output leaves initialization absent and retryable; successful output/commit exposes exactly one usable root credential. |
| `FIND-admin-principals-R4-7` | All pre-release audit compatibility code and the branch-only additive migration are absent; strict current schemas and hashes pass, while reordered schemas fail. |
| `FIND-admin-principals-R4-8` | Every durable provisioning-stage failure leaves no admitted tenant/usable credential and retry converges to one original tenant/admin/grant/credential. |
| `FIND-admin-principals-13` | `/openapi.json` is the exact complete `utoipa` contract for all mounted public operations and reachable errors; YAML and parallel catalogs remain absent. |
| `FIND-admin-principals-R4-9` | MCP uses shared Wyrd HTTP/auth behavior, performs one credential re-exchange/replay after a 401, and never retries twice. |
| `FIND-admin-principals-R4-10` | Active architecture, public docs, and generated LLM indexes describe the five-kind, two-plane, split-renewal model without stale machine-refresh prose. |
| `FIND-admin-principals-R4-11` | The real operator journey uses the printed tenant credential directly through the shipped CLI for issuer and restricted-principal operations. |
| `FIND-admin-principals-R4-12` | Every cumulative new/materially modified Rust item has accurate required Rustdoc and applicable contracts. |
| `FIND-admin-principals-R4-13` | Exact base-to-final-candidate `git diff --check` is silent and successful. |
| `FIND-admin-principals-R3-6` | Every named Rust proof has a literal reproducible exact command and nonzero passing result in the evidence table. |

## Focused and broader proof

For every new or changed regression test, record its exact
`mise exec -- cargo nextest run --locked` invocation with package, target,
exact expression, result count, and the repository Postgres wrapper when
needed. In particular, repair the existing R3 evidence with literal exact
commands for:

- `exchange_api_key::pg_tests::delegation_issues_no_refresh_token`,
  `refresh::pg_tests::rotation_carries_the_roles_login_persisted`, and
  `refresh::pg_tests::a_machine_refresh_row_cannot_rotate` in `wyrd-auth`;
- `refresh::pg_tests::f09_replay_containment_commits_and_kills_the_successor`
  and `refresh::pg_tests::reuse_detection_revokes_family` in `wyrd-auth`;
- `human_oidc_login_journey` in the `identity_e2e` target;
- `a_tenant_administrator_renews_by_re_exchanging_its_credential` and
  `a_failed_provisioning_can_be_retried_with_the_same_slug` in the
  `platform_admin_e2e` target.

Run the new focused proofs for verifier fail-closed behavior, User
revocation/refresh, role-change epoch, replay attribution, every tenant and
platform issuance audit branch, failing initialization writer, reordered
schema rejection, fresh current audit registration/hash/publication, every
provisioning failure stage, runtime OpenAPI routing/errors, MCP single retry,
and direct CLI credential use. Confirm exact names with `cargo nextest list`;
never use a selector that can pass after selecting zero tests.

At minimum, run on the final tree:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run test:shared`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:sql`
- `mise run test:bifrost:integration:redux`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:integration:sql`
- `mise run codegen:check`
- `mise run check:examples`
- `mise run docs:check`
- strict `wyrd-sql` rustdoc with warnings denied, plus the diff-based all-item
  Rustdoc audit required by `R4-12`
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`; approved `VER-003` excludes it. Append an
implementation evidence table mapping every acceptance row above to the commit,
exact focused proof, broader lanes, and result. A status narrative or aggregate
count is not proof of a specifically named test.

## Implementation state (in progress — handoff)

Base `c5c20754a167e8f4d74a555a720bd51df6179a6f`, branch
`claude/admin-principals-spec-qfsmjc`, worktree
`/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`.

All fifteen findings are implemented and committed. What remains is the tail of
the required verification list and the final evidence table below.

### Preserved uncommitted owner state — do not commit

`README.md`, `changes/active/admin-principals/spec.md` (revision 10), and the
remaining owner edits in
`changes/active/admin-principals/review/whole-branch-03/verdict.md` are the
branch owner's dirty working state and must stay uncommitted. The
whole-branch-04 review directory is untracked and stays untracked.

`a368d81e0` committed **only** the single trailing-blank-line deletion at the
end of that verdict (staged as an isolated hunk with `git apply --cached`),
because `R4-13` requires the base-to-candidate `git diff --check` to be silent
and that one line lived inside the owner's dirty copy. Every other owner hunk in
the file is still uncommitted. Preserve that split.

### Commit map

| Finding | Commit(s) |
|---|---|
| `FIND-admin-principals-R4-1` | `231612b3f` |
| `FIND-admin-principals-R4-2` | `fd2b12d77`, served journey `22a205985` |
| `FIND-admin-principals-R4-3` | `fd2b12d77`, served journey `22a205985` |
| `FIND-admin-principals-R4-4` | `fd2b12d77` |
| `FIND-admin-principals-R4-5` | `fb776e9f9`, `36517f5ba` |
| `FIND-admin-principals-R4-6` | `952f26217` |
| `FIND-admin-principals-R4-7` | `47e216866`, `6448a79f6` |
| `FIND-admin-principals-R4-8` | `c6a262fd2` |
| `FIND-admin-principals-13` | `c31113564` |
| `FIND-admin-principals-R4-9` | `945c15967` |
| `FIND-admin-principals-R4-10` | `b460710ef` |
| `FIND-admin-principals-R4-11` | `d60a2ac80` |
| `FIND-admin-principals-R4-12` | `5475d1168` |
| `FIND-admin-principals-R4-13` | `28c49b868`, `a368d81e0` |
| `FIND-admin-principals-R3-6` | evidence-only; literal commands recorded below |

### `R3-6` repaired exact commands (all run, all nonzero and passing)

```
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::delegation_issues_no_refresh_token) or test(=refresh::pg_tests::rotation_carries_the_roles_login_persisted) or test(=refresh::pg_tests::a_machine_refresh_row_cannot_rotate) or test(=refresh::pg_tests::f09_replay_containment_commits_and_kills_the_successor) or test(=refresh::pg_tests::reuse_detection_revokes_family)'"
→ Summary [0.988s] 5 tests run: 5 passed, 107 skipped

scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=a_tenant_administrator_renews_by_re_exchanging_its_credential) or test(=a_failed_provisioning_can_be_retried_with_the_same_slug)'"
→ Summary [0.036s] 2 tests run: 2 passed, 33 skipped

WYRD_IDENTITY_E2E=1 WYRD_KEYCLOAK_ISSUER=http://localhost:8080/realms/wyrd-test WYRD_DEX_ISSUER=http://localhost:5556 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --all-features --test identity_e2e --test-threads=1 -E 'test(=human_oidc_login_journey)'"
→ Summary [3.656s] 1 test run: 1 passed, 17 skipped
```

The identity selector needs Keycloak and Dex up and the realm users seeded. Run
`docker compose up -d keycloak dex --wait`, then the Python user-seeding block
embedded in `mise.toml`'s `test:identity:journey:inner`, before the command
above; `mise run test:identity:journey` does both and tears them down again.

### `R4-2`/`R4-3` served journeys (`22a205985`)

New in `crates/wyrd/wyrd-server/tests/identity_e2e.rs`:

- `revoking_a_human_kills_the_session_refresh_authority` — browser login,
  authenticated `/v1` `200`, `POST /v1/principals/{id}/revoke` with
  `principal_kind: "user"`, then old access refused
  `WYRD_AUTH_401_CREDENTIAL_REVOKED` and the refresh token refused with no
  successor in the body.
- `a_withdrawn_oidc_group_invalidates_the_roles_it_granted` — `group_role_map`
  over the realm `groups` claim; login, unchanged re-login leaves the first
  session working, then the `wyrd-admins` membership is withdrawn at Keycloak
  and the next login refuses the stale token while its successor can no longer
  delegate. Membership is restored before the test returns.

Supporting: the three human journeys now share `human_login`,
`human_issuer_entry`, `principal_id_of`, and `keycloak_admin` helpers, and
`wyrd_testing::OidcIssuerFixture` gained `set_group_membership` over a new
private `admin_token` accessor that `rotate_signing_key` now also uses.

```
WYRD_IDENTITY_E2E=1 WYRD_KEYCLOAK_ISSUER=http://localhost:8080/realms/wyrd-test WYRD_DEX_ISSUER=http://localhost:5556 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --all-features --test identity_e2e --test-threads=1 -E 'test(=revoking_a_human_kills_the_session_refresh_authority) or test(=a_withdrawn_oidc_group_invalidates_the_roles_it_granted)'"
→ Summary: 2 tests run: 2 passed, 18 skipped
```

### Required lanes run on the final tree so far

| Lane | Result |
|---|---|
| `mise run fmt` / `mise run fmt:check` | PASS |
| `mise run lints` | PASS |
| `mise run check:client-tier` | PASS |
| `mise run check:unwrap-audit` | PASS |
| `mise run check:clippy-allow-audit` | PASS |
| `mise run check:tenant-isolation` | PASS |
| `mise run test:shared` | PASS — 664 tests run, 664 passed |
| `mise run test:principals:unit` | PASS — 11 / 11 / 4 passed |
| `mise run test:principals:integration` | PASS — 2 / 12 / 15 passed |
| `mise run test:platform:journey` (run 1) | PASS — 35 tests run, 35 passed |
| `mise run test:platform:journey` (run 2) | PASS — 35 tests run, 35 passed |
| `mise run test:identity:journey` | PASS — 20 tests run, 20 passed |
| `mise run test:cli:journey` | PASS |
| `mise run test:bifrost:journey:mcp` | PASS — 9 tests run, 9 passed |
| `mise run test:wyrd` | PASS — 2022 tests run, 2022 passed |
| `mise run docs:check` | PASS — 60 pages, contrast AA |
| `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f HEAD` | PASS — silent, exit 0 |

### Remaining work for the next agent

1. Run and record the still-unrun required lanes:
   `mise run test:sql`, `mise run test:bifrost:integration:redux`,
   `mise run test:bifrost:integration:server`,
   `mise run test:bifrost:integration:sql`, `mise run codegen:check`,
   `mise run check:examples`, strict `wyrd-sql` rustdoc with warnings denied,
   and the diff-based all-item Rustdoc audit required by `R4-12`.
   Do not substitute `mise run gate`; approved `VER-003` excludes it.
   A failing lane is broken and gets fixed, whether or not it also fails at the
   base commit.
2. The `R4-12` Rustdoc audit was run from a throwaway script kept in the session
   scratchpad (never in the repo — the packet forbids a permanent documentation
   scanner). It diffs `git diff -U0 <BASE> -- '*.rs'` against the **working
   tree**, maps changed lines to the enclosing item declaration, walks upward
   past attributes and `//` comments to find a `///`, accepts an inner `//!` on
   the line after `mod X {`, and separately requires a `# Errors` section on any
   fn returning `Result<`. Rebuild it the same way; it reported zero findings at
   `5475d1168`.
3. Re-run `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`
   after any further commit and confirm it stays silent.
4. Append the final evidence table (acceptance criterion → implementation
   evidence → verification evidence → result) mapping every acceptance row to
   its commit, exact focused proof, broader lanes, and result, and confirm each
   non-goal stayed excluded and no unrelated file changed.
