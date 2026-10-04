---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7, FIND-TASK-004-8, FIND-TASK-004-9, FIND-TASK-004-10, FIND-TASK-004-11, FIND-TASK-004-12, FIND-TASK-004-13, FIND-TASK-004-14]
---

# Close accepted-job ownership, contract, and journey gaps

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Validated diagnosis: `changes/active/skald-workflow-runtime/review/TASK-004-r1/findings-validation.md`

## Outcome

Keep the approved process-local Workflow host and existing Cards, Skald, gateway, Oracle, audit, and Bifrost owners. Close two shutdown-ownership races, preserve the original query deadline during cancel-while-open, publish exact built-in tool declarations, align governing authority, and complete the already-required production-shaped journeys. Do not add a durable queue, actor, scheduler, compatibility surface, lifecycle service, timeout option, tenant simulator, second query engine, or standalone repository check.

## Diagnoses and required corrections

### Preparation ownership and shutdown (`FIND-TASK-004-1`, `FIND-TASK-004-2`)

`WorkflowRuns::admit` publishes a preparing key and consumes capacity before the preparation is registered with `WorkflowRuns.tasks`. Shutdown can therefore close and drain an empty tracker before a late task starts. Inside that task, cancellation can drop the `spawn_blocking` join handle while the blocking closure continues detached from Workflow ownership.

Correct the existing `WorkflowRuns` owner so task registration and reservation visibility are coordinated: observable preparation state must already have a tracked owner, and shutdown-winning admission must publish nothing and return the existing unavailable result. Run CPU-bound graph preparation through the existing tracker-supported blocking ownership so cancellation prevents acceptance but clean drain still waits for the closure. Reuse the current lock, tracker, cancellation token, reservation, and preparation outcome. Do not create another executor or queue.

### Query deadline and owner-loss recovery (`FIND-TASK-004-3`, `FIND-TASK-004-4`)

`RunningQueryControls::open_cancellable` lacks the absolute query deadline. When cancellation wins, it can await remote cancellation's fresh per-peer transport bounds and then the same open future beyond the original deadline. Pass the already-existing absolute deadline into this owner and bound the complete cancel-during-open phase with it. Preserve one cancellation attempt, the same open future, authenticated remote routing, and existing honest incomplete/unavailable/timeout results. Add no timeout setting or retry.

The pod-loss branch of the forwarded Workflow journey skips surviving-owner baseline recovery and a later query, and accepts any `WYRD_VALA_*` error. Extend that existing branch to assert its exact current owner-loss settlement class, no rows/model result, recovery of surviving owner counts, and a later successful query through the surviving topology. Reuse `PeerCluster`; add no new probe or cluster harness.

### Tenant and accepted-authority boundaries (`FIND-TASK-004-5`, `FIND-TASK-004-8`, `FIND-TASK-004-14`)

TASK-004 journeys use one authenticated tenant. Random foreign UUID configuration and same-tenant principals do not exercise credential-derived tenant selection. Use the repository's existing second-tenant provisioning/authentication path in Scenarios 1, 4, and 6 to prove cross-tenant root refusal before work, common unknown/foreign get and cancel responses, and no foreign Card/query data.

Extend the current admission and authority journeys to refuse an inactive root; preserve a pinned run across dependency mutation/deactivation; prove newly granted resources and replay cannot widen accepted authority; and prove a later gateway call still obeys current deployment/credential eligibility. Reuse current registry, token, grant, gateway, and deterministic upstream controls. Do not add an authorization cache or refresh mechanism.

Update only the existing relevant sections of `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md`. State that an accepted run retains a token-free snapshot of principal attribution and scopes bounded to its graph and deadline; token expiry or later grant change neither cancels nor widens it; later HTTP get/cancel/replay authenticate and authorize afresh; and Cards, Bifrost, and gateway owners still perform their own live per-call decisions/audit where specified.

### Built-in tool declarations and negative paths (`FIND-TASK-004-6`, `FIND-TASK-004-7`, `FIND-TASK-004-10`)

Add substantive item rustdoc to the eight existing `QueryTool` and `CardsTool` declaration methods. Do not add a documentation file, wrapper, lint allowance, or check.

Replace unconstrained object output schemas with exact projections of the existing result owners: `{columns, rows, terminal}` for the bounded query result and the canonical Card envelope for `cards.get`. Reuse existing schema facilities and actual serialization; do not invent a parallel durable contract or generator.

Extend Scenario 4 through the existing real Agent loop to cover oversized SQL and all numeric limits, independently missing tool permissions, inaccessible Card/table objects, and malformed terminal/partial-row failure with no data disclosure and redacted non-retryable errors. Reuse Cards/query authorization, collector, audit, and fault seams.

### Idempotency and lifecycle journeys (`FIND-TASK-004-9`, `FIND-TASK-004-12`)

Extend Scenario 2 with tenant-key isolation, one cancelled matching waiter while peers and preparation survive, shutdown waking all matching waiters exactly once, and lost acceptance-response recovery with exactly one accepted observation and one provider execution. Reuse the preparation gate, current request cancellation/shutdown path, audit/observation evidence, and upstream counters.

Extend Scenario 6 with completion-versus-deadline, GET during whole-snapshot replacement, global oldest-first eviction across tenants/principals, shutdown of an accepted queued run, and inspection of complete terminal snapshots after shutdown. Owner-level tests for the two preparation-tracker fixes must directly exercise the narrow race; the real server journey remains the end-to-end proof.

### Gateway projection journey (`FIND-TASK-004-11`)

The new in-process adapter has separate OpenAI Chat, OpenAI Responses, Anthropic, Gemini, and Vertex projections/decoders, but the journey covers only OpenAI. Extend Scenario 5 with the existing deterministic gateway/provider fixtures to directly cover every required branch, internal Vertex success, pre-upstream incompatible capability refusal, stored fallback order and concurrent fallback isolation, remaining deadline, and separate cancellation. Continue to call `GatewayInvocation`; add no transport, registry, option, or compatibility layer.

### Graph and snapshot limits (`FIND-TASK-004-13`)

Extend Scenario 7 with a deep but admissible graph, a sibling Bifrost query during held preparation, aggregate `max_run_bytes` overflow distinct from step-result overflow, and near-ceiling failure/cancellation/deadline terminalization. Assert stable errors, no retained oversized data, complete terminal snapshots, capacity release, and serviceability of Cards, gateway, and Bifrost. Reuse existing builders/fixtures; do not add a CPU pool, scheduler, transport surrogate, setting, or synthetic host load.

## Preserved behavior and non-goals

- Preserve process-local runs, affinity/restart loss, fixed retention, typed errors, canonical audit, exact Card pinning, captured authority, and current gateway/tool owner boundaries.
- Preserve the native Oracle attempt/driver/exchange/participant lifecycle and governed shared memory accounting.
- Do not restore the deleted Oracle reserved-byte polling, release refusal, poison state, or mechanism-specific tests. Validation found those to be unsupported bespoke DRIFT rather than required structured ownership.
- No durable run persistence, recovery, lease, cross-replica lookup, Workflow principal, bearer retention, runtime policy gate, arbitrary server tool registration, new MCP/language surface, second audit writer, second query engine, or new third-party dependency.
- Do not weaken, ignore, serialize globally, or delete existing tests/checks to obtain green results.

## Acceptance criteria mapped to findings

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-1` | No observable reservation exists without tracker ownership; shutdown cannot drain before late preparation ownership. |
| `FIND-TASK-004-2` | Blocking preparation remains counted until completion even after outer cancellation. |
| `FIND-TASK-004-3` | Cancel-during-open completes at the original absolute deadline without a fresh cleanup budget. |
| `FIND-TASK-004-4` | Pod-loss case proves exact settlement, surviving-owner recovery, no rows, and later query availability. |
| `FIND-TASK-004-5` | Authenticated second tenant cannot access first-tenant Workflow, run, Card, or Bifrost data. |
| `FIND-TASK-004-6` | All eight declaration methods have substantive rustdoc. |
| `FIND-TASK-004-7` | Both output schemas match concrete serialized results and are closed where the contract is closed. |
| `FIND-TASK-004-8` | Inactive/pinned/no-widening/live-gateway cases pass through existing journeys. |
| `FIND-TASK-004-9` | Scenario 2 covers every AC-021 tenant/waiter/shutdown/lost-response case with exact counts. |
| `FIND-TASK-004-10` | Scenario 4 covers remaining bounds, independent permissions, inaccessible objects, and terminal integrity. |
| `FIND-TASK-004-11` | Scenario 5 directly proves every in-process dialect/Vertex/fallback/deadline/cancel branch. |
| `FIND-TASK-004-12` | Scenario 6 covers required races, global eviction, queued shutdown, and complete terminals. |
| `FIND-TASK-004-13` | Scenario 7 covers deep graph, all sibling services, aggregate overflow, and terminal reserve. |
| `FIND-TASK-004-14` | Existing design and security authorities agree with REQ-032A and the implementation. |

## Focused and broader proof

Use the exact existing selectors from TASK-004 for Scenarios 1–7 and the existing forwarded Oracle Workflow selector. Add exact owner-level selectors for the preparation tracking race, tracked blocking work, and absolute-deadline open cancellation. Run all commands through the repository's prescribed `mise exec --`/`mise run` forms and repository-managed Postgres wrappers.

After focused Red-Green iteration, run the original task's broader affected lanes: `test:wyrd`, `test:shared`, principals unit/integration, gateway journey, Bifrost, codegen check, client-tier, tenant isolation, unwrap audit, format, lints, and `git diff --check`. Do not add a new repository check; the compiler, source review, existing focused tests, and existing lanes are the closure mechanisms.

## Implementation evidence

Commits: `cd726cb09`, `b51459ac0`, `882e02897`, `57bedc866`, `b3e30f51c`, `f047831b8`, `1ab97bcc7`, `f803c5d16`, `047d8bebd`, `3d050cc84`, `2f34edcd2`. Spec Revision 13 (`acb1025cc`) added provider-tagged `ProviderRequest` and the Vertex success proof to this remediation.

Scenario selectors (all in `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs`, run through the wyrd family with repository-managed Postgres):
S1 `admission_is_audited_and_side_effect_free_on_refusal`, S2 `tracked_preparation_replay_and_disconnect`, S3 `accepted_authority_outlives_submission_only`, S4 `declared_tools_use_captured_scopes_and_owned_services`, S5 `server_routes_keep_gateway_and_external_ownership`, S6 `lifecycle_races_retention_and_shutdown`, S7 `graph_and_snapshot_limits_preserve_sibling_services`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-004-1` | `WorkflowRuns` reservation takes a tracker token under the run-table lock drain closes admission under (`components/workflow/runs.rs`, `host.rs`) | `components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists`; S2, S6 | PASS |
| `FIND-TASK-004-2` | Blocking graph preparation runs on the Workflow tracker (`runs.rs`) | `components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned`; S2 | PASS |
| `FIND-TASK-004-3` | `RunningQueryControls::open_cancellable` takes the query's absolute deadline and bounds cancel-during-open with it (`oracle/lifecycle_controls.rs`, `query/collect.rs`, `query/scheduled.rs`) | `oracle::lifecycle_controls::tests::cancel_while_opening_ends_at_the_original_deadline` | PASS |
| `FIND-TASK-004-4` | Pod-loss branch asserts `WYRD_VALA_500_QUERY_EXECUTION_FAILED` without rows, surviving-owner baselines, and a later 3-row query after membership drops the killed pod; `await_membership` moved onto `PeerCluster` and shared with the join journey (`wyrd-testing/tests/bifrost/oracle/{workflow.rs,peer_cluster.rs,peer_network/join.rs}`) | `workflow::workflow_forwarded_query_settles_before_the_run_ends`, `peer_network::join::peer_join_and_remote_query` | PASS |
| `FIND-TASK-004-5` | A credential-derived second tenant (`foreign_admin`/`foreign_runner`) in S1, S2, S4, S6: unknown/foreign get and cancel answer alike, foreign Card read is `WYRD_REGISTRY_404_CARD_NOT_FOUND`, foreign `/v1/query` of the first tenant's table is `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`, same idempotency key is isolated per tenant | S1, S2, S4, S6 | PASS |
| `FIND-TASK-004-6` | Rustdoc on the eight `QueryTool`/`CardsTool` declaration methods (`components/workflow/tools.rs`) | `mise run lints`, source review | PASS |
| `FIND-TASK-004-7` | Output schemas are the closed `{columns, rows, terminal}` projection and the canonical Card envelope, checked against serialized results with `jsonschema` (already a workspace dependency) | wyrd-server tool tests; S4 | PASS |
| `FIND-TASK-004-8` | Inactive root is refused with `WYRD_REGISTRY_404_CARD_NOT_FOUND` before work; pinned run survives dependency mutation; later grants and replay do not widen authority; a later gateway call obeys current deployment eligibility | S1, S3, S5 | PASS |
| `FIND-TASK-004-9` | Tenant-key isolation, one cancelled matching waiter, lost acceptance response recovered with one accepted observation and one provider execution, shutdown waking every waiter with 503 | S2 | PASS |
| `FIND-TASK-004-10` | Oversized SQL and numeric limits, independently missing tool permissions, inaccessible Card/table, EOF-after-schema/batch terminal faults now applied (`QueryStreamFault` frames truncated in `BoundedQuery::claim_fault`) | S4 | PASS |
| `FIND-TASK-004-11` | OpenAI Chat, OpenAI Responses, Anthropic Messages, Gemini and Vertex `generateContent` each reach their own path and decode their own answer; operation capability refusal `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` before any arrival; stored fallback order per concurrent step; run-deadline `TimedOut`; one cancelled run leaves a concurrent run's call alone | S5 | PASS |
| `FIND-TASK-004-12` | Completion vs deadline, GET during snapshot replacement, global oldest-first eviction across tenants/principals, shutdown of an accepted queued run, complete terminal snapshots after shutdown | S6, owner tests above | PASS |
| `FIND-TASK-004-13` | Deep admissible graph (1024 steps), sibling Bifrost query during held preparation, aggregate `RUN_TOO_LARGE` distinct from step overflow with one retained payload, near-ceiling cancel/deadline terminals, Cards/gateway/Bifrost serviceable afterwards | S7 | PASS |
| `FIND-TASK-004-14` | Accepted-run authority described in `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md` | source review | PASS |
| Revision 13 tagged `ProviderRequest` | Derived `#[serde(tag = "provider", content = "body", rename_all = "snake_case")]`; hand-written deserializer, `RawProviderRequest`, `OpenAiChatCompatibleRequest`, manual `PartialEq` and `deserialize_request_like` deleted; `RawV1.body` is `serde_json::Value` (lead-approved) and is converted to `RawValue` only at the provider send sites | `request::round_trip::vertex_request_reads_back_as_vertex`, `request::round_trip::raw_v1_request_reads_back_when_body_precedes_tag`, `request::round_trip::mismatched_body_is_refused`; `test:skald`; `codegen:check` | PASS |
| Revision 13 Vertex success | Vertex deployment and Prompt in S5 reach `/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent` | S5 | PASS |

### Diagnoses

**Pod-loss later query (FIND-4).** Symptom: after the PodKill branch, the later query from the leader was refused with 429 `ADMISSION_REJECTED`. Evidence: the WYRD_LOG trace showed the leader planning onto node 0002 after `kill()`; the cached channel's reserve got tonic `Unavailable`, which `status_error` (`dispatcher.rs:1758`) maps to Terminal, and the graph fails closed (`join.rs:179`). Cause: `kill()` leaves the killed pod's Oracle lease live until its TTL, so leader membership still lists it. Fix site: the test, not the Oracle; the branch waits on the observable membership change and refreshes the leader snapshot. Diagnostician (read-only, given only the command, trace and diff) independently reported the same cause, fix site and no affected production callers. The lead approved the condition wait and the move of `await_membership` into the shared `PeerCluster` support. No sleep, timeout, retry or assertion was weakened.

**S6 stack overflow.** Symptom: S6 aborted with a stack overflow on the 2 MiB test thread. Evidence: it passed with `RUST_MIN_STACK=16777216`. Cause: the inline `cards.register_from_path` future was too large for the test-thread stack. Fix site: the fixture's `register_as` boxes that future, matching the existing `Box::pin` in `start_with` and `restart`.

**EOF terminal fault not applied (FIND-10).** Symptom: S4's EOF-after-schema/batch faults returned complete results. Cause: `claim_schema_stall` claimed the one-shot fault and ignored the EOF variants. Fix site: `BoundedQuery::claim_fault` in `query/collect.rs` now truncates the stream to the faulted frame count, and is the only consumer of the controller.

**Second-tenant harness limitation.** Foreign tenants cannot run a model step: the harness seeds gateway credentials and bindings for the fixture tenant only (`crates/wyrd/wyrd-testing/src/server.rs:4834-4893`), so a seeded tenant's deploy is refused with 503 "gateway credential protection is unavailable". Foreign tenants therefore register bundles without deploying, their runs end `Failed`, and the cross-tenant assertions (S2 key isolation, S4 Card and Bifrost refusals) do not depend on a foreign model call. Lead-approved without a harness change.

**Python state-journey alias list (lead-approved assertion change).** Symptom: `py:test:integration` `tests/integration/state/test_state_journey.py::test_service_bundle_hydrates_complete_python_runtime_offline` failed `state.aliases == EXPECTED_ALIASES` with an extra `default-Agent-triage-1.0.0`. Evidence: `crates/shared/wyrd-client/src/cards/hydrate/graph.rs:264-281` (an edge without an alias gets `default_alias`, `graph.rs:479`; an already resolved card accumulates the new alias). Cause: `abf34f34d` gave `typed_state/runtime.yaml` a step targeting `./agent-triage.yaml`, an alias-less edge, and updated the Rust twin list (`crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:207`) but not the Python mirror; the tagging commit changes no Card identity. Fix site: `EXPECTED_ALIASES` in the Python journey now lists `default-Agent-triage-1.0.0`, matching the Rust list. Diagnostician report: same cause, fix site and callers (only this list; no TypeScript copy). A second assertion in the same test, `state.workflow("runtime_workflow").spec == {}`, was stale for the same commit (the fixture spec now carries the step and `outputs`; it was already `{"steps": []}` before). Second diagnostician report: same cause, the Rust twin (`card_lifecycle.rs:1407-1413`) asserts only the Workflow name, nothing else in the test is stale. Lead-approved fix: the assertion now checks `spec["outputs"] == {"answer": "steps.triage.output.text"}`, not the step list, whose resolved target carries a per-run uid.

**Live-window producer count across an hour boundary (lead-approved fixture change).** Symptom: `test:bifrost` `oracle peer_network::analytical::remote_live_window_blocked_scribe_stops_cleanly` saw two open Scribe producers where it asserts one; it passed alone. Evidence: the peer fixture IPC carried no `wyrd_event_time`, so Scribe stamped receipt time and the default `TimeGranularity::Hour` partitioning applied (`vala-bifrost-redux/src/scribe/ingress.rs:180`); the run's ingests straddled 04:00Z. Cause: rows of one journey landed in two hourly partitions, giving two live routes and two producers. Fix site: `PeerCluster` fixes one `fixture_event_time_micros` at launch and `fixture_rows_ipc` stamps it in the managed event-time column (`wyrd-testing/tests/bifrost/oracle/peer_cluster.rs`), so every `ingest_rows`/`ingest_live_rows` caller lands in one partition; the `producers == 1` assertions are unchanged. `distributed.rs:1403` already writes one shared `now` per row and needed nothing. Diagnostician report: same cause; same assumption at `analytical.rs:1537` and `distributed.rs:1403`.

**Follower graph release after a scheduled query returns (human decision A).** Symptom: `test:bifrost` `server query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup` (`wyrd-testing/tests/bifrost/server/query.rs:297`) once saw a follower still holding `graphs: 1` at the scheduled-return snapshot (`assert_scheduled_owners_released`, `query.rs:847-870`); it passed alone and in the latest full `test:bifrost` run. Evidence: `AnalyticalGraphLifecycle::settle` (`vala-bifrost-redux/src/oracle/analytical.rs:2698-2780`) releases followers only by dropping `AnalyticalParticipantGrants`; a `ParticipantGrant` is a stream handle (`oracle/dispatcher.rs:617-642`) and the follower settles asynchronously on stream close in `AnalyticalStageIngress::drive_graph_settlements` (`analytical.rs:1429+`). Cause: no path awaits the follower's release, so a leader can return before the follower removes its graph. Diagnostician report: same cause; the public `settle` rustdoc (`analytical.rs` ~7776) claims follower work is joined. Fix site: the success path has no follower-release join to reuse (its only follower await is the metric fold, which ends on stage completion), so no join was written. Human decision A: no production behavior change; closing the grant stream is the release and the follower frees its graph itself. The `settle` rustdocs (`AnalyticalAttemptOwnership::settle`, `AnalyticalGraphLifecycle::settle` and its retirement comment in `analytical.rs`, and the settlement note in `query_stream.rs`) now say so instead of claiming follower work is joined. `assert_scheduled_owners_released` (`query.rs`), shared by the success and failure scheduled checks, first waits through the existing `await_clean_analytical` condition wait (its `ANALYTICAL_POLLS` bound) for every node to report no graph, then runs the strict running-query and resource checks; the leader retires its running-query entry only after its own graph is released, so the strict leader proof at return is kept.

### Verification results

| Lane | Result |
|---|---|
| `codegen:check`, `check:client-tier`, `check:tenant-isolation`, `check:unwrap-audit`, `check:pyo3-scope` | PASS |
| `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck` | PASS |
| `test:skald`, `test:shared`, `test:wyrd`, `test:principals:unit`, `test:principals:integration`, `test:gateway:journey` | PASS |
| `py:test:unit` (526), `py:test:integration` (74) | PASS |
| `ts:test:unit` (37), `ts:test:integration` | PASS |
| `wyrd-sdk-rust --test workflow_loading -E 'test(=workflow_loading_journey)'` | PASS |
| `wyrd-testing --test oracle -P journey --run-ignored=only` (41) | PASS |
| `test:bifrost` (9/9 lanes), then `test:bifrost:journey:server` and the oracle journey target after the follower-release change | PASS |

### Revision 13 test changes (lead-approved blanket rule)

Deleted tests (asserted the removed untagged shape inference), in `crates/skald/skald-spec/src/request.rs`:
`untagged_dispatch::provider_request_untagged_dispatch_per_provider`, `untagged_dispatch::vertex_generate_body_is_transparent_google_shape`, `untagged_dispatch::raw_v1_is_last_fallback_for_wrapped_unknown_request`, `untagged_dispatch::deny_unknown_fields_blocks_cross_variant_confusion`. Replaced by the three round-trip tests above; `mismatched_body_is_refused` tags `open_ai_embeddings`, `vertex_predict`, `anthropic_message` and `google_batch_embed` with another provider's body, and `deny_unknown_fields` remains on the wire structs. The `MessageNum` and `ProviderResponse` untagged tests remain.

Assertions changed for the removed behavior: Python `test_prompt_load_yaml_and_json_preserve_deep_fields` and `test_every_provider_builder_constructs_promptcard_and_round_trips_json` (`test_prompt_authoring.py`) no longer expect a Vertex Prompt to read back as `google`.

Inputs and paths moved to the tagged form with unchanged assertions: YAML Prompt fixtures (`examples/workflows/code-review/prompts/*`, `tests/fixtures/workflow-loading/**`, two `wyrd-cli` Agent fixtures), inline YAML/JSON in `wyrd-loader/src/parse.rs`, `pg_workflow_registration.rs`, `pg_workflow_runs.rs`, `oracle/workflow.rs`, `skald-spec` prompt tests, Python `state/support.py` and `test_workflow_gateway_context.py`; Python `request.model_dump()` accessors now read `["body"]`; RawV1 helpers in `skald-prompt`, `skald-cache`, `skald-runtime`, `skald-agent`, `wyrd-cards`, `wyrd-spec` construct `Value` bodies (byte-equality tests compare values and were renamed from `*_bytes` to `*_body`).

Regenerated: every `crates/wyrd-spec/schemas/*.json` and `crates/wyrd-spec/tests/schemas/*.json` embedding `ProviderRequest`, plus `tests/fixtures/eval/schemas/eval_{spec,task}.schema.json`, through `mise run codegen:regen`; Python stubs regenerated with no diff. No pinned Prompt content hash or snapshot failed, so none was regenerated by hand.

### Non-goals

No durable run persistence, recovery, lease, cross-replica lookup, Workflow principal, bearer retention, runtime policy gate, server tool registration, new MCP or language surface, second audit writer, second query engine, migration, compatibility reader, alias, new repository check, or new third-party dependency (`jsonschema` was already a workspace dependency). The deleted Oracle graph-drain polling and supervisor idle refusal stay deleted.
