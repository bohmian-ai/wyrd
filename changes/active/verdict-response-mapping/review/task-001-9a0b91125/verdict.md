# TASK-001 review verdict: FIX_REQUIRED

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Approved specification: SPEC-verdict-response-mapping revision 1, recorded at base `08998b9e9b6f04825f7743b877391247e0ad9621`.
- Original task: `changes/active/verdict-response-mapping/tasks/TASK-001-jev-verdict-checks.md` at that base; execution evidence read at candidate.
- Candidate: `9a0b9112523252328813041c0b11be375f0eed29`.
- Actual diff: `git diff 08998b9e9...9a0b91125`; nine implementation commits beginning `5a3caa8b3`. The base itself is approval/planning, not implementation.
- Source inspected in clean detached checkout `/tmp/wyrd-task-review-9a0b91125`; its HEAD and tracked source were checked unchanged. Shared-checkout HEAD `570c079570b47f654424524810139b2cff15c870` and unrelated edits were excluded. Reports live in the shared checkout and do not change the candidate.
- First review: no prior findings, remediation candidate or convergence assessment to carry forward.

Scope is the Jev leaf-policy workflow assigned to TASK-001. AllTransform aggregation and Google Decisions remain TASK-002/TASK-003. No implementation, merge, push or deployment was performed.

## Authority and independent assignments

Governing references are the approved spec and original task, root and changed-directory AGENTS, `changes/spec-driven-development.md`, `changes/implementation-execution.md`, `agent_docs/index.md` and applicable Rust/error/agent-surface/maintainer guides, and TESTING.md. Architecture owners are Prompt, Verifier, In-memory authoring, Recording and eventual consistency in `architecture/wyrd-design.md`; Storage format and Variant in `architecture/bifrost-design.md`; trust, tenant and audit boundaries in `architecture/wyrd-security-posture.md`.

| Reviewer | Assigned boundary | Report |
| --- | --- | --- |
| behavior-rev | Assigned behavior/scenarios, caller-to-result flow, ordinary regressions, repository rules, documentation and TDD | [task-review-behavior.md](task-review-behavior.md) |
| repo-rev | SDK/native contracts, construction/coercion, exports/stubs/generated projections, shared capture and authoring allocations | [task-review-repo.md](task-review-repo.md) |
| system-rev | Runtime response ownership, malformed-answer propagation, capture/recording and tenant reads, registration resolution, waits and benchmark qualification | [task-review-system.md](task-review-system.md) |
| fresh ponytail-rev | Independent coverage/proof reconciliation and source validation of every retained finding | [findings-validation.md](findings-validation.md) |

The caller orchestrated and packaged the independent reports. All selected reports are present. Separate invariant/maintainer/domain reviewers were omitted because no new shared-state, recovery, storage or authorization mechanism was introduced: primary and system review explicitly covered the affected lifecycle/tenant consumers, and repo review covered cross-tier ownership. No additional vote is required. CodeGraph confirmed the detached checkout unindexed; direct source inspection was therefore used without creating an index.

Navigation followed Prompt construction → client authoring/loader → shared AgentCheck/policy validation → authoritative registration → Vala invocation/native final response → transformer → existing CheckOutput/capture/staging → SDK/MCP details consumers.

## Reconciled acceptance matrix

PASS rows use inspected source/assertions and recorded focused execution evidence; they do not claim freshly rerun native suites. Finding numbers below refer to stable `FIND-TASK-001-<n>` IDs in the validation report.

| Requirement / scenario | Implementation and inspected proof | Result |
| --- | --- | --- |
| REQ-001/002 Jev; scenarios 1–2, J1/J8 | Ordinary Prompt builder, shared native lowering; Rust/Python/TS Choice journeys compare typed/native/mixed forms, stored Prompt/policy and redeclaration. Public export test blocks Pydantic for dictionary use. | PASS behavioral semantics; allocation qualification fails finding 5. |
| REQ-003 Jev; scenario 1, J1 | SDK journeys assert actual `/v1/systemone` request, literal quoted/newline state, questions/resolved model and one call per check. | PASS |
| REQ-005; scenarios 1–4/6–7 | AgentCheck retains policy through compiled/reference handling and stored Verifier; absent-policy branch preserves direct Verdict. | FAIL — findings 4 and 7 |
| REQ-006; scenarios 1/3 | Choice mapping owner tests and hosted custom labels, low-confidence pass/high-confidence failure, exact generated reason and actual. | PASS |
| REQ-007; scenario 3 | Shared threshold logic, owner boundary tests and hosted true/false polarity, inclusive bounds and uncertainty interval. | PASS |
| REQ-008; scenario 3 | Shared scalar operator semantics; hosted Score 2 versus 1.8 on three levels and owner arithmetic tests. | PASS arithmetic; malformed metadata fails finding 1 under REQ-010. |
| REQ-010; scenarios 3/5 | Hosted missing answer, wrong tag, unknown label and invalid Noul/Score isolate the affected requirement; existing refusal/retry owners retained. | FAIL — finding 1 |
| REQ-011; scenario 4, J6 | Shared local/sibling/registered validation; raw HTTP bypass and all SDK exact-CardRef registration refusal/correction assert no invalid upstream call. | FAIL — findings 2 and 3 |
| REQ-012; scenarios 1/3/6 | Generated reason/native actual, full/hash retained reads, existing capture/recording owner and tenant authority preserved. | FAIL — required MCP consumer proof missing, finding 7 |
| REQ-013; scenarios 2/4/7 | Public exports/stubs/generated leaf union, schema owner assertion and journey-tested refund example inspected. | FAIL — findings 3, 4 and 7 |
| REQ-014; scenarios 5–7, J7/J8 | Original direct-Verdict branch, recorded combined 48 regression journeys, existing native Jev refusal/retry and foreign-tenant retained-read 404 assertions. | PASS covered existing workflows |
| SDK/server allocation constraint | Shared CardDefinition captures and runtime response Arc reused; new full-request validation materialization and SDK conversions identified. | FAIL — finding 5 |
| Hot-path waits/performance | Zero added/moved hot-path PG/Scribe/object/Vault waits; two new non-hot authoritative registration SQL read sites documented below. | PASS wait inventory; benchmark gate withdrawn by maintainer |

## Validated blockers and remediation

Post-review direction: the maintainer explicitly rejected the benchmark-run requirement. FIND-TASK-001-6 is withdrawn from acceptance and remediation; six blockers remain. This current instruction overrides the packet benchmark gate for this review. The historical candidate and discovery reports remain unchanged.

The complete finding records, classifications, concrete triggers, authorities and independent validation are retained once in [findings-validation.md](findings-validation.md):

| ID | Validated issue |
| --- | --- |
| FIND-TASK-001-1 | Invalid Score confidence/probability metadata can yield a passed judgment. |
| FIND-TASK-001-2 | Typed Python probability construction accepts invalid self-contained bounds. |
| FIND-TASK-001-3 | TypeScript policy rejection discards structured Wyrd diagnostics. |
| FIND-TASK-001-4 | The advertised optional null policy is rejected by runtime decoders. |
| FIND-TASK-001-5 | Required authoring allocation qualification is absent. |
| FIND-TASK-001-6 | WITHDRAWN: maintainer rejected this benchmark gate after review; no remediation required. |
| FIND-TASK-001-7 | Applicable MCP execution/details projection proof was incorrectly excluded. |

All remain within settled approved behavior. Implement only [TASK-001-R1-jev-validation-and-proof.md](TASK-001-R1-jev-validation-and-proof.md) when requested. Evidence gaps alone do not authorize production redesign. No spec revision is required. Saved-bundle-specific reconstruction and weaker exact Prompt/Agent assertions were considered; neither is retained as an additional independent defect. Focused MCP closure should assert the existing provenance.

## Added/moved wait inventory

Reconciled from the system report and independently accepted by the validator:

| Changed boundary | Added/moved PostgreSQL, Scribe, object-storage or Vault wait | Authority |
| --- | --- | --- |
| `vala-verify/src/blocks/agent.rs` invocation/transform | None; existing invoke await remains, native response Arc retention and conversion are synchronous. | Approved in-memory conversion constraint; existing invocation owners unchanged. |
| `vala-verify/src/blocks/transform.rs` | None; borrowed selection, comparison and selected-answer capture are synchronous. | Approved no-additional-IO constraint. |
| `blocks.rs` / `blocks/engine.rs`, existing recording and gateway consumers | None; check traversal changes reuse existing capture and process Scribe staging. | Recording and eventual consistency; no audit acknowledgement or durable-handler write added. |
| `wyrd-server/src/components/cards/service.rs:2034` | Added Agent lookup per distinct referenced Agent UID per Verifier declaration. | Non-hot authoritative registration resolution, REQ-011, existing TenantConn owner. |
| Same file, line 2052 | Added Prompt lookup per distinct referenced Prompt UID per Verifier declaration. | Same non-hot registration boundary; UID deduplication reused within that validation. |

The complete added/moved hot-path wait set is empty. Registration reads are not execution-cache misses. No new runtime, pool, migration, evidence table or storage/recovery owner was introduced. The maintainer has rejected the benchmark gate; no benchmark run is required to close this review.

## Behavioral verification, TDD and limits

Recorded SDK journey results and actual assertions were inspected, including the reported 18 passing Python and 18 passing TS cases, Rust hosted proof, and recorded targeted checks. The fresh validator and repo reviewer each reproduced invalid typed probability construction against the exact candidate Python source using installed Pydantic. That reproduction is not a candidate native-wheel build. No native SDK journeys, Cargo checks, codegen, benchmarks or broad suites were rerun during review; no performance or real-model judgment-quality claim is made.

TDD is separate from behavioral acceptance. Scenarios 3/5/6 gained hosted proof after scenario 1 introduced their behavior; no historical RED is established. The task permits recording already-working behavior without manufacturing RED. Preserve the chronology and test-first deviation; require neither retrospective RED nor production rollback. Compilation/setup failures are not behavioral RED; scenario 1 also records the real missing-structured-Verdict failure. Scenario 4's disabled-validator check is mutation proof, not historical RED. Available notes do not independently establish full unexpected-failure diagnostic-process compliance; no retroactive code correction is prescribed.

Acceptance remains FIX_REQUIRED for the six remaining validated blockers, not for irreparable historical process evidence.
