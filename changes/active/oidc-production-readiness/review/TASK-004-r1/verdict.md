# TASK-004 R1 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Binding human direction: TASK-003 R1 issuer binding, TASK-003 R2 real
  connection testing, and TASK-003 R5 per-login refresh-chain logout scope.

The candidate remained `HEAD` through discovery, follow-up, validation, and
verdict preparation. `.codegraph/` is absent, so reviewers used the cumulative
Git diff, `rg`, and direct source/caller inspection.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-011 / CLI browser handoff | Server-owned, tenant-bound, expiring, verifier-held, one-use handoff; wrong verifier/tenant, expiry, cancellation, and replay return no credential; browser and CLI outputs expose no token | PASS |
| Poll throttling | The shared auth-route `tower_governor` covers begin, claim, cancel, revoke, callback, and token routes; pending responses also carry a retry interval | PASS |
| REQ-012 credential precedence and intended-tenant selection | Precedence is preserved, but explicit, bearer/API-key environment, workload-routing, and credentials-file authorities do not all bind the caller's tenant selector | FAIL — `FIND-TASK-004-1` |
| Cross-process cache coherence | Locked renewal and durable generations exist, but a fresh process-local bearer bypasses generation/state revalidation | FAIL — `FIND-TASK-004-2` |
| Crash/timeout/lock negative proof | Pending-state consumption is tested; the required production uncertain-timeout transition and lock-timeout path are not | FAIL — `FIND-TASK-004-3` |
| Pending-token sealing | `RefreshPending` serializes the raw refresh token; no approved laptop sealing/key-ownership contract exists from which a bounded correction can be derived | SPEC REVISION — `FIND-TASK-004-4` |
| TLS for secret-bearing CLI exchanges | Direct `TokenExchange` callers accept remote cleartext HTTP even though the existing transport policy permits it only on loopback | FAIL — `FIND-TASK-004-5` |
| First-class Rust, Python, and TypeScript projection | Rust and TypeScript project the shared owner. Python declarations omit `tenant`, and public Bifrost wrappers neither accept nor forward it | FAIL — `FIND-TASK-004-6` |
| Logout and binding TASK-003 direction | Tombstone-before-network holds; server revocation follows only the selected login's refresh chain and preserves other login chains | PASS |
| Canonical audit for CLI logout | Refresh-chain revocation commits without a canonical transactional audit event | FAIL — `FIND-TASK-004-7` |
| Non-goals and binding issuer/connection-test directions | No second IdP app, provider-token authority, workload/human conflation, language-specific credential store, issuer-rule regression, or side-effect connection probe entered the diff | PASS |
| Production Python wheel excludes `wyrd.testing` | `feac127a0` builds the default wheel, imports that exact wheel in an isolated no-project environment as a positive control, and fails if `wyrd.testing` imports | PASS — strengthened, not weakened |

## Independent review results

| Report | Result |
|---|---|
| `task-review-behavior.md` | FAIL — three proposed findings |
| `task-review-invariants.md` | FAIL — four proposed findings |
| `standards-review.md` | PASS — later declaration-parity claim superseded by focused follow-up evidence |
| `maintainer-review.md` | FAIL — Python public declaration mismatch |
| `system-review.md` | FAIL — cache coherence and tenant binding |
| `domain-review-security.md` | FAIL — cleartext secret transport and tenant binding |
| `domain-review-concurrency-durability.md` | FAIL — cache coherence and missing timeout proof |
| `domain-review-data-tenancy.md` | FAIL — tenant binding and logout audit |
| `followup-review.md` | RESOLVED — confirmed Python declaration and public Bifrost runtime gaps |
| `findings-validation.md` | SPEC_REVISION_REQUIRED — seven retained findings; one proposal rejected |

All required reviewers and reports were available. The follow-up was required
because the standards and maintainer reports materially disagreed about Python
runtime/declaration parity. It resolved that conflict from source and direct
typing/runtime evidence. No other discovery conflict remained unresolved.

## Validated finding ledger

- `FIND-TASK-004-1` — CONFIRMED: tenant selection does not bind every
  credential tier.
- `FIND-TASK-004-2` — CONFIRMED: saved-login cache hits bypass durable
  generation/state revalidation.
- `FIND-TASK-004-3` — CONFIRMED: uncertain-timeout and lock-timeout
  transitions lack the task-required proof.
- `FIND-TASK-004-4` — REVISED, SPEC_REVISION_REQUIRED: pending refresh
  authority is stored raw, while local sealing/key ownership, portability,
  recovery, and rotation remain undecided.
- `FIND-TASK-004-5` — REVISED: direct CLI `TokenExchange` paths permit remote
  cleartext secret transport; ordinary assembled SDK transport is narrower
  than the discovery claim.
- `FIND-TASK-004-6` — REVISED: Python stubs omit `tenant`, and public Bifrost
  wrappers also reject the option at runtime.
- `FIND-TASK-004-7` — CONFIRMED: CLI refresh-chain revocation lacks canonical
  transactional audit evidence.

`INV-REV-002` was rejected: the existing shared auth-route governor already
throttles claim polling, so a second limiter or persistent poll-cadence state
would duplicate an existing owner.

The decision-complete source traces, minimum bounded corrections, and focused
closure proofs for every retained finding are preserved in
`findings-validation.md`. No remediation task is emitted because
`FIND-TASK-004-4` first requires human approval of a security and persistent-
data contract.

## Production-wheel boundary determination

The `feac127a0` change strengthens `check:py-wheel-no-testing`. The prior task
inspected the testing-enabled development environment installed by `py:setup`,
not the production artifact. The candidate builds the default-feature wheel,
passes that exact wheel to `uv run --isolated --no-project`, requires
`import wyrd` to succeed, and then requires `import wyrd.testing` to fail.
Build, installation, or positive-import failure remains fatal under `set -e`.
The check therefore cannot pass because Wyrd is absent or because the editable
testing build leaked into the subject. Multiple independent reviewers and the
orchestrator ran the updated task successfully; structured validation recorded
a 36.00-second passing run.

## Verification limits

- The review independently ran `check:py-wheel-no-testing` and `git diff
  --check`; the implementation packet records the complete identity, CLI,
  shared-client, SDK, Python, TypeScript, codegen, boundary, format, and lint
  lanes as passing.
- Those green lanes do not exercise the retained tenant-mismatch, long-lived
  cache, uncertain-timeout, lock-timeout, Python Bifrost projection, or logout-
  audit gaps.
- No review inferred controlled-provider qualification from local mock-provider
  evidence.

## Prior-finding closure

This is the first TASK-004 review, so there are no prior TASK-004 findings to
close. The three supplied TASK-003 human decisions remain satisfied by the
cumulative candidate, including logout revocation of only the selected login's
refresh chain.

## Verdict

**SPEC_REVISION_REQUIRED**

The approved task requires cryptographically sealed pending refresh authority,
but it does not select an implementable laptop key owner or portability,
recovery, rotation, and failure contract. Choosing one would be a new security
and persistent-data decision. After human approval resolves
`FIND-TASK-004-4`, the six bounded findings can be packaged into remediation
under that revised authority.
