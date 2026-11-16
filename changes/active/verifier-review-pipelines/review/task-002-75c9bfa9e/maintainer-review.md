# TASK-002 maintainer review

Result: **PASS** within the assigned ownership, abstraction, workflow reuse, and maintainability scope. No material finding proposed.

## Immutable subject and authority

- Base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
- Candidate: `75c9bfa9e07163099eac40d5a556c4447c6a519d`; HEAD matched before and after inspection.
- First review of the full base-to-candidate diff; no prior TASK-002 acceptance carried forward.
- Original authority: `tasks/TASK-002-agent-tooling.md`, approved baseline revision 18 of `spec.md`, publication contract, and approved durability supplement revision 2. The current spec frontmatter's revision 21/draft status does not authorize unrelated draft wait/approval behavior. None was used as acceptance authority here.
- Read root instructions, task-review skill, `changes/spec-driven-development.md`, `agent_docs/{index,maintainer-style,rust}.md`, architecture authority map and Agent/tools/workspace section, applicable Python/TS and docs directory rules, and relevant testing guidance. CodeGraph was consulted before source navigation; source was then followed across actual callers and siblings.

The following authority hashes were identical before and after review:

| File under `changes/active/verifier-review-pipelines/` | SHA-256 |
|---|---|
| `spec.md` | `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683` |
| `output-publication.md` | `a02ceaf9c6f40f07e5995925161c64dc38fb7563471e993d3212810533b773e9` |
| `execution-state-and-durability.md` | `08c4bf050e2f73cf5bb4eabcd91d412c3e73cf727b1a415165d504e9cb804d9f` |
| `tasks/TASK-002-agent-tooling.md` | `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe` |

## Changed-surface coverage and reuse comparison

| Changed surface | Source, callers, siblings and proof inspected | Assessment |
|---|---|---|
| `AgentTool::invoke_call` and loop dispatch | `skald-tool/src/trait_.rs:24–35`; `skald-agent/src/loop_runtime.rs:590–665`; existing `ToolDef::invoke`, `AgentDelegateTool::invoke`, `ToolRegistry::{register,resolve}`; new replay journey | The old invocation API cannot convey the original provider call ID. The default method preserves existing tools' behavior; only recording and hosted built-ins override it. The existing loop still owns callbacks, journal, dispatch, and result processing. No second executor or registry. |
| `AgentExecutor::invoke`, `Role`, action/check implementations | `vala-verify/src/blocks/agent.rs:326–576`, `BlockCall.attempt`, engine action/retry and requirement call producers, changed integration call construction | One shared invoke body still resolves the Prompt, opens a ToolSession, wraps tools, runs the existing Agent, and returns provenance. `Role` is a closed value distinguishing producer attribution from check citation reachability. It holds no execution state and creates no parallel step/check workflow. Supplying producer context is required for host-derived publication identity. |
| `RecordedTool::{invoke,invoke_call,record}` | `vala-verify/src/blocks/tools.rs` session resolution, complete recording body, `finished`, refusal observer, bounded serializer; Rust public journey's receipt digest/byte/error assertions | `record` is a useful consolidation: both entry points keep the same ordering, result bound, digest, error and capacity handling. Deleting it would duplicate this workflow. Existing ToolRegistry resolution and provider-name conversion remain in their prior owners. |
| `HostedBlocks` and `HostedTools` execution binding | `hosted/blocks.rs:140–188`; `hosted/tools.rs` fields, constructor, `with_execution`, complete `open` path, Invocation dispatch and built-in wrappers | The engine's existing `Arc<RunState>` is shared with both TenantEvidence and tools. The new field is a reference to the existing authority, not a second ledger. The optional binding also preserves tooling without an execution; publish refuses there. Existing workspace admission, lifetime guard, registry and Python spend owner remain intact. |
| `Invocation::publish`, args and descriptor | `hosted/tools.rs:307–379,515–565,593–629,676–712`; existing `Publications::{json,path,receipt,accept,complete}` workflow; publication/read mounting through TenantEvidence and Workspace | The adapter parses sources and constructs host-derived `PublishCall`, delegates to the pre-existing backend, and shapes its small receipt. It does not freeze, upload, accept, store, replay, or recover independently. Source exclusivity belongs at this new tool boundary; null-preserving deserialization avoids losing a supported JSON source. Existing PublicationName validation is reused. |
| `Workspace::write` | Complete method at `workspace.rs:1033–1122`; existing `writer`, `relative`, `work_room`, `extract`, `freeze/snapshot`, materialization and sandbox mount paths; write unit test and Rust scratch journey | The existing Workspace owns private files and budget, so this is the correct extension. `extract` is not an interchangeable write workflow: it requires extraction input, refuses replacement, and returns extraction provenance. New write requires safe parent checks, replacement accounting and UTF-8 content. Both reuse the existing lock, confinement grammar, budget and tempfile dependency. No new filesystem/state service is justified or introduced. |
| Built-in selection/validation | `wyrd-spec/card/agent.rs` catalog and workspace predicates; `card/verifier/validate.rs` tool selection; runtime `HostedTools::open`; ToolSession resolution; selection fixtures | The catalog remains the built-in identity owner. Declaration-time validation and runtime opening have different trust/lifecycle roles; their small workspace refusal checks are not competing policy engines. Unknown/repeated spellings and step-only publication remain in existing validation rather than a new loader. |
| Cross-language declarations and proof setup | Python authored `BuiltinTool` literal; generated TS catalog; Python conftest drafter/consumer dispatch and support readers; Python and TS hosted-tool journey assertions; Rust `PublishDesk` setup/restart/readback and script helpers | The declarations add exactly the two catalog names. The fixture scripts simulate the external provider, while public execution surfaces perform actual storage and state operations. Python setup extends its existing shared receiver; Rust setup reuses Deployment, gateway, grants and Card helpers; TS reuses serverTest and assistant support. Similar protocol scenarios across languages prove their independent public surfaces and are not duplicated production orchestration. |

## Task-alignment matrix for this scope

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| One Agent loop, ToolSession, registry and recorded-tool flow | Existing `Agent::run_prompt`, ToolRegistry and ToolSession retained; identity passes through the new default hook | Rust replay and limit/provenance assertions; existing ToolDef and delegate implementations remain compatible with default dispatch | PASS |
| No separate publication snapshot/store/replay/state owner | `Invocation::publish` delegates to unchanged Publications; shared RunState passed in HostedBlocks | Direct/queued lifecycle journey inspects restored content and ensures drafter is not rerun; replay journey checks separate attempts and same-call receipts | PASS |
| Extend existing workspace writer and budget for scratch and overwrite | `Workspace::write` plus existing writer/work_room; HostedTools opens scratch through existing Workspaces | Unit cases inspect replacement accounting, unsafe parents/targets and untouched forbidden paths; real scratch workflow uses public SDK | PASS |
| Preserve built-in wire identity and first-model selection | BuiltinTool catalog, validator and existing provider-name conversion | Invalid selection fixtures plus selected-tool journey; Python/TS offered-descriptor assertions | PASS |
| SDKs project server-owned behavior | Only enum declarations and integration proof helpers added to Python/TS surfaces | Runtime file/tree/JSON/null consumption and public publication/download readback assertions | PASS |

## Material findings and calibration

None. In particular, a dependency-bearing type alone would not establish acceptable reuse. The complete changed publication path was compared: Agent loop → RecordedTool → hosted Invocation → existing Publications → existing Workspace snapshot/shared File completion → existing RunState acceptance. The candidate adds the missing dispatch and identity seams and does not reproduce the underlying lifecycle.

No preference-only refactor is requested. Combining the new write method with extraction would require making an existing extraction-specific operation more generic while preserving different replacement, validation and provenance contracts; there is no source-proven maintenance defect that earns that change. Removing the Role value or per-call Publications construction would likewise select an equally conventional private shape without deleting a duplicate workflow.

## Verification limits

This was a static specialist audit, not an independent rerun of the task's integration/format/lint claims. `git diff --check 990a85dc...75c9bfa9e` passed. No source or authority was edited. Runtime security, concurrent-state correctness, failure/recovery completeness and full public-surface acceptance remain the respective selected reviewers' obligations; this PASS does not stand in for those reports.
