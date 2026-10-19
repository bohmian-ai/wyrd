# TASK-001 independent maintainer review

Overall result: **FAIL**.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Immutable base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Immutable candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`.
- Reviewed the cumulative base-to-candidate diff independently; the earlier remediation range was navigation, not the acceptance subject.
- Inputs: approved revision **13** obligations in `changes/active/long-horizon-verification/spec.md`, original `tasks/TASK-001-grouped-verification.md`, prior `review/task-001-e4b1e313a-20261017/{verdict.md,findings-validation.md,TASK-001-R1-close-grouped-verification-gaps.md}`, and prior maintainer report. The draft-14 header/exception is not approved authority and supplies no upgrade requirement.
- Applied AGENTS, agent rules, spec-driven-development and maintainer-style, relevant Rust/Python/TypeScript references, design/doctrine, Bifrost query/visibility authority, and evaluation guidance. Consulted CodeGraph before source searching.
- TASK-002 schedule-input work and TASK-003 pressure/progress/capacity work are excluded. The explicit newest-active, one-page discovery simplification is accepted; this report requires no continuing cursor or replacement discovery subsystem.

No source, test or task edits, tests, or Cargo commands were performed. Only this report was written. HEAD remained the candidate. Other current-round reports were not read.

## Changed-surface coverage

| Changed surface | Owner, changed symbols and connected consumers inspected | Maintainer assessment |
|---|---|---|
| Shared client invocation and owner identity | `Observe::{group_id,grouped,record_or_stage,enqueue_run,target}`, `StagedRun`, `Target::request`, `Principal::owner_service_uid`; direct/submitted server producers, Python/Node wrappers, grouped journeys | Existing transport and projection carry the field. Observe retains meaningful per-surface state; principal identity is copied without new IO. No second SDK implementation is introduced. |
| Declarative source and Trigger contracts | `InputSlot`, `InputSlotSchema`, `SourceText`, `SourceReference`, shared `Tokens`, slot validation, `TriggerDuration`, `TriggerActivation`, coverage and execution-state types | Source grammar extends the existing parser cursor. The schema projection closes the earlier phantom object. Duration codecs now explain their refusal and formatter boundaries. |
| Binding and Operator consumers | `VerificationBinding::operator_lists*`, reference visitor, composition validator, `GroupChange`, template field applicability/rendering, `BindingActivation`, constructors in shared/Skald/auth/testing/SDK consumers | Both reaction lists traverse the same inventory. Existing binding projection, template rendering and dispatch ownership are extended rather than forked. |
| Registration, resolution and loader | `EffectiveSpecs::{validate_bindings,validate_binding,validate_sources,reaches}`, `source_binding`, `site_subjects`, `GroupSite::ambiguous`; `BindingProjector::{freeze,group_sources}`; server `validate_request`, loader `validate_tree` and artifact rule; `validate_artifact_manifest` | The effective-spec owner retains registry loading and source traversal. Registration/projection reuse `source_binding`. Artifact overlap validation is now one pure rule called by server and loader; the old loader implementation is deleted. Existing registry registration workflow remains the authority. |
| Scheduling and group selection | Runtime builder, `VerificationScheduler::{tick,poll_groups}`, all of `groups.rs`: `GroupPoller`, `BindingPoll`, `Sources`, `TupleCount`, `Ended`, `Payloads`, decoders and tests | New reader structs retain real state and narrow responsibilities. They reuse the scheduler, authenticated scheduled-query consumer and established SQL literal rendering. The remaining selection/count mismatch is MNT-R2-001. |
| Query reuse and failure contract | Group reader → `Caller::accepted` → `ScheduledQueryCaller::{authenticated,run_with,dispatch,consume_to_terminal}`; existing Drift reader/`ObservationWindow`; Bifrost per-query cut contract | No second transport, stream protocol parser, table-authorizer or audit path is added. Drift aggregation is not an existing implementation of grouped source membership, so forcing group selection into Drift would mix owners. Different query statements still have different cuts; that matters to the source-bound invariant. |
| Queue, persistence and reactions | Binding projection/arming; `VerifierExecutionQueue::{claim_due_group,group_states,selected_results,refused_submissions,enqueue_group,complete,execution_status,binding_status}`, row decoding, `group_coverage`, SQL constants and migrations; SQL serialization/replay test | Existing queue owns leases, selected result identity, one-outstanding fence, pinned snapshots and dispatch creation. No group membership/revision store, parallel adoption owner or compatibility route is added. |
| Engine, hosted tools and result publication | Engine `check`/`conform`, assertion helper, `AgentExecutor::{invoke,execute}`, `AgentToolset::open`, `HostedTools::open`; runner grouped branch, `source_limit_exceeded`/`summary`, result builder and `ResultRun` | Empty selections are judged at the existing evaluator. Check Agents use existing tool sessions and hosted authorization. Source-ceiling outcome generation is shared and is correctly gated by the pinned marker; the producer of that marker remains defective. |
| Analytical tables/configuration | `ExecutionInputsTable`, `ResultsTable`, physical layouts, table-contract assertions, result builder column mapping, `HostedLimits`, environment configuration, testing defaults | Columns map by name through existing publication machinery. The only new deployment ceiling has its approved source-execution role and documented default. No schema-upgrade machinery is requested. |
| Python, TypeScript and Rust projections | Rust GroupId export; native Python Observe and authored observe/verify stubs; Python status/typecheck cases; native Node methods, ergonomic Observe/options, generated wire types and both N-API declaration forms, TS type assertions | Signatures agree on optional group metadata and retain durable `group_id`. Foreign wrappers call shared validation. InputSlot now projects only typed slots or a source string. No declaration-parity blocker found. |
| Journey and seam proof | Full added grouped Rust/Python/TS story bodies; late-work and owner-grant Rust stories; Deployment/Grouped/Hooks helpers; provider scripts and language support; MCP support/media stories and `FrameAgent`; SQL, loader/artifact, engine and served OpenAPI changes | Tests assert durable values, order, membership, history, dispatches and refusals. Helpers reuse the deployment/provider owners and remove duplicated hook/tool-response setup. Stable-set ceiling tests do not prove a membership change between reads. The new MCP helper has MNT-R2-002. |
| Fixtures, documentation and dependency consumers | Grouped source/Trigger/Operator/media/cycle/refusal fixtures; changed design, evaluation and docs-site text; Cargo/SDK manifest context, lockfile and mise lanes; constructor-only sibling edits | Fixtures exercise the approved grammar; live query tools remain the narrow declared exception. `sha2` is already installed and added only as an MCP dev dependency to content-address media uploads. No unearned production dependency or unrelated UI work appears. |

The reusable alternatives were compared at workflow level: existing cards/effective-spec loading, binding projection, scheduler, execution queue, scheduled-query reader, evaluation engine, hosted tool session, payload builder and SDK transport remain the owners. The added source parser uses existing tokens; the source-cycle walk handles a relation the existing local step graph does not represent. No optional restructuring is demanded.

## Material proposed findings

### MNT-R2-001 — INCORRECT: overflow is decided from a different, less eligible set than the snapshot

**Prior ledger relationship:** incomplete closure of **FIND-TASK-001-2**, not a new per-caller defect.

**Changed locations:** `crates/wyrd/wyrd-server/src/verification/groups.rs:139–147` (`over` construction before eligibility filtering), `:245–273` (`counts`), `:282–296` (`identities`), `:320–330` (snapshot membership/payload read), `:349–363` (pinned overflow marker). Relevant consumer: `verification/runner.rs:420–430`.

**Governing obligation and guide principle:** REQ-009 counts distinct **eligible** source executions, exactly the ceiling is allowed, and overflow is determined before payload collection. Prior FIND-2 explicitly requires refused/finality rows not to consume source capacity. Maintainer-style requires invariant-bearing owners and their documentation to describe behavior a maintainer can safely change; here the claimed bound in `identities` and `TupleCount` is not enforced at the membership owner. Bifrost authority promises a pinned cut per statement, not across these statements.

**Producer-to-consumer evidence and concrete consequences:**

1. The producer stages a submitted input row before the queue writer can reject an idempotency conflict. `refused_submissions` is the existing authority for removing that row. But `counts` counts its distinct execution ID, and `poll` marks a tuple `over` before calling that authority. An over tuple is then excluded from the identity read entirely. With ceiling 20, 20 real source executions plus one rejected conflict become a false `source_execution_limit_exceeded` judgment, although the 20 eligible executions should be allowed. The later `rows.retain` cannot repair a tuple that was never listed. The comment on `Ended::judge` explicitly acknowledges counting refused rows for over tuples; a shortcut comment cannot waive REQ-009.
2. Conversely, `counts` and `identities` run separate ordinary Bifrost statements. A tuple counted at 20 can expose its 21st execution before identities opens. `identities` has no same-cut count or per-tuple bound, `Ended::executions` keeps all 21, and `snapshot` fetches their payloads while `exceeded_ceiling` remains absent. For a group already due by max_age but still within finality, new activity does not suppress this path. The runner therefore evaluates 21 values normally rather than returning the required unread overflow result. This is a supported visibility interleaving, not a claimed measured reproduction.

These paths share one cause: an earlier aggregate is treated as authoritative for the later snapshot's actual eligible membership. This makes source-bound changes unsafe and makes the documented deployment limit unreliable. All input/outcome source slots consume this shared path; adding guards independently in the runner or SDKs would be too late and duplicate the invariant.

**Smallest testable correction:** keep selection on the existing `BindingPoll`/`Sources` owner. Establish overflow from the deduplicated eligible identities that actually form that tuple's snapshot, including the existing refused-submission authority and finality; preserve bounded identity collection and defer/retry an inconsistent read rather than allowing a stale preliminary count to authorize an oversized payload read. Reuse the existing exceeded-ceiling marker and runner outcome. Do not add a lifecycle store, cross-query snapshot service, transport, budget, or per-SDK check. A preliminary count can remain an optimization only when it cannot decide an incorrect result or weaken the later bound.

**Focused proof:** at ceiling 20, 20 accepted executions plus a rejected idempotency-conflict row must judge the 20 eligible members normally; 21 accepted must produce the explicit overflow result without fetching those payloads. Force a count-at-20 / identity-at-21 publication interleaving at the existing source-query seam for a max_age-due group, and prove no ordinary oversized snapshot is enqueued and no oversized payload read occurs; a subsequent stable poll must produce the ceiling result. Preserve genuine absent-result null and source-slot alignment. Existing support replay/conflict is below the boundary, and the existing frame stories wait for a stable 21-execution set, so neither closes this gap.

### MNT-R2-002 — VIOLATION: new MCP upload helper omits its panic contract

**Prior ledger relationship:** the original listed documentation omissions in **FIND-TASK-001-6** are corrected, but the remediation adds this omission under the same mandatory rustdoc rule.

**Changed location:** `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1547–1555`, `FrameAgent::submit`; explicit panic at `:1585–1588`.

**Governing rule:** AGENTS §16 requires `# Panics` whenever a panic remains possible, including private test helpers. The task-review/maintainer guide explicitly preserves missing substantive rustdoc as a blocking exception.

**Evidence and concrete cost:** the helper returns `Result` and documents tool/plan/upload errors, but asserts that the HTTP PUT status succeeds. A reachable refused upload panics rather than returning one of those errors. Its documentation has no `# Panics`, concealing that distinction from a maintainer reusing the upload helper. Nearby new `group_binding` correctly separates `# Errors` and `# Panics` for its explicit assertion, demonstrating the local pattern.

**Smallest correction and proof:** document that a refused planned upload panics, or return the existing journey error instead of asserting and document that error boundary. Preserve the upload assertion's behavioral proof. Inspect the corrected source; no new test, checker, harness or separate style-only round is needed. Batch with behavioral remediation.

## Prior-finding closure relevant to this lens

| Prior finding | Source reassessment |
|---|---|
| FIND-1 discovery/late coverage | Completion watermark/slack is removed; in-finality first-source discovery and newest-active one-page bound are explicit; `GroupState::covers` detects increased coverage. The accepted page ceiling is not reopened. |
| FIND-2 source selection | Broad payload statements now use pinned execution IDs, silent raw skip is removed and completeness is checked. Eligibility/count-cut closure remains incomplete as MNT-R2-001. |
| FIND-3 extreme durations | `span`, checked timestamp arithmetic and elapsed comparisons replace the panicking operations; dedicated timing and neighboring-work story are present. |
| FIND-4 phantom InputSlot | Source schema now has typed objects plus SourceText; generated TS has no `type: source` object, with schema/runtime and TS negative typing cases. Prior MNT-001 is closed. |
| FIND-5 owner read grants | Group reader now uses `Caller::accepted` and `ScheduledQueryCaller::authenticated`; existing Drift reader exception is preserved. |
| FIND-6 required docs | Original duration Errors sections, SourceText field, group test module and Forward field are documented. New omission is MNT-R2-002; prior MNT-002 itself is closed. |
| FIND-7 tool authority | Changed active design/evaluation descriptions now state the explicit `bifrost.query`-only check exception and ordinary permissions/live visibility. |
| FIND-8 proof variants | Registration, external/missing/ambiguous sources, tool/query denials and MCP media cases are added in existing journey owners. This is source inspection of proof, not an independent claim that every acceptance criterion or command passed. |

## Verification limits and calibration

The task/remediation append passing focused Rust/Python/TypeScript journeys, MCP/OpenAPI and scoped static checks. They are supplied evidence; this reviewer did not rerun them. Root coordinates fresh verification separately. The two selection counterexamples are derived from reachable producer/query/consumer paths; no reproduction run is claimed. Green static-set stories cannot establish the missing invariant across query cuts.

No other material owner/layout/naming or public-declaration issue was established. Local import placement and equally clear alternative helper shapes do not independently block this report. No preference refactor, continuing discovery cursor, schema upgrade requirement, or out-of-scope TASK-002/003 work is requested.
