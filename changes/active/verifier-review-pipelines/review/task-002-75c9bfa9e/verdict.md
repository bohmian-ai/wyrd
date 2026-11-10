# TASK-002 acceptance verdict

**FIX_REQUIRED** — candidate does not yet satisfy the original approved task. Seven independently validated correction boundaries remain; no duplicate production workflow was found and no retained correction needs a new specification decision.

## Immutable subject and review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f` (parent of first implementation commit `973b78e5e`).
Candidate: `75c9bfa9e07163099eac40d5a556c4447c6a519d`, branch `task/verifier-review-pipelines/TASK-002`.
First task review of the complete base-to-candidate triple-dot range, including core production code in the first named commit. No previous TASK-002 acceptance or closure is carried forward; no convergence assessment applies.

Original task: `changes/active/verifier-review-pipelines/tasks/TASK-002-agent-tooling.md`.
Authority: approved specification baseline revision 18 plus explicitly approved corrections and applicable publication/durability supplements. The supplied packet is untracked here, hashed in `authority-sha256.txt`; revision-21 draft wait/approval additions were excluded. Root/nested contributor rules and narrower architecture owners govern task conflicts. See `subject.md` for full navigation, authority, contract producers/consumers and initial specialist applicability.

## Independent reviews and validated coverage

| Selected role | Report | Discovery result |
|---|---|---|
| behavior-rev: complete task and surfaces | [task-review-behavior.md](task-review-behavior.md) | FAIL; B-001/B-002 |
| invariant-rev: call identity, attempts and shared state | [task-review-invariants.md](task-review-invariants.md) | FAIL; INV-002-1 |
| repo-rev: all changed tiers/languages, docs, mandatory gates | [standards-review.md](standards-review.md) | FAIL; SSR-001–004 |
| maintainer-rev: complete workflow ownership/reuse | [maintainer-review.md](maintainer-review.md) | PASS; no duplication finding |
| system-rev: deployed direct/queued failure and recovery | [system-review.md](system-review.md) | PASS within scope |
| domain-rev security: accepted authority, tenancy, receiving permissions/audit | [domain-review-security.md](domain-review-security.md) | FAIL; SEC-001 |
| domain-rev data: durable publication identity/receipts/exact descriptors | [domain-review-data.md](domain-review-data.md) | FAIL; DATA-001 |
| domain-rev concurrency: shared writers, byte limits, cancellation/resources | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS within scope |
| followup-rev: File grant versus receiving decision/audit | [followup-review.md](followup-review.md) | RESOLVED; FOLLOWUP-SEC-1 |
| fresh ponytail-rev: independent source/materiality/correction validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED recommendation; seven retained boundaries |

The fresh validator independently confirmed specialist applicability and report coverage, including unchanged dependencies. No selected report is missing. UI and external provider/auth-protocol specialists are inapplicable; no such contract changed. No report substitutes a generic PASS for a specifically unexamined failure ordering. The one follow-up resolved a material File authorization/audit uncertainty; unique identity/lifecycle findings went directly to validation without quota-driven rediscovery. `claim-comparison.md` records routing; only confirmed/revised proposals appear below.

## Validated finding ledger

| Stable ID | Source IDs | Disposition / classification | Required correction boundary |
|---|---|---|---|
| FIND-TASK-002-1 | DATA-001 | CONFIRMED / INCORRECT | RunState receipt must reject changed names/arguments for an already accepted original call. |
| FIND-TASK-002-2 | INV-002-1 | CONFIRMED / INCORRECT | Existing invocation/forget authority must preserve spent producer attempts with retained publication effects across Busy deferral. |
| FIND-TASK-002-3 | SEC-001, FOLLOWUP-SEC-1 | REVISED / MISSING | Existing receiving path/File publication boundary must reuse its File-domain decision and best-effort audit. |
| FIND-TASK-002-4 | B-001 | REVISED / MISSING | Close only missing per-surface descriptor/invoke/domain, CLI and HTTP/schema acceptance assertions. |
| FIND-TASK-002-5 | B-002, SSR-002 | REVISED / MISSING, VIOLATION | Put deterministic private lifecycle proofs in the server tier, exercise actual lost reply and unavailable inventory, preserve public journeys and remove forbidden SDK controls/digest computation. |
| FIND-TASK-002-6 | SSR-004 | CONFIRMED / MISSING | Add required Agent-loop cumulative write-budget refusal/readback proof. |
| FIND-TASK-002-7 | SSR-001/import rule | CONFIRMED / VIOLATION | Batch changed-helper panic documentation and remaining import corrections with behavioral/proof work. |

The complete source trace, violated obligation, consequence, materiality, correction burden, affected siblings, exact locations and focused proof for each retained ID are in `findings-validation.md` and repeated self-contained in the remediation task. No orchestrator-originated finding or human claim received a FIND ID without independent validation. No new blocker was discovered by the validator.

## Reconciled acceptance matrix


| Obligation | Reconciled result | Evidence / remaining closure |
|---|---|---|
| Typed write, safe parents/paths, atomic regular-file replacement, shared writer/budget and isolated scratch | PASS implementation; required public budget proof FAIL | Workspace write and tests; real scratch assertions; FIND-6 |
| Publish one source, null, logical name, host producer; arbitrary file/tree names and immutable snapshots | PASS | Invocation args/dispatch; Publications/Workspace owners; actual public receipt/manifest/content assertions |
| Replay/conflicting original call; distinct fresh attempts and conserved recovery bounds | FAIL | FIND-1, FIND-2; identical replay and ordinary retry proofs otherwise valid |
| Dedicated tool then called-domain authorization/audit under accepted authority | FAIL File path boundary | FIND-3; other resource decisions and snapshot policy remain valid |
| Descriptor selection before model, identity, no check publication/no standalone interface | PASS implementation/Rust proof; mirrored proof FAIL | Catalog/validator/ToolSession and selected-tools assertions; FIND-4 |
| Direct/queued share state; exact descriptors; missing publication blocks consumer; cancellation fences | PASS for inspected implementation and ordinary paths | Shared Arc, TenantEvidence published body, NO-PUBLISH/cancellation/readback assertions |
| Lost reply, unavailable inventory, recovered Agent consumer and disconnect evidence at correct tier | FAIL proof | FIND-5; Task-001 source/tests retained as foundation evidence |
| Equivalent Python/TS/CLI/MCP and HTTP/OpenAPI closure | FAIL proof | FIND-4; existing happy-path typed content and backend route/page assertions carry into remediation |
| Bounded records, paging, receipts/digests; SDK proof conventions | PASS implementation; proof convention FAIL | RecordedTool full body/paged-read assertions; FIND-5 |
| No second lifecycle/store/loop; thin language projections; no remote integration acquisition | PASS | Whole-workflow comparison and full changed-owner diff; no manifest/dependency changes |
| Changed-item rustdoc/import rules | FAIL | FIND-7 |
| Original TDD sequence | Historical FAIL/deviation | SSR-003; irreparable history, no fabricated test fix |
| Format/lints/codegen/deps/tenant/targeted execution claims | Supplied green evidence, not reviewer reruns | Root independently checked diff; docs:check and ts:napi:check have no recorded result and remain evidence limits, not asserted failing commands |


## Authority resolutions and historical deviation

The independent whole-workflow comparison found earned extensions of existing owners: Agent loop → RecordedTool → Invocation → Publications → Workspace/File completion → RunState → exact-descriptor successor reads. `invoke_call` supplies missing provider identity; Role carries step/check context; record consolidates existing dispatch; no duplicate Agent loop, registry, snapshot, store, upload or replay workflow is established.

The security authority freezes accepted execution authority; arbitrary later role changes do not refresh it. Correct the private task's conflicting between-call grant-removal sentence through `$wyrd-plan`. Do not implement a second resolver, live grant intersection or new permission identity. Actual principal revocation/cancellation and initial narrowed/delegated checks are separate. No SPEC_REVISION_REQUIRED is inferred from this invalid private instruction; a future explicit request to change the approved security contract would require revision.

The approved publication supplement permits code.run to produce binary scratch and output-only Agents without artificial mounts. Same-invocation scratch reading follows that approved capability. The human-supplied drift interpretation was rejected; do not revert valid behavior. No new capture policy or speculative credential sanitizer is prescribed.

SSR-003 confirms historical production-before-RED. This remains a failed historical execution/deviation row; current green cannot repair it. Preserve the admission, do not fabricate RED or rewrite history, and apply genuine RED/GREEN to actual corrections. It is not a new behavioral finding or an added approval stage. The original all-PASS evidence table is not accepted completion and remains preserved as reviewed authority; use this reconciled matrix and accurate later implementation evidence.

## Verification, immutability, and handoff

The implementer records seven Rust journeys, Python two tests, TS one, MCP one, Rust owner checks and Python/TS checks green. Reviewers inspected their actual assertions and traced source; they did not rerun runtime suites, broad gates or mutating generators. Root independently ran full-range `git diff --check`: clean. No recorded docs:check or ts:napi:check result was supplied; this is a limit, not a claim those checks fail. Specific missing negative/failure proofs are retained above rather than hidden in test names.

HEAD, tracked candidate contents and all four recorded authority hashes were rechecked unchanged after review. Only reports and this remediation task were written. No implementation, commit, merge, push or deployment occurred.

One self-contained remediation: [TASK-002-R1-publication-integrity-and-proof.md](TASK-002-R1-publication-integrity-and-proof.md), routed to `$wyrd-implement`. It preserves the original task/candidate/verdict and seven IDs, selects existing owners and focused closure, batches mandatory documentation, and excludes full-suite/broad aggregate reruns. Future review checks closure and the candidate-to-candidate fix diff with unaffected source-backed acceptance carried forward.
