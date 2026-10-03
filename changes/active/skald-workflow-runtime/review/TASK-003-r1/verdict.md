# TASK-003-r1 verdict

**FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `a1792c45323489157e818eb29722913114014927`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12.
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`.
- Review directory: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`.

The human's 2026-10-03 approval of the minimal `pub model: ModelRef` amendment to `WyrdGatewayCall` is authoritative; the candidate's corresponding specification update is approved. It is not DRIFT. The complete base-to-candidate range was reviewed. Candidate source and tracked authority remained unchanged throughout review; the later commit contains review artifacts only and does not redefine this candidate.

The resumed review follows the human's source-only direction: source, cumulative diff, and the implementer's recorded task evidence are the acceptance inputs. No build, test, lane, Cargo, mise, package-manager, or other verification command was executed after resumption. Earlier discovery reports preserve historical reviewer executions, but those executions are excluded from this verdict's proof. No verification process started by this review remained running on resumption. Report writing and the explicitly requested review-directory commit are the only authorized mutations.

## Required independent reports

| Role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | FAIL |
| Invariants | `task-review-invariants.md` | FAIL |
| Repository standards | `standards-review.md` | FAIL |
| Maintainer | `maintainer-review.md` | FAIL |
| System resilience | `system-review.md` | FAIL |
| Security/credentials | `domain-review-security.md` | FAIL |
| Concurrency/cancellation | `domain-review-concurrency.md` | FAIL |
| Focused follow-up | `followup-review.md` | RESOLVED |
| Independent Ponytail validation | `findings-validation.md` | Five bounded retained findings |

All required reports are complete. The original interrupted follow-up agent left no report and was no longer live; a fresh independent replacement completed that role. Its report was returned to the orchestrator for preservation under the reviewer's file-edit restriction. A separate fresh Ponytail reviewer then independently validated every discovery and follow-up proposal. No required role or report is unavailable.

## Claim comparison and follow-up decision

Discovery claims grouped into Python owner/context loss, ambiguous native 401 replay, recognized-code message trust, unused public plaintext-secret surface, and function-local imports. Matching claims were corroboration rather than validation.

Follow-up was required because replay recommendations conflicted (code-gated replay versus no resend), and message-redaction recommendations targeted different owners (shared gateway relay versus new client normalization). Source establishes that native provider bodies can impersonate catalog codes; codes cannot prove pre-dispatch origin. Altering ordinary native relay would change sibling behavior. Validation selected existing `AuthMiddleware` renewal without model POST replay, and catalog-derived messages at the client normalization boundary. The private task retry wording has a same-spec task-correction route through `wyrd-plan`; approved authority does not mandate same-call replay or new provenance machinery. No material spec decision is required by the selected corrections.

## Reconciled acceptance matrix

PASS rows use source plus the implementer's recorded evidence in TASK-003:284-369. Green results do not override contradicted source or cover omitted scenarios.

| Obligation | Implementation evidence | Recorded proof and reconciliation | Result |
|---|---|---|---|
| REQ-024/031/045/046, AC-019: shared exact create/get/cancel/wait, direct DTOs, stable idempotency, fixed polling and drop semantics | `workflow/remote.rs`; existing shared submission/HTTP owners | `shared_workflow_client_contract`, shared family evidence; real lifecycle journey remains TASK-004/005 under task scope | PASS |
| Rust exports and Python-free first-class client boundary; INV-007 | Shared/Rust SDK exports and manifests | Recorded SDK/client-tier/PyO3 checks | PASS |
| REQ-036A/038/039/043, INV-020: immutable model/fallback/deadline/cancellation/correlation; existing public dialects and Vertex refusal | Skald plan/route; public gateway caller | Recorded caller context test and Skald 12-test suite; approved model amendment honored | PASS |
| Native call refusal handling preserves one governed model submission and renewal ownership | `transport/http.rs:365-401`; provider-refusal producer/relay | 401 omitted from recorded focused refusal cases; source shows unconditional resend | FAIL — FIND-TASK-003-2 |
| REQ-043, INV-012: only safe common error fields, catalog/category projection, no arbitrary upstream text | `workflow/gateway.rs:155-209`; native relay; Workflow error projection | Recognized-code tests retain message, unknown-code canary tests do not cover spoofing | FAIL — FIND-TASK-003-3 |
| AC-011A: authentication before fallback interpretation, limits/encoding/validation, typed forwarding and non-forwarding to providers; no override preserves tenant policy | `gateway/policy.rs`; server ingress/routes | Recorded PG ingress module and focused test; source trace supports ordering | PASS |
| AC-012: served OpenAPI/header contract; generated contracts | Four operation annotations and served OpenAPI test | Recorded `test:principals:integration` (20), codegen evidence | PASS |
| REQ-058, INV-004/011/012: shared route selection, selected-only secret preparation at run, no load/apply dispatch, explicit native injection | `workflow/local.rs`, config and shared facade | Recorded selected-dependency test and relevant families | PASS |
| REQ-058, AC-029/031, INV-007: SDK consumer closure retains originating Cards context | Shared Workflow retains client; Python unwraps it and later reconstructs context-free owner | Rust test cannot prove Python handoff; recorded Python checks omit conflicting ambient context | FAIL — FIND-TASK-003-1 |
| Shared env/file reader preserves gateway/server/client file checks and domain errors | `wyrd-utils/secret.rs` and three redacted consumers | Recorded gateway/server/shared suites; public plaintext helper now has no external consumer | FAIL for unnecessary public surface — FIND-TASK-003-4 |
| Mandatory repository source rules | Three function-local import sites | Source contradicts required module import boundary despite green recorded lints | FAIL — FIND-TASK-003-5 |
| Non-goals: no new ingress/Vertex endpoint, credential mutation, provider-body context, polling knob, arbitrary-header public API, language remote lifecycle, duplicate engine/graph/transport | Complete cumulative diff and callers | Source review; task-recorded boundary evidence | PASS |
| AC-013 proof sufficiency for this task | Existing transport/PG/OpenAPI/language tests | Three reachable runtime failures lack focused closure proof; implementation must add/run/record it | FAIL — FIND-TASK-003-1/2/3 |

## Validated finding ledger

The full independent evidence, source dispositions, caller tracing, correction boundaries, and closure proof are in `findings-validation.md`.

| ID | Status/classification | Discovery IDs | Required outcome |
|---|---|---|---|
| FIND-TASK-003-1 | CONFIRMED / INCORRECT | B-001, INV-REV-001, MAINT-TASK-003-1 | Retain the complete shared Workflow owner and Cards client through Python loading, mutation and execution. |
| FIND-TASK-003-2 | REVISED / INCORRECT | B-002, SYS-001, CONC-001, SEC-002, FU-001 | Renew refused credentials through existing auth owner, propagate renewal failure, and return original native refusal without resending model POST. |
| FIND-TASK-003-3 | REVISED / INCORRECT, VIOLATION | INV-REV-002, SEC-001, RR-001, FU-002 | Use trusted catalog text for recognized-code portable messages; preserve code/status/field/remediation and sibling native relay. |
| FIND-TASK-003-4 | REVISED / DRIFT, VIOLATION | RR-002 | Privatize unused plaintext file-reading stage; keep existing redacted SecretRef boundary. |
| FIND-TASK-003-5 | CONFIRMED / VIOLATION | RR-003 | Move imports to their owning module blocks under existing cfg restrictions. |

No rejected or unvalidated correction enters the remediation. Code-gated native replay, producer-level rewriting of ordinary native refusal bodies, novel provenance markers, new checks/settings/options, and new test harnesses are excluded. The human's established-standard direction governs all retained corrections; removal and reuse suffice.

## Evidence limits and prior closure

The task records successful focused, family, SDK, gateway, PG ingress, served OpenAPI, language typing/unit/declaration, codegen, boundary, format and lint commands. Those records are credible for exercised behavior, but source contradicts their blanket Python-context and safe-error claims. Missing Python context-retention, provider-origin/forged-code 401, and recognized-code canary cases are findings requiring implementer proof, not permission for review to run commands. Import/visibility corrections need static proof and ordinary implementation checks; no bespoke checks or runtime tests are required solely for those edits.

This is the first TASK-003 verdict. No prior stable finding IDs or remediation closure are claimed. TASK-004/005 proof explicitly assigned by the original task remains outside this task's acceptance boundary.

## Remediation route

`TASK-003-R1-preserve-client-context-and-native-call-safety.md` packages all five validated findings for `$wyrd-implement`. Reconcile the private native-retry instruction through `$wyrd-plan` under the same approved Revision 12 before relying on it. Review does not implement that correction. The next review must reassess the cumulative original base-to-new-candidate range and retain these finding IDs.
