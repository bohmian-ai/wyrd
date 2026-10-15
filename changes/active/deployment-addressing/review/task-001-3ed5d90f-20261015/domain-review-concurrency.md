# Concurrency domain review — TASK-001 R2 candidate

**Overall: PASS. Proposed material findings: none.**

## Subject, authority and method

Reviewed the immutable archive `/tmp/wyrd-task-review-3ed5d90f` at
`3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`, against original base
`1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. The complete cumulative diff
and latest remediation diff were available in this review directory; the latter
located changed owners without replacing cumulative acceptance. Prior reviewed
candidate: `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`.

Authorities: repository AGENTS.md and architecture/agent-rules.md; Wyrd design
and doctrine; spec-driven-development and maintainer-style references;
approved deployment-addressing spec r6, original TASK-001, prior verdicts and
R1/R2 remediation tasks with committed evidence. The archive has no CodeGraph
index; the orchestrator supplied the root exploration and shared map. This
independent discovery pass did not use another current reviewer's conclusions.

Boundary: coherent configuration snapshots; requested/fallback batch admission;
claim/replay/uncertain dispatch and cancellation; bounded visibility; invocation
route isolation through model/tool loops; new Prompt schema validation and the
installed-CLI child's process-global Prompt registry usage. Adjacent approved
changes are not treated as unauthorized scope.

## Source coverage and outcome

| Boundary | Actual owner and consumer evidence | Assessment |
|---|---|---|
| Configuration snapshot | `crates/wyrd/wyrd-server/src/components/gateway/service.rs:657` `GatewayAdministration::snapshot`; `wyrd-sql/src/queries/gateway.rs` snapshot query; `wyrd-gateway/src/snapshot.rs` owned domain values | One SQL statement yields one owned snapshot, without secret plaintext or a second cache. Administrators replacing configuration do not mutate an admitted call's snapshot. |
| Current admission | `invocation.rs:353` run → govern → `admit_call:457`; unconditional `decide` at 485; `route:540` uses that snapshot for requested and fallback candidates; `AdmittedCall` retains it | No authorization-skip flag remains. Preauthorization does not substitute for the decision on actual routing state. Requested denial refuses before dispatch; each fallback is separately authorized. |
| Fresh batch and completed replay | `batches.rs:386` create; `settle:485`; `refresh:752`; `call:786`; all provider calls reenter `GatewayInvocation::run` | The early audited decision protects claim creation, and fresh creation/refresh independently use current admission. Access replacement between the decisions is enforced without a new lock or handler-specific authorization mechanism. |
| Claim/fence writers and readers | `wyrd-sql/src/queries/gateway.rs:836–887` claim/by-request/read/list/record/release SQL and associated typed query functions; `batches.rs` created/batch_row/record/release | Tenant request-digest uniqueness owns duplicate suppression. Release deletes only the matching still-pending batch; record accepts the same provider ID. Replay of an uncertain attempt cannot produce another upstream batch. |
| Cancellation and uncertain dispatch | `batches.rs:450–518` tracked settlement and cancellation guard; `invocation.rs:399–436`, `CallTask::run`; `wyrd-gateway/src/engine.rs:465–650` execution, attempt and after_failure | Admission errors are undispatched; cancellation stops successor attempts. The engine marks dispatch before entering provider IO, including cancellation races; non-idempotent batch operations do not retry an ambiguous upstream dispatch. Settlement survives ordinary request departure, while ambiguous/process-loss state conservatively retains the fence. |
| Sibling batch paths | upload/retrieve/cancel/file content/delete in `batches.rs`; pinned call plan in `wyrd-gateway/src/routing.rs` | Existing provider-held state stays pinned to its deployment. Sibling provider IO enters common admission; metadata reads use their snapshot-based decision. No replacement batch lifecycle was added. |
| Bounded listing | `batches.rs:556–627` list; `invocable`; `wyrd-sql/src/queries/gateway.rs:862` BATCHES_SQL and gateway_batches | Scoped grants and All-minus-Granted reach prune before LIMIT; final audited decisions use the same snapshot. No per-row configuration reread or unbounded fill loop creates inconsistent continuation. |
| Per-invocation route | `skald-agent/src/agent.rs:334–387`; from_card/to_spec; `loop_runtime.rs:64–76,202–212`; `wyrd-client/src/observe/invoke.rs:73–120`; gateway registry/provider and delegate paths | Route/registry selection belongs to a run, from immutable Agent fields. Override construction yields a separate Agent. Existing loops retain that selected registry; native provider overrides cannot replace a gateway route. No mutable global route or concurrent Card mutation appears. |
| R2 offline schema validation | `skald-spec/src/prompt.rs:233` check_output_schema, `authoring.rs` callers, builder/Prompt validation consumers | Standard bundled meta-validation replaces private partial validation. The application adds no mutable shared schema state, cache, background fetch or synchronization owner. The existing OnceLock values in this module are immutable placeholder Regex instances. |
| R2 loaded-Agent fixture | `wyrd-client/tests/integration/local_development.rs:63–166`; Python installed-CLI `run_rust:300` and stages 743–757; `skald-agent/src/agent.rs:94` register_prompt_card | Prompt registration mutates the existing in-process registry under its existing owner, using downloaded exact pinned refs. The driver invokes an exact nextest selector in a fresh process before and after restart. Four distinct Prompt refs are registered sequentially; this does not mutate another test/process's registry. The loader does not rebuild a route from ambient process state. |

Unless otherwise qualified, invocation/service/batches paths are under
`crates/wyrd/wyrd-server/src/components/gateway`; crate paths in this report are
under `crates/`.

## Acceptance and prior closure

| Obligation | Source evidence | Meaningful proof inspected | Result |
|---|---|---|---|
| FIND-001-10; REQ-002 / INV-007: actual batch dispatch uses its routing snapshot's authorization | create retains early decision; settle/refresh enter common run; admit_call always decides against its own snapshot before plan/execution | `pg_invocation_tests.rs:3076` gateway_batch_dispatch_reauthorizes_against_its_routing_snapshot uses a DB claim-table lock, waits for actual blocked creation, replaces Open with Granted, then releases it. It asserts fresh denial, no new provider POST, released fresh claim and allowed/denied audit; completed replay refuses before GET, explicit scoped caller still succeeds. R1 evidence records exact RED/GREEN and P7/P9 results. | PASS, prior cause remains closed |
| FIND-001-7; REQ-002 / AC-002: older visible batches remain paginatable | SQL reach predicate precedes LIMIT and uses the same configuration as final decisions | gateway_batch_listing_is_one_bounded_pruned_read includes mixed access/grants and continuation. Its source is consistent with bounded SQL selection; historical committed proof records it passing. | PASS, closed |
| Claim/replay safety and request departure | tenant digest fence, tracked settle, undispatched-only release and no ambiguous batch retry | gateway_batches_converge_replays_on_one_upstream_batch; gateway_batch_creations_release_claims_only_without_dispatch at 2875: cross-replica replay, drain refusal, credential refusal/recovery, 5xx fence, denied call before claim, request abort while admission is blocked, then same-request successful replay. These force real seams rather than rely on timing sleeps. | PASS |
| INV-004 / AC-005: concurrent declared/overridden runs remain separate | independent Agent/registry values in both public Run invocation and reusable Agent loop | `skald-agent/tests/integration/agent_journey.rs:316` execution_route_isolated_through_tool_loop joins primary/backup Agent clones; both model iterations target their own deployment, native mock remains unused, original declaration/YAML unchanged. `wyrd-ai` gateway_inference::agent_routes_and_pre_run_overrides supplies real-server default/override and denied-override evidence. | PASS |
| Immutable admitted provider/capture facts survive concurrent updates | snapshot/plan/AttemptRecord carry the actual adapter/provider and deployment to execution/capture; no reconstruction from later config | Existing immutable snapshot and attempt capture tests are supporting proof; full retained-schema assessment belongs to the data domain. | PASS for this boundary |
| FIND-001-14 owner addition: restart uses downloaded loaded Agents | fresh Rust child uses public Agent::from_card with exact downloaded Prompt ref and no route setter/provider override; same downloaded bundle is supplied after restart | R2 P9 records success plus route-loss mutation failing; source asserts route/provider/model and correlated retained rows, while the outer driver checks upstream headers and counts. It does not replace race tests above. | PASS for concurrency/process isolation |

R2's owner-raised `tool_concurrency_cap` materialization changes serialized
identity; this domain does not adjudicate its task scope. No concurrent shared
mutation or route bleed follows from that conversion: it operates on local
owned runtime/spec values. The behavior/invariant reviewers own preservation
obligations for that conversion.

## Findings and verification limits

No material concurrency finding is proposed. Existing snapshot, typed admission,
claim/fence and per-run registry owners cover these outcomes without another
lock, cache, admission workflow or global mutation mechanism.

No tests or builds were run by this reviewer; committed execution evidence was
checked against actual selectors and assertions, not independently reproduced.
No synthetic load or baseline rerun was used. Process death can leave an
uncertain pending batch claim; retaining that fence is the existing safety
contract, not a claim of automatic uncertain-operation recovery.
