# Independent maintainer review — TASK-001

Result: **FAIL**. One mandatory rustdoc omission remains in a newly added remediation helper. No additional blocking layout, abstraction, declaration-parity or naming finding was established.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `ae9c5a6aee22b691e2dc04c805cb5d1dddfcb9bf`; HEAD checked before and after inspection.
- Reviewed the cumulative `git diff ef3a57b07...ae9c5a6ae`; the preceding candidate is navigation only.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-001-grouped-verification.md`; specification revision **13**, including the user's explicit discovery-page and single-selection amendments. Draft-14's header/exception supplies no authority; no retained-upgrade scope is inferred.
- Prior verdicts, validation ledgers, R1/R2 tasks and their appended evidence in `task-001-e4b1e313a-20261017` and `task-001-47e0c9ab7-20261018` were compared with current source. No other current-round reviewer report was read.
- TASK-002 schedule-input work, TASK-003 staging/pressure/progress work and REQ-015 capacity qualification remain excluded.

Applied AGENTS §§5, 11, 15–16, agent-rules, spec-driven-development and maintainer-style guidance under Wyrd design/doctrine and relevant Bifrost/evaluation and SDK guidance. CodeGraph preceded code navigation. Inspected manifest/lockfile changes and mise verification ownership; the only dependency delta is the installed workspace sha2 dependency for the MCP media test. No new feature or production dependency is introduced.

## Changed-surface coverage

| Surface | Owning symbols, consumers and relevant proof inspected | Maintainer assessment |
|---|---|---|
| Contract identity, request/state/coverage | `GroupId`, its validation; `VerificationRequest`, `VerificationExecutionState`, `GroupEndSettings`, `GroupSource`, `GroupCoverage`, `GroupSourceSnapshot`; producer and queue projections | Domain values remain IO-free. Optional caller group and background coverage are distinguishable and documented. |
| Trigger/source grammar and schemas | `TriggerDuration` parsing/serde/schema/display; `SourceText`, `SourceReference`, existing `Tokens`; `InputSlotSchema`, `InputSlot`; source/slot/check-Agent validation and schema tests | Parser extends existing grammar machinery. Source-owned schema adapter addresses the actual mixed-enum projection problem; it does not introduce another runtime validator or client-only accepted form. |
| Binding and Operator declarations | `VerificationBinding::operator_lists[_mut]`, reference visitor and composition validators; `GroupChange` and template fields; generated TS types, authored Python execution-state stubs and served OpenAPI proof | Both Operator lists flow through existing walking/validation owners; durable field spelling and generated/source shapes agree in the inspected projections. |
| Registration and loading | `EffectiveSpecs::validate_bindings`, `validate_binding`, `validate_sources`, `reaches`, `source_binding`, `GroupSite`, `BindingProjector::freeze/group_sources`; shared manifest validation, server preflight and loader diagnostic caller | Existing effective-spec loader/cache and transaction-scoped projection remain owners. Same-site source traversal supplies a missing source-dependency rule; it does not replace registry or composition traversal. Shared artifact validation removes the loader-only duplicate algorithm while retaining server authority. |
| Producer and shared client | `Principal::owner_service_uid`, `Observe`, `StagedRun`, `Target`, `VerificationControl` staging and `ResultPayloadBuilder`; direct and submitted runner attribution | Group identity is carried through existing evidence staging. No alternate producer workflow, parent lookup or durable client behavior is added. Owner/state fields and methods have clear roles. |
| Group reader | Full `GroupPoller`, `BindingPoll`, `Sources`, `SourceSlot`, `SourceRow`, `Ended`, `Payloads`, decoding/time/SQL helpers and unit module; scheduler and queue callers | Retained dependencies and per-poll state have concrete cohesive owners. Pure conversions stay synchronous. Selection, end timing, payload reads and snapshot construction are locally discoverable; source lists share one ordering authority. |
| Queue and persistence | Binding activation projection/arming; due-group claim, group states, refusal/selected-result reads, enqueue, claim decoding, coverage, completion/dispatch SQL and helpers; three altered migrations; SQL integration proof | Extends existing queue, cursor, lease and settlement ownership. Closed activation/input variants make the new path discoverable without a second lifecycle store. Caller transaction ownership remains visible. |
| Evaluation and hosted tools | `source_limit_exceeded`, runner grouped branch, shared empty-each engine behavior, Agent executor invocation, `AgentToolset::open` and `HostedTools`; existing unit/integration and grouped coding/media tests | Existing engine and tool-session abstraction serve steps and checks. No second check-Agent runtime, terminal parser or evaluator is introduced. The permitted tool restriction remains explicit. |
| Bifrost tables/results | Input/result Arrow fields/layout and schema assertions; `ResultRun`/builder input/result projections, runner result writer | New metadata stays in existing managed tables and one result writer. Column mapping remains named. Documentation explains owner/group meaning. |
| Query fault proof seam | `QueryStreamFaultController::apply`, `BoundedQuery` caller, `ScheduledQueryCaller::run_with`/dispatch/terminal consumer and public-route fault sibling | Test-only extraction reuses the existing one-shot controller. The new internal consumer can now meet the existing EOF fault; production terminal handling remains owned by the scheduled-query consumer. |
| Rust SDK and shared testing | Seven grouped stories and their `Grouped`/frame/query/history helpers; `Deployment::group_execution/hooks/restart`, `Hooks`/`Forward`; SQL and server grouped seam tests, registration/OpenAPI proof, existing testing limit changes | Tests assert snapshots, identities, values, verdicts and refusal codes through existing owners. Shared deployment helpers have meaningful retained resources. One new helper's returned-error contract is missing, below. |
| Python and TypeScript | Native Observe/NativeRun boundaries, authored observe/verify stubs, ergonomic TS Observe/options, N-API declarations, generated wire types, type/unit updates; grouped stories and server/provider helpers in both languages | Shared client performs group validation and transport. Runtime argument spelling and optionality match the authored public surfaces. No language-specific durable selection logic is added. |
| Agent/MCP and adjacent fixtures/docs | Group binding/review helpers, support discover/act/observe and frame upload/media cases, FrameAgent; fixture source graph, Trigger/Operator and negative declarations; design/evaluation/runtime-limit/site/error updates and mechanical request initializers | Existing tools and evidence upload are reused. Media helper panic docs are now present. Authority text reflects the approved narrow live-query exception. Mechanical field additions do not create another capability. |

The existing alternatives were compared as workflows: Drift's `ObservationWindow` reads bounded observation windows but does not supply exact grouped source lists; `ScheduledQueryCaller` already owns authorization, streaming and terminal settlement; `VerifierExecutionQueue` owns acceptance/refusal, selected results, immutable queued inputs and dispatch; `EffectiveSpecs` owns referenced-body loading; Vala owns judgments. The candidate extends those owners and confines new group orchestration to the missing behavior. A line-count split, zero-state utility owner, new generic reader or replacement resolver is not warranted.

## Material finding

### MNT-R3-001 — VIOLATION — returned poll errors have no mandatory Errors contract

**Prior ledger association:** residual `FIND-TASK-001-6`; the original sites and R2's FrameAgent site are fixed. This is a new helper added by R2, not a reopening of those fixed locations.

**Changed location:** `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:188–196`, `Support::tick`; body at `:223–225`.

**Governing rule:** AGENTS §16 and agent-rules require every new/materially changed fallible Rust method, including private test helpers, to include `# Errors` naming its error conditions. The task-review skill expressly retains missing rustdoc as blocking.

**Evidence and concrete cost:** `tick` returns `Result<Vec<EnqueueOutcome>, GroupPollError>`, forwards `poller.poll(...)` unchanged after committing the cursor/enqueues, and is called by both new grouped seam tests. Its rustdoc describes the tick and has only `# Panics` for setup/transaction failures. It never documents the returned SQL/read error distinction or that an error is returned after the helper commits. That distinction is central to the partial-read test: a read error allows the advanced cadence to commit and later retry. A maintainer reading this helper's contract cannot distinguish that expected failure path from setup panics, and the explicit rustdoc completion gate fails.

**Smallest testable correction:** add `# Errors` to this existing helper documenting the `GroupPoller::poll` errors it returns and the commit-before-return behavior; preserve its assertions, transaction flow and error propagation. Nearby `GroupPoller::poll` already documents the variants and earlier-whole-snapshot semantics to reuse. No new helper, behavior test, checker or dependency is needed; source inspection closes this prose correction, batched with any other confirmed remediation.

## Prior-finding source closure and proof limits

| Prior ID | Current source disposition in this maintainer pass |
|---|---|
| FIND-TASK-001-1 | Closed under the user amendment: first-source finality, newest activity and bounded newest page remain explicit; no continuing cursor is required or proposed. |
| FIND-TASK-001-2 | Separate `counts` authority is removed. `selection`/`Sources::identities` supplies ranked identities, expands only to replace refused places, filters refusals, and passes the same rows to end/overflow/snapshot membership. Exact-ID payload predicates and `complete` remain. New at-ceiling-conflict and partial-read seam bodies were inspected. This is source assessment, not a claim that this reviewer ran them. |
| FIND-TASK-001-3 | Checked timestamp and elapsed-span timing remains; maximum-duration and neighboring-work proof bodies are present. |
| FIND-TASK-001-4 | Phantom source-object projection remains removed; typed/source schema, generated TS and schema/type-negative proof agree. |
| FIND-TASK-001-5 | Group reader continues using accepted-owner authenticated scheduled queries; no manufactured grouped SYSTEM grants reappeared. |
| FIND-TASK-001-6 | Original duration Errors, SourceText/Forward tuple fields, group test-module documentation and FrameAgent rejected-upload Panics are source-closed. New `Support::tick` omission remains as MNT-R3-001. |
| FIND-TASK-001-7 | Design/evaluation and public guidance retain the narrow declared `bifrost.query` exception, ordinary authority, no workspace and live-data trade-off. |
| FIND-TASK-001-8 | Internal grouped reader now applies the existing fault controller. The new focused real-server/queue test forces EOF after a batch, observes a Read error, then checks one complete snapshot and a further idle tick. Snapshot-statement truncation is not separately armed; the supplied evidence explicitly states that limit. |

No Cargo commands, tests or source/test edits were performed by this reviewer. Appended implementation evidence was treated as a claim; the orchestrator owns independent sequential reruns. Verification coverage is assessed from actual test bodies rather than green summaries. Only this report was written. No preference-only blocker or optional refactoring recommendation is proposed.
