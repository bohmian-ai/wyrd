# TASK-009 verdict

SPEC_REVISION_REQUIRED

## Immutable subject and scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Branch: task/github-verifier-pipelines/TASK-004-pinned-pr-preparation.
Base: d07c55158.
Candidate: 4d001e6d9ed3daf88bf81995510319373bd8c353.
Range: git diff d07c55158...4d001e6d9.
Approved specification: changes/active/github-verifier-pipelines/spec.md, revision 10.
Original task: changes/active/github-verifier-pipelines/tasks/009-reports-and-github-delivery.md.

This is the first TASK-009 review of the full immutable range. No earlier TASK-009 findings or acceptance evidence are carried forward; no remediation closure or convergence assessment applies. The uncommitted tasks/README.md and untracked tasks/011-code-review-pipeline-tutorial.md are excluded. Source, tests, architecture, task and specification were not edited. No commit, merge, push or deployment occurred. HEAD and the tracked diff were checked against the candidate throughout; only this report directory was added.

## Decision and required authority reconciliation

FIND-009-8 requires a specification decision before this task can pass. A current PR execution can park at accepted revision N and record a delivered in_progress Check. Public cancellation changes only queue SQL status and removes its lease. It does not create accepted revision N+1. The Check can therefore stay in_progress indefinitely: a restarted delivery sees N's final outcome and skips it, while inspection reports cancelled with that earlier delivered outcome.

Two owning contracts prevent packaging a mechanical correction:

- architecture/wyrd-design.md:2127–2130 requires cancellation/revocation to settle only the queue row; the lost holder records nothing more. The state writer enforces this loss of authority.
- architecture/bifrost-design.md:487–495 makes the first delivery outcome for an accepted state revision final, with replay recording nothing.

REQ-013 and task RENDER still require cancelled→cancelled. Adding a callback alone cannot overcome the existing outcome; changing its identity or authorizing a cancellation checkpoint writer changes an explicit persistent-data or resource-ownership decision.

Reconcile how SQL-only terminal transitions participate in canonical delivery version/identity, current inspection and restart, or explicitly authorize a cancellation checkpoint owner and its fencing protocol. Cover manual and principal cancellation, preserve committed lease cancellation and response latency, and retain the existing old-frozen-revision interval unless expressly amended. This review selects neither alternative. No new table, column, result mirror, progress ledger, finalizer or post-loss writer is implicitly authorized.

Nine other findings are bounded implementation corrections under current authority. They remain recorded below; they do not resolve the cancellation decision. No implementation remediation task is issued because the overall verdict is SPEC_REVISION_REQUIRED. A later implementation packet must preserve these finding IDs and incorporate the approved cancellation resolution rather than requiring its implementer to invent it.

## Independent coverage and results

scope.md records contract-by-contract selection, owners, producers and consumers before discovery began. All required reports exist and findings-validation.md independently validates applicability and coverage.

| Selected review | Contract coverage | Report | Discovery result |
| --- | --- | --- | --- |
| Behavior | Full task obligations, negative paths, final-output selection, all shipped authoring/public-story surfaces | task-review-behavior.md | FAIL |
| Invariants | Logical outputs, park/settle/cancel/revision lifecycle and sibling writers | task-review-invariants.md | FAIL |
| Repository standards | All changed layers, nested rules, actual SQL capabilities, generated parity and mandatory rustdoc | standards-review.md | FAIL |
| Maintainability | Changed owners, complete workflow reuse, retained-source interpretation and test clarity | maintainer-review.md | FAIL |
| System resilience | Deployed recovery, provider failure/timing, shared shutdown and background isolation | system-review.md | FAIL |
| Security domain | Tenant/current access/current generation, source trust, safe inspection and provider writes | domain-review-security.md | FAIL |
| Persistent-data/concurrency domain | Canonical Bifrost outcomes, fence visibility, SQL coordination, multi-writer ordering | domain-review-data.md | FAIL |
| Focused follow-up | Ordering correction versus connection-lifetime constraints; SQL cancellation versus canonical outcome identity | followup-review.md | UNRESOLVED |
| Independent source/materiality validation | Every discovery proposal, smallest correction burden, coverage omissions and reconciled acceptance | findings-validation.md | SPEC_REVISION_REQUIRED recommendation |

UI/accessibility discovery is inapplicable because this candidate ships no UI. A separate performance-domain reviewer is unnecessary; system/invariant/data coverage includes waits and resource ownership. Full benchmark qualification is a verification limit, not an inferred performance defect. No selected role/report is missing, and no report substitutes for another role. Detailed coverage and assertion evidence remain in the reports.

## Claim reconciliation and follow-up

claims-comparison.md groups proposals by violated behavior. Discovery agreement was not treated as proof. The focused follow-up was required because the proposed cross-process transaction lock conflicted with release-before-provider-wait rules and the cancellation correction crossed existing authority. No extra discovery was run merely because a claim was unique.

Independent validation rejects absolute remote uniqueness and transaction-lock/lease-ledger prescriptions. The approved mechanism is lookup-before-create with no exactly-once remote-effects promise. FIND-009-7 is narrowed to the explicit completed-Check regression: skip an existing Check for a nonterminal waiting presentation; terminal delivery still updates it. This uses the authorized update-or-skip branch, needs no distributed owner, and preserves initial waiting explanation and terminal output. Residual absent-create duplication is not converted into a stronger unapproved guarantee.

The follow-up's cancellation authority conflict remains material after that narrowing. The source-validating pass retains it as FIND-009-8 and does not prescribe a post-cancellation state writer. Nonmaterial import-shape observations create no work. No newly discovered blocker required another independent validation pass.

## Validated ledger

Every entry below was independently CONFIRMED or REVISED in findings-validation.md. That report contains producer-to-consumer and sibling-writer traces, exact authorities, materiality, correction burden and focused closure proof for every ID. This ledger adopts only those validated entries.

| Stable ID | Discovery IDs / status | Classification and exact source | Observable failure and required disposition |
| --- | --- | --- | --- |
| FIND-009-1 | BEH-009-1, INV-009-3; CONFIRMED | INCORRECT; server components/github/delivery.rs:375–409, presentation.rs:142–191 | A code-only report without extraction silently loses summary/unlocated findings and malformed/ambiguous errors. Pass selection independently of optional canonical source; refuse unavailable anchors visibly. Prove one/malformed/multiple reports without extraction with unchanged authored verdict. |
| FIND-009-2 | BEH-009-2, SEC-009-2; CONFIRMED | INCORRECT; delivery.rs:392–395; verification/hosted/blocks.rs:349–369 and engines.rs:109–117 | Ordinary extraction-shaped output can supply fabricated/stale source authority. Reuse successful server-owned pull_request extraction provenance and accepted activation identity. Prove decoy-before-genuine and decoy-only cannot validate wrong-source suggestions. |
| FIND-009-3 | BEH-009-3, MNT-009-1; REVISED | INCORRECT; presentation.rs:273–367; extraction.rs:243–253 and extraction/output.rs:29–48,159–195 | Valid retained base/before or unchanged-context evidence rejects an otherwise valid head finding. Validate exact retained before/after/context ranges separately from head-diff inline placement, including retained rename-side identity. Prove supporting evidence survives and unretained ranges still refuse. No source fetching or LEFT-placement extension. |
| FIND-009-4 | BEH-009-4; CONFIRMED | MISSING; presentation.rs:185–200, delivery.rs:417–432 | Valid empty/unlocated-only report has no promised pinned COMMENT review. Review existence must follow valid report rendering, including zero inline comments. Reuse marker lookup; prove empty/unlocated review identity and retry/restart reuse. |
| FIND-009-5 | BEH-009-5, SYS-009-02; CONFIRMED | INCORRECT; github/app.rs:849–872; delivery.rs:444–482 | Primary reset timing is discarded and Retry-After above 15 minutes is shortened, exhausting retries early. Preserve provider minimum timing in existing classifier/wait owner, finite attempts and safe errors. Deterministic deadline tests plus existing short connected cooldown suffice. |
| FIND-009-6 | SEC-009-1; CONFIRMED | INCORRECT; delivery.rs:137–169,459–463,491–537 | New head/disconnect during a successful awaited lookup can be followed by a mutation under stale authority. Reuse current access/generation checking after the final lookup immediately before each actual mutation. Prove no later mutation when invalidation occurs during successful Check/review lookup. No SQL connection held during remote waits. |
| FIND-009-7 | INV-009-2, DATA-009-1; REVISED | INCORRECT; delivery.rs:147–159,191–232; verification/runner.rs:1530–1538 | An older waiting PATCH can overwrite a completed Check after consulting stale lookup status. Skip existing Checks for nonterminal waiting delivery, retaining initial create and terminal in-place update. Prove held old lookup released after completion cannot send waiting PATCH or replace terminal output. No distributed serialization or stronger uniqueness requirement. |
| FIND-009-8 | INV-009-1 plus focused follow-up; REVISED | INCORRECT / SPEC_REVISION_REQUIRED; verification/service/inspection.rs:248–275,1015–1041; SQL verifier_executions.rs:607–623,3175–3222; delivery.rs:279–300,548–606; service.rs:301–310 | Delivered waiting revision masks later SQL cancellation indefinitely. Reconcile approved canonical identity/ownership as described above before implementation. After approval, prove park→delivered waiting→public cancel→same-ID cancelled Check/current outcome→restart, and cancellation before waiting delivery. |
| FIND-009-9 | SYS-009-01; CONFIRMED | INCORRECT; delivery.rs:207–249; app/server.rs:661–670,898–917; app/supervise.rs:285–319 | Supervised startup recovery waiting on provider cooldown can consume the entire shared shutdown deadline and prevent Bifrost role drain. Reuse the adjacent existing shutdown-select boundary around active recovery scan/delivery. Prove prompt cancellation while provider remains held and role drain receives budget. |
| FIND-009-10 | REPO-009-1; CONFIRMED | VIOLATION; changed report.rs, delivery.rs, presentation.rs, github/app.rs and implicated test helpers enumerated in findings-validation.md | Missing mandatory changed-item rustdoc, Errors/Panics and cancellation/partial-progress semantics blocks merge. Complete documentation within existing changed owners and batch with behavior corrections; static diff audit plus format/lints, no new test/check script. |

Locations in the ledger are under crates/wyrd/wyrd-server/src unless explicitly identified as SQL or contract source. ReportVersion and report unit documentation in FIND-009-10 are in crates/wyrd-spec/src/verification/report.rs. All proposed corrections preserve verdict ownership, retained-only delivery, tenant boundaries, existing accepted-state custody and excluded machinery.

## Reconciled acceptance matrix

Source/assertion detail is retained in the comprehensive discovery matrix and independent ledger. Passing rows below are accepted only within their described scope, not as evidence for the failing integration seam.

| Original obligation | Implementation / assertion evidence | Result |
| --- | --- | --- |
| REPORT closed schema, named code_review beside verdict, thin Rust/Python/TS authoring | report.rs contract/resolver tests; Skald/SDK authoring assertions; codegen parity | PASS |
| Successful final logical outputs: fanout each, repeat final only, no failed/discarded speculation; no verdict mutation | select_report and engine final-output producer trace; connected selection journey asserts retained outputs, ambiguity/error and authored verdict | PASS at selection; FAIL at absent-source consumption, FIND-009-1 |
| Exact-source validation, every anchor/suggestion, no fabricated location or source change | retained extraction producer and presentation validator; head tests assert exact RIGHT path/range/suggestion; source/context traces | FAIL FIND-009-2/3 |
| Summary-only/unlocated findings, severity/confidence, report COMMENT | renderer heading/summary assertions; empty/unlocated review assertions compared with authority | FAIL FIND-009-1/4; identity/severity/confidence otherwise PASS |
| Check name/head/external execution identity and verdict/conclusion mapping; waiting explanation; completed monotonicity | Settled mapping tests, real waiting/terminal same-ID journey; independent overlapping-writer trace and cancellation SQL trace | FAIL FIND-009-7/8; ordinary mapping/identity PASS |
| Background after settlement/park, retained-only, no recomputation/verdict wait | runner detached handoff after settlement, presented accepted read, restart asserts unchanged repository-read count | PASS; shutdown isolation FAIL FIND-009-9 |
| Lookup-before-create including lost answer, restart finds landed effects | lost-answer and restart journeys assert one found Check/review and unchanged POST/source-read counts | PASS for approved mechanism; no exactly-once strengthening |
| Current access and generation before every actual write | short existing access/generation owner; failed-mutation retry tests versus successful lookup-to-write trace | FAIL FIND-009-6 |
| Provider cooldown, finite retry, safe permanent failure without verdict change | short cooldown timing assertion, retry classifier/loop, permanent refusal inspection/accepted-verdict assertion | FAIL FIND-009-5; finite retry/refusal separation PASS |
| Canonical fenced Bifrost outcome, no SQL payload mirror, absence distinguished from unavailability | record/recorded keys/fences, checkpoint event filter, tenant SQL capability and inspection traces | PASS for accepted revisions; SQL-terminal identity FAIL FIND-009-8 |
| SURFACES HTTP/MCP safe outcome separate from verdict under existing read authorization, no delivery SDK/CLI projection | authorized retained read and HTTP/MCP equality assertions; native authoring/generated projection coverage | PASS; cancelled outcome staleness FIND-009-8 |
| AC-011 complete public journey and runnable report story | CLI App/declaration/PR/evidence/approval flow ends with Check and MCP outcome; public YAML example test asserts authored high-finding failure and exact suggestion | PASS for recorded approved language-neutral proof composition |
| No annotations, automatic approval/change request/resolution/source edits, helper library, new engine, progress ledger, SQL table/column, mirror or finalizer | complete diff and reachable provider/schema owners | PASS; corrections may not introduce these |
| Mandatory changed-item Rust documentation | exact standards inventory and independent source checks | FAIL FIND-009-10 |

The out-of-write-set extraction PointerConflict retry remains at the existing pinned acquisition owner, with bounded retryable exhaustion. No independent defect was established. Its diagnosis/approval and the no-RED process exceptions are recorded in candidate Evidence; this review does not manufacture a runtime claim about approval provenance or waive required behavior/gates.

## Verification and limits

verification.md records independent commands and results: report units 4/4; renderer/delivery units 7/7; real Postgres delivery/extraction journeys 29/29; CLI GitHub journeys 5/5; migration setup, fmt:check, codegen:check, py:typecheck, check:tenant-isolation and workspace/standalone-server Clippy passed. Python and tenant checks used a temporary writable uv cache with no-sync. Generated artifacts remained identical to candidate.

Fresh docs:check and ts:napi:check stop at pnpm installation under sandbox database/cache restrictions; candidate Evidence records earlier passes. A temporary XDG workaround began tool bootstrap and was stopped. No pass is claimed for those reruns. No new race test, live GitHub call, broad aggregate, load or benchmark was run. Shared-memory hot-path guidance was considered: the runner handoff adds a branch/clone/spawn after settlement and no awaited IO; full clean benchmark qualification is not claimed and remains an integrated-change concern before PASS where applicable.

Green checks demonstrate the exercised cases, not the missing source-proven paths. Focused closure proof is attached to each finding; no remediation is instructed to rerun broad journey suites. After approved authority reconciliation and implementation, review the finding closure, fix diff and affected callers under the remediation scope rules, preserving this candidate, verdict and source-backed acceptance context.
