---
id: TASK-001-R4
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-004, INV-004, AC-007]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-24, FIND-TASK-001-25]
---

# Close final provider metadata-screening and Rust import gaps

## Authority and immutable review subject

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md`
- Reviewed base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Reviewed candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Reviewed candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-001-r4/findings-validation.md`

Implement this task through `$wyrd-implement`. A later review must reassess the
complete cumulative base-to-candidate range, not only this remediation diff.

## Outcome

Finish TASK-001 by closing the two exact cloud-metadata holes in the shared
provider address policy and bringing the two R3-added signatures into the
repository's required import shape. Reuse the existing classifier, import
blocks, and tests; add no product scope, public contract, dependency,
configuration, or abstraction.

## Issue diagnoses and required corrections

### FIND-TASK-001-24 — permissive OIDC screening admits two cloud metadata endpoints

TASK-001 requires screened provider IO and INV-004 requires tenant-controlled
network behavior to fail closed. Repository security authority requires cloud
metadata and link-local addresses to be blocked in every deployment profile,
while ordinary loopback, private, CGNAT, and ULA addresses may remain available
under the permissive local-provider policy.

The shared OIDC classifier at
`crates/shared/wyrd-auth-oidc/src/screening.rs:229-307` normalizes IPv4-mapped
addresses and always rejects link-local, broadcast, and documentation ranges,
but omits Alibaba metadata `100.100.100.200` and AWS IMDS IPv6
`fd00:ec2::254`. Those addresses are caught only by the profile-dependent
CGNAT/ULA classification, so `AddressPolicy::AllowInternal` admits them.
`ScreenedHttp::client_for` uses this predicate for literal hosts and every DNS
answer on live discovery, candidate probe, login/callback token, and JWKS
paths. An authorized connection administrator can therefore cause blind
server-side requests to cross into cloud metadata control planes on a
permissive deployment.

Correct the existing `is_always_blocked` owner by adding exact comparisons for
`Ipv4Addr::new(100, 100, 100, 200)` and
`Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254)`. Preserve the current
normalization so the IPv4-mapped Alibaba spelling is covered. This matches the
repository's existing gateway classifier and closes the shared root cause.
Do not classify all CGNAT or ULA addresses as always blocked, add caller-level
guards, add a policy knob, or introduce a cross-crate classifier abstraction.

Extend the existing address-policy unit test to prove both policies reject
`100.100.100.200`, `::ffff:100.100.100.200`, and `fd00:ec2::254`, while
neighboring `100.100.100.201` and `fd00:ec2::253` remain allowed only under
`AllowInternal`. This is deterministic policy proof; do not contact a live
metadata service.

### FIND-TASK-001-25 — two new signatures bypass their module import blocks

Repository rules require Rust types in signatures to be imported in the
module's top dependency block and used by bare name. Two R3-added signatures
compile but violate that mandatory source shape:

- `crates/shared/wyrd-auth-oidc/src/screening.rs:245-250` spells the lookup
  future output as `std::io::Result<I>`.
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2429-2441` spells the pool
  parameter as `&sqlx::PgPool`.

Import `std::io::Result as IoResult` in `screening.rs` and use
`Future<Output = IoResult<I>>`. Import `sqlx::PgPool` in the existing
`identity_e2e.rs` import block and use `&PgPool`. These are dependency-manifest
corrections only: do not alter the lookup helper, SQL query, fixture pool,
ownership boundary, or callers, and do not add wrappers or traits.

## Constraints and preserved behavior

- Keep OIDC optional and retain connectionless boot and machine authentication.
- Preserve `AllowInternal` support for ordinary local/private/CGNAT/ULA
  providers; only the exact metadata endpoints and their normalized spellings
  become always blocked.
- Preserve one-resolution address pinning, production HTTPS, proxy and redirect
  refusal, DNS and request deadlines, decoded-body bounds, and redacted errors.
- Preserve the one-Active/one-Candidate lifecycle, `TenantConn`/RLS boundary,
  exact connection provenance, transactional audit, and migration behavior.
- Keep every public route, field, error code, schema, and generated artifact
  unchanged.

## Non-goals

- No shared network-policy crate, classifier consolidation, new dependency,
  resolver abstraction, retry, timeout or address-policy configuration.
- No live cloud metadata request or host-network manipulation in tests.
- No pool, query, fixture, helper, workflow, or unrelated import refactor.
- No UI, CLI login, hosted signup, callback-routing change, compatibility
  surface, new client-auth method, or unrelated documentation sweep.

## Acceptance criteria and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-001-24` | Both address policies reject Alibaba `100.100.100.200`, its IPv4-mapped spelling, and AWS `fd00:ec2::254`; adjacent CGNAT/ULA addresses retain the current `AllowInternal` behavior. | Extend `screening::tests::the_policy_decides_only_the_internal_ranges` and run `mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib -E 'test(=screening::tests::the_policy_decides_only_the_internal_ranges)'`. Retain the existing screening, pinning, proxy, scheme, body-bound, and provider journey coverage. |
| `FIND-TASK-001-25` | Both cited signatures use their module-level imported type names with no behavior change. | Direct source inspection, `mise run fmt`, `mise run lints`, the exact stalled-DNS test, and the filtered tenant-connection rotation journey. |

After focused proof, run the original TASK-001 and prior-remediation coverage:
the filtered administration, rotation, and session-cutoff journeys; the
unfiltered identity journey; `test:principals:integration`; `test:sql`;
`test:platform:journey`; `codegen:check`; `check:tenant-isolation`;
`check:from-pools-allowlist`; applicable client, PyO3, unwrap, Clippy-allow,
fixture, and tonic boundaries; `docs:check`; `fmt`; `lints`; and
`git diff --check`. Keep identity journeys on their intended minimal features
and prove every named selector selects exactly one test.

Run the existing focused regression proofs with exact selectors:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
  -E 'test(=screening::tests::a_stalled_lookup_is_unresolved_at_the_deadline)'
WYRD_IDENTITY_FILTER=tenant_connection_rotation_journey \
  mise run test:identity:journey
```

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-24` Both policies reject `100.100.100.200`, `::ffff:100.100.100.200`, and `fd00:ec2::254`; `100.100.100.201` and `fd00:ec2::253` stay allowed only under `AllowInternal` | `crates/shared/wyrd-auth-oidc/src/screening.rs` `is_always_blocked`: two exact comparisons after the existing `normalize_ip` | `screening::tests::the_policy_decides_only_the_internal_ranges` extended (exact selector, 1 run); full `wyrd-auth-oidc` lib 38/38 | PASS |
| `FIND-TASK-001-25` Both cited signatures use imported bare type names | `screening.rs`: `use std::io::Result as IoResult;` and `Future<Output = IoResult<I>>`; `identity_e2e.rs`: `use sqlx::PgPool;` and `&PgPool` | Source inspection; `mise run fmt`; `mise run lints`; `screening::tests::a_stalled_lookup_is_unresolved_at_the_deadline` (exact selector, 1 run); `WYRD_IDENTITY_FILTER=tenant_connection_rotation_journey mise run test:identity:journey` (1 selected) | PASS |

Accepted reuse findings. Applied:
- `permits_scheme` was inlined into `screen_scheme`.
- The signing-key test was reduced to one owner-only success and one permissive refusal. The sealing-key test already covers the `read_secret_file` matrix.
- The recovery `AuditEvent` now uses `..bearer.clone()`.
- `forbid()` moved into the sourced `scripts/checks/forbid.sh`. A probe forbidden match still fails the check.
- `pg_migration` case (d) no longer has the redundant `consume_active_refresh` assert.

Skipped:
- Finding 1 (inline `browser_authorization_endpoint` and replace `login::destination_tests`). It would remove the only login-path proof of the R3 `FIND-TASK-001-20` criterion.
- Finding 3 (drop `bounded_lookup`, its paused-clock test, and `test-util`). This remediation requires that stalled-DNS test as focused proof.

All of these exited 0 in this session:
- `mise run fmt` and `mise run lints`
- `git diff --check`
- `mise run codegen:check` and `mise run docs:check`
- `mise run test:principals:integration`, `mise run test:sql`, and `mise run test:platform:journey`
- `WYRD_IDENTITY_FILTER=<name> mise run test:identity:journey` for `tenant_connection_admin_journey`, `tenant_connection_rotation_journey`, and `tenant_connection_session_cutoff_journey`. Each selected exactly one test.
- Unfiltered `mise run test:identity:journey`: 23/23
- The Postgres-wrapped `wyrd-auth` lib suite: 119/119
- `check:tenant-isolation`, `check:from-pools-allowlist`, `check:fixtures-no-server`, `check:no-tonic-outside-wyrd-tonic`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, and `check:clippy-allow-audit`

Non-goals stayed out of scope. There is no new public contract, route, dependency, configuration, policy knob, or classifier abstraction, and no test contacts a live metadata service.
