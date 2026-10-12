---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-023, REQ-026, REQ-027]
primary_requirements: [REQ-005, REQ-006, REQ-007, REQ-008, REQ-009]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-023, AC-026, AC-027]
scenarios: [S05, S06, S07, S08, S09]
depends_on: [TASK-001]
---

# Approved durability and lifecycle overlaps

## Outcome and Value

Source engine owners retain the approved main concurrent-flush, Forge lease/term/quiescence, Oracle memory settlement, membership lifetime and independent Scribe routing guarantees.

Primary closure ownership: REQ-005 / AC-005, REQ-006 / AC-006, REQ-007 / AC-007, REQ-008 / AC-008, REQ-009 / AC-009. Supporting clause contributions: REQ-001, REQ-002, REQ-004, REQ-010, REQ-023, REQ-026, REQ-027. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 2 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

## Owners, Scope, Consumers, and Prohibited Changes

Own approved M safeguards inside the actual S staging/persistence, Forge, governor/executor, cluster and peer owners. Keep source replacement retention, non-blocking cuts, native v3 rewrites and assignment shape. Membership and outbox drain are a real cross-owner dependency: exercise the retained main outbox already preserved by TASK-001 rather than inventing a temporary sink. TASK-003 reruns the same drain proof after changing audit/ingress projection; it does not supply an excuse to omit TASK-002's delayed-discovery assertion.

Depends on accepted TASK-001 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-001 / AC-001 — Source analytical engine | S BifrostCatalog, OraclePlanner, ScribeIngress, ForgeRewriteCommit; catalog/oracle/scribe/forge/storage owners | S published/distributed journeys; P01/P06/P07/P22 | M consumers still assume replaced providers/schema/storage | Import cohesive S engine; adapt consumers, not old owners | S rewrite owns real native execution/storage state; no additional engine justified | S01/S03/S04/S06/S07/S20 |
| REQ-002 / AC-002 — Snapshot retention/deadlines | S OraclePlanner::request_deadline/pin_cut; OracleTableCuts::read; Forge expiry protections | P03/P06 source retention and non-blocking-cut proofs | M active-reader SQL and unlimited budgets conflict | Use S retention/clamp and delete superseded reader workflow | Existing S retention is replacement authority; no reader-pin layer | S02/S06 |
| REQ-004 / AC-004 — Scribe ACK/recovery | S shard group commit; NewestFence/FenceLookup; ScribePersistence recovery | P04/P22; SQL newest acknowledged fence | M lacks S compressed/group/fenced recovery changes | Import S WAL/replay/SQL as one durable protocol | Existing shard/recovery owners suffice | S04/S20 |
| REQ-005 / AC-005 — Concurrent flush | M StagingAssembler/OwnedMembers; PersistenceWorker::publish_residue | P05; scribe staging/persistence callers | S may return while another publisher owns snapshot members | Port M snapshot ownership into S staging | No new flush coordinator or queue | S05 |
| REQ-006 / AC-006 — Forge safety | M ForgeLeadership::with_term/relinquish; ForgeLoopQuiescence; PreparedExpiry; S native rewrite | P06; ForgeTasks/ForgeOperations SQL consumers | S lacks newer term/quiescence/lease corrections | Adapt M invariants to S owners, preserve S independent cuts | No active-reader barrier or duplicate publication scheduler | S06 |
| REQ-007 / AC-007 — Oracle memory drain | M OracleAdmission::shutdown; OracleQueryResources; S resource governor | P07; distributed executor release | S drain ignores Oracle headroom and includes Forge memory | Adapt M owner-specific settlement to S resources | Use existing shared memory root; no per-role pool | S07 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-009 / AC-009 — Independent peers | M ScribeRoute::select/ScribePeers::channel; S ScribeFragmentExecutor | P10/P11; private grpc mounts and ingress | S capture requires local Oracle for remote Scribe TLS | Keep M routing + S assignment/fencing and limits | No additional peer service/transport | S09 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S restores independent old publishers/staging | Retain M one process outbox; change row/frame projection | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-026 / AC-026 — Native graph/tooling | S native Cargo pins/ObjectStore factory helpers; M deps.sh/Hakari/profiles | Static I26; scoped dependency/Clippy/codegen checks | Locks/lints/test scripts/signing drift between snapshots | Resolve combined graph; retain M deleted tooling and artifact storage | Combined compatible compaction pin + required analytical helpers; source workload modes in existing capacity target; no unrelated signer rewrite | I26/S23 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P31; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

## Approved Requirement Detail

The following requirement bodies are copied verbatim from revision 2 for packet-local implementation. Primary requirements are fully owned here; supporting requirements contribute only the scope above and remain mapped to their closure owner. Do not reinterpret their full acceptance obligations.

### REQ-001 — One source-authoritative analytical engine

Integrate the source native scans, Variant/shredding, declared timestamp types,
ObjectStore storage, Iceberg v3 publication, cut cache, Scribe recovery, and
distributed executor together. Do not revive replaced analytical scan, pruning,
fixed-IPC, payload-cache, or duplicate storage owners to satisfy old imports.
Rehome compatible newer main behavior in the actual source owner.

### REQ-002 — Source snapshot retention and deadlines

Retain source replacement-time retention, supersession roots, read-cut function,
configured default/max query lifetime, and request deadline clamp. Remove the
superseded per-query SQL reader-pin workflow. Do not introduce main's unbounded
deadline policy or an alternative over-limit rejection policy. Propagate the
source behavior consistently through server and public clients/documentation.

### REQ-004 — Source Scribe durability and recovery

Keep group-fsynced acknowledgements, compressed IPC, durable batch fences,
fenced final-record corruption handling, and publication-aware recovery. No ACK
precedes its durable authority, replay does not duplicate writes, corruption
covered by an acknowledged fence fails closed, and published recovery does not
reupload objects. Wire, WAL, ingress, SQL, and crash proof must agree.

### REQ-005 — Flush includes concurrently owned rows

Port main's flush snapshot ownership repair into source staging/persistence.
Flush cannot complete while another publisher owns a member in its promised
snapshot. Preserve source publication/recovery behavior. Fault controls and
backlog inspections remain table-scoped so audit traffic cannot consume another
table's injected fault or invalidate its proof.

### REQ-006 — Compatible Forge durability and lifetime repairs

Retain main's lease-bounded destructive-effect handling, prepared/submitted
distinction, uncertain-outcome reconciliation, leader-term linearization,
authority revocation on scheduler exit, and same-owner reclaim only after proven
prior-loop quiescence. Retain main's bounded catalog calls and definite-conflict
publication retry policy, including unchanged operation/output identity on
retry. Compose these with source retention and native v3 rewrites; do not
restore old active-reader SQL or publication wall deadlines.
Source cuts remain non-blocking while expiry holds maintenance authority,
including prepared expiry. Do not port main's barriers/refusals for new cuts;
retain its durable uncertain-operation evidence and lease bounds instead.

### REQ-007 — Oracle-owned memory settles correctly

Adapt main's shutdown fixes to source governor/executor ownership. Oracle drain
waits for Oracle governed memory and infallible headroom, including late child
ownership, without waiting for independent Forge reservations. A query slot
can settle while a legitimate child view returns its bytes; successful release
never masks poisoned or abandoned ownership. Keep the source resource model.

### REQ-008 — Role heartbeat and one membership poller

Retain main's final role-owned ClusterTask lifetime and one process membership
poller. Readiness withdrawal never re-advertises readiness, but heartbeat and
discovery continue while outbox/role drain needs them. Teardown joins/stops
them; failed construction and abort terminate their tasks. Do not restore
source's premature process-token cancellation or per-role poller duplication.

### REQ-009 — Scribe capability and routing independent of Oracle

Preserve Scribe-only fragment execution, role-specific readiness/fences,
tenant/body authority, and source assignment shape. Internal writes choose
local or ready remote Scribes from boot-resolved TLS and membership without
requiring a local/ready Oracle. Keep configured decode ceilings, per-peer
deadlines, retry rotation, and stale endpoint eviction. Do not invent a second
peer protocol solely to match remembered ScribePeer/OraclePeer names.

### REQ-010 — One shared non-blocking Scribe outbox

Retain main's process outbox for authorization audit, gateway captures, and
Verifier results. Use destination-grouped bounded Arrow frames and
content-derived batch IDs; retry identical slices, consume/count terminal
rejections, and count observable shutdown loss. Permission completes before
operation/refusal and audit delivery never delays or fails it. No separate
publisher, SQL staging, relay, audit WAL, or public AuditUnavailable error.

### REQ-023 — Retained product owners and ordered shutdown

Keep main Workflow/MCP/gateway/Operator/Trigger/verification/identity/principal
owners, routers, configuration, and tracking connected in state and boot. Drain
accepted Workflows while their dependencies remain available; query tools settle
before run terminal state. Account for tracked calls, drain shared Scribe outbox
while Bifrost accepts, then settle roles/storage and observation queue within
the existing process deadline. Preserve errors and lifecycle settlement rather
than merely restoring a shutdown call.

### REQ-026 — Coherent native graph and retained main simplifications

Take source Arrow/Parquet 60, DataFusion 55.1, object_store 0.14.2, Variant crates,
and analytical forks as a coherent graph. The compaction pin must be one reachable
immutable commit combining source compatibility with main's no-REST/Ring
narrowing. The supplied commit is
`22cd178d14009468bc951a14d7acf63237af808a` on the fork’s `wyrd/main`,
already pinned by integration commit `914495674`; preserve it when importing
source manifests. Neither old source 6965606 nor main 4e9f325 is sufficient. No
Ring waiver is allowed. Preserve unrelated locked versions during directed graph
resolution rather than performing a blanket dependency update; preserve main unrelated deps,
features, profiles, pedantic rules, tier boundaries, and removed Ring/unearned
features. Regenerate combined lock/Hakari output. Carry only shared-storage
helpers needed by source Bifrost; retain main artifact signing/storage workflow.
Preserve source query/write/ingest-client benchmark workloads, inputs, assertions,
metrics, report meaning and usable mise entrypoints, but compile them into main's
existing consolidated capacity target. Preserve main mixed-capacity defaults;
retain source Host versus main scoped placement and experiment lifetime semantics.
Do not restore per-workload source binaries, create target directories per case,
or change the reference variant worktree. Do not restore unrelated SignerFuture churn, retired checks/family scripts,
checked-in proto descriptor, or source review-skill/.claude changes.

### REQ-027 — Consumer-complete tests, artifacts, and documentation

Port useful source journeys/Arrow fixtures into main consolidated targets and
current public fixture stories. Reuse main isolated TS server fixture. Cover all
three SDKs and agent surfaces, registering new Variant/time/timestamp cases in
focused lanes and CI selection without silent omission. Regenerate schemas,
stubs, native TS declarations, errors, and proto outputs from source owners;
remove managed_candidates consistently. Update served OpenAPI, runtime MCP,
examples, and owning architecture where their actual contracts change. No
weakened assertions, manufactured tests for static work, or restored obsolete
test binaries/harnesses.

## Approach

1. Inspect each M safeguard's body and map the exact approved exception before changing its source owner.
2. Port snapshot membership and table-scoped fault/backlog behavior into S persistence without changing publication-aware recovery.
3. Compose Forge term/quiescence/prepared/submitted/uncertainty and bounded retry semantics with source retention/native rewrite and non-blocking cuts.
4. Integrate Oracle-owned memory/headroom settlement over the source shared root and executor, preserving poisoned/abandoned ownership diagnostics.
5. Retain one M process poller and role task lifetimes, boot-resolved TLS/Scribe routing, configured frame/deadline limits and source frozen role/fence assignment.
6. Complete S05–S09 including competing audit traffic, pre/post submission expiry, delayed remote discovery and valid above-default private frame readback; record affected source regressions.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S05 — Flush waits for its snapshot, not unrelated future work

**Behavior.** Flush includes ready and concurrently claimed members durable at its snapshot; a later unrelated claim does not hold it indefinitely. Audit rows cannot steal another table fault injection. Mapped obligations: REQ-005 / AC-005.

**RED.** Run P05; extend its existing concurrent claim proof with source raw staging rows and table-scoped fault traffic. Expected RED is early flush return or unrelated claim/fault interference.

**GREEN.** Port main snapshot membership/settlement into source persistence while retaining source recovery and no-reupload behavior. Rerun S04 and the changed flush selector.

**REFACTOR.** Reuse assembler/persistence ownership; no new queue, retry coordinator, or global test serialization.

### S06 — Forge safety within source retention and native rewrites

**Behavior.** Term replacement linearizes schedule effects, scheduler exit revokes authority, only joined prior work permits same-owner reclaim, and expiry distinguishes unsubmitted from uncertain submitted effects under its lease. New cuts retain source non-blocking behavior during expiry; native v3 lineage survives rewrites. Mapped obligations: REQ-006 / AC-006, REQ-001 / AC-001, REQ-002 / AC-002.

**RED.** Run P06. Extend the existing uncertain-operation test for pre/post-submission lease expiry and exact retained operation identity. Expected RED is stale-term work, panic-authorized reclaim, lost uncertain evidence, or premature reset. Old main cut-barring assertions are superseded and must be replaced, not ported.

**GREEN.** Compose main compatible Rust/SQL lifetime and submission invariants with source retention/native rewrite; keep bounded definite-conflict retries and no ambiguous resubmit. Rerun S02 and S04 where publication changes.

**REFACTOR.** Remove retired wall-deadline/reader-barrier machinery without dropping exact uncertainty, fencing, or lineage checks.

### S07 — Oracle drain accounts for late children and excludes Forge

**Behavior.** A released query slot may have a child still returning bytes; Oracle shutdown waits for Oracle governed/headroom bytes, does not wait on independent Forge, and never reports abandoned ownership as clean. Mapped obligations: REQ-007 / AC-007.

**RED.** Run P07; extend the existing shutdown assertions against the source executor/view release seam. Expected RED is early drain, poisoned legitimate child release, or Forge blocking Oracle shutdown.

**GREEN.** Adapt main owner-specific accounting/settlement to source memory root/executor. Rerun affected S06 shared-root work.

**REFACTOR.** Keep source resource governance; no old executor, per-role root, or speculative pool wrapper.

### S08 — Withdrawn roles keep discovery alive until teardown

**Behavior.** Heartbeat advances after readiness is withdrawn without ready becoming true. One membership poller remains current through delayed remote-Scribe outbox drain; shutdown, abort and failed boot end all owned tasks. Mapped obligations: REQ-008 / AC-008.

**RED.** Run P08 and P09. Expected RED is frozen roster, missed newly ready Scribe, re-advertised readiness, duplicate pollers, or leaked heartbeat on teardown.

**GREEN.** Retain main final ClusterTask and single process poller ownership/order around source role engines. Rerun S07 and retained audit drain after outbox changes.

**REFACTOR.** Remove source per-role poller duplication; keep only lifecycle owners that retain real tasks/state.

### S09 — Independent fenced Scribe peers and configured transport

**Behavior.** Scribe-only fragments work under exact role/fence/tenant/body authority; router-only internal writes reach ready Scribes without local Oracle. Lost/silent peers retry within configured timeout and valid frames use the configured decode ceiling. Mapped obligations: REQ-009 / AC-009.

**RED.** Run P10/P11. Extend the existing private-listener journey to submit/read back a legitimately encoded frame above tonic default when configured above it. Expected RED is Oracle dependency, wrong sibling-role checks, dropped valid frame, missing deadline, or widened peer authority.

**GREEN.** Retain main boot-selected route/TLS/discovery and transport limits, adapting source assignment codec and Scribe follower authority. Rerun S08 during route changes.

**REFACTOR.** No second peer service or transport; preserve mTLS-only mounts and one frozen endpoint/fence per query attempt.

## Acceptance Criteria

- **TASK-002-C01.** Flush waits for exactly its captured ready/claimed membership, ignores later unrelated claims and remains immune to competing audit-table faults.
- **TASK-002-C02.** Forge proof covers term replacement, revoked scheduling, prior-loop join, both pre- and post-submission lease expiry, exact retained operation identity and no ambiguous resubmit; new cuts remain non-blocking.
- **TASK-002-C03.** Oracle-only drain includes late child bytes and infallible headroom, excludes independent Forge memory and never reports poisoned/abandoned ownership as clean.
- **TASK-002-C04.** One membership poller and role heartbeat continue through required drain; delayed remote Scribe discovery is forced during drain; shutdown/abort/failed construction settle tasks.
- **TASK-002-C05.** Scribe-only roles enforce frozen assignment/fence/tenant/body authority; valid frames above tonic default below the configured ceiling obtain ACK/readback; silent/stale peers obey bounded retry/eviction.
- **TASK-002-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-002-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

Scribe assembly/staging/persistence and fault controls; Forge leadership/scheduler/worker/expiry/publication plus SQL authority; Oracle admission/resources/executor teardown; cluster tasks, server role state/composition/poller, peer routing/listeners/TLS; owning concurrent/crash/memory/membership/private-peer tests and docs.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P04, P05, P06, P07, P08, P09, P10, P11. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

### Non-TDD proof

Relevant I18/I24/I26/I27 contributions (removal/history/graph/registration/artifact/docs metadata) use static or regression proof; changed runtime scenarios still require the complete TDD cycle.

### Exact Selected Recipes

#### P04 — Source group/fence/crash recovery

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed)'
```

#### P05 — Main flush snapshot and concurrent ownership

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::tests::owns_any_tracks_only_snapshot_members)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::persistence::tests::flush_waits_for_an_outside_claim_over_members_durable_at_drain)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=scribe::persistence::tests::flush_ignores_an_outside_claim_taken_after_its_snapshot)'
```

#### P06 — Forge term/quiescence, uncertainty and source non-blocking retention

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::schedule_use_is_linearized_with_term_replacement)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::relinquished_term_refuses_every_handler)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::failed_scheduler_ends_its_term_before_restart)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::interrupted_startup_waits_for_its_prepared_lease)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration -P journey --run-ignored=all -E 'test(=pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'
```

#### P07 — Main owner-specific Oracle memory settlement

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::admission::tests::oracle_shutdown_drains_oracle_infallible_headroom)'
```

#### P08 — Role task stop/abort/drop

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=cluster::tests::stop_joins_the_task_once) | test(=cluster::tests::stop_aborts_a_task_past_its_deadline) | test(=cluster::tests::abort_and_drop_end_the_task)'
```

#### P09 — Real membership lifetime and audit drain

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'
```

#### P10 — SQL role fences and private peer trust

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(=pg_oracle_membership::same_node_roles_have_independent_fences) | test(=pg_oracle_membership::stale_role_mutation_is_rejected) | test(=pg_oracle_membership::live_discovery_excludes_expired_heartbeat)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete) | test(=peer_network::security::peer_context_refusals) | test(=peer_network::transport::peer_transport_uses_immutable_fenced_destinations)"'
```

#### P11 — Shared outbox no-Oracle routing, bounded retry and refusal

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes) | test(=scribe_outbox::tests::an_oversized_detail_reaches_scribe_in_bounded_frames_that_replay_identically) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'
```

## Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 2 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

An unexplained substantive S divergence outside approved M exceptions is a material authority conflict, not permission for a local redesign. Preserve previous task checkpoints; do not edit their acceptance outcome to hide a regression.

## Authority Links

- [Spec revision 2](../spec.md)
- [Six-task traceability and dependency map](../decomposition.md)
- [Shared implementation contract](../implementation-contract.md)
- [Complete original task, verbatim historical source](../source/TASK-001-integrate-bifrost-variant-with-main.md)
- [Complete proof recipes and inspected selectors](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Reconciliation history](../reconciliation.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Commits: `ba86fe129` (S07), `f10f735dd` + `53dfc2a1c` (S05), `136e460d6` (S06),
`3712b9ba3` (S08), `b7b2de753` (S09), `985aa12f8` (lint at release features).
All tests ran through `mise` with
`WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-002-C01 | `scribe/persistence.rs` flush proof: outside claim on the test table, audit staged at generation 2 under a 2-slot claim budget, fault scoped by `target_table_for_test` | P04 (6 lib selectors + Postgres crash replay) and P05 (3) pass; extended `flush_waits_for_an_outside_claim_over_members_durable_at_drain` asserts armed fault, scoped backlog `(audit 0, table 1)`, flush returns 1 | PASS |
| TASK-002-C02 | `forge/expire.rs` hold = `lease.remaining() - uncertainty_margin`; `expiry_gates.rs` post-preparation gate; `snapshot_expiration.rs` `Unsubmitted` variant plus `operation_id`/`operations` identity on retained and settled state | P06 3 lib + 7 integration + 1 SQL pass; `forge::snapshot_expiration::*` (8) and `forge::reader_expiry_ordering::*` pass; term replacement, revoked handlers, joined-loop reclaim, non-blocking cuts, v3 lineage covered by the P06 selectors | PASS |
| TASK-002-C03 | `resources.rs` shared-root `released` Notify and `oracle_infallible_headroom_bytes`; `oracle/admission.rs` drain over Oracle governed + headroom bytes, poisoned ledger reports `usize::MAX` | `oracle_release_paths_are_exact_and_idempotent`, `oracle_shutdown_drains_only_oracle_memory`, `oracle_shutdown_drains_oracle_infallible_headroom`; 119 owner tests and 187 `oracle::` lib tests pass | PASS |
| TASK-002-C04 | Main ClusterTask/single poller retained (no source per-role poller); `WyrdTestCluster::stop_node_restarting_another_during_drain` boots the Scribe only after drained roles are unready and the drain is still running | P08 (3) pass; P09 `unready_roles_heartbeat_until_teardown_ends_membership_tasks` and extended `oracle_only_pod_retains_audit_staged_before_its_drain` pass | PASS |
| TASK-002-C05 | Capture peer listener decodes at `transport.message_limit_bytes()` (`grpc/mod.rs`); listener journey submits a 6,295,638-byte audit frame as a member, requires ACK, reads back the exact row length through an Oracle | P10 SQL (3) and peer-network (3) pass; P11 outbox (6) pass, including silent-peer retry and terminal refusal | PASS |
| TASK-002-EVIDENCE | This section; `dispositions.tsv` rows for the write set set to TASK-002 | — | PASS |
| TASK-002-REVIEW | Independent review not run here; the user decides | — | OPEN |

### Per-scenario RED/GREEN

- **S05.** Production behaviour was already ported in TASK-001, so this scenario only verifies it. The extended test was proven by a mutation RED: unscoping `take_claim_retirement` fails it, and the source was restored.
  - **Fixture diagnosis 1:** a backlog of `(1,1)` came from a one-slot claim budget held by the outside claim. Fixed with `start_with(faults, 2)`.
  - **Fixture diagnosis 2:** a "writer-v2 replay identity" mismatch came from both members using shard 0, generation 1. Fixed by staging the audit rows at generation 2.
  - **GREEN:** P04 and P05 commands as listed in the recipe.
- **S06.**
  - **RED:** `forge::snapshot_expiration::unsubmitted_expiry_at_the_lease_bound_is_released` failed with `Sql(InvariantViolation { "maintenance lease fence lost …" })`.
  - **Cause:** the hold timeout equalled `lease_ttl`, so the pre-submission reset always ran after the fence had lapsed.
  - **Fix site:** `run_snapshot_expiry_for_table_inner`. An independent read-only diagnostician confirmed this site.
  - **Source check:** main carries the same bound and the variant has no lease-bounded expiry, so this is a correction inside main's REQ-006 code, not a merge from either branch. `architecture/bifrost-design.md:1216` still holds, because the lease still bounds the hold.
  - **GREEN:** the same selector plus the accepted, rejected and retryable variants, then the full P06 set.
- **S07.**
  - **RED:** compile failure on the missing `oracle_infallible_headroom_bytes` field, then three behaviour failures:
    - late child: `Some(Accounting)` vs `None`;
    - headroom: 67112960 vs 4096;
    - drain reporting Forge bytes: 1052672 vs 4096.
  - **GREEN:** the same selectors plus the owner lanes.
  - **Removed:** source `LiveConsumers`, the graph child drain and its wall deadline, the supervisor child idle/debt refusal, and `a_registered_zero_byte_child_keeps_its_query_busy`. That test's property is now unreachable under main's settlement model.
  - **Replaced:** five source analytical tests, superseded by main's versions.
- **S08.** The P08 and P09 baselines passed before and after the change.
  - **Change:** the journey now boots the Scribe strictly after observing readiness withdrawn while the drain is still running. Before, the Scribe booted before the drain started.
  - **Result:** GREEN on the first run. Main's poller already keeps discovery alive, so the preserved behaviour is now proven.
- **S09.**
  - **RED:** a mutation removing `.max_decoding_message_size(...)` from the capture peer service failed the listener journey with `decoded message length too large: found 6295638 bytes, the limit is: 4194304 bytes`. The source was restored.
  - **GREEN:** the listener journey and P10/P11.

### Static lanes

`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation` and `git diff --check` all pass.

`lints` first failed twice, and both were fixed in `985aa12f8`:
- **Gate fields:** clippy's `struct_field_names` lint fired on the expiry gate fields; they were renamed.
- **Graph release:** `unused_async` fired on the analytical graph release in the release-feature binary pass. The release is now synchronous, and the test pause moved into its async callers, as on main.

### Write set and exclusions

The write set is listed in `dispositions.tsv`. Two of its files are main-only and therefore have no B→S row:
- `forge/expiry_gates.rs` (test-support gate);
- `tests/bifrost/oracle/peer_network/listener.rs`.

Exclusions held:
- No SQL schema, public contract, generated artifact, dependency or configuration change.
- No compatibility shim.
- No `#[allow]`.
- No reader barrier restored.

Deviation for user decision: the S06 production correction above.
