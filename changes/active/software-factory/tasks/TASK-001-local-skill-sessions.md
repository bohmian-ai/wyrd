---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-software-factory
spec_revision: 2
spec_status: approved
requirements: [REQ-001, REQ-002, REQ-003, REQ-011, REQ-013, INV-001, INV-002, INV-006, AC-001, AC-002, AC-010, AC-012]
depends_on: []
---

# Create packets and run native skills in the local application

## Outcome and Value

The maintainer can start a local application, register a repository, import a
packet or create a spec, approve that spec, and run planning through the existing
skills. Claude Code and Codex sessions stream into the same task view and can
request permission. This is a usable manual session launcher before automatic
implementation/review handoffs arrive in TASK-002.

## What Changes and What Must Stay True

Create the standalone TypeScript application in tools/wyrd-factory, its local
service/browser UI, SQLite metadata, and native integrations. Keep packet text
in repository files. Record session/skill/model identities and redacted events.
The service survives browser disconnection. No server, SDK, Card, or Postgres
integration is required. No execution loop, memory retrieval, or Slack is owned
here. Do not change the existing skills to accommodate the application.

TASK-004 attaches shared MCP memory tools to these same native sessions. Keep
the native MCP configuration and delegated-agent tool scoping available; this
task does not build a separate memory client or expose the tool contract early.

This task defines the new package/test owner and mise entrypoints. Specification
revision 2 is approved; implementation begins only when the maintainer requests it.

## Approach

1. Establish the standalone package, frozen lockfile, commands, and local start.
2. Register repositories and import packet files with dependency validation.
3. Integrate both native harnesses, including delegated-agent capability and
   host permission requests; record context and session identity.
4. Deliver the browser's packet/session/decision views and skill-driven spec
   authoring, explicit approval, and planning.
5. Prove trusted-local access, redaction, installation, and real native sessions.

## Ordered Implementation Scenarios

### Scenario 1 — Import a packet into a persistent local application

**Behavior.** Start the built service with fresh state, register a temporary Git
repository, and import its packet through the browser. Show its task files,
dependencies, and status after browser reconnect. Reject missing spec/tasks,
duplicate IDs, unknown dependencies, cycles, and paths outside the registered
repository without partial registration.

**RED.** Add the planned packet-import journey in the verification table.
Assert persisted packet/task identity and each refused case; first demonstrate
the missing service/import behavior rather than an installation failure.

**GREEN.** Provide the minimum service, SQLite, packet loading, and browser flow
needed to pass those observations.

**REFACTOR.** Keep browser state a projection of service state; avoid a second
packet parser or editable database copy of Markdown.

### Scenario 2 — Draft, approve, and plan through existing skills

**Behavior.** The maintainer submits intent to wyrd-spec, reads the resulting
draft file, explicitly approves its revision/content, and invokes wyrd-plan.
Show the resulting tasks. No agent can approve a draft. Editing approved
content returns it to draft, and planning or implementation cannot consume
unapproved content. Planning does not mark its own tasks implemented.

**RED.** Add the planned authoring journey with deterministic native-process
fixtures that write the real packet files. Assert approval provenance and
rejection of draft/changed approval inputs.

**GREEN.** Implement authoring sessions, file inspection/editing, approval, and
planning invocation using actual repository skills and explicit decisions.

**REFACTOR.** Use the same session and decision paths for manual skill runs;
do not invent an application-specific spec or task format.

### Scenario 3 — Operate both native harnesses without replacing their loops

**Behavior.** Both integrations stream output, retain native session IDs,
receive feedback, and return native results. Required fresh subagents can
actually be delegated. Missing executable/authentication/capability blocks
dispatch visibly. Native permission requests reach the browser and a denial
cannot be mislabeled success. Record the resolved tool/model/effort/skill hash.

**RED.** Add the planned native-boundary integration test for both protocols,
including event fragmentation, failure, denied permission, and unavailable
delegation. The live smoke lane separately exercises both installed native
harnesses against a disposable repository; a fake process is not that proof.

**GREEN.** Use the official Claude SDK and Codex app-server interfaces with
native tool/delegation behavior. Preserve provider-specific details inside
the integration that owns them.

**REFACTOR.** Share only the application's start/message/interrupt/result
operations. Do not create a generic model-provider or agent framework.

### Scenario 4 — Protect local access and persisted output

**Behavior.** An authenticated local browser can read/stream state and answer
permission requests. An unauthenticated stream, foreign Origin, invalid Host,
or forged mutation cannot access or change it. Configured secrets in agent
events and errors are redacted before storage or streaming. State is outside
managed repositories with owner-only access; native approval controls remain.

**RED.** Add the planned access/redaction test and inspect both streamed output
and persisted artifacts after sending a configured secret through a fixture.

**GREEN.** Provide local session/host/origin checks and shared redaction at
the persistence/streaming boundary. Keep credential values out of responses.

**REFACTOR.** Use one service boundary for UI actions and events; do not copy
Wyrd tenancy or authentication infrastructure into this separate tool.

### Scenario 5 — Install and start on supported hosts

**Behavior.** A clean macOS arm64 or Linux x86_64/arm64 host can install from the
frozen lockfile, build, and start the service/UI with documented native agent
prerequisites. Closing the browser leaves the service alive. Missing native
tools produce actionable preflight failures. Prove the native SQLite dependency
installs on each claimed platform; do not substitute an unsupported platform.

**RED.** Add the planned built-service journey asserting browser disconnect
does not terminate the service. Record platform installation failures as setup
evidence; they are not a fabricated behavioral RED.

**GREEN.** Supply the package lifecycle, executable start, and concise setup
documentation. Expose agent/preflight health through the application.

**REFACTOR.** Use the installed toolchain and standard package scripts, with
mise as the repository entrypoint; no platform-specific execution behavior.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-001, INV-001, AC-001 | Import/authoring browser journeys preserve real files and dependencies; only explicit approval permits consuming the current revision. |
| REQ-002, INV-002, AC-002 | Protocol integration tests plus explicit real Claude/Codex smoke establish sessions, permissions, and actual delegation. |
| REQ-003 | Packet/session/decision views reconnect to persisted service state; browser lifetime is independent of execution. |
| REQ-011, INV-006, AC-010 (browser) | Access/redaction tests refuse foreign/unauthenticated callers and find no configured secret in stored/streamed fixture output; session events retain identities. |
| REQ-013, AC-012 (installation) | Frozen installs/build/start are recorded on the three platforms; the built-service browser journey remains available after browser disconnect. |

## Implementation Reference

| Inspected owner | Reuse decision and gap |
|---|---|
| .agents/skills/wyrd-spec/SKILL.md, Establish authority / Write and approve / Handoff | Reuse spec file shape and human approval; the application supplies sessions and UI. |
| .agents/skills/wyrd-plan/SKILL.md, Write tasks / Handoff | Reuse task metadata and results; no parallel planning schema. |
| .agents/skills/wyrd-implement/SKILL.md and wyrd-task-review/SKILL.md | Native delegation is mandatory; the integration must retain it rather than flatten roles. |
| sdks/wyrd-sdk-python/tests/agent_readiness/live.py::Agent, AGENTS, preflight, Redactor | Existing paid smoke checks launch the same binaries and redact known credentials. They target Python SDK/Wyrd evidence and use broad bypass flags; do not turn them into a desktop service or copy their bypass policy. Use them as prerequisite/failure-case references. |
| mise.toml tools, test:agent-readiness:smoke:live, ts:test:unit; TypeScript SDK and Wyrd UI package.json | Existing Node/pnpm, Vitest, and browser-test tooling inform commands. Neither package owns this standalone application, and its tests must not require SDK native bindings or Postgres. |

CodeGraph exploration did not identify an indexed factory/scheduler/native SDK
integration; a targeted search found no existing Claude SDK, Codex app-server,
Slack Bolt, or better-sqlite3 owner in the inspected tooling. New machinery is
limited to the missing local application. Likely changes: tools/wyrd-factory,
root mise.toml, TESTING.md, and a focused CI lane for the new standalone package.
Do not alter the production UI, SDKs, or existing skills.

## Verification and Evidence

All names below are **planned**, not existing tests or commands. Establish the
package, Vitest/Playwright runner setup, and mise tasks here; confirm selectors
against actual added tests before recording RED. Browser tests start the real
factory service with temporary SQLite/Git and controlled native processes.

| Proof | Exact focused command |
|---|---|
| Packet import | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/packets.spec.ts --grep '^imports a packet and rejects invalid dependencies$'` |
| Authoring | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/packets.spec.ts --grep '^drafts approves and plans the current specification$'` |
| Native boundaries | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/native-sessions.test.ts -t '^preserves native sessions capabilities and permission decisions$'` |
| Access/redaction | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/local-access.test.ts -t '^protects local actions streams and persisted output$'` |
| Built service | `mise exec -- pnpm --dir tools/wyrd-factory exec playwright test tests/journeys/start.spec.ts --grep '^built service outlives its browser$'` |
| Real native skills | `mise run factory:smoke:live -- --agent claude` and `mise run factory:smoke:live -- --agent codex` |

Define factory:install, factory:dev, factory:build, factory:start,
factory:format:check, factory:lints, factory:typecheck, factory:test,
factory:test:browser, factory:smoke:live, and factory:check. The smoke task's
--agent argument is part of this task's new command contract. Offline tests
must not launch billable agents. Run the touched focused tests, factory build,
format/lint/type checks, and git diff --check. Full offline test/browser
aggregation runs at final change review. Add the new lane to TESTING.md and
scoped CI selection without widening unrelated boundary exclusions.

Record clean-host install/build/start evidence per supported platform and the
real harness's skill/delegation observations. Missing credentials or platforms
leave that evidence incomplete, never PASS.

## Material Stop Conditions

Stop for an unapproved spec, inability to retain required native delegation,
unsupported authentication requiring a new credential system, a need to change
existing skill semantics, or a proposed move into the Wyrd product runtime.
Resolve ordinary SDK wiring and private module choices locally.

## Authority Links

- [Specification](../spec.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Native integration research](../spec.md#costs-alternatives-and-research-grounding)
