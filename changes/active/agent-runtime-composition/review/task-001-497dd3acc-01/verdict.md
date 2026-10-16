# TASK-001 review verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-runtime-composition`.
- Base: `06a368906187be20364a2d959da311151964b617`.
- Candidate: `497dd3acc8fd06567a0e0ff01baf599f9af1711b`.
- Full cumulative range includes the first implementation commit `178129c6c`.
- Approved authority: [specification revision 3](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-delete-sdk-agent-runtime.md), read at base; candidate adds implementation evidence without revising the task obligations.
- Candidate HEAD and tracked source remained unchanged during review. Only new review reports/remediation artifacts were written.

## Independent reviews

| Role | Report | Discovery result |
|---|---|---|
| behavior-rev | [Behavior](task-review-behavior.md) | FAIL |
| invariant-rev | [Invariants](task-review-invariants.md) | FAIL |
| repo-rev | [Repository standards](standards-review.md) | FAIL |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL |
| system-rev | [System resilience](system-review.md) | FAIL |
| domain-rev-security | [Security and correlation](domain-review-security.md) | FAIL |
| domain-rev-sdk-contracts | [SDK/native contracts](domain-review-sdk-contracts.md) | FAIL |
| followup-rev | [Focused coverage follow-up](followup-review.md) | RESOLVED |
| ponytail-rev | [Independent findings validation](findings-validation.md) | FIX_REQUIRED |

Security/correlation and SDK/native contracts received distinct domain reviewers because application requests and public binding contracts materially changed. No server persistence, transaction/RLS, shared-state concurrency, analytical storage, or deployment mechanism changed; those did not require separate domain reviewers. The system reviewer covered the changed application request lifetime and recovery boundary.

The orchestrator grouped claims in [discovery-comparison.md](discovery-comparison.md), then assigned one fresh follow-up because reviewers materially differed on deleted live Prompt coverage. It distinguished retained compiler unit proof from lost authored-Prompt/provider acceptance and limited preservation to existing cases. A separate fresh Ponytail validator then read the complete diff, authorities, every discovery report and follow-up, traced sources/callers, and independently confirmed or narrowed every retained correction. No finding below is based solely on reviewer agreement or an orchestrator-generated defect.

## Reconciled acceptance matrix

Source and verification evidence are detailed in the independent reports and final validation ledger. Runtime outcomes in implementation evidence are reported claims, not independently rerun results.

| Obligation | Implementation / verification assessment | Result |
|---|---|---|
| REQ-012 | Public executable SDK Agent construction/loop removed; native/root/package absence inspected; Rust/TS add no replacement. | PASS |
| REQ-013 | Runtime-only constructor remains, actual Prompt consumer proof and authored-Prompt smoke coverage are incomplete; bounded migration defects remain. Findings 2–4, 6–8. | FAIL |
| REQ-014, AC-008 | Declarative Cards, hydration/navigation, internal Skald/Vala/server execution retained in source; preservation journeys reported green. | PASS |
| REQ-015 | Run invocation removed from shared, Python, TS native/public/declarations without SDK execution replacement. | PASS |
| REQ-016, AC-009 | Python producer loses subjects; TS fixture content is bypassed; literal input and total request budget regressions remain. Findings 1, 2, 6, 7. | FAIL |
| REQ-017 | Existing external-framework/MCP/auth/telemetry integrations and receiving-server owners preserved. | PASS |
| REQ-018, AC-011 | Primary boundary docs updated, but callback-only constructor guidance, retired-smoke description and required Rust operational docs need closure. Findings 3–5. | FAIL |
| AC-007 | Main deletion succeeds; public callback-only mock constructor remains. Finding 3. | FAIL |
| AC-010 | Receiving-server authz, subject validation, stripping and audit preserved; missing migrated attribution and weakened app error projection prevent complete migration acceptance. Findings 1, 8. | FAIL |
| INV-001 | Durable serving/governance remains server-owned. | PASS |
| INV-002 | Wyrd caller/provider credential ownership retained; no demonstrated provider-key leak. | PASS |
| INV-003 | No inferred tool permission or new tool binder. | PASS |
| INV-004 | Hosted/direct MCP checks and nonblocking audit owner unchanged. | PASS |
| INV-005 | Internal server-controlled native-route refusal retained. | PASS |
| INV-006 | Skald stays independent of Wyrd client/server mechanics. | PASS |
| INV-007 | No platform-specific mechanism added; four-target runtime execution not independently performed. | PASS on source |
| INV-008 | No Card kind, persisted contract, evidence store or internal verification removal. | PASS |
| Non-goals | No revision-2 lifecycle/Jev expansion, replacement SDK executor, MCP relocation, binder move, new server execution, migration, alias or new durable correlation state. | PASS |

## Validated finding ledger

All retained IDs and correction boundaries come from [findings-validation.md](findings-validation.md). No prior task-review finding IDs exist for this first review.

| ID | Status / classification | Source and consequence | Smallest correction boundary |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED / REGRESSION | Python gateway journey helper omits Run/Card headers; its four requests lose required attribution and readback misses the omission. | Existing request producer and exact retained-call assertions. |
| FIND-TASK-001-2 | REVISED / MISSING | TS gateway journey uses hard-coded messages instead of its actual portable/native Prompts. | Bind only its two existing authored forms locally and assert upstream content. |
| FIND-TASK-001-3 | REVISED / MISSING | Python `ProviderResponse.text` remains public for a deleted mock/callback lifecycle. | Delete that constructor/declaration and directly stranded glue; preserve shared views. |
| FIND-TASK-001-4 | REVISED / VIOLATION | Entire smoke module retirement removes surviving authored-Prompt/live acceptance contracts. | Migrate only prior non-loop cases to test-owned native calls and retain credential gating. |
| FIND-TASK-001-5 | CONFIRMED / VIOLATION | Changed Rust IO/workflow docs omit cancellation, uncertain completion, partial progress and accurate early-exit/errors. | Batch required rustdoc corrections at existing owners/callers. |
| FIND-TASK-001-6 | REVISED / REGRESSION | Rust/TS application calls lose former finite total request budgets. | Existing native reqwest timeout and AbortSignal timeout; no SDK machinery. |
| FIND-TASK-001-7 | CONFIRMED / REGRESSION | TS replacement-string semantics alter valid literal customer questions. | Standard replacement callback and actual-helper upstream assertion. |
| FIND-TASK-001-8 | REVISED / REGRESSION | Reference HTTP app returns arbitrary native refusal/local IO detail through its public error response. | Existing application boundary: catalog/fixed safe text, preserved allowed code/status. |

These are eight shared-cause corrections, not separate findings for every consumer. Validation rejected broader renderers, blanket provider-view deletion, new runtime/retry/timeout configuration, new reference-app harnesses, revived runtime tests, revision-2 sweep expansion, and unsupported claims of an observed secret leak or server authorization bypass.

## Verification limits

- All reviewers inspected source/diff/available evidence; no Cargo, repository test/build/lint/package suite, collection or live provider run was repeated during this review.
- The independent validator used a source-only Node expression for replacement semantics and pinned local reqwest source for timeout behavior.
- Reported green checks do not close missing values/assertions or lost test seams. `ts:test:unit` is not explicitly recorded in candidate task evidence; remediation must record it rather than infer it from builds/types. The original migration RED is not recorded; do not fabricate historical evidence.
- The Rust HTTP app is reference-only and has no local compilation lane. Its bounded correction receives explicit source-level proof; adding an app crate/harness is outside scope.
- Restored live Prompt acceptance remains credential-gated; collection/skips do not establish provider acceptance.
- No source/authority blocker or new material design decision prevents the bounded corrections.

## Remediation handoff

One self-contained task: [TASK-001-R1-close-sdk-deletion-gaps.md](TASK-001-R1-close-sdk-deletion-gaps.md).
Route it directly to `$wyrd-implement` under approved revision 3. This review does not implement, merge, push, deploy, or mark the original task complete. Next review must reassess the complete base-to-remediated candidate with this verdict, original task and remediation, preserving the FIND IDs.
