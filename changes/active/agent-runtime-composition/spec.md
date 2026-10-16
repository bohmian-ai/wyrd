---
id: SPEC-agent-runtime-composition
revision: 3
status: approved
---

# Remove the public SDK Agent runtime

## Objective and user value

Wyrd is verification and assurance infrastructure, not a general-purpose agent
framework. Users build and execute applications with their chosen libraries;
Wyrd identifies their components, records behavior, governs gateway and MCP
access, evaluates evidence, and retains attributable results.

Remove Wyrd's public executable Agent surface from all first-class SDKs.
Preserve declarative Agent Cards and the internal Skald engine needed by Vala
and server-owned Verifier workflows. Do not replace the removed runtime with
another SDK execution abstraction.

This revision replaces the composition objective in revisions 1–2. The user
explicitly approved SDK Agent deletion and then Run invocation deletion on
2026-10-16. Those instructions are the approval authority for this revision.

## Current evidence

- Python exports an executable `Agent`, execution results, local tools,
  callbacks, sessions, and runtime helpers through `wyrd.agent` and root
  imports. The Python extension registers `wyrd._wyrd.agent` using Skald's
  `python_register` in `sdks/wyrd-sdk-python/src/lib.rs`.
- The Rust and TypeScript SDKs do not currently expose an equivalent public
  Agent constructor/loop. Do not add one. Both still expose Agent Cards and
  Run invocation, which are separate surfaces.
- `crates/shared/wyrd-client/src/observe/invoke.rs::Run::invoke_on` constructs
  a Skald Agent from a hydrated Agent Card, installs a gateway caller, and
  invokes `run_prompt`. It refuses tools but still owns application execution.
- The three support-desk examples, gateway-inference journeys, and
  local-development consumers currently rely on Run invocation. Migration
  must preserve their useful behavior, including correlation and permission
  refusals, rather than merely deleting their assertions.
- Existing native gateway APIs accept `wyrd-run-id` and `wyrd-card-uid`.
  Rust `gateway_inference::gateway_call_records_whichever_attribution_it_carries`
  proves caller/subject capture, with and without each optional header.
  Python and TypeScript gateway-inference journeys also prove header capture.
- Vala's Agent executor and simulator use Skald internally. Server Verifier
  workflows use the existing hosted tool binder. These owners do not require
  exposing an application Agent runtime in a language SDK.

## Requirements

New IDs distinguish this deletion scope from the retired composition
requirements REQ-001–011 and acceptance criteria AC-001–006.

- **REQ-012: Remove executable SDK Agents.** Remove public runtime Agent
  construction and execution from Python, Rust, and TypeScript packages,
  including native registration, imports, exports, authored typing, generated
  declarations, and published-package reachability where present. No
  compatibility alias, private-extension escape hatch, or renamed runtime
  replacement remains.
- **REQ-013: Close runtime-only consumers.** Remove SDK helpers whose only
  purpose is the deleted Agent runtime, including its execution results,
  local-tool registration, lifecycle callbacks, session-memory interfaces,
  and mock-provider facilities where exclusively owned by that surface.
  Inspect actual consumers before deletion: shared observation identifiers,
  server error contracts, Prompt authoring, and external-framework integration
  are not runtime-only merely because their names mention agents or sessions.
  Migrate runtime-dependent examples, documentation, tests, and test utilities
  to the surviving product boundary; retain meaningful coverage elsewhere.
- **REQ-014: Preserve declarative and internal Agent capabilities.** Keep
  `AgentCard`, `AgentSpec`, Agent Card registration/versioning, hydration,
  Prompt navigation, lineage, verification bindings, and `state.agent(...)`.
  Keep the internal Skald runtime and Vala/server Agent execution, including
  existing tool, route, workspace, resource, budget, and cleanup semantics.
- **REQ-015: Delete Run invocation.** Delete `Run.invoke`
  and `Run.invoke_on` and every SDK projection of that execution capability.
  Migrate actual consumers to application-owned execution through existing
  native gateway APIs or supported external-framework integrations. Do not
  add `Run.execute`, a new Prompt runner, an SDK Agent, or an equivalent
  renamed execution convenience.
- **REQ-016: Preserve correlated application workflows.**
  Migrated support-desk, gateway-inference, and local-development workflows
  continue to authenticate as the same caller, target the intended gateway
  deployment/model, and preserve Run/Card correlation, observations, and
  verification. Use existing gateway correlation headers and authentication
  integrations. Preserve gateway denials and server-side subject
  authorization. Add no new durable correlation state or SDK execution loop.
- **REQ-017: Preserve supported integrations.** External agents may continue
  to call Wyrd's gateway and MCP tools and emit traces/observations. Preserve
  the existing authentication adapters, trace processors, observation scopes,
  MCP permissions, and framework integration behavior. Removing Wyrd's own
  runtime does not remove support for observing or verifying other agents.
- **REQ-018: Publish an accurate product boundary.** Update owning
  architecture explanations and user documentation to distinguish declarative
  Agent Cards, internal Verifier execution, and application-owned inference.
  Replace executable-Wyrd-Agent examples with supported existing surfaces;
  remove obsolete runtime APIs from published docs and generated surfaces.

## Invariants

- **INV-001:** Durable behavior stays on the server: gateway authorization,
  capture, accounting, tool permissions and audit. Clients route calls to
  existing server surfaces.
- **INV-002:** Gateway calls carry the caller's Wyrd credential; provider
  credentials remain server-owned and never reach SDK users through Wyrd.
- **INV-003:** Tool permission is never inferred from gateway access, Agent
  declarations, holding a client, or catalog visibility.
- **INV-004:** Server-hosted tools retain delegated `tool:invoke` and domain
  checks; direct MCP calls retain domain checks as the caller. Audit remains
  non-blocking and follows the existing server write path.
- **INV-005:** Server-controlled execution still refuses native routes.
- **INV-006:** Skald remains independent of Wyrd client/server mechanics;
  SDK deletion does not move internal execution into a language binding.
- **INV-007:** Surviving behavior remains portable across macOS/Linux and
  x86_64/arm64 without platform-specific execution mechanisms.
- **INV-008:** This change removes public application execution, not Card
  kinds, persisted Agent contracts, evidence, or server-owned verification.

## Scope and non-goals

In scope: SDK runtime deletion and its necessary public/native/typing/docs/
consumer closure; preservation of declarative Agent operations and internal
Verifier workflows; Run invocation removal and necessary consumer migration.

Out of scope:

- The unified Skald run-input/lifecycle refactor and Jev fixes from revision 2.
  Internal defects can be addressed separately when required by their owner;
  they are not justification to rebuild a public SDK Agent.
- A new public Agent, Runner, Prompt execution API, or general framework.
- Client MCP toolsets, MCP transport relocation, third-party MCP expansion,
  new public server Agent execution, or moving the hosted server tool binder.
- Removing Prompt authoring/loading, Agent Cards, native gateway protocols,
  external-framework integrations, Wyrd MCP tools, or observational sessions.
- Persisted/wire migrations, changed permission or audit semantics, provider
  credential changes, or removing shared public error codes still emitted by
  the server.
- Compatibility aliases, deprecation shims, and unrelated package cleanups.

## Decisions that are expensive to reverse

- Public SDK Agent execution is removed instead of expanded to three languages.
  Internal Skald/Vala/server behavior remains within its current owners.
- Runtime-only native and language exports are deleted together, with no
  alternate public path to the same SDK-owned execution capability.
- Agent Cards remain declarative and registrable; SDK deletion does not change
  their wire schema or turn a hydrated Card into a runtime object.
- **D1 — resolved: delete Run invocation.** The user explicitly rejected
  retaining the correlated Prompt-execution convenience. Remove both Run
  invocation methods and their SDK projections. Application code owns
  inference through existing native gateway APIs and may supply correlation
  headers. Existing Run identity/scoping and observation behavior are unchanged.

## Acceptance criteria and evidence

- **AC-007** (REQ-012/013): Public packages, native exports, authored stubs,
  generated declarations, and docs expose no executable Wyrd SDK Agent or
  runtime-only companion surface. Import/export inspection, language compile/
  typing checks, package checks, and source-driven regeneration prove absence.
  Do not create a permanent name-ban check or indiscriminately ban `Agent`.
- **AC-008** (REQ-014, INV-008): Existing Agent Card registration, hydration,
  Prompt navigation, and verification journeys retain their behavior across
  all SDKs. Internal Vala and server Verifier tests continue to cover Agent
  steps/checks, delegated tools, route restrictions, and workspace guards.
- **AC-009** (REQ-015/016): Run invocation is absent from
  every public and native projection. Migrated real-server/mock-upstream
  support-desk, gateway-inference, and local-development journeys in all three
  SDKs preserve caller attribution, Run/Card capture, observations,
  verification, and under-privileged gateway denial. No substitute runtime
  is introduced to make migration pass.
- **AC-010** (REQ-017, INV-001–007): Existing external-framework gateway/MCP
  and telemetry journeys remain valid. Gateway subject headers are captured
  and authorized and are not forwarded upstream; provider secrets remain
  server-owned. Existing tenant, permission, audit, and dependency checks
  remain intact.
- **AC-011** (REQ-018): Architecture and user documentation describe the
  surviving boundary consistently. Examples and generated/export artifacts
  reference supported APIs; owning docs, typing, codegen, package, format,
  lint, and dependency checks pass for the actual deletion write set.

## Approval and open decisions

Revision 3 is approved by the user's explicit deletion instructions on
2026-10-16. No material decision remains open. Revision 2's composition task is
replaced by one deletion task; the previous runtime-expansion plan must not run.

## Authority

- `architecture/wyrd-design.md`: protocol, Agent/Verifier, and client/server
  ownership.
- `architecture/wyrd-doctrine.mdx`: runtime boundary; user code owns inference
  loops and agent processes; Wyrd owns verification and assurance.
- `AGENTS.md`: first-class SDKs, ownership, test integrity, and journey coverage.
- `architecture/references/languages/spec-driven-development.md`: material
  revisions return to draft and require explicit human approval.
- `crates/shared/wyrd-client/src/observe/invoke.rs`: current Run execution owner.
- `docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md`: existing
  native client and external-agent integration paths.

## Revision history

| Rev | Status | Change |
|---|---|---|
| 3 | approved | User approved SDK Agent deletion and explicitly chose Run invocation deletion. Preserve declarative Cards, internal execution, and correlated application workflows. Replaces revision 2 composition scope with one deletion task. |
| 2 | approved | Approved by the user on 2026-10-16; requested one implementation task covering the complete change. |
| 2 | draft | From the independent review and the external SDK review. Changes: one Skald run entry point and lifecycle with explicit single-call dispatch (REQ-001 to REQ-003); eager `connect` in Rust and TypeScript (REQ-004); correlation included (D4); MCP client ownership moved to `wyrd_client::mcp` to avoid a cycle; collisions checked on provider-visible names; the permission difference written as INV-004; server binder move dropped (D5); full Rust and TypeScript runtime (REQ-010); `Run.invoke` deletion gated on replacement journeys (REQ-011); "before IO" changed to "before any model or tool call"; sweep hardening added to AC-006. |
| 1 | draft | Correlation deferred; D4 dropped as a reversible implementation choice. No open decisions. |
| 1 | draft | D3 resolved: `Run.invoke` deleted, no AgentCard-to-Agent conversion; REQ-005 reworded to the SDK Agent; D5 opened. |
| 1 | draft | D2 resolved: the Agent follows the shared connect, connect_with and lazy-Python handle convention. |
| 1 | draft | D1 resolved: explicit and implicit MCP tool attachment. |
| 1 | draft | Initial draft from the TASK-005-S1 review, which found that Agent tools and gateway routing are composed only inside Verifier execution. |
