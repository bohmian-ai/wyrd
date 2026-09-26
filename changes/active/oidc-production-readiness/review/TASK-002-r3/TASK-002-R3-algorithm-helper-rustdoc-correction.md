---
id: TASK-002-R3
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
parent_task: TASK-002
remediates: [FIND-TASK-002-10]
---

# Correct the tenant ID-token algorithm helper's rustdoc

## Authority and immutable subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks:
  `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-002-r2/TASK-002-R2-repository-rule-corrections.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `d861845f3f5d89aca413857dcfb8c1bbfaee349d`
- Validated finding: `FIND-TASK-002-10`

## Issue diagnosis

### FIND-TASK-002-10 — Algorithm-policy helper documents a filter it no longer performs

The repository requires accurate substantive rustdoc for every new or
materially modified Rust item, including a fallible function's actual error
conditions. The authorized reuse cleanup in commit `3747b2d26` correctly
removed a duplicate HMAC rejection from
`crates/wyrd/wyrd-auth/src/callback.rs::verify_id_token_algorithm`, because
the sole production caller immediately invokes
`ExternalVerifier::verify_external_against`, which rejects `HS256`, `HS384`,
and `HS512` before `kid` extraction or JWKS lookup.

The helper's existing summary and `# Errors` section still say that the helper
itself requires an asymmetric provider-advertised algorithm and rejects values
outside an advertised asymmetric set. Its body actually decodes the token
header, parses the provider-advertised algorithms, and succeeds whenever the
header algorithm is in that set, including an advertised symmetric algorithm.
The composed callback remains secure, but the helper's own documented contract
is false. A maintainer can therefore reuse or change the reachable trust-boundary
helper based on a guarantee it does not supply, and the candidate fails the
repository's hard documentation rule despite green runtime tests.

## Intended correction outcome

The helper's documentation accurately describes only the check its body owns,
while the composed callback documentation remains clear that the shared
external verifier owns symmetric-algorithm rejection. Executable behavior,
callers, public contracts, and security policy remain unchanged.

## Decision-complete recommendation

Revise only the existing rustdoc on `verify_id_token_algorithm` at
`crates/wyrd/wyrd-auth/src/callback.rs:568-585`:

- describe the operation as checking that the decoded ID-token header
  algorithm occurs in the provider's advertised supported set;
- state the actual errors: a malformed or undecodable JWT header, an
  unparseable advertised algorithm value, or a header algorithm absent from
  the advertised set; and
- identify the immediately following
  `ExternalVerifier::verify_external_against` call as the owner of symmetric
  algorithm rejection and signature/JWKS verification.

Keep the implementation and shared verifier unchanged. This is the minimum
correction because restoring the local HMAC filter would duplicate the shared
security owner and undo the authorized reuse cleanup; deleting or restructuring
the helper would introduce executable churn to close a documentation defect.
No dependency, abstraction, configuration, or new test harness is needed.

## Constraints and preserved behavior

- Preserve the complete tenant callback control flow and the current order of
  advertised-set, shared-verifier, nonce, authorized-party, connection,
  identity, role, audit, issuance, sealing, and commit checks.
- Preserve `ExternalVerifier::verify_external_against` as the single shared
  owner of symmetric-algorithm rejection.
- Preserve the closure of `FIND-TASK-002-1` through `FIND-TASK-002-9`, including
  the runtime closure of advertised asymmetric verification in
  `FIND-TASK-002-4`.
- Do not change code, visibility, callers, errors, tests, schemas, generated
  artifacts, dependencies, or public APIs.
- Do not restore the duplicate HMAC filter or add another policy helper.
- Do not implement TASK-003 BFF or TASK-004 CLI handoff behavior.

## Non-goals

- Redesigning provider algorithm policy or the shared external verifier.
- Adding runtime coverage for unchanged executable behavior.
- Refactoring callback orchestration or moving the helper.
- Broad documentation cleanup outside the cited rustdoc.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| `verify_id_token_algorithm` documents advertised-set membership rather than claiming it independently enforces asymmetry. | `FIND-TASK-002-10` |
| Its `# Errors` section names the actual malformed-header, invalid advertised-value, and absent-from-advertised-set failures. | `FIND-TASK-002-10` |
| The rustdoc identifies `ExternalVerifier::verify_external_against` as the subsequent symmetric-algorithm rejection owner without changing either body or caller. | `FIND-TASK-002-10` |
| The remediation diff contains no executable, contract, test, generated-artifact, dependency, or unrelated documentation change. | `FIND-TASK-002-10` |

## Focused proof and broader verification

Static inspection must compare the corrected rustdoc with the unchanged
complete bodies of `verify_id_token_algorithm`, its sole caller
`AuthorizationCodeExchange::finish_id_token_exchange`, and
`ExternalVerifier::verify_external_against`. Then run:

- `mise run fmt`
- `mise run lints`
- `git diff --check`

No new runtime test is warranted because executable behavior must not change
and the existing advertised-algorithm refusal tests already prove the composed
security boundary. If implementation or generated output changes, remove that
drift rather than expanding this remediation.

## Implementation evidence

Remediation commit `176905ddc` (rustdoc only). Lead-directed items are
separate commits: `3b3c0a6b7` (item 2) and `97f2c8e24` (item 3).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `verify_id_token_algorithm` documents advertised-set membership rather than claiming it independently enforces asymmetry. | `crates/wyrd/wyrd-auth/src/callback.rs` `verify_id_token_algorithm` summary; the caller `finish_id_token_exchange` summary no longer says "asymmetric" either | Static comparison with the unchanged bodies of the helper, `finish_id_token_exchange`, and `wyrd-auth-verify` `verify_external_against` (HS256/384/512 refused before `kid`/JWKS) | PASS |
| Its `# Errors` section names the actual malformed-header, invalid advertised-value, and absent-from-advertised-set failures. | `# Errors` names a malformed/undecodable header and an absent-from-set algorithm, and states that an unparseable advertised value is skipped, not raised, so it can only contribute to the absent-from-set failure (matches the `filter_map` body) | Static inspection | PASS |
| The rustdoc identifies `ExternalVerifier::verify_external_against` as the subsequent symmetric-algorithm rejection owner without changing either body or caller. | Intra-doc link ``[`ExternalVerifier::verify_external_against`]`` plus a link to the sole caller | `cargo doc -p wyrd-auth --no-deps`: both new links resolve with no warnings; `git show 176905ddc` touches only `///` lines | PASS |
| The remediation diff contains no executable, contract, test, generated-artifact, dependency, or unrelated documentation change. | `git show --stat 176905ddc`: one file, rustdoc lines only | `mise run fmt`, `mise run lints`, `git diff --check HEAD~3 HEAD` all exit 0 | PASS |
| Lead item 2: `terminal_json` uses `let … else` instead of `map_or_else(panic)`. | `crates/wyrd/wyrd-gateway/src/adapter/tests.rs` `terminal_json` (`3b3c0a6b7`) | `cargo nextest run --locked -p wyrd-gateway --lib -E 'test(/^adapter::tests::/)'`: 39 passed; `mise run lints` exit 0 | PASS |
| Lead item 3: an identity journey case where the provider advertises HS256 proves that the shared verifier's rejection is the guard. | `tenant_callback_refusal_journey` step 5 (`97f2c8e24`): discovery advertises `["EdDSA", "HS256"]` and an HS256 token passes the advertised-set check, then gets `401 WYRD_AUTH_401_INVALID_TOKEN` with no completion. An HS256-only advertisement is refused earlier at discovery (`503`, no asymmetric algorithm), so the mixed set is the only reachable case. It shares one loop with the existing unadvertised case. | `WYRD_IDENTITY_FILTER=tenant_callback_refusal_journey mise run test:identity:journey` exit 0; full `mise run test:identity:journey` exit 0 (27 passed) | PASS |

Non-goals held: no executable, caller, error, schema, generated, or dependency
change in the remediation commit; no HMAC filter restored; no TASK-003/004
work; the human-connection claim-type follow-up was not done.
