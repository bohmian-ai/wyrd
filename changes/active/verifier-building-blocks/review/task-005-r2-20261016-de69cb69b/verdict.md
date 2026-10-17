# TASK-005 cumulative re-review verdict

**FIX_REQUIRED**

The immutable candidate does not pass TASK-005's independent completion gate. Seven independently validated findings remain: residual FIND-TASK-005-2/7/8 and new FIND-TASK-005-10/11/12/13. Six prior findings are closed. All retained corrections fit approved r9; the previous public-upload specification stop is resolved.

Remediation: [TASK-005-R2-input-capacity-and-provider-corrections.md](TASK-005-R2-input-capacity-and-provider-corrections.md), routed directly to `$wyrd-implement`. Review does not implement the fixes, mark the task complete, merge, push or deploy.

## Immutable subject and user scope

- Original repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Reviewed source only: detached checkout `/tmp/wyrd-task005-review-de69cb69b`.
- Cumulative base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Candidate: `de69cb69b68eced72f7d760681d27253e013f8a1`.
- Previous reviewed candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`.
- Actual subject: `git diff f2a726c90...de69cb69b`. Recent `99882cf82...de69cb69b` only locates remediation owners and prior closure.
- Approved authority: `changes/active/verifier-building-blocks/spec.md` revision 9 at the candidate.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md`; original task snapshot and prior verdict/ledger remain in `review/task-005-20261016-99882cf82/`.
- Prior remediation: that directory's `TASK-005-R1-verification-corrections.md`, including the human-directed queued trace follow-up. Supplemental `TASK-005-S1-provider-conformance-sweep.md` and adjacent `findings.md` are read at the candidate.
- The user explicitly continued editing during review. Uncommitted edits, branch movement and commits after `de69cb69b` were excluded. No mutable original-worktree code, authority, generated output or external build was used as evidence. Candidate commit and detached source remained unchanged and clean through final writing.
- Reports only are written to this new review directory. No diff, patch or source archive is stored. No user source, index, branch, task status, approved specification or prior report was changed. [subject.md](subject.md) records the common navigation map and exact isolation rules.

The owner's earlier **nothing has shipped; no migrations needed** constraint remains in force. No upgrade/catalog/history migration or legacy pending-dispatch compatibility is required. R9 approves the local evidence PUT and its operation-level admission. The separate agent-runtime-composition r2 document is draft, not implementation authority. Rejected prior DATA-001 and BEH-04 do not re-enter scope.

## Independent topology and reports

All ten discovery reviewers were fresh, separate and independent for this round. Each received the same pinned subject/map, full cumulative range, approved task/spec, applicable rules and prior closure hypotheses. Current-round peer conclusions and an intended verdict were withheld. Reports expanded the map through actual full owners, callers, siblings and proof sources.

| Role | Report | Discovery result |
|---|---|---|
| Caller-to-result implementation | [task-review-behavior.md](task-review-behavior.md) | FAIL; proposed absent-target conflict is independently rejected |
| Producer-to-sink implementation | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer specialist | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | PASS; capacity closure narrowed by follow-up/validation |
| Security/RBAC/tenancy domain | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | PASS |
| Persistent data/durability domain | [domain-review-data-durability.md](domain-review-data-durability.md) | PASS |
| Concurrency/resource lifecycle domain | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Evidence/evaluation/trace domain | [domain-review-evidence.md](domain-review-evidence.md) | PASS |
| Provider wire/Agent boundary domain | [domain-review-provider.md](domain-review-provider.md) | FAIL |
| Fresh focused follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail source validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

The orchestrator groups claims and records the fresh validator's ledger; it does not validate its own findings. Agreement among discovery reviewers was not treated as proof. Every discovery proposal, including unique claims and contradictions, has an explicit disposition in findings-validation.md.

## Comparison and focused follow-up

[claims-comparison.md](claims-comparison.md) groups proposals by owning invariant. A material conflict concerned FIND-2: system/implementation reports treated typed Busy, the before-model hook and post-run observer as complete closure; concurrency found that a pending parallel tool can prevent either observer from running before an outer authored timeout.

One fresh focused reviewer investigated only that uncertainty and traced resource producer, per-session latch, Skald concurrent collection, independent query wait, outer timeout, engine classification and queued release. It resolved the path as reachable. The validator independently confirmed the residual and retained the existing FIND-2 ID. Ordinary generic tool-error semantics, earlier cancellation/deadlines, query lifecycle and existing queue/fencing are preserved by the selected session-owned wake correction. Other unique proposals went to mandatory source validation, not extra voting passes.

## Independently validated findings

Full exact source locations, producer-to-consumer proof, sibling coverage, violated obligations, corrections and focused closure scenarios are in [findings-validation.md](findings-validation.md) and reproduced in the self-contained remediation task. Only CONFIRMED or REVISED findings appear below.

| Stable ID | Validation / classification | Observable defect and selected owning correction |
|---|---|---|
| FIND-TASK-005-2 | CONFIRMED residual; INCORRECT | An observed Busy refusal can be hidden behind a pending parallel Agent tool until outer timeout spends retries or settles an outcome. Wake the existing ToolSession/AgentExecutor on first refusal and reuse current Busy/deferred/refunded release. |
| FIND-TASK-005-7 | REVISED residual; VIOLATION | Concrete changed helpers/tests still miss mandatory item/error/panic docs; related result/recovery prose is false. Correct the listed items in place without behavior/assertion changes. |
| FIND-TASK-005-8 | CONFIRMED residual; INCORRECT | Two current runtime pages teach contention as authored retry/inconclusive judgment. Align them with approved queued deferral/refund, direct 503 and separate genuine failure semantics. |
| FIND-TASK-005-10 | CONFIRMED new; MISSING / INCORRECT | Present evidence is not authoritatively checked against selected slot names/kinds/JSON schema/media constraints, so declared-invalid evidence can pass. Validate once at the current Vala execution boundary; resolved media constraints stay with the authorized server evidence owner. |
| FIND-TASK-005-11 | CONFIRMED new; MISSING / VIOLATION | Separately uploaded Files/selected Artifact files bypass the execution-wide combined byte limit. Enforce the original pinned input set's overflow-safe total at existing TenantEvidence/hosted admission, without prefetching bodies or double-charging internal reads. |
| FIND-TASK-005-12 | CONFIRMED new; REGRESSION | Google builder effective output override leaves both mutually exclusive response-schema fields. Clear the replaced native field at existing shared Gemini/Vertex builder owner; preserve unrelated settings and precedence. |
| FIND-TASK-005-13 | REVISED new; MISSING | Newly composed hosted Google tool loop discards required opaque function-call-part signature on typed decode/replay. Preserve that optional native continuation field in the shared part representation and current history path; no alternate loop or draft feature. |

New findings start at 10, after the prior maximum 9. The declaration-validation and combined-file proposals are split because pure slot conformity and execution resource accounting are distinct invariants and owners. No retained correction needs a new public, persistent-data, security, architecture or resource-ownership decision.

### Rejected proposals and excluded scope

BEH-R2-01's zero-row conflict trace is factual, but its prescribed durable outcome for a nonexistent/foreign target is rejected: current executions require exact tenant-owned target Cards, and a general rejected-target receipt contract is not approved. Do not copy a different target into that identity, weaken tenant-qualified integrity, add nullable/raw rejection persistence or create a receipt/store. This is separate from permitted outbox loss and does not waive valid-target replay/conflict handling. The rejected proposal is not optional remediation advice.

S1's open JEV/SystemOne dispatch and the separate draft composition features are not added to TASK-005 requirements. Equal-millisecond traces have no newly approved parent-first ordering rule. No migration, benchmark redesign, optional non-function Google signature handling, new caller/transport/loader/queue, or dependency is requested.

## Prior-finding closure


| Stable ID | Candidate source and proof assessment | Disposition |
| --- | --- | --- |
| FIND-TASK-005-1 | enqueue passes requested subject to authorize_run; exact Verifier permission then signed scope.uids check precedes Submission/Scribe stage, with one nonblocking audit decision. Direct intentionally supplies no subject to that gate; scheduled authority remains separate. pg_verification_routes signed own/foreign/delegated cases target the real boundary. | **CLOSED** |
| FIND-TASK-005-2 | Sandbox admission/unserved checkout Busy, workspace try_acquire, typed ToolError cause, engine Busy and runner fenced defer/refund are corrected. Existing real hosted consumer test requests only one code.run; queued capacity test proves refund once Busy arrives. Concurrent pending sibling remains as above. | **OPEN — residual** |
| FIND-TASK-005-3 | runner passes frozen scheduled slots to DriftEngine::checks; producer filters check.on slots, emits no report on omission; DriftChecks then preserves inconclusive missing samples. Actual retained-row optional-slot fixture distinguishes omitted versus explicit mapping. | **CLOSED** |
| FIND-TASK-005-4 | both preparation operations use authorize_evidence/covers_operation; execute/enqueue still check exact selected Verifier; tenant/digest/length checks preserved. Real exact-V-grant fixture includes direct File judgment, enqueue acceptance and wrong-Verifier/no-grant refusals. | **CLOSED** |
| FIND-TASK-005-5 | ResultPayloadBuilder::build mints one ID per produced payload and uses it for summary/features. Runner builds once; settle stages once before commit; queue completion selects that ID under token; status queries selected pointer; INSERT_DISPATCH_SQL freezes the same pointer. Rollback/reclaim test retains abandoned and winning outputs and reads the winner. Stale comments are prose, not behavior. | **CLOSED** |
| FIND-TASK-005-6 | approved r9 explicitly adds local-only PUT; conditional route, caller tenant, bounded body, digest check and atomic store match it. Operation-level admission shares FIND-4 closure. No r8 approval stop remains. | **CLOSED** |
| FIND-TASK-005-7 | Original enumerated omissions repaired, but material changed items listed above still miss mandatory docs. False identity prose accompanies the new recovery correction. | **OPEN — residual** |
| FIND-TASK-005-8 | Old architectural outcome Variant/recorded_at/labels/scheduled authority prose corrected. Two current runtime docsite pages still contradict capacity behavior. | **OPEN — residual** |
| FIND-TASK-005-9 | AWAIT_TRACE_SQL, TraceWaitOutcome and await_trace removed; ordinary release, retry, leases/fencing and actual CallerTraces read path remain. No production dependency lost. | **CLOSED** |

The human-directed queued trace work is separately **satisfied within recorded proof limits**, with no retroactive FIND ID. SQL Cause carries caller sampling through claim/reclaim; runner creates an unentered attempt, installs linked-work context/sink before entry, adopts claim timing, and binds tenant route from accepted authority/SYSTEM actor. Gateway continued_context uses that attempt. Telemetry/runtime sampled and unsampled cases, ratio-zero operator policy and all three SDK caller timeline journeys provide focused recorded evidence. A dedicated queued same-external-trace-ID/two-tenant run and fully reclaimed SDK judge timeline were not freshly executed; existing route ownership and cause tests support those seams without inventing a new requirement.

## Reconciled acceptance matrix


| Obligation family | Reconciled result |
| --- | --- |
| One Verifier contract, dependency graph, retained missing/skip/aggregation semantics | **PASS**, except present-invalid evidence admission FIND-10 |
| Shared direct/queued engine, built-in tools, sandbox/workspace ownership | **FAIL** FIND-2; signed Google tool continuation FIND-13 |
| Authoritative evidence kinds/schema/media constraints and execution file limits | **FAIL** FIND-10/11 |
| File transport, exact-grant preparation, r9 local route, signed subject scope | **PASS**; no new public/security approval required |
| ACK-only batching, no request PG write, accepted authority, valid-target replay/conflict, lease/fencing/shutdown/loss | **PASS**; invalid-target receipt proposal rejected |
| Real selected result, distinct payload identity, feature/Operator context and independent delivery | **PASS** |
| Explicit scheduled mappings, requirement baselines/statistics, original citations, retained scenario execution | **PASS** |
| Provider-shaped Google schema override regression | **FAIL** FIND-12 |
| HTTP/MCP/SDK/CLI/UI/generated cutover and external example evidence | Source/recorded proof otherwise supported; no new all-provider/live/example execution claim |
| Mandatory changed-item docs and current operator runtime documentation | **FAIL** FIND-7/8 |
| Nothing-shipped migrations, new benchmark qualification and draft runtime composition | **NOT REQUIRED**; rejected proposals do not become remediation scope |


| Explicit task / acceptance mapping | Implementation and proof basis | Result |
|---|---|---|
| REQ-001–003 / AC-001: one contract, graph, supplied slot conformity | Contract/graph and registration source remain coherent; present-invalid declarations bypass validation as proved in FIND-10 | FAIL for input conformity; other inspected obligations PASS |
| REQ-004–009 / AC-003: preparation/checks, bounded built-ins and selected provider loop | Existing Vala/Skald/hosted owners reused; concurrent-refusal residual and signed Google continuation break reachable paths | FAIL FIND-2/13; Google builder sibling regression FIND-12 |
| REQ-010 and Deployment limits / AC-006 | Transport/digest/tenant integrity corrected under r9; aggregate execution File/Artifact budget and slot constraints remain absent | FAIL FIND-10/11; transport otherwise PASS |
| REQ-011–013 / AC-005/006: direct/queued lifecycle and negative flows | ACK-only batched queue, valid-target replay/conflict, accepted authority, status/wait/reconnect, fenced recovery source and recorded proof | PASS normal lifecycle; affected Busy/input negatives FAIL FIND-2/10/11 |
| REQ-014 / AC-007: scheduled evidence/baseline/scoring/reactions | Frozen selectors reach report producer; omitted optional evidence stays inconclusive; requirement-specific fitted state and failed-only delivery retained | PASS |
| REQ-015: attributable canonical result and winning pointer | Per-produced-payload identity, feature key, SQL result selection, actual status read and frozen dispatch context agree | PASS |
| REQ-016 / AC-008: examples | Three immutable external source commits and recorded deterministic tests; no fresh build/live quickstart claim | PASS within recorded exercised paths |
| REQ-017 / AC-009: consumer/documentation agreement | Current operational contention prose remains stale; schema/generated/MCP/CLI/UI cutover otherwise supported | FAIL FIND-8 |
| INV-001/002: correct owner tiers and shared clients | Existing owners extended; no client durable authority or duplicate orchestrator introduced | PASS; missing authoritative validation separately FIND-10 |
| INV-003/004: scope, tenancy, exact invocation, accepted authority/audit/loss | Signed queued subject and operation-level file staging fixed; accepted caller, RLS, SYSTEM attribution and one Scribe outbox retained | PASS |
| INV-005/007 and AC-004/006: bounded read/compute/resource ownership | Existing sandbox ownership unchanged; runtime admission observation and combined-input accounting remain gaps | FAIL FIND-2/11; no new platform mechanism requested |
| INV-006: invalid evidence cannot manufacture success | Present-invalid JSON can pass an ordinary assertion | FAIL FIND-10 |
| INV-008: final replacement / retained unrelated scenario work | Retired live code and trace-wait API removed; existing scenario/evidence paths remain | PASS |
| AGENTS §16 and AC-009: mandatory changed-item docs | Validated concrete omissions and contradictory recovery prose | FAIL FIND-7 |
| AC-010: verification integrity | Original pre-R1 gate and subsequent focused records are credible for named paths; no weakened check; no fresh candidate aggregate claimed | Recorded proof supported, but source findings block acceptance |

## Verification and limits

[verification-evidence.md](verification-evidence.md) records provenance and the review's exact scope.

- The original full `mise run gate` at `03712af5e` predates R1 and is not a gate pass for this candidate. R1 records focused RED/GREEN, runtime 40/40, SQL 22/22, telemetry and three-language tracing, lint/docs/dependency/tenant checks. These are committed evidence claims inspected against source, not fresh review executions.
- S1 records a live sweep with 42 passed, 1 failed, 0 skipped and explicit missing coverage; it is not universal provider conformance or a passing full live lane. Primary provider protocol sources cited in the validated ledger support FIND-12/13 without review credential access or live requests.
- Recorded external source commits are TS `5c79080ebc144d5a3bc571cf1c9469fe5393b297`, Python `1e27f6873049113fbe937f6abbed74103d58185a`, Rust `669d130b601c0c7dca4c137c8951c792c64b0ef8`. Their objects were resolved and inspected via Git; moving external worktrees and local SDK artifacts were excluded. No source archives were written.
- Review ran `git diff --check f2a726c90...de69cb69b` successfully and repeatedly checked the pinned detached HEAD and clean source. No fresh test, build, generator, live-provider/cloud, benchmark or external-example execution occurred. Test references in reports identify inspected proof source or recorded results.
- No uncommitted original-worktree edit or post-candidate commit closes a finding in this verdict. The owner may already have newer fixes; implementation must reconcile them without reverting work, and a later review must name its new immutable endpoint.

## Remediation and completion gate

[TASK-005-R2-input-capacity-and-provider-corrections.md](TASK-005-R2-input-capacity-and-provider-corrections.md) packages exactly the seven retained findings, including their diagnoses, minimal existing-owner corrections, preserved behavior, explicit non-goals, acceptance and focused proof. Documentation is batched with functional corrections. No full gate, whole-language journeys or broad family aggregates are prescribed for bounded repair; final change review owns integrated qualification.

A later `$wyrd-task-review` must inspect the complete original base-to-new-candidate range against approved r9, the original task, R1/R2, prior verdicts and all stable FIND IDs. TASK-005 remains incomplete until that independent review passes.

Skipped fresh tests/builds and live/external reproduction. Seven validated runtime, input, provider and documentation gaps remain within the named range.
