# TASK-001 cumulative re-review verdict

**FIX_REQUIRED — FIND-TASK-001-5 only (documentation).**

The functional remediation and runtime Python type export are accepted. The
remaining correction is two function documentation contracts, not a runtime
change, new test, or reopening of the owner's accepted deletion race.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt`.
- Original cumulative base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`.
- Candidate: `a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`.
- Previous candidate: `02c06e21bd5fd613341935a843fabe5350ea0a9d`.
- Latest fix commits `dcf16f862..a21a804bc` locate changes; acceptance covers the
  full original-base-to-current-candidate range, including original consumers,
  prior reports, [R1](../TASK-001-02c06e21b-20261015/TASK-001-R1-registration-and-public-type-closure.md)
  and its recorded evidence.
- Approved [spec r2](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-agent-prompt.md), and owner-directed FIND-1
  acceptance recorded in R1 and the prior verdict addendum govern the review.
- HEAD stayed at the candidate and reviewed tracked source stayed unchanged.
  Only this fresh review directory was written; no implementation, merge,
  commit, push, deployment or completion action occurred.

## Independent roles, completeness and follow-up

| Separate fresh role | Report | Result |
|---|---|---|
| behavior-rev | [Behavior](task-review-behavior.md) | PASS for inspected acceptance; original doc locations closed |
| invariant-rev | [Invariants](task-review-invariants.md) | PASS for inspected acceptance; original doc locations closed |
| repo-rev | [Standards](standards-review.md) | FAIL; continued FIND-5 at newly modified SQL functions |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL; same continued documentation cause |
| system-rev | [System resilience](system-review.md) | PASS; no new recovery/lifecycle finding |
| domain-rev: registry data | [Registry data](domain-review-registry-data.md) | PASS; exact-pin/current-operation defects closed |
| domain-rev: security/tenancy | [Security/tenancy](domain-review-security-tenancy.md) | PASS; operation eligibility remains tenant/request bounded |
| ponytail-rev | [Independent validation](findings-validation.md) | One retained finding; all proposals and prior closures dispositioned |

All required reports include the scoped authority/source/proof coverage and
results. Both implementation reports supplied cumulative obligation matrices.
The [shared map](subject.md) was a starting point expanded independently, not a
scope limit. Current reviewers received historical findings and remediation,
not implementation completion summaries or each other's current conclusions.

[Comparison](discovery-comparison.md) records why no extra discovery follow-up
was needed: PASS reports closed the originally enumerated doc sites, whereas
standards/maintainer reports located newly modified SQL functions. This is
location coverage, not an unresolved runtime-path dispute. The fresh validator
independently read those bodies/callers and applied the explicit documentation
rule rather than counting votes. No additional pass was used to seek agreement.

## Prior-finding closure and final ledger

| Stable ID | Reconciled disposition and evidence |
|---|---|
| FIND-TASK-001-1 | OWNER-ACCEPTED RISK. Concurrent deletion of a reused generated Prompt remains possible; no lock/recheck/concurrency-test demand is reopened. Existing auth/tenant/transaction protections remain. |
| FIND-TASK-001-2 | CLOSED. Existing exact Agent selects its pinned generated Prompt before ordinary child/parent canonical comparison; fresh-key reapply and real changed-content/parent conflict regression cover the producer and sibling binding. |
| FIND-TASK-001-3 | CLOSED. Shared version query admits only current-operation Pending rows alongside Active rows, preserves normal auto/scope/pin behavior and dedups generated outcomes by UID; same/different/authored-child tests cover valid composition and rollback. |
| FIND-TASK-001-4 | CLOSED. One ordinary Python union definition, runtime/stub/__all__ reexports and exact five members agree; independent public imports and the exact unit test passed. |
| FIND-TASK-001-5 original sites | CLOSED at the retained PromptCard field and all six originally cited HTTP items; substantive lifecycle/direct/transitive panic docs match their bodies. |
| FIND-TASK-001-5 SQL continuation | CONFIRMED; VIOLATION of explicit rustdoc gate. Sources STD-001 and MNT-R2-001. `version_resolve.rs:49` lacks helper rustdoc/Errors; public resolver at :101 lacks Panics for its exact-Pin precondition at :113. Add only those function contracts, preserving all executable behavior. |

No new FIND ID is assigned. Detailed producer/caller, source and scope evidence,
each proposal disposition and minimal correction are preserved in
[findings-validation.md](findings-validation.md). No specification revision or
additional product/architecture/concurrency decision is required. The retained
item is an explicit documentation gate, not optional style or a production-panic
claim: current registration dispatches exact pins before calling that resolver.

## Reconciled acceptance matrix

“Recorded” means supplied execution evidence, not commands independently rerun by this validator. A/B/R/P/T/I are the exact focused proof selectors in the original task. Full integrated suites remain change-review work under AC-005.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: synchronous Agent PromptCard/ref, prompt_ref/promptRef, same invocation relation, no state-level agent_prompt | Shared local borrowed view and typed dispatch; Python/TS thin projections; invoke consumes agent.prompt payload; removed old resolver | R/P/T/I sources and recorded green results; precise public static contracts | PASS |
| REQ-002: ref/path/inline registered exact transitive Prompt, direct HTTP, ordinary transaction/outcomes/lifecycle | Existing loader unchanged; inline server writer/persist_node before parent canonicalization; normal replay/completion | A/B and original R/P/T; new same-operation regression; row/ref/edge/Active assertions | PASS |
| REQ-003: validated derived name/effective space/tenant; independent normal line, old pins, reuse/conflicts, no implicit cross-Agent sharing | inline_prompt_submission, exact-parent pin helper, common operation-aware SQL owner; normal lock/hash authority | A/B plus inline_agent_prompt_reapply_preserves_pin and inline_agent_prompt_reuses_transaction_selection; recorded RED/GREEN and original regressions | PASS |
| REQ-004: generic/typed alias agreement and explicit Prompt alias without redundant Service component | Existing alias index/typed maps and native wrapper dispatch, no generated Service edit | R/P/T and generic state unit bodies | PASS |
| REQ-005: no IO/write/version selection on local read; stable unknown/kind/invalid-bundle errors | Borrowed/cached/closed-over local values; load-time Agent target validation; existing error owners | Rust actual shutdown; Python/TS unreachable URL and accessor inspection; journey unknown/kind negatives and local malformed-state tests | PASS, with execution distinction below |
| REQ-005: preserve auth/tenancy/atomicity/conflict/replay/audit | Same caller TenantConn, writer operation, exact/hash comparison, ordinary route/audit; owner-waived deletion risk isolated | A denial/no-writes, B tenant refusal, new conflict/operation counts, recorded tenant check | PASS subject to accepted FIND-1 |
| REQ-006: all SDKs/public typing/exports/Prompt capability/consumer/docs closure | Single public Python union, runtime/native/stub agreement; TS discriminated union/declarations; shared Rust export and invocation; scoped doctrine exception | Public Python/TS positive and negative compile contracts; import/unit proof; R/P/T runtime parity; recorded type/codegen/native/docs checks | PASS |
| INV-001/002: server durable owner, declarative/IO/PyO3-free pure spec | Server-only materialization/versioning; local hydrated holders retain no registry IO; wyrd-spec unchanged | Cumulative diff/boundary source and recorded check:deps | PASS |
| INV-003: reuse transaction/reference inventory/index/hydrator | Existing writer/persist_node/SQL/graph/index owners extended, no replacement workflow | Producer, callers, siblings and lifecycle tracing above | PASS |
| INV-004/non-goals: no other inline redesign, standalone Workflow/cascade/migration/compat/features/Bifrost/UI/external changes | Scope of materialization is registered Agent nodes; cumulative paths/manifests exclude prohibited machinery | Full cumulative diff and scoped protocol/docs | PASS |
| AC-001/002: HTTP and SDK behavior/edge/security evidence | Actual test assertions reach required registration boundaries including remediated failures | A/B and new HTTP focused regressions, all 30 route tests recorded green | PASS at task scope |
| AC-003/004: language journeys/navigation/offline/invocation and public/local negatives | R/P/T/I bodies and shared malformed graph refusal | Recorded focused green results; source inspected, not independently rerun | PASS at task scope |
| AC-005: consumer types/checks and required repository documentation | Runtime export fixed; source typing/declarations coherent; changed SQL lacks required rustdoc/Panics | Recorded canonical checks do not supersede source documentation rule | FAIL solely FIND-5 |

## Verification and limits

Original and R1 task evidence records focused A/B/R/P/T/I, remediated exact-pin
and current-operation RED/GREEN, all 30 registration-route tests, Python state
units and applicable required format/lint/typing/boundary/generated/docs checks.
Reviewers inspected actual proof and connected owners; Cargo/Postgres results
were recorded evidence, not independently rerun by this review.

Discovery independently reproduced the public HydratedCard import and exact
five-member union. Standards also ran the exact no-sync public-union Python
unit test: 1 passed. No dependencies or native libraries were rebuilt. Source
inspection confirms corrected typing and no hidden navigation IO.

Rust's navigation proof physically shuts down its server; Python/TS change
ambient URLs while fixtures remain live. Those tests are not misrepresented as
physical shutdown proof. Original missing RED history remains disclosed; no
historical checkout or fabricated RED was performed. Complete integrated
journeys remain the later change-review obligation.

The optional strict workspace cargo-doc failure is not a required repository
lane and is not another finding. The missing SQL contracts fail the source
rule independently of that command. No new strict-doc lane, broad rerun,
behavioral test or unrelated private-link repair is required to close prose.
The only accepted runtime risk remains the owner-waived deletion interleaving.

## Single remediation handoff

[TASK-001-R2 — SQL rustdoc](TASK-001-R2-sql-rustdoc.md) packages the one remaining
finding for `$wyrd-implement`. It requires two documentation additions with
static/format proof and no new runtime work. Later task review retains the full
cumulative subject, all prior closure, both remediation tasks and accepted risk.
This review does not mark the task complete while the explicit rustdoc gate
remains unsatisfied.
