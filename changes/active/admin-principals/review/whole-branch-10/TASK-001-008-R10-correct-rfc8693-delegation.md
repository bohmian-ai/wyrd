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
