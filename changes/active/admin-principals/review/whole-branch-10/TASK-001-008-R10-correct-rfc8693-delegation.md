# TASK-001-008-R10 — Correct delegation to RFC 8693 subject/actor semantics

## Route and authority

Implement this remediation with `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete cumulative candidate.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14, status `approved`, SHA-256
  `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`.
- Parent remediation:
  `changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`.
- Cumulative candidate before this correction:
  `dc6ddf6bcfd1efe39a6727b61d698d400170e6e6`.
- Human-approved correction: replace the inverted delegation implementation
  with RFC 8693 subject/actor semantics and expose one shared delegation helper
  through the Rust, Python, and TypeScript SDKs.
- Remediates: `FIND-admin-principals-R10-1`.

## Outcome

When Service A calls Service B and B performs work against Bifrost for A, the
delegated JWT represents A as the subject, B as the current actor, Bifrost as
the audience, and only the authority shared by A, B, and the requested resource.
Applications do not construct token-exchange requests by hand: the shared
client owns the exchange and every first-class SDK projects that one behavior.
Once issued, ordinary server middleware verifies and authorizes the signed
delegated JWT automatically with no database read on the Bifrost request path.

## Diagnosis

The current implementation reverses RFC 8693. It accepts A's token plus a
`requested_subject` B, then issues a token whose effective principal is B and
whose `act` chain names A. That means “A acts for B,” not the required “B acts
for A.” It also authorizes exchange through the generic `delegation:issue`
permission bundled into `runtime_admin`, so a non-admin agent must receive an
administrative role while B never authenticates as the actor.

This is observable in the current token contract, `DelegateToken`,
`TenantGrant::Delegation`, JWT issue/verify projections, `Caller`,
`/v1/authz/check`, the `runtime_admin` builtin role, and the delegation tests
that assert `sub=B, act=A`. The result is incorrect attribution and the wrong
authorization principal for A-read/B-read-write workflows.

## Decision-complete correction

### 1. Use the existing RFC token-exchange boundary correctly

Keep the existing `POST /auth/token` endpoint, typed token request/response,
five-minute issuer, local verifier, canonical errors, and audit path. Correct
the token-exchange variant to carry the standard meanings:

- `subject_token`: A's verified Wyrd access token;
- `actor_token` and `actor_token_type`: B's verified Wyrd access token and its
  access-token type;
- `audience`: the target Wyrd resource, with Bifrost represented by its one
  canonical audience value;
- no `requested_subject` field.

The existing JSON token endpoint remains the Wyrd wire encoding; do not add a
second form endpoint or parser merely to restate the same typed contract.

Verify both tokens through the existing concrete `TokenVerifier`. They must be
valid, tenant-equal, and usable for new issuance. Resolve B's current principal
and grants through the existing `TenantTokenIssuer` issuance path; do not add
request-time introspection or another issuer.

### 2. Reuse the existing directed invoke-policy decision

Reuse the existing cross-service invoke-policy owner to decide whether B may
act for A. A valid subject token alone does not authorize every actor, and a
valid actor token alone does not authorize B to represent every subject. The
directed A-to-B policy decision is the one relationship check; do not add a
delegation table, grant store, role editor, `may_act` persistence model, or a
second policy engine.

Delete the generic `delegation:issue` permission, its `runtime_admin` grant,
and authorization branches/tests/docs whose only purpose is that inverted
model. No replacement `delegator`, `delegation:act`, or other role is needed:
B proves its identity with `actor_token`, and the existing invoke policy proves
the A-to-B relationship. Policy or audit unavailability fails closed.

### 3. Issue and consume the standard JWT shape

The issued token must carry:

```json
{
  "sub": "<A principal id>",
  "act": { "sub": "<B principal id>" },
  "aud": "bifrost",
  "permissions": ["<A ∩ B ∩ requested-resource authority>"]
}
```

Top-level subject/principal fields represent A. The outermost `act` object is
the current actor B; any earlier actors nest inside it in RFC order. `act` is
identity and attribution, never an independent authority source. The signed
`permissions` claim remains the only tenant authority and is the semantic
intersection of A's verified permissions, B's current permissions, and any
narrower requested resource scope. Existing wildcard, schema, exact-object,
and disjoint intersection behavior must be reused.

Update the existing authenticated-principal/`Caller` path rather than adding a
parallel delegated context. RBAC and Bifrost authorize from A's top-level
subject plus the already-attenuated permissions; audit and policy retain B and
the full verified actor chain. Audience validation must ensure a Bifrost token
cannot be replayed against another Wyrd audience.

### 4. Make the client workflow automatic without hiding the authority decision

Add one public `on_behalf_of` operation to the existing `WyrdClient` owner. It
accepts the inbound subject token and target audience, obtains B's actor token
from that client's existing authentication state, calls the existing token
endpoint, and returns a client bound to the delegated token. It caches only the
short-lived delegated result and repeats the same exchange before expiry or
after the existing single authentication-refusal retry; it stores no refresh
token and never logs either bearer.

Project that exact owner through:

- the Rust SDK by its existing `wyrd-client` re-export;
- an idiomatic Python `on_behalf_of(...)` method in `wyrd-sdk-python`; and
- an idiomatic TypeScript `onBehalfOf(...)` method in `wyrd-sdk-ts`.

Python and TypeScript must call the Rust-owned implementation. They must not
duplicate token exchange, caching, refresh, HTTP, header, or error logic.
Application code therefore chooses the inbound subject and audience once; the
helper performs the exchange. On each subsequent request, existing server
middleware automatically verifies signature, issuer, target audience, expiry,
subject, actor chain, and permissions. The server cannot safely invent B from
host/path/header text, so actor authentication remains mandatory.

### 5. Replace stale authority and examples

Update `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`, and
`architecture/wyrd-doctrine.mdx` wherever they describe top-level principal as
the callee or `act` as the caller/delegator. Update other live docs, comments,
examples, generated schemas, and agent guidance only where the stale model is
actually present. Historical review records remain historical and are not
rewritten.

## Constraints and non-goals

- Reuse the existing token endpoint, `TenantTokenIssuer`, `TokenVerifier`,
  permission intersection, invoke-policy evaluator, canonical audit append,
  `Caller`, shared client transport, and Bifrost authorization owners.
- Preserve five-minute stateless tenant JWTs and database-free request
  verification; current principal/grant state is read only during issuance.
- Preserve platform-plane current-state authorization, tenant RLS,
  `TenantConn`/`OperatorPool`, credential lifecycle, refresh behavior, Card
  scope, request IDs, and the retained audit-publisher `FOR UPDATE NOWAIT`
  correction.
- Add no delegation table, new role, new permission, new verifier/issuer trait,
  second client transport, second audit path, compatibility endpoint, or
  migration for the unshipped inverted contract.
- Do not make delegation implicit from an untrusted hostname, route, header, or
  claimed `act`. Middleware may automate the exchange only because it holds A's
  inbound subject token and B's independently authenticated actor token.
- Do not broaden this task into unrelated policy, RBAC, credential, or Bifrost
  redesign.

## Acceptance criteria

| Obligation | Required observable result |
|---|---|
| RFC request semantics | Token exchange accepts verified `subject_token=A`, `actor_token=B`, and target audience; it rejects a missing/invalid actor token, cross-tenant pair, policy-denied A-to-B pair, unsupported audience, and reversed or malformed identity input without issuing a token |
| JWT structure | Verification of the issued token yields top-level subject A, current outer actor B, RFC-ordered nested actors, Bifrost audience, no authority from `act`, and no stale callee-as-subject projection |
| Attenuation | A with exact-table read and B with read/write receives only exact-table read; write, wildcard, schema, exact-object, and disjoint cases reuse the semantic intersection and never amplify either party |
| Automatic request verification | Existing middleware accepts the valid Bifrost-audience delegated token, constructs the existing caller context, authorizes read, rejects write, and performs no database/cache/introspection read during the Bifrost request |
| Client helper | Rust, Python, and TypeScript expose the same `on_behalf_of` behavior; Python/TypeScript delegate to `wyrd-client`; token exchange, caching, renewal, headers, redaction, retry, and errors have one Rust owner |
| Removal | `delegation:issue`, its `runtime_admin` grant, `requested_subject`, inverted `principal=callee/act=caller` logic, and live stale documentation have no production caller or generated contract residue |
| Audit | Exchange/policy decisions identify subject A, actor B, audience/resource, outcome, and applicable credential attribution through the canonical path; an unrecordable decision issues no token |

## Required proving scenarios

### Primary user journey

Revise or replace the existing delegation journey with one real
client-to-server-to-Bifrost scenario named
`query::service_b_acts_for_service_a_with_only_a_table_authority` under the
existing `wyrd-testing` Bifrost server journey binary:

1. Create Service A with read permission for one concrete table.
2. Create Service B with read and write permission for that table.
3. Establish an allowed A-to-B invoke policy using the existing policy owner.
4. Obtain A's subject token and configure the ordinary shared client as B.
5. Call `on_behalf_of(A token, Bifrost audience)`; do not hand-build the HTTP
   exchange in the test.
6. Decode only for contract assertions: `sub=A`, outer `act.sub=B`,
   `aud=bifrost`, and permissions contain the exact-table read but not write.
7. Use the returned delegated client against the real Bifrost surface: the
   table read succeeds and a write is denied before effect.
8. Assert audit attribution names A as subject and B as actor.

Exact focused command:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test server -P journey --run-ignored=all \
  -E 'test(=query::service_b_acts_for_service_a_with_only_a_table_authority)'"
```

The test must select exactly one test and must fail RED because the current
implementation produces `sub=B, act=A` and has no actor-token client flow.

### Focused negative and contract proof

Use the current auth issue/verify and Postgres test owners to prove missing or
invalid actor, tenant mismatch, policy denial, audience mismatch, audit
failure, nested actor order, and all permission-intersection shapes. Revise the
existing inverted delegation tests rather than retaining parallel old/new
suites. Every named test added to evidence must include its literal exact
`mise exec -- cargo nextest ... -E 'test(=...)'` command and positive selected
count.

Add narrow public-surface tests proving the Rust, Python, and TypeScript helper
names and delegation behavior. Use the existing SDK test harnesses; do not add
a delegation-only harness or duplicate network fixture.

## Broader verification

Run the narrowest owning lanes for every touched surface, including:

- `mise run fmt:check`
- `mise run lints`
- `mise run py:format:check`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run py:test:unit`
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run ts:test:unit`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run
  db:migrate:all:inner && WYRD_AUTH_E2E=1 mise exec -- cargo nextest run
  --locked -p wyrd-server --test auth_e2e"`
- `mise run test:bifrost:journey:server`
- `mise run test:bifrost:integration:server`
- `mise run test:shared`
- `mise run test:sql`
- `mise run codegen:check`
- `mise run docs:check`
- strict rustdoc for every affected crate
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`, `test:rust`, or an ad hoc all-features test
aggregate. Append one evidence table mapping each acceptance row to
implementation commits, exact focused commands and selected counts, owning
lanes, and results.

## Implementation evidence

Status: `IMPLEMENTED`. Commits `596a50bdd`, `47548e32f`, `29a3cad30`,
`57229ffbe`, `10315484f`, `3912612fe`, `5794e6cf9`, `be0ec96fd`, `61ab3c689`,
`3a3bcc60d`, plus the evidence commit that carries this table.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| RFC request semantics | `596a50bdd`, `47548e32f`: `TokenRequest::TokenExchange { subject_token, actor_token, audience }`; `crates/wyrd/wyrd-auth/src/exchange_api_key.rs` verifies both tokens, checks the same tenant and invoke policy, and fails closed | `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::invalid_or_malformed_identity_input_records_no_decision) \| test(=exchange_api_key::pg_tests::a_policy_denied_exchange_commits_one_denied_decision) \| test(=exchange_api_key::pg_tests::an_allowed_exchange_for_a_missing_actor_commits_one_allowed_decision) \| test(=exchange_api_key::pg_tests::a_refused_exchange_audit_issues_no_token) \| test(=exchange_api_key::pg_tests::an_exchange_names_subject_and_actor_and_carries_only_the_intersection)'"`: 5 run, 5 passed (the cross-tenant actor, missing/invalid actor and malformed input are cases inside `invalid_or_malformed_identity_input_records_no_decision`) | PASS |
| JWT structure | `TenantGrant::into_access_grant` (`crates/wyrd/wyrd-auth/src/issuance.rs`); `flatten_act_chain` orders the chain earliest first, so the current actor is last | `mise exec -- cargo nextest run --locked -p wyrd-auth-issue --lib -E 'test(=tests::issue_access_token_delegated_names_subject_and_outer_actor)'`: 1 passed; `mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib -E 'test(=tests::into_verified_keeps_subject_and_orders_actors_earliest_first) \| test(=tests::bifrost_audience_is_accepted_only_on_the_bifrost_surface) \| test(=tests::token_verifier_rejects_cross_tenant_token)'`: 3 passed | PASS |
| Attenuation | `PermissionSet::intersection` reused in `into_access_grant` | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::intersection_keeps_only_the_narrower_shared_authority)'`: 1 passed; `an_exchange_names_subject_and_actor_and_carries_only_the_intersection` (above); `auth_e2e` `journey_delegation_cannot_amplify_the_subject` | PASS |
| Automatic request verification | `29a3cad30`: `require_bifrost_authenticated` layers only the Bifrost and query routers; the gRPC gate uses `verify_on(.., TokenAudience::Bifrost)`; verification is JWT-only (no DB read) | Primary journey `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query::service_b_acts_for_service_a_with_only_a_table_authority)'"`: 1 passed (read succeeds; table write gets RBAC 403; a Wyrd route gets 401; audit names A as subject and B as actor); `mise run test:bifrost:journey:server` PASS; `mise run test:bifrost:integration:server` PASS | PASS |
| Client helper | Rust `WyrdClient::on_behalf_of` (`57229ffbe`); Python `wyrd.WyrdClient.on_behalf_of` (`3912612fe`, `sdks/wyrd-sdk-python/src/client.rs`); TS `WyrdClient.onBehalfOf` (`5794e6cf9`, `sdks/wyrd-sdk-ts/native/src/client.rs`); both SDKs call the Rust method, with no exchange, caching or HTTP logic of their own | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=auth::tests::on_behalf_of_caches_the_exchange_and_redacts_the_subject)'`: 1 passed; `mise exec -- uv run pytest tests/unit/client/test_client.py` (in `sdks/wyrd-sdk-python`): 3 passed; `mise run ts:test:unit`: 15 passed, including `wyrd-client.test.ts` (2) and the `WyrdClient.connect` no-credentials case | PASS |
| Removal | `delegation:issue`, `Resource::Delegation`, `Action::Issue`, `requested_subject` and the callee/caller model are removed from code; schemas regenerated (`61ab3c689`); docs rewritten (`3a3bcc60d`) | `git grep -e requested_subject -e RequestedSubject -e delegation:issue -- ':!changes'` returns nothing; `mise run codegen:check` PASS; `mise run docs:check` PASS | PASS |
| Audit | Exchange audit: operation `auth.token.exchange`, principal is the subject, detail carries actor and subject, resource is the audience, and the actor credential is attributed; `a_refused_exchange_audit_issues_no_token` shows an audit failure issues no token | Focused `wyrd-auth` pg tests (above); `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_AUTH_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-server --test auth_e2e"` PASS; the primary journey asserts both the exchange audit row and the read-decision audit row | PASS |

Owning lanes: all passed at the final candidate.
- Build, format and lint:
  - `mise run fmt:check`
  - `mise run lints`
  - `mise run py:format:check`
  - `mise run py:lints`
  - `mise run py:typecheck`
  - `mise run py:test:unit`
  - `mise run ts:build`
  - `mise run ts:typecheck`
  - `mise run ts:test:unit`
- Boundary checks:
  - `mise run check:client-tier`
  - `mise run check:pyo3-scope`
  - `mise run check:unwrap-audit`
  - `mise run check:clippy-allow-audit`
  - `mise run check:tenant-isolation`
  - `mise run check:from-pools-allowlist`
- Test lanes:
  - `mise run test:principals:unit`
  - `mise run test:principals:integration`
  - `mise run test:shared`
  - `mise run test:sql`
- Generated artifacts:
  - `mise run codegen:check`
  - `mise run docs:check`
- Strict rustdoc: `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --locked --no-deps` passes for:
  - `wyrd-spec`
  - `wyrd-auth-issue`
  - `wyrd-auth-verify`
  - `wyrd-auth`
  - `wyrd-auth-check`
  - `wyrd-client`
  - `wyrd-sdk-ts`
  - `wyrd-server`
  - `wyrd-sdk-python --features python`
- Whitespace: `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f HEAD` is clean.

Limits:
- **Python toolchain through mise and uv.** Several script lanes called a bare `python` or `python3`, which mise never supplied. Following https://mise.jdx.dev/lang/python.html#mise-uv:
  - `mise.toml` now declares `python = "3.12"` in `[tools]` (matching CI);
  - `[env]` sets `UV_PYTHON = { value = "{{ tools.python.path }}", tools = true }`;
  - every Python task and task-invoked script now runs through `uv run python`.

  All of these pass through plain `mise run`:
  - `check:unwrap-audit`, `check:clippy-allow-audit`, `check:tenant-isolation`
  - their `:self` / `audit-script` tests
  - `check:test-contracts`, `check:no-testing-in-prod-deps`, `test:postgres:inventory`
  - `docs:check`, `docs:a11y`, `docs:check:commands`, `docs:linkcheck`

  `py:test:unit` (468 passed) and `py:typecheck` pass on the SDK `.venv`, which was rebuilt on the mise interpreter.
- **Flaky first `test:shared` run.** It failed once because the test server's port was already taken (`Address already in use`) and passed 658/658 on the re-run.
- **Strict rustdoc for `vala-bifrost-redux`.** It fails on missing docs this change did not introduce; only one line of `gate/auth.rs` changed there. That crate is not in `check:docs`.
- **Trailing blank lines.** Two review records from `33feb673b` had trailing blank lines. Only the whitespace was removed, so `diff --check` passes.
- **Python/TS delegation journey.** There is no Python or TS end-to-end delegation journey: the SDK test harnesses cannot seed two services and an invoke policy. The delegation behavior is proven by the Rust journey, and the SDK tests prove the projection and that errors flow from Rust.

Non-goals were kept out of scope:
- no table, role, permission, trait, migration, second audit path or compatibility endpoint was added;
- Bifrost, Cards and `WyrdState` SDK constructors are unchanged.
