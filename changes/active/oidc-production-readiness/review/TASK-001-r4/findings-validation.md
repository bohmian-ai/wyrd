# TASK-001 r4 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation inputs: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md`

The candidate and tree matched the immutable subject at the start and end of
validation. I inspected the complete cumulative diff, all five Wave 1 reports,
the three prior validated ledgers and remediation tasks, applicable repository
authorities, and the candidate source and callers named below. The repository
has no `.codegraph/` directory, so caller tracing used `rg`, the cumulative
diff, and complete function bodies.

## Wave 1 proposal validation

| Wave 1 proposal | Result | Final finding | Validation |
|---|---|---|---|
| `task-review.md` empty proposal set | **CONFIRMED** | — | The task matrix and the cumulative implementation expose no additional task-acceptance finding. Its overall PASS does not resolve the independent standards and security proposals below. |
| `standards-review.md:STD-R4-001` | **CONFIRMED** | `FIND-TASK-001-25` | Both cited candidate-added signatures use qualified type paths despite the explicit bare-import rule. Each function and every caller were read. The correction is two imports and two signature substitutions, with no behavior or abstraction change. |
| `domain-review-security.md:SEC-R4-001` | **CONFIRMED** | `FIND-TASK-001-24` | `AllowInternal` admits the exact Alibaba IPv4 and AWS IMDS IPv6 metadata addresses because they are classified only by the profile-dependent CGNAT/ULA arms. The shared client is live on discovery, candidate probe, token exchange, and JWKS paths. The repository already names the same two addresses as always blocked in its gateway classifier. |
| `domain-review-tenancy-data.md` empty proposal set | **CONFIRMED** | — | Tenant acquisition, forced RLS, shared lifecycle/issuance locking, exact revision provenance, migration behavior, and recovery-decision transactionality expose no additional material finding. |
| `domain-review-secrets.md` empty proposal set | **CONFIRMED** | — | Secret intake/redaction, versioned sealing, keyless inventory, cross-store CAS rewrap, post-writer rotation proof, and restrictive file loading expose no additional material finding. |

## Caller and reachability validation

### Shared OIDC address classification

`ScreenedHttp::client_for` is the sole owner that converts a provider URL into
a pinned reqwest client. Its complete body screens literal IPv4 and IPv6 hosts
directly and sends domain answers from `resolve_and_screen` through the same
`is_blocked` predicate. `is_blocked` always calls `is_always_blocked`, then
calls `is_internal` only for `BlockInternal`. The complete
`is_always_blocked` body covers IPv4/IPv6 link-local, IPv4 broadcast, and IPv4
documentation ranges, but not `100.100.100.200` or `fd00:ec2::254`.
`is_internal` classifies those two addresses as CGNAT and ULA respectively.
Consequently `AllowInternal` permits both exact metadata endpoints while
`BlockInternal` refuses them only incidentally.

The policy is reachable. `DeploymentProfile::screened_http` selects
`AllowInternal` for the default Development profile, which is also the stated
self-hosted/local-provider path. Production fetch callers all route through
this owner: provider discovery and callback token exchange in
`wyrd-auth/src/callback.rs`, authorization and client-auth candidate probes in
`wyrd-auth/src/connections.rs`, live-login discovery in
`wyrd-auth/src/login.rs`, cache-miss JWKS retrieval in
`wyrd-auth-oidc/src/jwks.rs`, and the admin discovery path in
`wyrd-server/src/components/admin/routes.rs`. Both literal URLs and DNS answers
therefore reach the faulty predicate before any request is sent.

The path is required by TASK-001's provider qualification and live-login
behavior, and `architecture/agent-rules.md` plus
`architecture/wyrd-security-posture.md` require cloud metadata to be blocked
in every profile. This is not speculative hardening. The gateway's existing
`always_blocked` implementation independently establishes the repository's
exact classification for these two non-link-local metadata endpoints.

The smallest correction stays in the existing OIDC classifier. Factoring a
new cross-crate network-policy abstraction would add a layer for two constants;
blocking all CGNAT/ULA would break the approved permissive profile. Add only
the two exact comparisons after existing IPv4-mapped normalization and extend
the existing policy unit test. Discovery, pinning, proxy, redirect, TLS, body,
and timeout behavior remain untouched.

### Qualified types in new signatures

`bounded_lookup` is called only by `ScreenedHttp::resolve_and_screen`; its full
body merely wraps the supplied lookup future in the fixed timeout and collects
the result. The qualified `std::io::Result<I>` appears only in its future
output. Importing it as `IoResult` avoids collision with the ordinary prelude
`Result` already used throughout the module and changes no runtime behavior.

`identity_e2e::api_key_id` is called twice by
`tenant_connection_rotation_journey`, once for the denied runtime-admin
recovery principal and once for the allowed recovery principal. Its full body
runs one scalar query through the journey's fixture superuser pool. The pool is
a legitimate external integration-test capability; the defect is solely the
qualified `sqlx::PgPool` spelling in the signature. Importing `PgPool` in the
file's existing dependency block and using the bare name is the whole
correction. A helper, wrapper, trait, or query rewrite would broaden the change
without closing another obligation.

## Validated finding ledger

### FIND-TASK-001-24 — CONFIRMED — VIOLATION: permissive OIDC screening admits two cloud metadata endpoints

- **Wave 1 source:** `domain-review-security.md:SEC-R4-001`.
- **Violated obligation:** `architecture/agent-rules.md` requires cloud
  metadata and link-local addresses blocked in every profile;
  `architecture/wyrd-security-posture.md` repeats that rule while allowing
  ordinary loopback/private/CGNAT/ULA only outside production. TASK-001 and
  INV-004 require the shared rule for issuer, discovery, authorization probe,
  token, and JWKS provider IO.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/screening.rs:229-307`; policy selection at
  `crates/wyrd/wyrd-server/src/config.rs:138-151`; existing repository
  classification at `crates/wyrd/wyrd-gateway/src/endpoint.rs:142-158`.
- **Evidence:** `is_always_blocked` omits Alibaba metadata
  `100.100.100.200` and AWS IMDS IPv6 `fd00:ec2::254`. `is_internal` catches
  them only as CGNAT/ULA, so `AddressPolicy::AllowInternal` admits both. The
  gateway classifier already always blocks these exact addresses while leaving
  neighboring CGNAT/ULA addresses profile-dependent.
- **Observable consequence:** An authorized tenant connection administrator on
  a permissive deployment can cause discovery, qualification, login/callback,
  or JWKS work to send blind server-side requests into cloud metadata control
  planes. The response is not directly relayed, but request outcome/timing is
  observable and the request crosses from tenant authority into deployment
  authority.
- **Decision-complete correction:** In the existing OIDC
  `is_always_blocked` function, add exact comparisons for
  `Ipv4Addr::new(100, 100, 100, 200)` and
  `Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254)`. Preserve the existing
  `normalize_ip` call so the IPv4-mapped Alibaba spelling is covered. Do not
  move all CGNAT or ULA addresses into the always-blocked set, add a policy
  knob, duplicate guards in callers, or introduce a shared abstraction.
- **Focused closure proof:** Extend the existing address-policy test to prove
  `100.100.100.200`, `::ffff:100.100.100.200`, and `fd00:ec2::254` are refused
  under both policies, while neighboring `100.100.100.201` and
  `fd00:ec2::253` remain allowed only under `AllowInternal`. Retain the existing
  literal metadata, DNS screening, pinning, proxy, scheme, and provider-journey
  checks.

### FIND-TASK-001-25 — CONFIRMED — VIOLATION: two new signatures bypass their module import blocks

- **Wave 1 source:** `standards-review.md:STD-R4-001`.
- **Violated obligation:** `architecture/agent-rules.md` requires types to be
  imported in the top-of-module dependency block and used as bare names in
  fields, parameters, returns, trait bounds, and `where` clauses, including
  tests.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/screening.rs:245-250` and
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2429-2441`.
- **Evidence:** The candidate-added `bounded_lookup` spells its future output
  as `std::io::Result<I>`, and the candidate-added `api_key_id` parameter is
  `&sqlx::PgPool`; both modules already maintain top-level import blocks.
- **Observable consequence:** The candidate violates the repository's
  mandatory Rust dependency-manifest shape, even though the qualified paths
  compile and do not change behavior.
- **Decision-complete correction:** Import `std::io::Result as IoResult` in
  `screening.rs` and use `Future<Output = IoResult<I>>`. Import `sqlx::PgPool`
  in `identity_e2e.rs` and use `&PgPool`. Make no other function, test, pool,
  or ownership change.
- **Focused closure proof:** Direct source inspection shows both signatures use
  the bare imported aliases; run `mise run fmt`, `mise run lints`, the exact
  stalled-DNS unit test, and the filtered tenant-connection rotation journey.
  No new test or harness is warranted for these mechanical substitutions.

## Prior-finding closure

| Prior finding(s) | Candidate closure independently retained | Result |
|---|---|---|
| `FIND-TASK-001-1`, `-2` | Callback and client-auth probes accept only the exact state-bearing callback arm and exact `invalid_grant` proof. | CLOSED |
| `FIND-TASK-001-3`, `-18`, `-20` | One configured callback is used by administration and login; production scheme checks cover server fetches and the fresh browser destination. | CLOSED |
| `FIND-TASK-001-4`, `-19`, `-23` | Shared provider IO remains proxy-free, pinned, redirect-free, decoded-body-bounded, and DNS-deadline-bounded. `FIND-TASK-001-24` is a distinct missing always-blocked address pair, not a reopening of those correction boundaries. | CLOSED |
| `FIND-TASK-001-5` | Login state and refresh families retain exact connection id/revision provenance; issuance and renewal recheck Active state under the shared lock. | CLOSED |
| `FIND-TASK-001-6`, `-14` | Connection and issuer-resolver owners acquire tenant work through `WyrdPostgres`; no task-owned production runtime path propagates a raw pool. | CLOSED |
| `FIND-TASK-001-7`, `-22` | Post-provider stamping performs a new evaluated decision, and activation appends the resolved recovery principal's evaluated decision transactionally. | CLOSED |
| `FIND-TASK-001-8`, `-9`, `-16`, `-21` | Previously cited documentation and import sites are corrected. `FIND-TASK-001-25` covers two new R3-added signature sites and does not reopen the old locations. | CLOSED |
| `FIND-TASK-001-10` | Identity journey commands retain minimal features and exact nonzero selection. | CLOSED |
| `FIND-TASK-001-11`, `-12`, `-17` | Keyless boot inventories all secret stores; retirement requires the post-writer pass; active, retained, and signing key files share the restrictive bounded loader. | CLOSED |
| `FIND-TASK-001-13` | Public rejects every present secret and secret methods require present nonempty material. | CLOSED |
| `FIND-TASK-001-15` | Candidate PUT retains authorization before bounded typed semantic decoding. | CLOSED |

The previously rejected host-selected callback-tenant proposal remains outside
TASK-001: it is unchanged from the base, the task excludes that callback
routing decision, and correcting it would require a separate approved
persistent-state/security decision.

## Verification limits

- Static validation only, as assigned. No Cargo-backed or `mise` lane was run;
  committed tests and the task's recorded green evidence were inspected but
  not independently executed.
- No live cloud metadata endpoint, controlled commercial IdP, stalled system
  resolver, proxy environment, or deployed multi-pod topology was exercised.
  The metadata finding is source-proven by exact classification and live caller
  flow; its closure proof is an existing pure policy test and needs no network
  access.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory. Caller tracing used `rg`, the cumulative diff, and direct source
  inspection.
- The twenty-minute sub-review limit did not omit a reviewer: all required Wave
  1 reports were present before this validation began.

## Final disposition

**FIX_REQUIRED** — retain `FIND-TASK-001-24` and `FIND-TASK-001-25`. Both are
bounded corrections within approved behavior; neither requires a product,
public API, architecture, compatibility, concurrency, resource-ownership, or
persistent-data decision.
