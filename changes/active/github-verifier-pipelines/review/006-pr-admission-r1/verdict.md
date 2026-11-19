# Task 006 — FIX_REQUIRED

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `2165fe606efe498db4e0455866d29236e85d889c`.
- Candidate: `efda8cb2c59c8206b52dbe1d92fcae1ba34be430`.
- Range: `git diff 2165fe606...efda8cb2c`, complete first formal task review. Implementation checker passes are not prior acceptance; no formal evidence is carried forward.
- Approved specification: revision 5 of `changes/active/github-verifier-pipelines/spec.md`, available with `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`. Current revision 6 is draft for the Task 004 storage amendment; the task index explicitly retains Task 006's revision 5 basis.
- Original task and implementation evidence: [006-pr-admission-and-supersession.md](../../tasks/006-pr-admission-and-supersession.md).
- Governing repository and architecture authorities, navigation map and specialist applicability: [scope.md](scope.md).

The candidate and tracked source remained unchanged throughout review. Only review reports and this remediation packet were written. Review did not implement, commit, push, merge or deploy.

## Independent review coverage

| Fresh role | Scope | Report | Discovery result |
|---|---|---|---|
| behavior-rev | Complete A1–A18, user paths, eligibility, public projections and exclusions | [Behavior](task-review-behavior.md) | FAIL; B1–B4 |
| invariant-rev | Frozen fanout, ordering, supersession, shared queue consumers | [Invariants](task-review-invariants.md) | FAIL; I1–I2 |
| repo-rev | All 42 changed paths and applicable Rust/SQL/Python/TS/docs/tooling rules | [Standards](standards-review.md) | FAIL; REPO-006-1/2 proposed |
| maintainer-rev | Complete existing-owner reuse, ownership, abstraction and maintainability | [Maintainer](maintainer-review.md) | PASS; no findings |
| system-rev | ACK, failure recovery, process topology and shared shutdown | [System](system-review.md) | FAIL; SYS-006-1/2 |
| domain-rev-security | Signature, tenant routing, declared authority, revocation and inspection permissions | [Security](domain-review-security.md) | FAIL; SEC-006-1/2 |
| domain-rev-data | Persistent transactions, RLS, concurrent writers and recovery identity | [Data](domain-review-data.md) | FAIL; DATA-1/2 |
| ponytail-rev | Independent source, materiality, corrections, all proposals and specialist coverage | [Validation](findings-validation.md) | FIX_REQUIRED; seven retained findings |

The validator independently confirmed all materially affected contracts have required specialist coverage, including unchanged dependencies and every shipped language. No required report is missing. UI, model-quality and OIDC/JWT profile specialists are inapplicable to the actual task. No follow-up discovery was needed: source tracing resolved shared causes and correction alternatives; unique proposals received the same independent validation. No new blocker originated with the validator.

[Claim comparison](claim-comparison.md) groups discovery proposals by invariant. The final ledger contains only independently confirmed or revised findings. The validator rejected the SQL tuple proposal under the present-failure/materiality threshold; it creates no remediation obligation. Full existing-owner comparison established appropriate reuse of GithubConnections, VerifierExecutionQueue, Scribe, SubmissionSink and the runner; no DRIFT finding remains.

## Validated finding ledger

The complete source evidence, producer/consumer traces, materiality, correction burden, selected correction and focused proof are preserved in [findings-validation.md](findings-validation.md). That ledger is incorporated here without changing its dispositions.

| ID | Status / classification | Violated behavior and correction boundary |
|---|---|---|
| FIND-006-1 | REVISED / INCORRECT | Replay, first close/draft and repeated deactivation can leave older provider reads able to win. Hold the existing per-PR transaction lock across the bounded provider observation and decision, preserving immutable canonical input bytes. |
| FIND-006-2 | REVISED / MISSING | Empty future policy skips the next source's supersession. Source transition must cancel/fence old work even with zero replacement targets and prevent replay of an inactive cancelled generation. |
| FIND-006-3 | CONFIRMED / INCORRECT | A rate-limited PR GET 403 becomes 202/no work and is excluded from recovery. Propagate 403 through existing retryable provider error handling; retain separate 404 behavior. |
| FIND-006-4 | CONFIRMED / INCORRECT | Delayed first admission can commit after access loss. Carry exact token-associated binding/revision and routed installation identity, then validate/lock current slot access before execution writes and through commit. |
| FIND-006-5 | REVISED / MISSING | PR execution and sibling inspection omit repository-read authorization. Apply/audit existing cards:read in shared retained inspection using immutable queue or accepted-input origin; preserve non-PR reads. |
| FIND-006-6 | CONFIRMED / REGRESSION | In-progress failed-delivery recovery ignores shutdown and can exhaust the shared durable drain deadline. Make that maintenance future cancellation-selectable in the existing worker. |
| FIND-006-7 | CONFIRMED / VIOLATION | Nine named changed items omit required panic documentation. Correct the bounded rustdoc inventory alongside substantive fixes; preserve runtime behavior. |

These findings require no new approved behavior, public API, permission model, persistence subsystem or expensive boundary decision. SPEC_REVISION_REQUIRED is not indicated.

## Reconciled acceptance matrix

Evidence references below point to the independently reconciled matrix and full assertions in [validation](findings-validation.md); the discovery reports retain detailed source navigation.

| Obligation | Implementation / verification evidence | Result |
|---|---|---|
| A1 signed connected selected admission; no head policy | Signature before routing; stored selected policy; bad-signature/unselected assertions. Final access check missing. | FAIL: FIND-006-4 |
| A2 frozen complete policy/source/authority | Queue reservation and refusal rows; full PR JSON, Trigger forms and declaring-principal assertions; reselection retains IDs. | PASS |
| A3 event eligibility | Closed action match and provider state classification; metadata/draft/base-retarget/reopen assertions. | PASS |
| A4 current authority and durable refusal | Shared accountable issuer and per-binding audit; revoked-principal errored/code/202/empty-claim assertions. | PASS |
| A5 consequential ACK and retry | Existing commit-before-reply and stalled ACK proof; 403 and zero-row ordering gaps. | FAIL: FIND-006-1/3 |
| A6 canonical replay and restart fanout | Execution-keyed Scribe frames, frozen input, idempotent activation; original IDs and remainder-only activation assertions. | PASS with partial-commit modeling limit |
| A7 bounded failed-delivery recovery | Page/origin bounds, GUID grouping and redelivery cap assertions; false 403 success prevents recovery. | FAIL: FIND-006-3 |
| A8 supersession with completed baseline | Queue fences and P/A/B history/StaleLease proof; empty-policy exception. | FAIL: FIND-006-2 |
| A9 current observation wins | Existing delayed-source tests cover rows, leaving no-row/replay/repeated-deactivation gaps. | FAIL: FIND-006-1 |
| A10 close/draft/access-loss fencing | Existing cancellation codes/history/late-settlement proof; delayed first-admission gaps. | FAIL: FIND-006-1/4 |
| A11 bounded 41-PR progress | Existing outboxes/fair queue; 82 completed rows, two per head, every served head asserted. | PASS |
| A12 central pins/fences | Existing queue and authority reused; missing transitions leave stale work current. | FAIL: FIND-006-2/4 |
| A13 first-class typed inspection | Rust/Python/TS/CLI/MCP/HTTP assertions and wire/stub parity; missing shared repository permission. | FAIL: FIND-006-5 |
| A14 eligibility/pins/cancellation/diagnosis docs | Guide, concept, MCP guide and machine docs match intended contract; implementation breaks documented guarantees. | FAIL consequential to FIND-006-1/2/3/4 |
| A15 owning architecture amendments | Trigger inline/referenced activation, source/ACK/recovery and non-atomic remote IO limit documented. | PASS |
| A16 existing owners, no new table/broker, isolation/secrets | Queue-only migration, TenantConn/RLS, shared Scribe/runner, served-secret assertions; fresh boundary checks. | PASS structural; access integrity covered by FIND-006-4 |
| A17 ordinary admissions preserved | Existing scheduled/group/submitted branches and runner retained; recorded verification/SQL suites. | PASS |
| A18 connected installation tenant | Two-tenant row-set assertions prove ordinary routing; final association race remains. | FAIL: FIND-006-4 |
| Additional security, process lifecycle and mandatory rustdoc obligations | Shared retained consumers, supervisor/drain path and changed-item inventory independently traced. | FAIL: FIND-006-5/6/7 |
| Additional exclusions, language boundaries, reuse and failure diagnosis | No webhook DAG/head policy, extraction/park/provider-result/UI scope; thin SDKs; shared owners; disconnect diagnosis matches post-update NULL host. | PASS |

## Verification and limits

[verification.md](verification.md) records fresh passing whitespace, four GitHub unit tests, Python formatting/lints/stubs/typecheck, TS formatting/lints, dependency/tenant checks, both managed migration proofs and all 15 focused server PR/connection tests. The behavior reviewer also independently passed the four GitHub unit tests. Implementation-reported SQL suites, five surface journeys, codegen/docs/examples and Rust lints remain attributed to the original task.

No new reproduction tests or generators were run during review. Retained concurrency, permission, 403 and shutdown failures are independently source-proven; the green existing lanes do not exercise their missing paths. The partial-fanout test models one already-activated binding rather than injecting a real partial production ACK. No broad aggregate or complete journey repetition was performed.

Prior-finding closure and convergence assessment are inapplicable: this is the first formal Task 006 review.

## Verdict and next task

**FIX_REQUIRED** — implement [006-R1-close-admission-lifecycle-and-authority.md](006-R1-close-admission-lifecycle-and-authority.md) through `$wyrd-implement`, then review the immutable remediation candidate against this verdict, the original task, the prior candidate and cumulative range. Carry forward unaffected source-backed acceptance; check closure and affected paths rather than restarting full discovery.
