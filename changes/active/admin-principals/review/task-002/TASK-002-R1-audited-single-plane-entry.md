---
task: TASK-002-R1
title: One audited platform authentication and authorization entry point
spec: SPEC-admin-principals
spec_revision: 6
remediates: TASK-002
findings: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3]
---

## Subject

- Approved spec: `changes/active/admin-principals/spec.md`
  (`SPEC-admin-principals` revision 6, approved).
- Original task:
  `changes/active/admin-principals/tasks/TASK-002-auth-context-and-planes.md`.
- Reviewed candidate: `e551d5d..f3923cc` on `claude/admin-principals-spec-qfsmjc`.
- Review verdict: `changes/active/admin-principals/review/task-002/verdict.md`.

## Issue diagnosis

### `FIND-TASK-002-1` — a second, unaudited platform authorization entry point

**Violated obligation.** `architecture/agent-rules.md`: "Every decision that
evaluates a principal's permission appends one audit row — allowed and denied
alike" and "Every audited authorization decision appends its row in the same
transaction as the decision… A decision that cannot be recorded fails closed."
`AGENTS.md` §2, same. TASK-002 acceptance criterion: "An authorization decision
appends its audit row in the deciding transaction for allowed and denied
outcomes alike; an unrecordable audit refuses the operation and commits
nothing." REQ-016, INV-011.

**Current behavior.** The candidate ships two platform authorization APIs.
`crates/wyrd/wyrd-auth/src/platform_authz.rs::PlatformAuthorization::authorize`
is audited, transaction-coupled, and fail-closed, and has no production caller.
`crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:52-63`
`PlatformCaller::authorize` is a `pub` permission decision that takes no pool,
opens no transaction, and appends nothing:

```rust
pub fn authorize(&self, required: &Permission) -> Result<(), WyrdErrorResponse> {
    if self.context.effective_permissions().contains(required) {
        return Ok(());
    }
    Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac { .. }))
}
```

**Evidence.** `grep -rn "PlatformAuthorization" crates/` returns only its own
definition and its `pg_tests`. `PlatformCaller::authorize` hangs off the axum
extractor every platform route must take, and the original task's evidence table
cites its unit tests (`authorization_follows_the_grant_and_never_reaches_a_tenant`,
`empty_grant_authorizes_nothing`) as its REQ-017 and REQ-018 proof.

**Observable consequence.** A platform route that calls `PlatformCaller::authorize`
executes and commits a privileged operation with no `platform.audit_authz` row
and no fail-closed behavior when audit is unavailable. Nothing in the repository
steers a route author to the audited path instead.

**Why the existing proof falls short.** The four `platform_authz::pg_tests` prove
the audited path in isolation. None of them proves it is the *only* path, and no
test covers `PlatformCaller::authorize` producing an audit row, because it cannot.

**Compounding defect.** `PlatformAuthorization::authorize` returns
`Transaction<'a, Postgres>`, but every existing
`crates/wyrd/wyrd-sql/src/queries/platform/*` slot —
`insert_platform_principal`, `set_platform_grant`,
`platform_grant_for_principal`, and the credential slots — takes `&OperatorPool`
and runs on a pool-drawn connection. No existing platform write composes into
that transaction, so a caller holding it must either commit its operation
outside the audited transaction or invent transaction-taking slots. A correction
that leaves the coupling uncomposable has not closed the finding.

### `FIND-TASK-002-2` — the per-request platform path authorizes from credential material

**Violated obligation.** REQ-012a: "The per-request path MUST construct the
authenticated context only from verified Wyrd token claims, subject to the
existing authorization epoch. No served surface MAY authorize from credential
material, a lookup prefix, a credential record, or a provider token directly."
TASK-002 Constraints restate it verbatim. Also REQ-012, REQ-013, REQ-005,
INV-013.

**Current behavior.** `platform_extractor.rs:76-82` reads the `Authorization`
request header and extracts the raw secret;
`platform_extractor.rs:130-176` passes it to
`PlatformCredentials::authenticate(&pool, &presented)`, which resolves the
non-secret lookup prefix and verifies the secret against the
`platform.credentials` record, then reads the grant and builds
`AuthContext::Platform` from that credential record. This happens on every
request. No platform-scope Wyrd token is minted anywhere in the candidate.

**Evidence.** All four inputs REQ-012a names — request header, credential
material, lookup prefix, credential record — are read on the request path.
`crates/shared/wyrd-auth-verify/src/lib.rs:811` rejects
`PrincipalKindTag::GlobalAdmin` outright, so no platform-scope token can verify
into any context today; and
`crates/wyrd/wyrd-auth/src/revocation_resolver.rs:128-133` fails closed for
`GlobalAdmin` because the only epoch source is tenant-keyed.

**Observable consequence.** The platform plane has no entry-path / request-path
separation. The credential secret travels on every request rather than once;
every request pays an Argon2 verification plus two operator-pool round trips;
and INV-013 is unsatisfiable on this plane because there is no token and no
epoch. REQ-045 ("a platform session MUST carry platform scope only"), which
TASK-007 must deliver, has nothing to build on.

**Why the existing proof falls short.** The task recorded this as "the platform
plane issues no access tokens, so it has no epoch to advance." That statement is
true of the implementation but is a consequence of the deviation, not a
satisfaction of the requirement: REQ-012a names the authorization epoch as a
property of the per-request path, and the design that would carry it was not
built. `platform_credentials::pg_tests::revocation_takes_effect_on_the_next_request`
proves credential revocation stops a credential; it does not prove that an
already-issued context stops being honored, because no context outlives a
request.

### `FIND-TASK-002-3` — cross-plane proof on a real route, and the reverse direction

**Violated obligation.** AC-003: "Cross-plane negative evidence proves a tenant
credential cannot invoke any platform operation **and a global credential cannot
invoke ordinary tenant operations**, with stable errors and no enumeration."
TASK-002 approach step 6: "Prove plane separation and tenant-boundary
enforcement on an existing protected route before any new administrative route
exists." `AGENTS.md` §11 and the agent-rules journey bullet.

**Current behavior.** `crates/wyrd/wyrd-server/tests/auth_e2e.rs` and
`crates/wyrd/wyrd-server/tests/pg_authz_check_route.rs` — both named in the
original task's *Relevant Surface* — are untouched. The only cross-plane
evidence is four in-process unit tests over header-shape and lookup-prefix
helpers in `platform_extractor.rs::tests`.

**Observable consequence.** A proof gap, not a live hole: the tenant plane
requires `X-Wyrd-Access-Token` carrying a compact Wyrd JWT
(`components/auth/token_extract.rs:17-47`), so a `wyrd_global_*` secret on
`Authorization` cannot authenticate there today. But that separation is
undefended by any test and will silently regress.

**Why the existing proof falls short.** A unit test over
`extract_platform_credential` proves a helper returns `None` for a token-shaped
string. It does not prove a served route refuses a cross-plane credential, does
not exercise the stable error, and does not show that the refusal reveals
nothing about tenant or principal existence.

## Intended correction outcome

The platform control plane has exactly one authentication path and exactly one
authorization path. Authentication mints a platform-scope Wyrd token at an entry
path and derives the per-request `AuthContext::Platform` from that token's
verified claims alone, subject to a platform authorization epoch. Authorization
is a single reachable decision that appends its row in the transaction the
operation runs in and refuses when it cannot. Both plane boundaries are defended
by real-server evidence on an already-protected route.

## Decision-complete recommendation

Everything below stays inside approved behavior; REQ-012, REQ-012a, REQ-013,
REQ-005, and INV-013 have already decided each choice. Nothing here requires a
new product, API, architecture, security, or persistent-data decision.

### 1. Collapse the two authorization entry points into the audited one

Delete `PlatformCaller::authorize`. Do not replace it with a second synchronous
checker and do not add a "convenience" unaudited variant. The one reachable
platform decision is the audited, transaction-coupled one that already exists in
`crates/wyrd/wyrd-auth/src/platform_authz.rs`; it is the owner to reuse, not to
duplicate. `PlatformCaller` keeps its `context` and `request_id` and is the
natural owner of the call into that decision, so put the entry point where a
route author will find it and make the audited transaction the only thing it can
hand back.

Do not weaken the audited path to accommodate the deletion: allowance stays
uncommitted until the caller commits, denial stays durable, and an unappendable
decision stays a rollback-and-refuse.

### 2. Make the coupled transaction composable with platform writes

The correction is not complete while no platform write can join the audited
transaction. Reuse the repository's own precedent rather than inventing a new
shape: `crates/wyrd/wyrd-sql/src/queries/platform/audit_authz.rs` already takes
`&mut Transaction<'_, Postgres>`, which is what
`scripts/check_tenant_isolation.py:276` requires of a platform query module and
what `OperatorPool::begin()` is documented to produce, with `vala-sql`'s
`forge_operations.rs` and `forge_tasks.rs` as in-repo precedent. Bring the
platform writes a platform decision actually guards onto that same boundary so
the decision and the operation share one commit. Scope this to the platform
query slots the corrected entry point needs; do not migrate unrelated slots.

Alternative resolved: leaving the pool-taking slots as they are and having
callers commit the audit transaction separately is rejected — it reproduces
exactly the decoupling `FIND-TASK-002-1` reports, and it would make the four
existing `pg_tests` prove a guarantee production never gets.

### 3. Move platform authentication to token claims, with an epoch

Split the platform plane the way REQ-012 and REQ-012a already specify.

- **Entry path.** Platform credential verification —
  `PlatformCredentials::authenticate` — moves off the request path and becomes
  the credential-exchange entry that mints a platform-scope Wyrd token carrying
  the same principal representation as the tenant plane's. Reuse the existing
  issuance and verification seams (`wyrd-auth-issue`, `wyrd-auth-verify`); do
  not add a second token format, signer, or verifier.
- **Request path.** `PlatformCaller::from_request_parts` verifies that token and
  builds `AuthContext::Platform` from its verified claims plus the principal's
  platform grant. It must not read `platform.credentials`, a lookup prefix, or
  any credential secret. Keep the single indistinguishable rejection already
  implemented, and keep the platform plane's header distinct from the tenant
  plane's `X-Wyrd-Access-Token` so a misrouted credential cannot cross.
- **Verifier.** `wyrd-auth-verify` must resolve a platform-scope claim set to a
  platform context. Its existing refusal of `PrincipalKindTag::GlobalAdmin`
  (`crates/shared/wyrd-auth-verify/src/lib.rs:811`) is correct and must stay: a
  platform kind may never become a *tenant* `Principal`. Add the platform
  resolution as a distinct outcome rather than relaxing that guard.
- **Epoch.** Revoking a platform credential, suspending or deleting a platform
  principal, or changing a platform grant advances a platform authorization
  epoch in the same transaction as the revocation, and a token issued before it
  stops verifying. Resolve it through the existing `RevocationCheck` seam;
  `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:128-133` currently fails
  closed for `GlobalAdmin` because the only epoch source is tenant-keyed, so
  give the platform side its own source. Where that value is stored — a column
  on the platform principal row versus a separate table — is a reversible local
  decision; that it advances transactionally with the revocation is not.

Alternative resolved: keeping the platform plane credential-only per request is
rejected as an implementation choice. If the author decides the platform plane
*should* stay credential-only, that is a change to approved behavior and
requires a spec revision carving the platform plane out of REQ-012a and adding a
platform clause to INV-013 — not an implementation decision this task may make.
Do not proceed on that reading without the revised spec.

### 4. Prove both plane boundaries on a real route

Use the two surfaces the original task already named —
`crates/wyrd/wyrd-server/tests/auth_e2e.rs` and
`crates/wyrd/wyrd-server/tests/pg_authz_check_route.rs` — driven by
`WyrdTestServer` against repository-managed Postgres. Do not add a third server
test binary; both files already earn their place under the agent-rules test-home
bullet.

## Constraints and preserved behavior

- The audited decision's semantics are preserved exactly: allowance uncommitted
  until the caller commits, denial durable, unappendable decision rolled back and
  refused, one row per decision naming principal, permission, resource, outcome,
  and target tenant.
- `AuthContext` stays a closed two-variant type; `AuthContext::Platform` gains no
  tenant field and no tenant accessor (REQ-013, INV-004a).
- The permission vocabulary added by TASK-002 (`Resource::Tenants`,
  `Action::Suspend`, `Action::Recover`, and the four constructors) stays as is.
  One vocabulary, one synchronous checker, no second grant store or cache.
- Absence of a grant stays denial.
- The single indistinguishable platform rejection stays; a store outage stays
  distinguishable from a failed authentication.
- Tenant-plane behavior is unchanged: `Caller`, `AuthenticatedPrincipal`,
  `TenantConn` RLS, and the existing tenant revocation epoch are not touched
  except where the platform epoch shares the `RevocationCheck` seam.
- No compatibility route, alias, or shim; none of these contracts has shipped.

## Non-goals

- Platform-plane routes, tenant provisioning, initialization, credential
  management routes, platform OIDC, and human platform principals. Those are
  TASK-003, TASK-004, TASK-006, and TASK-007.
- Migrating tenant handlers to carry `AuthContext` instead of `Caller`.
- Any change to `wyrd apply` Card-bound principal provisioning.
- Any RBAC redesign, explicit deny, or customizable roles.
- Diagnosing or fixing the seven environmental `wyrd-auth-verify`
  `verify_external_*` failures (`VER-005`).

## Acceptance criteria

1. (`FIND-TASK-002-1`) Exactly one reachable platform-plane authorization entry
   point exists. No `pub` API decides a platform permission without appending its
   audit row. An allowance is invisible until the caller commits and disappears
   when the caller rolls back; a denial is durable and refuses; an unappendable
   decision rolls back and refuses.
2. (`FIND-TASK-002-1`) The platform writes the corrected entry point guards
   execute inside the same transaction as their authorization row, demonstrated
   by a test in which rolling back the operation leaves no allowance and no
   operation effect.
3. (`FIND-TASK-002-2`) A platform credential exchanges for a platform-scope Wyrd
   token, and the per-request platform path derives `AuthContext::Platform` from
   that token's verified claims alone. Presenting a raw platform credential on
   the request path no longer authenticates.
4. (`FIND-TASK-002-2`) A platform-scope token still cannot resolve to a tenant
   `Principal`.
5. (`FIND-TASK-002-2`) Revoking a platform credential, suspending the platform
   principal, or changing its grant advances the platform authorization epoch in
   the revoking transaction, and a token issued before that epoch stops
   verifying.
6. (`FIND-TASK-002-3`) Real-server evidence on an already-protected route shows a
   platform credential or token refused on the tenant plane, and a tenant access
   token refused on the platform plane, each with its stable error and with a
   response that distinguishes nothing about tenant or principal existence.

## Proof

Focused proof directly exercising each gap, run with the repository-managed
Postgres wrapper where required:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
  -E 'test(/platform_authz::pg_tests::.*/)'
mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
  -E 'test(/platform_credentials::pg_tests::.*/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/components::auth::platform_extractor::.*/)'
mise exec -- cargo nextest run --locked -p wyrd-server --test auth_e2e \
  -E 'test(=<the cross-plane test name>)'
mise exec -- cargo nextest run --locked -p wyrd-server --test pg_authz_check_route \
  -E 'test(=<the cross-plane test name>)'
```

Broader verification, per `VER-001`–`VER-006`:

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-runtime -p wyrd-auth \
  -p wyrd-auth-check -p wyrd-auth-verify -p wyrd-server --all-targets
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib
mise exec -- cargo nextest run --locked -p wyrd-auth-check --lib
mise run check:tenant-isolation
mise run check:unwrap-audit
mise run check:clippy-allow-audit
mise run codegen:check   # if the error catalog or permission contract moves
```

No broad aggregate is required or accepted as evidence (`VER-003`). Failures
outside the authentication, authorization, and audit surfaces are out of scope
(`VER-005`) and must not be worked around by weakening a test.

If `mise` is unavailable, substitute direct `cargo` pinned to the repository
toolchain (`rustup run 1.97.1`) and record the substitution, as the original task
did.

Route this task to `$wyrd-implement`.
