# TASK-007 verdict: FIX_REQUIRED

## Immutable subject and round

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Approved specification: `changes/active/github-verifier-pipelines/spec.md`, revision 7, with the task's explicit maintainer amendments.

This is the first review of the entire supplied base-to-candidate diff (92 files). The base is also the merge base, so the prescribed three-dot range and requested `git diff 16ea9694c HEAD` coincide. No prior findings, closure, or acceptance evidence is carried forward; no convergence assessment applies. HEAD remained fixed throughout review and tracked source is unchanged. Only this new review directory was written.

Approved decisions remain unchanged: park refunds the attempt; the CLI has one `--file` flag with its documented JSON/media limitation; AC-007 proves ordinary restart/retry/refusal outcomes without new test-only crash hooks. No finding requires a new specification or architecture decision.

## Independent coverage and results

[scope.md](scope.md) records the initial common navigation map, authority and selection rationale. [findings-validation.md](findings-validation.md) independently validates every required role's applicability and coverage, all proposals, correction burden and acceptance reconciliation.

| Required independent role | Report | Discovery result / validated scope |
|---|---|---|
| behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL; comprehensive full-task obligations and assertions |
| invariant | [task-review-invariants.md](task-review-invariants.md) | PASS within shared lifecycle/state scope; ongoing test routing qualified below |
| repository standards | [standards-review.md](standards-review.md) | FAIL; complete Rust/Python/TS/SQL/docs/tooling authority coverage |
| maintainer | [maintainer-review.md](maintainer-review.md) | PASS; existing-owner and whole-workflow comparisons, no material DRIFT |
| system resilience | [system-review.md](system-review.md) | PASS within failure/recovery source scope; ongoing test routing qualified below |
| security domain | [domain-review-security.md](domain-review-security.md) | FAIL; authorization, tenancy, completed media and delegated provenance |
| persistent-data/concurrency domain | [domain-review-data.md](domain-review-data.md) | FAIL; immutable custody, canonical reads, competing publications and coordination |
| focused follow-up | [followup-review.md](followup-review.md) | RESOLVED; four ignored tests have no normal executing lane |
| fresh Ponytail validator | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; five independently retained findings |

Lifecycle complexity requires invariant/system reviews; new ownership requires maintainer; broad tiers require standards; publication authority and durable concurrent custody require their two coherent domain reviews. All selected reports are complete. No UI or model-protocol implementation change requires another specialist. Concurrency is covered by the data reviewer and state/recovery specialists, not omitted. Standalone custody is justified by pre-execution acceptance that execution_inputs cannot represent; existing Scribe, storage, RunState and queue remain owners.

[claim-comparison.md](claim-comparison.md) groups shared Artifact proposals into one producer defect. One focused follow-up was necessary because the standards report's provisional dedicated-lane convention statement did not establish routing, conflicting with behavior's absence-of-lane claim. Actual commands resolve that uncertainty. Unique actor/receipt/SQL claims required independent validation, not additional discovery votes. The validator discovered no new blocker needing separate validation.

## Reconciled acceptance matrix

Evidence means source and inspected assertions unless fresh execution is expressly recorded in [verification.md](verification.md). The discovery matrices and validator's reconciliation preserve exact implementation/assertion citations.

| Original obligation / applicable constraint | Implementation and proof reconciliation | Result |
|---|---|---|
| REQ-008: JSON/media deferred slots; derived queue-only; ordinary direct baseline | Typed decoder/registration changes and assertions; fresh slot test passed | PASS |
| REQ-008: only dependent consumers wait; independent settled work is preserved | Existing reference walker/engine and exact first/resumed call-list assertions; fresh engine test passed | PASS |
| REQ-009: stable tenant/repository/commit/input, retained subscription before execution, no publication-started run or historical subscription replay, exact policy pins | Subscriber SQL, standalone custody, exact claimed UID/source reads; main journey asserts zero executions, retained consumption, old/new pins and new subscriber waiting | PASS for matching; publication validation qualified below |
| REQ-009: completed verified typed media before durable acceptance | File/JSON baseline covered; supported Artifact bypasses metadata/permission/slot admission until consumption | FAIL FIND-007-1 |
| REQ-009: durable ACK, conflicting replacement refusal, identical retry replays canonical receipt | ACK/fence/conflict/unavailability source and sequential replay assertions credible; losing overlapping offer returns local time/status | FAIL FIND-007-3 |
| REQ-011: sole state owner; release compute before park ACK; claim kept until ACK; same ID, accepted work/budget, original deadline, approved refund | Source traces and inspected runtime assertions support implementation; fresh spent-budget test passed | PASS implementation; proof routing FAIL FIND-007-4 |
| AC-007: accepted input survives ordinary restart, unknown outcome retries, running consumer eventually receives it | Existing tests assert revised outcomes without new crash hooks; four tests have no normal executing lane | FAIL FIND-007-4 |
| INV-004: unavailable custody never becomes absent/fresh | Fence-visible distinction and runner defer/retry; approved source inspection plus publish refusal assertion | PASS |
| INV-005/006: exact policy/accepted authority, original deadline; revocation/supersession/no terminal reopening | Existing claim/cancel/wake and source tests preserve boundaries | PASS |
| REQ-016 / AC-011: independent source precedence, shipped SDK/CLI/HTTP/MCP, inspection, generated contracts/errors, docs and upload example | Full behavior/standards/maintainer coverage and surface assertions; publication defects qualify shared callers | PASS baseline |
| Mandatory delegated evidence provenance | Existing Caller attribution owner bypassed by new publication | FAIL FIND-007-2 |
| Mandatory SQL column-name decoding | New subscriber/wake result tuples use ordinal decoding | FAIL FIND-007-5 |
| INV-001/002/003/007 and non-goals | No broker, new PostgreSQL payload table/mirror, Trigger, generic wait framework, permission category, or second execution writer; consumer accepted authority retained | PASS baseline; Artifact producer boundary qualified by FIND-007-1 |

Scoped PASS reports do not negate unique findings on another boundary. No material unrelated scope expansion or unearned replacement workflow was retained. No rejected proposal is repackaged as optional remediation.

## Validated findings

The authoritative full ledger, producer-to-consumer traces, materiality, correction burden and closure proof are in [findings-validation.md](findings-validation.md).

| ID / classification | Exact source boundary | Observable consequence / smallest correction |
|---|---|---|
| FIND-007-1 CONFIRMED / INCORRECT | server `components/verification/service/evidence.rs:98,261` | Invalid Artifact receives immutable accepted receipt, later consumer errors and correction conflicts. Reuse existing TenantEvidence metadata/admission under publisher authority before custody; preserve consumer authority checks. |
| FIND-007-2 CONFIRMED / VIOLATION | same file `:133` | Delegated publisher actor is persisted as subject. Reuse `Caller::attribution()`. |
| FIND-007-3 REVISED / INCORRECT | same file `:143–176` | Losing identical concurrent offer returns rejected offer's timestamp and accepted status. Compose recovered success from actual canonical acceptance; no new cross-subscriber atomicity contract. |
| FIND-007-4 CONFIRMED / VIOLATION | server `tests/integration/pg_deferred_evidence.rs:499,549,598,674` | Normal gates skip four required recovery/resource/deadline proofs. Remove the four ignores and use existing Postgres family ownership. |
| FIND-007-5 CONFIRMED / VIOLATION | SQL `queries/verifier_executions/pull_requests.rs:532–540,570–578` | Explicit mandatory name-mapping rule is currently violated. Use existing named projection/row access without changing SQL semantics. No current swapped-ID exploit is alleged. |

Artifact discovery IDs BEH-001/SEC-007-1/DATA-007-1 consolidate into FIND-007-1. Other mapping: SEC-007-2→2, DATA-007-2→3, BEH-002/FOLLOW-007-01→4, STD-007-01→5. Receipt claim was narrowed to canonical single-subscriber losing-race proof, avoiding new global publication semantics. Minor EvidenceTable rustdoc links may be corrected with material remediation but receive no finding ID or standalone round.

## Verification and handoff

Fresh checks passed: range whitespace, dependency boundary (including its selected security-inline test), tenant-isolation, typed deferred-slot test, independent-consumer engine test and restored Agent-budget test. Exact commands and counts are in [verification.md](verification.md). Initial uv cache failure was resolved by placing its cache in `/tmp`.

Fresh Postgres/Bifrost/SDK journeys could not run because access to the Docker socket is denied in this sandbox. Source-inspected assertions and implementation-recorded runs are explicitly distinguished from new executions. This limitation does not prevent source-backed FIX_REQUIRED; none of the five findings depends on interpreting an environment error as a product defect. No formatter, generator, full gate, commit, push or Git configuration was run.

**Verdict: FIX_REQUIRED.** Route [007-R1-close-publication-and-proof-gaps.md](007-R1-close-publication-and-proof-gaps.md) directly to `$wyrd-implement`. A later task review checks closure, the remediation diff and affected callers while retaining this original task, candidate, reports and accepted source evidence; it does not restart full discovery.
