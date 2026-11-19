# TASK-002 first remediation — independent findings validation

Recommended verdict: **PASS**. **FIND-TASK-002-1 is CLOSED**; the independently validated outstanding finding ledger is **empty**.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Original base: `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e`.
- Prior candidate: `ff78b51b61fea8f215a2139b17840044f65b17e4`.
- Cumulative candidate: `bcf61769e9c7b9b6632ed93521a695ff72dc36f4`; HEAD independently checked at that commit.
- Reviewed `git diff <prior-candidate> <candidate>` in full. Approved specification revision 2 was read from candidate Git; original TASK-002 was read from original-base Git. Working revision 3 is excluded.

Read the review skill, current scope, original verdict and validation, all four prior discovery reports, and the remediation task and its execution evidence separately. Applied the supplied root rules, docs AGENTS/contribution/product rules, spec-driven workflow, maintainer and agent-surface guidance, and the owning Agents/tools/workspaces architecture. No CodeGraph directory exists. The source subject was not changed; this report is the validator's only write.

This is a focused remediation review: prior-finding closure, all changed prose/comment/generated copies, the output producer and model-delivery consumer, fixture consumers, and whether the fix invalidates previously accepted behavior. No new authority, runtime, schema, dependency, SDK implementation or test assertion enters the remediation diff. The unrelated working specification and other task work are excluded.

## Prior finding closure

### FIND-TASK-002-1 — CLOSED

Prior discovery source: `BEH-002-1`; prior classification/validation: **INCORRECT / CONFIRMED**. The original diagnosis and minimal correction remain valid: the public explanation omitted the existing last-expression channel; changing runtime visibility would violate preserved REQ-001 behavior.

Independently traced these candidate sources:

1. `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs::Sandbox::exec` (189 onward) collects printed text using `tool_result_max_bytes`, stopping on a UTF-8 character boundary. `TurnEvent::Complete` returns the worker value separately. It supplies both outputs to the existing invocation owner.
2. `hosted/tools.rs::Invocation::code` (331–375; full body read) receives `(value, output)`, calls `repr(value, tool_result_max_bytes)`, and returns `{value, value_truncated: !whole, output}`. `repr` (466 onward; full body read) uses the capped formatter, preserving Python string repr semantics and reporting whether formatting completed. The CodeRun descriptor (608 onward) explicitly describes printed output and last-expression repr.
3. `crates/vala/vala-verify/src/blocks/tools.rs::Dispatch::call` (612–700; full body read) serializes the complete result under the common result cap. `RecordedTool::invoke` (922–928) calls it with recipient `Model` and delivers the whole returned value. Neither bridge-backed nor ordinary code execution strips the last-expression channel.
4. Server `config.rs` maps `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` through the KiB scale into `tool_result_max_bytes`. Thus the revised per-channel truncation claim names the actual configuration owner. This does not promise two independently maximal channels will fit: the reference already states the general result cap, and the unchanged dispatcher rejects oversized complete JSON rather than delivering it. `value_truncated` describes repr truncation, exactly as the revised text says.

The revised source reference at `docs/src/content/docs/reference/agent-tools-and-authorization.md:34` names `output`, `value`, last-expression repr and `value_truncated`; it expressly warns that printing a nested result or leaving it as the final expression sends it to the model. Its savings paragraph now includes the entire `code.run` result. These statements close the reachable print-free final-expression case from the prior finding without claiming automatic delivery of all intermediate values.

The shared `nested-investigation.py:14` comment now describes both channels and identifies `None` for this program's final `print` expression. The diff changes only its comment, leaving every executable statement and its selected printed output identical. `reduce-notes.py` is unchanged. Existing Rust SDK `include_str!`, Python fixture loading, TypeScript `readFileSync`, MCP `include_str!`, and docs `CodeFromFile` remain the consumers; no alternate fixture or response filter was introduced. The earlier accepted provider assertions and record behavior are therefore preserved. Changes to source text are covered additionally by the supplied Rust SDK/MCP reruns.

The complete generated `docs/public/llms-full.txt` diff reproduces exactly the corrected reference paragraphs and expanded fixture comment. The false general print-only claim is absent from the reference, both fixture programs and generated copy. The remediation packet records normal `docs:generate` and passing `docs:check`; no hand-authored generator, schema or runtime change appears.

The correction meets TASK-002's documentation obligation and approved REQ-001/REQ-014 using the existing owner. Deletion of required guidance would be incomplete; a runtime/client guard, dependency, new harness or API option would be unnecessary. No retained consumer guard or additional correction is needed.

## Acceptance and coverage carry-forward

| Obligation | Source / prior evidence retained or updated | Verification assessment | Result |
| --- | --- | --- | --- |
| Accurate bounded outer output and intermediate visibility; REQ-001/014 | Updated reference/comment/generated copies; independent producer-to-model trace above; original behavior report BEH-002-1 and prior validation | Direct source agreement closes the prior counterexample; supplied docs check | PASS |
| Shared nested preparation/check workflow, existing catalog and direct behavior | Prior `task-review-behavior.md` matrix and original verdict; fixture operations unchanged in remediation diff | Prior focused Python/TS/MCP proof carries forward; supplied Rust SDK/MCP reruns strengthen shared-source preservation | PASS |
| Permission denial, exact code, safe failure record and recovery | Prior behavior and `domain-review-security.md` trace accepted caller, exact tool/downstream checks and zero-byte/no-digest denial; executable calls unchanged | Prior real-principal negative/authorized flows carry forward | PASS |
| Immediate/queued/status/retained records, custom data, historical defaults | Prior `task-review-invariants.md` traces common serde owner, settlement, Python/TS data-only conversion and retained producer/reader | Prior journey/type/default evidence carries forward; no producer, projection or persistence change | PASS |
| Original pinned citations and no scratch/live promotion | Prior behavior/invariant evidence and citation assertions; no declaration, evidence or executable program change | Prior focused public assertions carry forward | PASS |
| Served HTTP/MCP, authored stubs, generated declarations and catalog parity | Prior invariant/standards reports and independent validation; no contract/projection/generated SDK change | Prior served-schema, MCP/type/codegen proof carries forward | PASS |
| Other public guidance: limits, authority/audit, occupancy, failures, upgrade and honest savings | Prior behavior/standards reports; only savings sentence changes to accurately include the whole result | Existing source-backed acceptance remains valid; no universal cost/latency claim added | PASS |
| Ownership, repository rules, generated discipline and non-goals | Prior standards matrix plus complete four-file remediation diff; source documentation/comment/evidence and existing generated projection only | Supplied generation/docs checks and orchestrator range whitespace recheck; no gate weakening | PASS |

No changed dependency or governing authority invalidates these prior acceptance rows. Reopening the full TASK-002 or TASK-001 fault matrix is unwarranted.

## Specialist applicability independently validated

The recorded choice of one fresh `ponytail-rev` is sufficient for this remediation. The prior selected behavior, invariant, repository and security reports are present and cover the contracts carried forward above; prior independent validation also established their coverage. Their conclusions are retained with their source traces, rather than inferred merely from unchanged filenames.

- **Invariant specialist omitted this round:** no state, lifecycle, record/default conversion, retained reader/writer or coordination change. Prior invariant evidence covers each affected public result path.
- **Repository specialist omitted this round:** the new write set is a bounded docs/comment/evidence correction and existing generated projection. The validator checked those applicable rules directly; prior standards coverage for Rust, Python, TypeScript, served contracts and fixtures remains intact.
- **Security specialist omitted this round:** no caller, permission, tenant, capture/access, dispatcher or result-reader change. The output explanation was independently traced through its actual producer and consumer; it describes preserved semantics. Prior accepted-authority and retained-visibility coverage remains applicable.
- **Maintainer specialist omitted:** no ownership/abstraction change, helper or configuration surface. Existing documentation and shared fixture owners suffice.
- **System and additional concurrency/persistent-data specialists omitted:** no executable/deployment, failure-propagation, transaction, concurrency or persistent-state change. Existing recovery/lifecycle evidence is unaffected.

No selected report is missing. No material conflicting claim, unresolved common cause or new blocker was discovered, so no focused follow-up or separate validation of a new finding is required. This first remediation round does not require a convergence assessment.

## Stable ledger and recommendation

| Stable finding | Disposition | Remaining correction |
| --- | --- | --- |
| FIND-TASK-002-1 | CLOSED with independent source proof above | None |

**Outstanding validated finding ledger: empty.** No new IDs, optional improvements, style-only remediation or specification decision are proposed. Recommend **PASS** for TASK-002 at the cumulative candidate.

No builds, tests, generation or database commands were rerun by this validator. Passing docs generation/check and focused Rust SDK/MCP journey results are supplied remediation evidence; range whitespace verification is supplied by the orchestrator. Full suites remain for integrated change review. No new material risk was identified in this explanation-only remediation.
