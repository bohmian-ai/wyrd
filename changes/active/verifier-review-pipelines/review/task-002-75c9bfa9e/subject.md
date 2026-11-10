# TASK-002 immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow
Base: 990a85dc7155ff0d4de1acc17e4aeaa7a42d582f
Candidate: 75c9bfa9e07163099eac40d5a556c4447c6a519d
Branch: task/verifier-review-pipelines/TASK-002
First acceptance review; full `git diff 990a85dc7155ff0d4de1acc17e4aeaa7a42d582f...75c9bfa9e07163099eac40d5a556c4447c6a519d`.
The user named implementation commits 973b78e5e through 75c9bfa9e; 973b78e5e contains core production code, so its parent is the base.
No prior TASK-002 acceptance evidence is carried forward. TASK-001 reports are foundation context, not TASK-002 acceptance.
Source must stay immutable. Only assigned review reports may be written. No implementation, commits, or source edits.

## Authority and evidence

Original task: changes/active/verifier-review-pipelines/tasks/TASK-002-agent-tooling.md.
Specification: changes/active/verifier-review-pipelines/spec.md, approved baseline revision 18 plus explicitly approved corrections, not unrelated draft wait/approval additions. Read applicable publication and durability supplements linked by task. Packet is untracked in this worktree and supplied as review authority; record its hashes before and after review. Its current frontmatter says revision 21/draft and approved_baseline_revision 18; distinguish approved baseline from draft additions explicitly.
Read root AGENTS.md, changes/spec-driven-development.md, agent_docs/maintainer-style.md, applicable guides from agent_docs/index.md, architecture/README.md and owning sections, TESTING.md, nested rules in docs and Python/TS SDKs. Use mise for commands. CodeGraph exists: use `mise exec -- codegraph explore "<focused symbol/file query>"` before locating/reading code. No new navigation scripts.
Verification claims are in original task Implementation Evidence; treat them as claims, inspect source/tests and report what was actually verified. No need to rerun broad aggregates for static review. Never modify proof to clear a failure.

## Shared navigation map (starting points, not coverage conclusions)

- Contract: wyrd-spec card/agent.rs BuiltinTool, writes_scratch; card/verifier/validate.rs tool selection/workspace/check validation. Consumers AgentSpec tool_names, permission scopes, generated TS card-types and authored Python verify stub.
- Invocation: skald-tool trait_.rs AgentTool::invoke_call and skald-agent loop_runtime.rs provider tool-call dispatch. Existing ToolRegistry/ToolResolver and all wrappers are sibling consumers to trace.
- vala-verify blocks/agent.rs AgentExecutor::invoke, Role, ActionExecutor/CheckExecutor; blocks.rs BlockCall attempt; blocks/engine.rs producer attempt propagation; blocks/tools.rs AgentToolset, ToolSession, RecordedTool::record and invoke_call. Existing Agent loop and recording remain reuse comparison candidates.
- Server: verification/hosted/blocks.rs HostedBlocks composition; hosted/tools.rs HostedTools::open, Invocation, Builtin descriptors and dispatch; workspace.rs Workspaces/Workspace writer, scratch, safe paths and byte budget; publications.rs existing Publications, execution run state acceptance/replay, File completion/upload are reuse comparison owners. Compare whole workflows and sibling callers, not only leaf calls.
- Proofs: Rust SDK tests/integration/hosted_agent_tools.rs (seven journeys), fixtures/cards/hosted_agent_tools; server workspace/tests.rs, tools/pg_tests.rs and pg_hosted_artifacts/files. Python integration test_hosted_agent_tools.py plus changed conftest/support, TS hosted-agent-tools.test.ts, MCP tests/bifrost/mcp/verification.rs. Trace HTTP/OpenAPI/CLI proof references in task to actual assertions.
- Docs: architecture/wyrd-design.md changed Agent tool/workspace contract, docs reference agent-tools-and-authorization.md and generated llms-full.txt.

## Specialist applicability (source-backed initial selection)

| Contract/invariant | Owners, producers, consumers and change evidence | Selected review |
|---|---|---|
| Full task behavior, all acceptance/non-goals and reuse | TASK-002 scenarios 1 and proof 1–6/lifecycle/public surfaces; 41-file full diff | behavior-rev comprehensive |
| Call identity, attempt, shared run state and immutable publication receipt | AgentTool::invoke_call new trait seam -> loop -> RecordedTool -> Builtin -> existing Publications; BlockCall.attempt -> Producer; shared execution passed in HostedBlocks | invariant-rev |
| Cross-tier standards, documentation, language projections and mandatory gates | Rust spec/Skald/Vala/server, three SDKs, MCP, docs and generated artifacts in diff | repo-rev |
| Ownership and duplication | New invoke_call hook, Role, record helper, HostedTools execution binding, workspace write and publish dispatch; existing lifecycle/store/registry owners | maintainer-rev; explicitly compare complete workflows for duplication |
| Failure propagation, cancellation, disconnect and queued recovery | Task requires direct disconnect and publication recovery; changed invocation lifecycle plus new lifecycle journey | system-rev |
| RBAC, tenancy, credentials and trust boundaries | New protected fs.write/publish dispatch, accepted caller authority, domain File permission, scratch read/write exposure, tool arguments | domain-rev-security |
| Persistent output identity/integrity and acceptance | New publish adapter calls existing Task-001 Publications and retained run state, durable ACK receipt, arbitrary file/tree/JSON sources | domain-rev-data |
| Concurrent scratch mutations and bounded resources | New Workspace::write uses shared writer/byte budget; concurrent tool calls and publication snapshots share authority | domain-rev-concurrency |

No UI/accessibility specialist: no UI changed and no task UI obligation. No provider-protocol standards specialist: provider-call identity changes are internal dispatch and must be checked by behavior/invariant reviewers; no OIDC/JWT protocol change. No GitHub integration specialist: task explicitly excludes integration acquisition and connection implementations.
All selected reports are required. Independent fresh ponytail-rev follows discovery and any necessary focused follow-up, validates applicability and every finding without intended verdict. Orchestrator only reconciles validated reports.
