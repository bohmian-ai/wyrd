---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-software-factory
spec_revision: 2
spec_status: approved
requirements: [REQ-008, REQ-009, REQ-011, REQ-014, INV-005, INV-006, INV-007, INV-008, AC-007, AC-008, AC-013]
depends_on: [TASK-003]
---

# Share memory through native agent tools and scoped preferences

## Outcome and Value

A correction made once can guide later Claude Code and Codex sessions. The
maintainer decides whether feedback applies only to this task, its repository,
or all repositories, and can inspect or remove what the application remembers.
For example, a repository-scoped preference against unnecessary traits reaches
the next implementor and reviewer without copying it into both conversations.
Agents can also query current packet context, search relevant memory, and save
packet discoveries/checkpoints using three local MCP tools backed by that same
store. Compacted or replacement sessions can retrieve the latest state.

## What Changes and What Must Stay True

Expose TASK-003's persisted messages/context through the local MCP interface;
the existing service and SQLite remain its owners. Task scope is the default.
Every remembered preference has its source,
scope, enabled/superseded state, and author visible. An agent suggestion cannot
become a permanent instruction without the maintainer's action. Keep packet
discoveries and handoff assertions separate from explicit preference authority.
Do not modify AGENTS.md, CLAUDE.md, or skill files automatically.

The public tools are factory_context() for current task/preferences/decisions,
memory_search(query) for additional scoped records, and
memory_record(content, kind) for packet note/discovery/checkpoint writes.
Connection identity fixes repository, packet, task, attempt, and agent role.
An agent cannot choose a foreign scope, establish a global preference, or
change task status/verdicts through these tools. Initial reviewer access must
exclude implementor checkpoints and unverified narratives, including search
results, while retaining the review skill's required evidence.

## Approach

1. Add explicit feedback scope and preference provenance to current messages.
2. Provide preference inspection, edit, disable, supersede, and delete in UI.
3. Expose the three MCP tools through a local stdio entrypoint forwarding to
   the existing service; bind credentials/scope/role per attempt and propagate
   the proper scoped access to native delegated agents.
4. Extend the single context owner with scoped preferences and bounded older
   note retrieval; persist direct context and tool results supplied per attempt.
5. Preserve repository authority, independent review, and explicit decisions.
6. Verify actual tool use and refresh across tasks, both harnesses, native
   compaction, and replacement sessions.

## Ordered Implementation Scenarios

### Scenario 1 — Remember only the scope the maintainer selected

**Behavior.** Default feedback reaches only its task. Explicit repository
feedback reaches later tasks in that repository but not another repository.
Explicit global feedback reaches both. Claude and Codex receive the same
applicable entries. A suggested preference remains unapproved until selected.

**RED.** Add the planned scoped-memory journey using two temporary repositories
and later sessions from both native-process fixtures. Assert actual saved
context rather than an agent's claim that it remembered.

**GREEN.** Persist explicit scope/provenance and supply applicable entries
through the existing context owner.

**REFACTOR.** Use SQLite scope queries and existing feedback actions; no
embedding pipeline, agent persona service, or duplicate vendor memory.

### Scenario 2 — Correct or remove an earlier preference

**Behavior.** The maintainer can view source messages and edit, disable,
supersede, or delete a remembered entry. Later attempts use the current enabled
entries; historical context remains an accurate record of what was supplied
then. A repository-specific replacement can explicitly supersede a general
entry for that repository. Do not guess semantic equivalence between unrelated
entries; unresolved conflicts are exposed for a decision.

**RED.** Add the planned preference-lifecycle integration test. Assert later
context excludes removed/superseded instructions, another repository retains an
unaffected global preference, and prior attempt snapshots remain unchanged.

**GREEN.** Add preference lifecycle actions and explicit replacement linkage,
with audit history that does not silently reinstate deleted instructions.

**REFACTOR.** Keep one preference record owner; avoid a generated rules file
that can drift from the editable source.

### Scenario 3 — Supply useful bounded context without promoting guesses

**Behavior.** A fresh session receives applicable explicit rules, unresolved
packet decisions, actual task/review paths, and a bounded selection of older
notes. Expose the context budget and omitted-material references. If mandatory
instructions alone exceed the budget, show the conflict for the maintainer
rather than silently dropping rules. A handoff assertion cannot become verified
progress or override approved behavior. A material repository-rule conflict
blocks the affected action and asks for resolution.

**RED.** Add the planned context-authority test with a large note history,
conflicting instructions, stale handoff assertions, and mandatory-context
overflow. Inspect the saved input for both adapters.

**GREEN.** Use scoped/tagged/SQLite full-text retrieval for optional notes,
preserve explicit authority, and reuse the existing decision flow.

**REFACTOR.** Keep native compaction and just-in-time file reading; do not
inject the full conversation archive or infer a hidden personal profile.

### Scenario 4 — Call shared memory without expanding agent authority

**Behavior.** Claude Code and Codex, including appropriately scoped native
subagents, can call factory_context and memory_search and persist a packet
note/discovery/checkpoint with memory_record. Results are bounded and include
provenance and asserted/verified distinctions. A successful write returns its
record ID and survives replacement. A failed write is an error, not an
acknowledged checkpoint. Reject empty/invalid input, foreign scope requests,
role elevation, and credentials from an ended attempt. Agent notes cannot
modify controller facts, create approved preferences, or alter a verdict.
Reviewer search/context excludes implementation narratives that would anchor
independent review; required evidence and explicit maintainer decisions remain.

**RED.** Add the focused MCP-boundary test below against the actual local MCP
entrypoint and factory service with SQLite. Use two packet connections and
distinct implementor/reviewer/delegated roles. Assert real returned records,
write durability, input validation, closed-attempt refusal, role/scope
separation, review filtering, and unchanged controller-owned facts.

**GREEN.** Attach the same local MCP tools to both native integrations, forward
through the existing context/memory actions, and validate bound identity in
the service. Record actual reads/writes without exposing attempt credentials.

**REFACTOR.** Reuse the service's context retrieval and SQLite writer. No second
database, generic memory framework, public MCP endpoint, or model-specific
memory store is needed.

### Scenario 5 — Refresh tool-backed context after compaction

**Behavior.** Startup and TASK-003's post-compaction refresh directly supply
critical instructions and tell the agent to call factory_context; additional
history is available through memory_search. Both paths use the same context
owner and role filtering. A checkpoint written before compaction describes
unfinished work and the next action, not verified completion. After compaction
or replacement, actual factory_context returns the newest decisions/preferences
and that checkpoint. Without a checkpoint, current controller state and file
references still recover the task. Retain TASK-003's capability blocker when
the native refresh mechanism is unsupported.

**RED.** Add the focused MCP-recovery test below. Record a note, replace the
connection/session, change a decision/preference, trigger the supported refresh,
and assert current direct context agrees with the subsequent tool result.
Repeat with no checkpoint and a reviewer role; ensure no unverified assertion
becomes a completed obligation.

**GREEN.** Use the same retrieval result for baseline injection, optional
inspection files, and MCP calls. Reattach correctly scoped tools on replacement
and preserve their availability after native compaction.

**REFACTOR.** Keep native lifecycle ownership in TASK-003's runner. A reminder
and an available tool are not substitutes for observed context delivery.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-009, INV-007, AC-008 | The browser journey and lifecycle test prove default/task/repository/global scope, explicit promotion, provenance, and removal for both agents. |
| REQ-008 (preference context), INV-005, AC-007 (memory continuity) | Fresh/replaced sessions reconstruct applicable preferences and decisions from stored state without needing prior transcripts. |
| REQ-009 (retrieval/authority) | Large-history and conflict tests keep context bounded without silently dropping mandatory rules or upgrading assertions to facts. |
| REQ-011 (memory audit) | Preference/feedback decisions identify the maintainer and source; snapshots show exactly what each attempt received. |
| REQ-014, INV-006, INV-008, AC-013 | MCP integration and real native smoke prove callable tools, durable packet notes, scoped/role-bound access, ended-attempt refusal, honest write errors, and controller/review separation. |
| REQ-008, REQ-014, AC-007 (MCP refresh) | MCP recovery tests and actual native compaction demonstrate current direct/tool context agrees after compaction or replacement, including missing checkpoints. |

## Implementation Reference

TASK-003's context generation, durable messages, decisions, and attempt snapshots
are the behavior owners. Inspect their actual implementation/callers/tests
before extension. The existing skills and repository instruction files already
own coding standards; preferences supplement them rather than replace them.
The current .dev/factory.md explicitly asks for cross-task personal continuity,
which those static instruction files and native-session resumes do not provide
as an editable shared store. Extending the existing factory SQLite/context
owner is sufficient; no new graph, vector, or external memory service is needed.
The native SDK/app-server integrations from TASK-001 already own MCP attachment
and delegated capability configuration. Extend those existing integrations;
use official MCP SDK transport/schema handling rather than writing a protocol.
The missing capability is agent-callable access to the existing service, not
another retrieval engine. Likely changes are confined to factory
messages/context/decisions/UI, native MCP configuration, the local stdio
entrypoint, and their tests; private module names remain implementation-owned.

## Verification and Evidence

Names are **planned**; use TASK-001's test setup and confirm selectors before RED.

| Proof | Exact focused command |
|---|---|
| Scoped memory | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/memory.spec.ts --grep '^shares only explicitly scoped preferences across agents and tasks$'` |
| Lifecycle | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/preferences.test.ts -t '^edits disables supersedes and deletes preferences for later attempts$'` |
| Context authority | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/context.test.ts -t '^bounds optional memory and preserves explicit authority$'` |
| MCP boundary | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/memory-mcp.test.ts -t '^shares scoped memory tools without granting controller authority$'` |
| MCP recovery | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/memory-mcp.test.ts -t '^refreshes the same live context through MCP after compaction and replacement$'` |

Run focused proofs, factory:format:check, factory:lints, factory:typecheck,
factory:build, and git diff --check. Extend the existing real-agent smoke to
inspect supplied context across one later task for both harnesses; do not treat
an agent's answer about its memory as the only proof.
Add the explicit invocation `mise run factory:smoke:live -- --memory` to exercise
all three tools in both real native harnesses, correctly scoped delegated tool
access, actual compaction, refreshed direct/tool context, and session replacement.
Use native tool events and persisted service records as evidence. Missing native
compaction/tool/delegation proof remains incomplete, never inferred from a
fixture. This invocation is opt-in and billable like the existing live smoke.

## Material Stop Conditions

Stop if the change requires automatic rewriting of repository instructions,
silent preference inference, new external memory services, overriding
approved semantics, unavailable required native MCP/delegated access, or an
unobservable post-compaction refresh. Context budget values and UI arrangement are reversible
choices; document and test the selected values without another planning round.

## Authority Links

- [Specification](../spec.md)
- [Dependency](TASK-003-control-and-recovery.md)
- [Original request](../../../../.dev/factory.md)
- [Codex MCP](https://developers.openai.com/codex/mcp/)
- [Claude custom tools](https://code.claude.com/docs/en/agent-sdk/custom-tools)
