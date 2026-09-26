---
id: TASK-002-R8
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-007, REQ-008, INV-002, INV-004, AC-003, AC-007]
depends_on: [TASK-002-R7]
parent_task: TASK-002
remediates: [FIND-TASK-002-16, FIND-TASK-002-17]
---

# Correct OIDC Subject Identifier validation and CLI rustdoc

## Authority and immutable review subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r8/verdict.md`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-002-r8/findings-validation.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`

This task closes two bounded defects without changing the approved login,
identity, workload, persistence, or public contracts.

## Issue diagnoses and required outcomes

### FIND-TASK-002-16 — false rustdoc on the refresh token printer

The materially modified private token-output helper in
`crates/wyrd/wyrd-cli/src/auth/refresh.rs` prints a returned access token,
optional refresh token, and expiry. Its first rustdoc sentence instead says it
performs argument parsing. That sentence moved away from the parser tests and
now describes the wrong item. The accurate output-oriented rustdoc already
present is sufficient.

The current behavior is not defective, but the candidate violates the hard
repository rule that materially modified Rust items carry accurate,
substantive documentation. A maintainer is directed to the wrong workflow
responsibility.

**Required outcome:** remove only the stale argument-parsing claim. Preserve
the accurate output documentation and all executable behavior.

**Recommendation:** delete the false sentence from the existing item. Do not
rename, move, expose, split, or behaviorally test the helper; direct source
inspection and the repository format/lint checks are the proportionate proof.

### FIND-TASK-002-17 — invalid OIDC Subject Identifiers pass verification

The generic external-token mapping accepts any JSON string at the configured
subject path. The OIDC-specific ID-token verifier adds `iat` validation but
does not enforce the OpenID Connect Subject Identifier syntax: `sub` must be
nonempty ASCII and no longer than 255 bytes. Both tenant and platform OIDC
login paths use the returned subject as their external durable identity. An
empty, non-ASCII, or oversized signed value therefore reaches identity
persistence or pinning instead of failing authentication.

Missing and non-string subjects already fail. Workload assertions deliberately
use the generic external-token path and have a separate configurable subject
mapping contract, so widening the restriction into the mapper or generic
verifier would alter adjacent behavior without authority.

**Required outcome:** every OIDC ID-token verification rejects an empty,
non-ASCII, or greater-than-255-byte `sub` before returning verified claims,
while a normal subject continues to pass. Tenant and platform login inherit
the same fail-closed result from their existing shared verifier. Generic
workload assertion behavior remains unchanged.

**Recommendation:** place the validation in the existing OIDC-specific
`ExternalVerifier::verify_id_token_against` owner and use Rust's native string
operations. Update that owner's error documentation to cover the new refusal.
Do not add a new type, helper, dependency, downstream callback guard,
persistence rule, or public error contract. Extend the existing focused
ID-token claims test; a separate callback/Postgres proof would duplicate a
branch that returns before either identity owner is invoked.

## Preserved behavior and constraints

- Preserve issuer, audience, signature, key, expiry, `nbf`, numeric non-future
  `iat`, nonce, `azp`, advertised-algorithm, and screened-provider checks.
- Preserve exact `(issuer, subject)` identity and email non-linking once the
  OIDC subject is valid.
- Preserve the generic workload assertion subject-mapping contract and all
  machine/platform/tenant authority separation.
- Preserve callback state consumption, connection binding, tenant RLS,
  transaction ownership, audit coupling, session issuance, refresh-family
  serialization, and sealed completion behavior.
- Preserve CLI token output exactly.
- Add no dependency, Cargo feature, migration, public route, wire/schema
  change, compatibility path, new abstraction, or alternate validation owner.
- Keep all prior corrections for `FIND-TASK-002-1` through
  `FIND-TASK-002-15` closed.

## Explicit non-goals

- Do not attempt to detect an issuer that reuses a syntactically valid subject
  across provider accounts; Wyrd cannot infer issuer-side account identity.
- Do not change workload subject syntax or configurable claim paths.
- Do not redesign external identity storage or introduce a subject newtype.
- Do not add callback-specific duplicate validation or persistence tests.
- Do not refactor the CLI refresh command or its output helper.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| The refresh token-output helper's rustdoc accurately describes only its token-printing role, with the stale argument-parsing sentence absent and behavior unchanged. | `FIND-TASK-002-16` |
| OIDC ID-token verification rejects missing, non-string, empty, non-ASCII, and 256-byte `sub` values, and accepts a normal subject. | `FIND-TASK-002-17` |
| The validation is owned by the existing OIDC-specific verifier; generic external/workload verification semantics remain unchanged. | `FIND-TASK-002-17` |
| The verifier's rustdoc/error contract accurately names the Subject Identifier refusal. | `FIND-TASK-002-17` |
| Normal tenant login, identity journeys, prior concurrency fixes, and all prior finding closures remain green. | Both |

## Focused proof and broader verification

Extend the existing
`tests::oidc_id_token_requires_binding_and_time_claims` proof to cover a normal
subject plus missing, non-string, empty, non-ASCII, and 256-byte subjects. Run
its exact selector:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims)'
```

Then run the affected broader proof sequentially:

```bash
mise run test:principals:unit
mise run test:identity:journey
mise run fmt
mise run lints
git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..<new-candidate>
```

Directly inspect the corrected CLI rustdoc. Record the expected focused RED
for an admitted invalid subject, the GREEN result, and the final verification
in this task's implementation-evidence section. If the exact test name has
changed, confirm the current selector with `mise exec -- cargo nextest list`
and record the equivalent non-weaker command.

## Material stop conditions

Stop for renewed specification authority if correctness requires changing the
public authentication contract, workload subject semantics, durable identity
key, persistence schema, error catalog, or another security decision beyond
the validated OIDC syntax rule. Ordinary local implementation and test details
inside the existing verifier owner remain implementer-owned.

## Implementation evidence

Status: `IMPLEMENTED`

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Refresh token-output rustdoc describes only token printing; behavior unchanged | `crates/wyrd/wyrd-cli/src/auth/refresh.rs` `print_tokens`: stale "Argument parsing" line deleted; body untouched | Direct inspection; `mise run fmt`, `mise run lints` | PASS |
| OIDC ID-token verification rejects missing, non-string, empty, non-ASCII, 256-byte `sub`; accepts normal subject | `crates/shared/wyrd-auth-verify/src/lib.rs` `ExternalVerifier::verify_id_token_against` subject check (`is_empty`, `is_ascii`, `len() > 255`) | `tests::oidc_id_token_requires_binding_and_time_claims` extended with the five refused cases plus a 255-byte accepted subject; RED before fix (`empty sub must be refused: Ok(..)`), GREEN after | PASS |
| Validation owned by the OIDC-specific verifier; generic/workload semantics unchanged | Check lives only in `verify_id_token_against`; `verify_external_against` and `map_claims` untouched | Same test still proves workload assertion without `iat` passes generic entry | PASS |
| Verifier rustdoc/error contract names the Subject Identifier refusal | Summary and `# Errors` of `verify_id_token_against` | Direct inspection | PASS |
| Tenant login, identity journeys, prior fixes remain green | No other files changed | `mise run test:principals:unit` (exit 0); `mise run test:identity:journey` (27/27 journey tests, exit 0) | PASS |

Commands (all exit 0, run sequentially):

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims)'
mise run test:principals:unit
mise run test:identity:journey
mise run fmt
mise run lints
git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..HEAD
```

Non-goals held: no new type, helper, dependency, feature, migration, route,
error code, callback guard, workload subject change, or CLI refactor. Only the
two source files and this record changed.
