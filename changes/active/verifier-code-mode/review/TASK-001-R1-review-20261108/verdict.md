# TASK-001 R1 re-review

**Verdict: FIX_REQUIRED.** Nine prior findings close; one remains:
**FIND-TASK-001-2**. A configured engine attempt timeout can start a retry or
produce its failed result while the prior extraction is still running.
Owned guards are repaired, but the required settlement-before-progression is
not. No new finding is introduced. CodeTool finding 7 remains withdrawn.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Prior reviewed candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
- Runtime remediation candidate: `0393740405bb47ab95ff1a404a20be44597fbdeb`.
- Cumulative candidate/HEAD: `f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`.
  Its final commit records the packet/evidence; runtime source matches 039374040.
- Reviewed delta: `git diff 0e51af2a2332f362ab44b7c5b88b51c3cb4f8011 f8b45a7208dbc3e8ec6c3cdac3605f6f84b6cc18`;
  original-base cumulative range remains context.
- Approved spec revision 2 from candidate Git blob, SHA-256
  `cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055`.
- Original TASK-001 SHA-256
  `494bca6fa15325ace56ffe116e45d7d4f745361c56567e5b8351af6cb9fb1521`.
- R1 with appended evidence SHA-256
  `b87167384de03c5b5d494997d625e8c726b37a1042f5a043370b8c65c0416105`.

Local uncommitted benchmark revision 3, its revision-2 snapshot and TASK-003
predate this review and are excluded from this runtime subject. Read the pinned
candidate blob or exact snapshot for runtime authority. No source, existing
packet/evidence, index, commit or planning file was changed during review; new
reports and the R2 handoff are the only writes. HEAD remained the candidate.

This is the first remediation review. Scope is prior-finding closure, the fix
diff and affected paths. No two-unsuccessful-remediation convergence assessment
is triggered. The original task, prior verdict/ledger and R1 remain intact.

## Scope, coverage and independent results

[scope.md](scope.md) records the shared navigation map and source-backed role
selection before discovery. The orchestrator did not validate its own findings.
All discovery reviewers were fresh and separate; the validator was fresh and
separate from them.

| Role | Report | Result / reconciliation |
| --- | --- | --- |
| Remediation ponytail discovery | [Closure](task-review-closure.md) | FAIL; nine findings close, residual FIND-2. |
| Lifecycle/value invariants | [Invariants](task-review-invariants.md) | FAIL; same timeout/settlement cause. |
| Sensitive concurrency | [Concurrency](domain-review-concurrency.md) | FAIL; same cause at retry/new-session boundary. |
| Failure and recovery | [System](system-review.md) | FAIL; result-producing timeout path traced. |
| Fresh independent ponytail validation | [Final ledger](findings-validation.md) | FIX_REQUIRED; sole finding REVISED to bounded result/retry obligation; all other closures confirmed. |

The validator independently accepted applicability and omissions. Broad repo,
security/tenancy, persisted contract and substantial replacement-ownership
specialists were not recreated: original source-backed coverage remains and
those boundaries do not materially change here. Changed Rust docs, recording,
shared state, host failures and caller ownership were freshly inspected by the
selected reviewers and validator. No required report is missing.

Carry forward the first review's accepted declaration/catalog, authority/tenant/
audit, request arithmetic/final allowance, public reduction/citations,
historical/additive/custom-data contracts, privacy/container protections,
normal workspace, portable configuration and governing-document rows. The
settlement integration reopens its actual engine timeout caller because that
caller destroys the new close future. No unrelated discovery was restarted.

[comparison.md](comparison.md) consolidates four proposals into one lifecycle
cause. No focused follow-up was needed: the caller trace is complete and no
material conflict remains. Agreement was corroboration only. Independent
validation rejected broad arbitrary-Drop/result-after-abandonment/new shutdown
drain demands as additional blockers. No validator-discovered new finding
required a separate source validation pass.

## Reconciled acceptance and closure

The detailed source/proof matrix and producer-to-consumer traces are in the
[validated ledger](findings-validation.md); this is its compact disposition.

| Finding / obligation | Reconciled evidence | Result |
| --- | --- | --- |
| 1: generated failure/time-stop ends invocation | Typed host_stop/Halt; real hosted one-turn failure and SDK Failed/Inconclusive/no extra model call | Closed / PASS |
| 2: extraction cleanup | Writer/workspace/permit retention and normal close repaired; Engine::attempt timeout_at still drops close before retry/Attempted settlement | Open / FAIL |
| 3: workspace setup deadline | Existing deadline bounds writer/mount wait; owner test proves no worker and spent/reduced allowance | Closed / PASS; started-work settlement consolidated under 2 |
| 4: admitted waiters recorded | Record starts before permit wait; deterministic cancellation has ordered slot/no bytes/digest | Closed / PASS |
| 5: actual delivery evidence | Delivery and real resumption boundary; dropped completed Delivery remains cancelled, prior delivered read survives later failure | Closed / PASS |
| 6: no post-halt dispatch | Halt-select and after-acquisition check; same-poll deterministic refusal/release proves waiter never invokes | Closed / PASS |
| 8: mandatory rustdoc | Actual changed production/private/test helpers audited, missing Errors/Panics repaired | Closed / PASS |
| 9: recovery proof executes program | FIFO script fixed; script consumption and CodeRun/FsRead records/digests prove real recovery execution | Closed / PASS |
| 10: supported JSON integer domain | Checked installed Monty conversion plus endpoints and authorized real Card u64::MAX path | Closed / PASS |
| 11: exact Oracle QueueFull stop | Exact code observer, both-origin Vala engine proof, ordinary resources-error control and existing hosted deferral sink | Closed / PASS |
| 7 | Withdrawn as previously directed; CodeTool retained | No work |
| Unchanged accepted obligations/non-goals | Prior validated matrix plus affected-source recheck; no new public contract/dependency/service/storage/runtime | PASS, carried forward |

## Retained ledger and correction

**FIND-TASK-001-2 — REVISED / INCORRECT.** REQ-012/AC-009 require
settlement before completed invocation cleanup. Hosted opening keeps the
tracker wait only in ToolSession; AgentExecutor awaits it only inside the
future timed by `Engine::attempt` (`blocks/engine.rs:658–702`). Expiry drops
that future and immediately retries or builds Attempted/settled records.
Started extraction retains its guards and continues; a new session can admit
work concurrently or encounter capacity still occupied by the previous attempt.

Complete the existing timeout-owner/session/tracker lifecycle connection:
retain access to started-work settlement beyond the timed future, stop new
execution/dispatch, await bounded work, then retry or produce the established
timeout result. Preserve guards, partial scratch during cleanup, retry/budget/
record/error/queue semantics and sibling availability. No async Drop, new
service, arbitrary abandonment result or new shutdown contract is prescribed.
The full decision-complete diagnosis, burden and proof are in the ledger and
one [TASK-001-R2 remediation](TASK-001-R2-settle-before-retry.md), routed directly
to wyrd-implement. No specification revision is required for this approved
cleanup obligation.

## Verification and limits

[verification.md](verification.md) records **12 independently rerun tests, all
passing**, with exact commands and run IDs: four Vala dispatch/delivery/capacity,
three hosted lifecycle/conversion, four hosted consumer/queue and one public
Rust SDK failure/recovery journey. Cumulative diff whitespace also passed.
These tests do not drive Engine::attempt timeout while started work is held;
the remaining failure is independently source-proven at that actual consumer.

Reported wider owner lanes, formatting/lints/dependencies and historical RED
experiments were inspected as reported evidence, not rerun or newly claimed.
No broad gate, platform matrix, performance benchmark, broken-source experiment
or whole SDK suite was run. This review does not implement, commit, merge,
push, deploy or complete TASK-001.

Next review is bounded to FIND-2 closure, the subsequent delta and affected
callers. Carry the nine closures and unchanged accepted rows forward unless
new source/dependency evidence invalidates them; do not restart all discovery.
