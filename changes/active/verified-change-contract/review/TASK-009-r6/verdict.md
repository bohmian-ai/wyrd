# TASK-009 round-six verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E
  (superseding TASK-009-R4), and TASK-009-R5
- Scope: complete cumulative base-to-candidate range

The candidate remained `HEAD` throughout discovery, focused follow-up,
validation, and verdict preparation. The reviewed source was not modified. No
`.codegraph/` directory exists, so repository search and direct source
inspection were used. Every required independent report is present; no missing
reviewer or report was converted into a verification limit.

## Reconciled acceptance matrix

| Obligation | Reconciled source and proof | Result |
|---|---|---|
| Root-default and local initial Card selection use the shared Rust owner and mint one UUIDv7 invocation | `WyrdState::run`, `run_for_card`, `Run::new`; shared and language-surface tests | PASS |
| Immutable sibling views preserve the invocation while selecting exact hydrated Cards; unknown aliases fail locally | `Run::for_card`, shared alias lookup, Rust/Python/TypeScript assertions | PASS |
| Rust, Python, and TypeScript expose the approved Run APIs with declaration parity | Shared owner, thin projections, generated declarations, typing and journey evidence | PASS |
| Python entry and exit maintain the approved token-free execution-local tuple stack across nesting, awaits, copied tasks, and concurrent use of the same immutable Run | Import-time OTel key, top-matching attach-only pop, deterministic focused tests | PASS |
| Active and child spans receive the exact CardRef and Run ID without nested active-span overwrite | `_enter_run`, `_RunCorrelationProcessor`, focused span tests and persisted journey | PASS |
| Registration outcomes are weakly tracked by provider identity; equal providers do not alias; failed retained processors remain inert | Identity-based weak outcome entries and per-attempt activation; focused R5 regressions | PASS |
| Optional provider integration remains fail-open and cannot block Run entry or application execution | The provider callback executes while `_outcomes_lock` is a non-reentrant `Lock`; same-thread callback re-entry deadlocks | **FAIL — `FIND-TASK-009-14`** |
| Missing/API-only provider, registration, attach, enrichment, and exit-update failures do not block explicit observations or mask user exceptions | Focused failure cases and source separation | PASS |
| Authenticated stock OTLP/HTTP persists trace/custom/Eval joins with server-derived identity | Existing extended Python journey and immutable implementation evidence | PASS |
| Context exit owns no flush, shutdown, exporter lifecycle, network IO, or durability acknowledgement | Exit source and separately ordered journey barriers | PASS |
| Changed architecture and task artifacts identify revision 46 and correct lifecycle state | `run_api.md` revision 46; replaced R4 marked `superseded`; R4E remains authoritative | PASS |
| Every named Rust test has exact repository-native replayable command evidence | TASK-009 and R5 record both shared tests and the fully wrapped SDK journey | PASS |
| No server Run, second pipeline, wrapper span, process-global Card scope, required OTel dependency, client-managed identity, or log/metric promise entered the diff | Cumulative source and dependency inspection | PASS |

## Independent results and follow-up

| Required role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | PASS |
| Invariants | `task-review-invariants.md` | FAIL — `INV-R6-001` proposed |
| Repository standards | `standards-review.md` | PASS |
| Maintainer | `maintainer-review.md` | PASS |
| System resilience | `system-review.md` | PASS |
| Telemetry domain | `domain-review-telemetry.md` | PASS |
| Concurrency domain | `domain-review-concurrency.md` | FAIL — `CONC-R6-001` |
| Focused follow-up | `followup-review.md` | RESOLVED |
| Fresh Ponytail validation | `findings-validation.md` | one retained finding |

The focused follow-up was required because discovery disagreed on two provider
paths. It confirmed the reentrant-registration deadlock and rejected the
failed-global-provider active-span claim: revision 46 independently requires
provider-agnostic stamping of an eligible active span, while terminal provider
failure governs the registration-owned processor. Independent validation
confirmed that resolution.

## Validated finding ledger

| Stable ID | Discovery sources | Status / classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-009-14` | `CONC-R6-001`; focused follow-up resolution 1 | CONFIRMED / INCORRECT | Make the existing provider-outcome critical section safely reentrant so a provider callback can observe the provisional result without a second provider call or deadlock. |

The exact producer-to-consumer trace, reachability, rejected proposal,
correction boundary, and focused closure proof are preserved in
`findings-validation.md`.

## Prior-finding closure

| Prior IDs | Result |
|---|---|
| `FIND-TASK-009-1` through `FIND-TASK-009-8` | CLOSED or SUPERSEDED AND CLOSED under revision 46. |
| `FIND-TASK-009-9` | CLOSED — distinct equal provider objects have independent weak identity outcomes. |
| `FIND-TASK-009-10` | CLOSED — a processor retained by failed registration remains inert. |
| `FIND-TASK-009-11` | CLOSED — Run authority identifies revision 46. |
| `FIND-TASK-009-12` | CLOSED — replaced TASK-009-R4 is `superseded`. |
| `FIND-TASK-009-13` | CLOSED — exact replayable Rust command evidence is recorded. |

`INV-R6-001` does not reopen finding 10 because direct active-span stamping is
an independently required entry operation and is not performed by the failed
provider's retained processor.

## Verification and limits

Independent reviewers reran the complete focused Python observation surface
successfully (`39 passed`); the system reviewer also reran four provider-focused
tests successfully. The concurrency reviewer and focused follow-up reproduced
the remaining defect in a bounded fresh process: callback re-entry timed out
with exit status 124. The cumulative `git diff --check` passed. Broader Rust,
Python, TypeScript, journey, typing, generation, boundary, format, and lint
results were inspected from the immutable task and remediation records rather
than redundantly rerun in every review.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-009-14` is a bounded correction in the existing Python provider
registration owner. It requires no new product, public API, architecture,
security, compatibility, cross-service, persistent-data, provider-lifecycle,
or concurrency-semantics decision.

Remediation: `TASK-009-R6-reentrant-provider-registration.md`.
