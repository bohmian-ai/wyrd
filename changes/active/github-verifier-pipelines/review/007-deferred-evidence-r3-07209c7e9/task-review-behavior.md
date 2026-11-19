# Round 3 independent remediation discovery

Result: **FAIL**. Production behavior remains source-correct. FIND-007-1 and FIND-007-6 close; FIND-007-3 still lacks proof of the corrected reconciliation. FIND-007-2 is rejected by the maintainer and creates no work.

## Subject, authority and limits

Candidate `07209c7e9af1664a30bf548e64be08937c4f88df`, prior candidate `74fc2190e255a3e3cf7515091ff45ab86a288f6b`, original base `16ea9694c34a5ec1867306bd5e34df62f0888611`. HEAD was checked. Scope is the incremental diff, R2 closure and affected callers, retaining original task/spec revision 7 and R1/R2 accepted coverage. No source changes or Cargo/Docker execution by this reviewer. Connected runtime PASS below means implementation-recorded Evidence, corroborated by source assertions, not fresh execution.

Read root rules, skill, scope, original task/remediation context, R1/R2 verdict and R2 validation, spec REQ-008/009/011, spec-driven authority/scope discipline, applicable agent Rust and maintainer guides, Verifier deferred-input authority, standalone Bifrost custody, and security deferred publisher authority. CodeGraph was used before source navigation. No nested server AGENTS applies.

## Coverage and applicability

The only runtime change moves existing receipt status/time computation into a pure synchronous function beside its caller. It does not change fencing, canonical acceptance, persistence, authorization, queue ownership, runner/recovery or wire semantics. The other product file extends a real-server Artifact test and removes a limit from the unused report slot of the shared JSON gate. The focused report gate retains the 64-byte limit and asserts its refusal. Existing JSON/subscription tests keep their assertions; no limit assertion is deleted. No new workflow owner, dependency, synchronization mechanism or public surface is added.

The scope's omitted specialists are justified: prior R1 security/data and invariant/system reports plus R2 accepted source closure cover unchanged sensitive and lifecycle semantics. This review independently traces the changed publisher-to-consumer assertions, receipt caller and affected sibling proof, rather than inferring acceptance from unchanged files alone. No new sensitive-domain uncertainty needs a separate specialist. The final independent validator must decide the proposal below; this report is discovery only.

## Reconciled acceptance matrix

| Obligation / constraint | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-007-1: publisher denied Artifact read refuses before retention | `service/evidence.rs:100,224–275`: subscribers and publisher-owned TenantEvidence admission complete before custody acceptance | `pg_deferred_evidence.rs:834–860` seeds only verifier-run permission, publishes the valid Artifact as that user and asserts 400 / INPUT_INVALID; `:886–900` later admin publication of the identical source/input/value answers accepted, not replayed | PASS source/assertions; recorded exact connected test PASS |
| FIND-007-1: valid publication reaches same waiting execution and is consumed | `pg_deferred_evidence.rs:790–800` declares extract-from-report plus CSV-format requirement; unchanged runner consumes accepted deferred declarations under its own authority | `:831–835` waits on report for returned execution; `:901–902` fetches completion for that exact ID and asserts passed. Extract is an actual prerequisite, so a receipt alone cannot satisfy this check | PASS source/assertions; recorded module 9/9 |
| FIND-007-6: format/path/version/size assert stable code/status | Negative matrix `pg_deferred_evidence.rs:862–884` preserves each invalid publication and checks code/status | Wrong format/path/version assert INPUT_INVALID/400; oversized file INPUT_TOO_LARGE/413; valid same source then accepted | PASS |
| FIND-007-3: canonical losing-offer reconciliation has a regression | `service/evidence.rs:145–156` remains correct: canonical raced rows extend held and clear fresh; `:162` calls extracted receipt | `:403–442` creates already-canonical held state and manually supplies accepted_now=false. No assertion exercises reconciliation or fails when its two corrective statements are removed | FAIL proof only; production source PASS |
| FIND-007-2 | Existing caller.attribution owner still passed at service `:138` | Maintainer explicitly rejects required delegated runtime test with recorded reason; no further proof obligation retained | REJECTED; no work |
| FIND-007-4 lifecycle tests execute in normal family routing | Four formerly ignored tests unchanged; R2 routing acceptance retained | R1/R2 matrices and implementation module pass retained; no new lifecycle changes | PASS carried forward |
| FIND-007-5 SQL named decoding | No SQL changes; R2 named column mapping acceptance retained | Prior validator source trace | PASS carried forward |
| REQ-008 deferred-only typed slots, derived queue-only/direct refusal, independent work | Unchanged registry/engine; original/R2 matrices retain exact tests and assertions | Prior behavior/invariant/standards acceptance | PASS carried forward |
| REQ-009 stable source, active subscriptions, exact pins, unmatched/no retained history, no automatic run, canonical ACK/conflict and sequential replay | Unchanged matching/custody; publication service uses same owners and permissions | Main publication test `:345–362` asserts accepted then replayed, same evidence_id and recorded_at, conflict code; `:363–440` zero runs, retained consumers/pins and later subscriber waits. ACK-uncertain test retained | PASS baseline; race proof qualified above |
| REQ-011/AC-007 same execution, spent budget/work/deadline, compute release, restart and authority fences | Unchanged runner/state/queue; R1 specialist and R2 matrices retain caller/result evidence | Named lifecycle/resource/restart tests retained, recorded 9/9 module pass | PASS carried forward |
| INV-004 unavailable is not absence/fresh | Unchanged fence/custody and recovery ownership | Approved source inspection and unacknowledged-publication refusal/retry test carried forward | PASS carried forward |
| INV-005/006 accepted authority/pins, supersession/revocation/no terminal reopening | No consumer authority change | Prior invariant/system/security acceptance | PASS carried forward |
| REQ-016/AC-011 public projections/docs/source inference | HTTP and MCP share publication service; facades unchanged | Prior full cross-tier acceptance retained | PASS carried forward |
| Non-goals and minimum owner reuse | Pure receipt extraction and existing connected fixture; no new database table, permission category, test hook, broker, runtime writer, API, library or protocol | Actual incremental diff | PASS |

## Proposal R3-BEH-1 — retain FIND-007-3 as MISSING regression proof

Violated obligation: R2 remediation FIND-007-3 requires deterministic proof of the actual production receipt reconciliation with a distinct losing offered timestamp. Its governing R2 `findings-validation.md`, FIND-007-3, explicitly requires a focused regression that fails on the prior reconciliation and passes on the corrected production path. Original REQ-009 promises replay of the identical accepted receipt; this is not a newly invented concurrency guarantee.

Exact boundary: `components/verification/service/evidence.rs:145–162` and new test `:403–442`. Producer: a publisher reads no held acceptance and offers a fresh subscriber; another publisher wins. Consumer: the losing request reads canonical raced rows after custody error, checks conflicts/completeness, then returns status/time. The two R1 corrective statements at `:155–156` transfer canonical rows into held and clear the losing fresh offer. Their deletion restores the original wrong accepted status/local timestamp. HTTP (`components/verification/routes.rs:214–224`) and MCP share this service result.

The new test calls only `receipt(&held, false, offered)` after constructing held with the winner and hard-coding that nothing was accepted now. It verifies the established receipt calculation given already-correct inputs. It does not verify how the losing offer becomes those inputs. Deleting either or both corrective statements leaves every new assertion unchanged. The identity assertion merely compares a value put into the local map with the value it was cloned from; the receipt function does not return or choose identity. The sequential HTTP replay test starts with canonical held state and bypasses the failure/reconciliation branch, so it cannot fill this precise gap.

No production failure is alleged at this candidate. The material consequence is that the required regression does not protect the exact previously diagnosed supported concurrent publication failure. A fresh green execution of this unit cannot establish that missing coverage; this conclusion is source-based and independent of unavailable Docker.

Smallest correction boundary: keep the existing publication/custody/fence/wake protocol. Exercise the production-used transition from a losing fresh offer plus canonical raced acceptance to canonical held/replay state, and assert the resulting status and first timestamp. A bounded extraction at the existing publication owner may include that transition; no public API, new persistence owner, test-only production hook, schedule-dependent interleaving, retry loop or concurrency harness is needed. Preserve conflict/completeness checks and existing real custody/sequential replay integration evidence. The regression must fail if the canonical row is not transferred or the losing fresh state remains, rather than arranging both corrected outcomes in test setup.

Correction burden is one local deterministic regression boundary, already authorized in R2, not a runtime redesign. Fewer tests or source inspection alone do not supply the explicit missing proof. Do not prescribe the private helper's name/type/layout. Independent validation is required before retaining this proposal.

## Ledger quality notes

R2 Obligations FIND-007-1 and FIND-007-6 overlap intentionally on negative Artifact admission/status and valid acceptance: these are shared proof, not independent behavior promises. Their sources are existing REQ-009 and mandatory stable-code rule, with current maintainer mapping. FIND-007-3's planned proof names an extracted private `receipt`; that private shape is not contractual, and proof adequacy must be assessed against losing-offer behavior. The current Evidence overclaims reconciliation and canonical-identity coverage for the reasons above. FIND-007-2 records rejection and must not count as missing implementation or a failed contract. The original task AC-007/REQ-011 and INV-004/REQ-009 rows also share proof; prior matrix reconciliations cover them without multiplying obligations.

## Convergence

This is the second remediation review. The remaining proposal is incomplete proof of the same original receipt defect, not a fix-induced production regression, missed original defect, or expanded requirement. Root cause of non-convergence: the deterministic boundary selected for testing excludes the state transfer that repaired the defect. Limit any further work and review to that transition and its focused assertion; preserve closed Artifact, stable-code, SQL and lifecycle rows and the maintainer rejection.

Skipped fresh connected execution and broad discovery under the authorized scope. Remaining risk: the race regression would remain undetected by the new unit even though current production source is correct.
