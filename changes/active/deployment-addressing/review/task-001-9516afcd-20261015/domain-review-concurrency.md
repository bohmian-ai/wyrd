# Concurrency domain review — TASK-001 remediation round 2

**Result: PASS. Proposed material findings: none.**

## Immutable subject and authority

Reviewed `/tmp/wyrd-task-review-9516afcd`, candidate
`9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`, cumulatively against
`1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. The prior candidate is
`ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`. Live edits are excluded.
This is an independent static review of concurrency and lifecycle boundaries,
not a whole-task verdict or a claim that runtime checks were independently run.

Authorities used: AGENTS.md, architecture/agent-rules.md,
architecture/wyrd-design.md, the invocation/host-dependency doctrine,
architecture/references/languages/spec-driven-development.md and maintainer-style.md;
approved deployment-addressing spec revision 6; original TASK-001; prior verdict,
FIND-001-7/FIND-001-10 diagnoses and R1 correction/proof requirements; appended
R1 execution evidence. The archive has no `.codegraph`; root navigation was
supplied in the shared subject map. No source modifications or builds were made.

## Boundary and source coverage

| Boundary | Producer, consumers and existing owner inspected | Assessment |
|---|---|---|
| Coherent tenant configuration | `service.rs:657–684` `GatewayAdministration::snapshot`, SQL `gateway.rs` `SNAPSHOT_SQL`/`gateway_snapshot`, gateway `snapshot.rs` domain values; administration deployment/credential replacements and snapshot tests | One SQL statement produces an owned admission snapshot; subsequent authorization, plan and execution receive that same value. No independent cache or administration lock was introduced. |
| Requested/fallback authorization | `invocation.rs:456–589` `admit_call`/`route`, `decide:707`, public `invoke`, internal `run`, batch `call`, server judge `caller.rs`, native ingress/routes | No caller can skip admission authorization with a stale boolean. Requested authorization precedes routing; each distinct fallback is authorized against the admitted snapshot. A denied requested target never enters fallback dispatch. |
| Batch creation and replay | `batches.rs:386–517`, `refresh:752–774`, `call:786–797`; SQL `CLAIM_BATCH_SQL`, `BATCH_BY_REQUEST_SQL`, `RECORD_BATCH_SQL`, `RELEASE_BATCH_SQL`, and their query functions | Existing canonical digest/tenant unique claim owns duplicate suppression. Fresh and completed replay paths both reenter ordinary admission. An undispatched admission refusal releases only its own still-pending claim; ambiguous dispatch retains its fence. |
| Batch sibling calls | upload, retrieve/cancel, file metadata, file content, delete in `batches.rs:286–774`; `CallPlan::pin` in `routing.rs:113–121` | Metadata uses its own current snapshot decision; all provider IO uses common admission. Provider-held state remains pinned to its deployment and cannot be rescued by a fallback. No new per-resource admission workflow was added. |
| Bounded listing | `batches.rs:556–627`, `invocable`; SQL `BATCHES_SQL:862–872`, `gateway_batches:1003–1022` | Open/Granted visibility is projected from one snapshot into SQL eligibility before limit, with existing per-model audited decisions afterward against the same snapshot. Concurrent administration is not reread mid-page. Tenant RLS and bounded selection remain. |
| Execution/settlement | gateway `engine.rs:464–618`; `invocation.rs:399–436`, `CallTask::run:1091`, buffered/stream settlement; `ledger.rs` admission/accounting owners | Owned planned candidates and the same snapshot reach credential resolution and dispatch. Cancellation/deadline still bound credential/dispatch work. No successor attempt follows cancellation or ambiguous non-idempotent dispatch. Existing tracked settlement, reservation locking and accounting remain owned by the gateway pipeline. |
| Agent invocation-local route | `agent.rs:334–387`, from_card/to_spec; `loop_runtime.rs:64–76,202–212`; `gateway.rs` registry/provider; `observe/invoke.rs:35–108`; Python `py_run:379–402`; delegate adapter; server gateway caller | The route/registry is selected per run from immutable Agent data. Overrides use a separate Agent value; neither a registry singleton nor a mutable shared route carries invocation state. Tools run through the selected registry, delegates enter their own Agent's run, and missing host gateway dependency refuses before native/journal/session IO. |
| Surviving task scope | Named-target engine and plan change; approved Workflow removal; fixture/SDK journey consumers | Named deployment replaces weighted selection without adding shared route mutation. Deleting Workflow is approved behavior, not a recovery regression. No new concurrency mechanism is needed. |

Paths in the table are under `crates/wyrd/wyrd-server/src/components/gateway/`
unless a gateway, SQL, Agent or shared-client owner is explicitly named.
The cumulative changed source and R1 diff were both used; the latter only located
fixes. No peer discovery reports were used.

## Prior-finding closure and proof assessment

| Obligation / prior finding | Current source evidence | Verification evidence inspected | Result |
|---|---|---|---|
| FIND-001-10 / REQ-002, REQ-010, INV-007: batch actual dispatch must not reuse earlier authorization | `create` retains its early snapshot decision before claiming; `settle` and completed replay `refresh` call `GatewayInvocation::run`. `admit_call` unconditionally decides at lines 482–486 and passes that owned snapshot into plan and execution. The prior `authorized` parameter is removed from the owner and callers. | `pg_invocation_tests::gateway_batch_dispatch_reauthorizes_against_its_routing_snapshot` (3076–3265): SQL table-lock barrier proves initial authorization completed before replacement; Open→Granted replacement denies fresh create and completed replay before provider IO; fresh claim count returns to zero; audit records both allowed and denied; explicitly scoped caller succeeds through the same race. R1 table records RED→GREEN and P7/P9 passing. | PASS; closed in this boundary |
| FIND-001-7 / REQ-002, AC-002: denied newest batches cannot strand visible older batches | `list` computes Granted exclusions and scoped grants before its single limit+1 query. The SQL predicate includes scoped deployment/model reach or All reach excluding Granted deployments. The final decision uses the same snapshot as pruning. | Existing `gateway_batch_listing_is_one_bounded_pruned_read` extends mixed access/grants and continuation proof; its source and the R1 P7 result were inspected. | PASS; closed |
| Fresh claim release, uncertain dispatch and replica replay are preserved | `settle` releases only `!failure.dispatched` or a known provider refusal below 500; unknown/5xx/malformed-success paths retain pending state. SQL request digest conflict is tenant-scoped, and refresh only acts on recorded provider IDs. `pin` retains just the holding deployment. | `gateway_batches_converge_replays_on_one_upstream_batch` and `gateway_batch_creations_release_claims_only_without_dispatch`: cross-replica pending conflict, created replay, 5xx fencing, credential/drain refusal, denied call before claim, and caller cancellation while admission waits followed by successful identical replay. R1 records P9 passing. | PASS |
| INV-004 / AC-005: default/overridden invocations remain isolated, including tool iterations | Runtime Agent route and registry are local immutable values; `Run::invoke_on` creates its own Agent without mutating the hydrated Card; Python uses an override clone. Server caller stores no per-call deadline or mutable route. | `agent_journey::execution_route_isolated_through_tool_loop`: concurrent primary/backup runs both make two model calls through their own routes, native provider untouched, original Card/YAML retained. Rust real-server `gateway_inference::agent_routes_and_pre_run_overrides` at 650–725 joins default and override calls, checks distinct credentials and unchanged declaration, then proves Granted override denial before IO. | PASS; FIND-001-2 concurrency aspect closed |
| Cancellation/timeout and already-admitted snapshot semantics survive named addressing | `CallTask` owns its snapshot/plan, `execute` borrows those exact values, credential resolution reads the admitted credential entry, and cancellation classifies dispatch before settlement. Administration replacement has no route into an existing call's owned configuration. | Source of engine cancellation/deadline and non-idempotent failure tests; administration immutable-snapshot test; pg invocation cancellation/settlement and batch abandonment tests. These are supporting regression evidence, not a newly demanded availability contract. | PASS |

FIND-001-9's concurrency-sensitive attribution aspect is also consistent: the
attempt's provider name now comes from its admitted candidate adapter and is
retained on `AttemptRecord`; capture does not reconstruct it from a later
administration snapshot. The data-domain review owns full retained-schema proof.

FIND-001-14 is an explicitly human-directed addition. Its restart journey is
relevant supporting evidence for saved bundles/configuration survival; it does
not replace the dispatch-race and concurrent-run tests above. This domain review
does not independently adjudicate all provider plumbing, every authoring round
trip, docs visual criteria, or other findings outside concurrency.

## Findings and limits

No material concurrency finding is proposed. Source supports closure of the
prior stale-snapshot cause at its existing common admission owner; adding a new
lock, authorization cache, per-handler guard, or replacement batch workflow would
not be necessary.

No test/build command was run by this reviewer. Passing results above are
implementation evidence from the candidate packet, independently checked for
selector existence and meaningful assertions, not independently reproduced runs.
No artificial load, baseline checkout, or expanded suite was used. An abrupt
process loss can still leave an uncertain batch claim pending; retaining that
fence is the existing safety behavior, not evidence of a duplicate-dispatch fix
being incomplete or permission to release ambiguous work.
