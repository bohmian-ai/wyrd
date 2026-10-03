# TASK-003-r6 verdict

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R5 and their prior verdicts and
  validated ledgers

The review covered the complete base-to-candidate range. The candidate remained
the stated commit throughout discovery, follow-up, and validation. The
repository has no `.codegraph/` directory, so reviewers used Git and ordinary
source navigation.

This review was strictly source-only. No reviewer built or compiled the
repository or ran tests, Cargo, mise, pnpm, pytest, lints, formatting, code
generation, test listing, or any verification lane. Recorded task evidence was
assessed as historical evidence and compared with candidate source; it was not
rerun.

## Independent review results

| Required report | Result | Material proposal |
|---|---|---|
| `task-review-behavior.md` | PASS | None |
| `task-review-invariants.md` | PASS | None |
| `standards-review.md` | PASS | None |
| `maintainer-review.md` | FAIL | `MNT-R6-001`: the public PyO3 `Workflow.run` documentation retains a false local-only headline and omits the run-start timing stated by both shipped stubs |
| `system-review.md` | PASS | None |
| `domain-review-security.md` | PASS | None |
| `domain-review-concurrency.md` | PASS | None |

All required discovery reports were present and complete. Security/auth and
concurrency/lifecycle received separate domain reviews because the cumulative
candidate changes those sensitive boundaries.

## Claim comparison and follow-up decision

The maintainer proposal conflicted materially with the behavior, invariant,
and repository-standards acceptance of the same Python public-contract area.
A fresh focused reviewer therefore traced the PyO3 method, shared-client
delegate, native-module registration, public Python re-export, hand-authored
stub, assembled stub, and R5 authority.

`followup-review.md` returned **RESOLVED** and independently proposed
`FUP-R6-001`: the PyO3 method's first sentence is materially false for Wyrd and
external gateway routes, and its selected-secret sentence omits the approved
run-start timing. The issue is reachable through public runtime help.

## Reconciled acceptance matrix

| Obligation | Reconciled result |
|---|---|
| Shared remote create/get/cancel/wait facade, stable idempotency, snapshot returns, fixed polling, and drop behavior | PASS |
| Per-attempt model, fallback, deadline, cancellation, and correlation isolation across supported public dialects | PASS |
| Native model send-once behavior and every-known-`401` renewal before body collection, without replay | PASS |
| Safe portable error projection with catalog-owned recognized-code text | PASS |
| Authenticated, bounded fallback-header decoding, refusal, consumption, and non-forwarding | PASS |
| Retained loading-client authority, route-first local dependency assembly, selected-secret-only resolution, and Native injection | PASS |
| One blocking run-start `GlobalConfig` snapshot feeds both client-less mixed-route consumers | PASS |
| Loading and apply read no execution secret and dispatch nothing | PASS |
| Active architecture records the fallback-header contract | PASS |
| Shared Rust run-start operations accurately document ambient IO, cancellation, and partial progress | PASS |
| Python public runtime and typed declarations describe one route-selected execution contract | **FAIL — `FIND-TASK-003-15`** |
| No new ingress, credential mutation API, arbitrary-header API, language-specific lifecycle, duplicate engine/transport/configuration owner, bespoke check, or compatibility path | PASS |
| Standing DRIFT rule: no mechanism, check, file, setting, or option unsupported by established standards and comparable widely used projects | PASS; the correction expressly forbids such additions |

## Validated finding ledger

The fresh structured Ponytail reviewer independently inspected all discovery
and follow-up proposals and produced one deduplicated finding:

| Stable finding | Status | Classification | Required correction |
|---|---|---|---|
| `FIND-TASK-003-15` | REOPENED / REVISED | INCORRECT / VIOLATION | At the existing PyO3 `Workflow.run` owner, replace the obsolete local-only opening sentence with the route-selected wording already shipped in both stubs, and state that selected external secrets are read at run start. Preserve runtime behavior, signatures, stubs, and assembly. Add no checker, test harness, generator, declaration file, setting, option, or runtime mechanism. |

The validator consolidated `MNT-R6-001` and `FUP-R6-001` under the existing
stable ID because they identify incomplete closure of the same R5 Python
public-contract obligation. No proposal was rejected, no disagreement remains,
and the correction requires no new product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision.

## Verification limits and prior-finding closure

Historical evidence recorded in TASK-003 and R1-R5 is specific and corresponds
to the candidate source, including the remote client contract, fallback policy
and authenticated ingress, native send-once renewal paths, post-dispatch
cancellation, retained Python client context, mixed-route single-snapshot path,
generated declarations, and ordinary repository checks. It was not rerun under
the caller's strict read-only instruction.

`FIND-TASK-003-1` through `FIND-TASK-003-14` are closed in the cumulative
candidate. `FIND-TASK-003-15` is reopened only because the public PyO3 runtime
method still contradicts the corrected hand-authored and assembled stubs. The
defect and its closure are directly inspectable in source.

## Verdict

**FIX_REQUIRED**

The cumulative implementation behavior is otherwise accepted, but the
validated ledger is not empty. Apply
`TASK-003-R6-correct-python-runtime-help.md` through `$wyrd-implement`, then
review the complete original base-to-new-candidate range again.
