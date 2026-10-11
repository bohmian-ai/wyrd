---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027]
primary_requirements: [REQ-026, REQ-027]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027]
scenarios: [S23]
depends_on: [TASK-005]
---

# Final qualification

## Outcome and Value

Source benchmark workloads run through main's consolidated capacity target without altered semantics, and the integrated candidate has complete provenance/requirement/proof evidence ready for the specification-owned immutable change review.

Primary closure ownership: REQ-026 / AC-026, REQ-027 / AC-027. Supporting clause contributions: REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

This is task 6 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

## Owners, Scope, Consumers, and Prohibited Changes

Own source workload retention in existing capacity/release/profile/report/mise owners and global disposition/evidence reconciliation. This task does not defer or absorb missing implementation, journeys, exports, generators or docs from TASK-001–005. Correctness gate, both production geometry lanes and all six matched parity runs remain specification-owned final change-review execution; preparation here never counts their absent results as PASS.

Depends on accepted TASK-005 because its actual owners/contracts are prerequisites for this outcome. Start from that integrated checkpoint and preserve its proof. No parallel overlapping implementation or Cargo-backed verification.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-001 / AC-001 — Source analytical engine | S BifrostCatalog, OraclePlanner, ScribeIngress, ForgeRewriteCommit; catalog/oracle/scribe/forge/storage owners | S published/distributed journeys; P01/P06/P07/P22 | M consumers still assume replaced providers/schema/storage | Import cohesive S engine; adapt consumers, not old owners | S rewrite owns real native execution/storage state; no additional engine justified | S01/S03/S04/S06/S07/S20 |
| REQ-002 / AC-002 — Snapshot retention/deadlines | S OraclePlanner::request_deadline/pin_cut; OracleTableCuts::read; Forge expiry protections | P03/P06 source retention and non-blocking-cut proofs | M active-reader SQL and unlimited budgets conflict | Use S retention/clamp and delete superseded reader workflow | Existing S retention is replacement authority; no reader-pin layer | S02/S06 |
| REQ-003 / AC-003 — Read-cut cache | S CutCache::watch/reset; BifrostCatalog::read_cut_cached | P02; S catalog notification migration | M has no cache; stopped watcher must not serve stale cuts | Keep S listener/generation/fallback behavior | S owns actual invalidation state; no second cache/lifetime owner | S03 |
| REQ-004 / AC-004 — Scribe ACK/recovery | S shard group commit; NewestFence/FenceLookup; ScribePersistence recovery | P04/P22; SQL newest acknowledged fence | M lacks S compressed/group/fenced recovery changes | Import S WAL/replay/SQL as one durable protocol | Existing shard/recovery owners suffice | S04/S20 |
| REQ-005 / AC-005 — Concurrent flush | M StagingAssembler/OwnedMembers; PersistenceWorker::publish_residue | P05; scribe staging/persistence callers | S may return while another publisher owns snapshot members | Port M snapshot ownership into S staging | No new flush coordinator or queue | S05 |
| REQ-006 / AC-006 — Forge safety | M ForgeLeadership::with_term/relinquish; ForgeLoopQuiescence; PreparedExpiry; S native rewrite | P06; ForgeTasks/ForgeOperations SQL consumers | S lacks newer term/quiescence/lease corrections | Adapt M invariants to S owners, preserve S independent cuts | No active-reader barrier or duplicate publication scheduler | S06 |
| REQ-007 / AC-007 — Oracle memory drain | M OracleAdmission::shutdown; OracleQueryResources; S resource governor | P07; distributed executor release | S drain ignores Oracle headroom and includes Forge memory | Adapt M owner-specific settlement to S resources | Use existing shared memory root; no per-role pool | S07 |
| REQ-008 / AC-008 — Membership lifetime | M ClusterTask; ClusterRegistry; BifrostComposition.membership_poller | P08/P09; Oracle/Scribe teardown and boot | S process token stops discovery before drain | Retain M final role tasks + one process poller | Existing concrete owners retain task lifetime | S08 |
| REQ-009 / AC-009 — Independent peers | M ScribeRoute::select/ScribePeers::channel; S ScribeFragmentExecutor | P10/P11; private grpc mounts and ingress | S capture requires local Oracle for remote Scribe TLS | Keep M routing + S assignment/fencing and limits | No additional peer service/transport | S09 |
| REQ-010 / AC-010 — Shared outbox | M ScribeOutbox/ScribeSink/ScribeBatch; AuditStage; AppState | P11/P12/P13; audit/capture/results | S restores independent old publishers/staging | Retain M one process outbox; change row/frame projection | Existing outbox already owns batching/retry/loss | S10/S11 |
| REQ-011 / AC-011 — Audit schema/attribution | M project_audit_event; GateAudit blanket impl; Oracle query_audit_event; S AuditLogTable types | P12/P13; actual keyed Gate/Oracle -> retained history | S brings chain/staging; M builders omit credential id | M event content + S created_at/Variant; carry credential | Existing event/sink/projection; no chain/hash authority | S11 |
| REQ-012 / AC-012 — Trusted internal writes | M ScribeCapturePeerGrpc::batch; ScribeIngressFrame attribution | P13; six destinations and public Gate refusals | S older two-destination path loses result/audit attribution | Retain six-destination mTLS and exact trusted UID scope | Existing private route; no minting publisher | S10/S11 |
| REQ-013 / AC-013 — UID observation scope | M CardAttributionRegistry::attributed_cards; AuthContext; S decode/conform | P14/P19/P23; OTLP/native/client correlation | S older CardRef correlation drops unbound UID authorization | Port M trusted UID resolution into S admission/decode | Existing registry/signed scope; no new lookup/cache in CPU decoder | S12 |
| REQ-014 / AC-014 — Observation activation | M ObservationRunOutbox/ObservationRunSink; enqueue_observation_batch | P15/P16; ACK decoder created_at and writer scope | S best-effort task and subject-only activation regress M | Retain M retry/bulk/writer/replay; adapt ACK fields | Existing outbox/SQL; no semaphore task replacement | S13 |
| REQ-015 / AC-015 — Verification runtime | M VerifierRunQueue::renew/tick_schedule; EvalEngine::execute; writer_caller; gateway caller | P15/P16; runner/cache/leases, direct routes | S removes renew/hydration/delay and direct-calls providers | Retain M runtime; port only S read/type projections | Existing verification owner; no second runner/provider path | S14 |
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | S typed structs + existing Task AssertionResult as assertion_result Variant | Existing AssertionResult; no new Task type/table | S15 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-018 / AC-018 — Remove dev table | M canonical built-in inventory/provisioning; S dev/CodeAxis sole owner | Static I18; source dev tests -> supported/dynamic property proof | S approved dev inclusion conflicts with latest user override | Keep ten, no dev/CodeAxis; transfer still-reachable assertions | No test-only inventory or extra name-ban check | I18/S16/S20 |
| REQ-019 / AC-019 — Typed SQL parameters | M Bifrost::sql/sql_as/stream; QueryParam; S OraclePlanner | P18/P19/P23; HTTP/grpc/SDK query bodies | S unchanged-base signatures omit newer params | Bind M typed ordered values in S plan and keep public argument order | Existing planner/transport; no interpolation/second client | S17 |
| REQ-020 / AC-020 — Terminal problems | S WyrdProblem; tonic query_conversion; M from_problem/error catalog | P19/P22/P23; grpc/http/client/CLI/MCP collectors | Wire terminal changes must agree and preserve M other errors | Atomic S problem field with M params/security | Existing typed catalog/conversions; no compatibility parser | S18 |
| REQ-021 / AC-021 — Shared types/client boundaries | S wyrd-types schema mapper/Variant/timestamps; M Bifrost handle and Service declared_tables | P01/P19/P23; cards/service.rs preflight and SDK exports | Queue mapper removed; broad source exports regress M internals | Use S shared mapper and M narrow facade/preflight | Source shared type owner replaces duplicate mapper, no new transport | S01/S16/S19 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-024 / AC-024 — Migration history | M MIGRATOR/verify_migrations; S three forward migrations | P27/P28; serving/operator readiness and role RLS | Historical checksum/source old grants cannot be transplanted | M bytes + S forward DDL with current tenancy | Existing migrators; no reset or checksum bypass | S22/I24 |
| REQ-025 / AC-025 — Fresh v3 contract | S create_physical_table/physical_table_matches; spec no data migration | P06/P28; dynamic/fresh built-ins and format refusal extension | Old tables/objects are not schema-compatible | Fresh isolated Bifrost; reject old schema/version without mutation | No conversion/dual reader; old-state deployment excluded | S20/S22 |
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

### REQ-003 — Source read-cut cache

Retain tenant-qualified combined snapshot/hot-source cuts, transactional
invalidation, listener liveness, and invalidation-raced fill protection. Normal
watcher cancellation disables and clears caching so queries use Postgres;
membership's extended drain lifetime is not imposed on the cache watcher.

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

### REQ-011 — Combined audit content and credential attribution

Retain main's 11 decision-content fields and direct AuditEvent projection; add
explicit decision `created_at` and source Variant `detail` (12 content fields
before the physical envelope). Preserve nullable credential attribution through
actual Gate/Oracle event builders, not just prepopulated events. No seq/hash
chain columns or sequence ordering. Preserve trusted system-owner rules and
audit Card identity content; `audit_card_ref` is not observation correlation.

### REQ-012 — Six-destination trusted internal writes

Retain main's private mTLS audit/two-capture/three-result write path. Only result
destinations carry exact Verifier attribution; the reserved system owner is
admitted only for audit. Public writes to gateway calls and result tables remain
refused for every principal. Preserve validated attributed UIDs in source
ingress without trusting client-supplied internal scopes.

### REQ-013 — UID-only observation correlation

Use card_uid in native correlation, OTLP attributes, shared contracts, client
encoding, Gate, ingress, ACK decoding, private peer frames, and SDKs. Bound
principals use trusted signed scope; unbound principals resolve at most 32
distinct UIDs in their tenant and require observation-target Cards. Missing
correlation is valid/null; foreign, missing, wrong-kind, malformed, or forbidden
UIDs refuse the complete frame. Authentication stamps publisher/request identity.
Keep legitimate CardRef contracts elsewhere; no global rename or alias.

### REQ-014 — Acknowledged observations activate the correct writer's bindings

Keep main ObservationRunOutbox retries, bulk enqueue, subject/writer matching,
durable replay fence, and lock ordering. Acknowledged observations do not wait
for derived queue writes. Freeze admitted row created_at as input_event_time,
not receipt time; next-day replay reads the same row and activates once. Do
not replace this with source best-effort tasks or activate every subject binding.

### REQ-015 — Main verification authority and runtime lifecycle

Keep main claim hydration, cache/baseline ownership, lease renewal, delayed
schedule clock, queue SQL, direct invocation permissions, and trusted tokenless
reads. LLM judges use the governed gateway under the correct judging authority;
no direct ProviderRegistry bypass. Preserve authenticated writer and application
Run lineage while adapting Variant/timestamp readers. Result delivery remains
on the shared outbox and never delays durable run settlement.

### REQ-016 — Typed Drift/Eval summaries and complete Task evidence

Use source typed drift_report/eval_summary. Preserve nullable assertion_result
Variant for Task results only, using the existing AssertionResult
contract; no new Task payload type/table or duplicate summaries. Preserve
nullable/source summary semantics and sensitivity metadata. Drift/Eval summaries
remain typed Structs; open features inside them may be Variant. Sensitivity is
descriptive except main's specific gateway payload-read permission; do not add
a general elevated-read requirement for verification summaries. Result run_id is
the application Run being judged, not the Verifier execution UUID; result and
Verifier-run linkage remain separately attributable.

### REQ-017 — Declared time axes for all retained tables

Remove managed wyrd_event_time and use source types, precision, bounds, and
layout semantics consistently across ingestion, WAL, hot/sealed reads, Forge,
and consumers. card_uid/run_id are correlation-only; server-owned columns stay
unwritable. Dynamic tables default to wyrd_ingested_at or use a valid declared
non-null source-supported timestamp axis. Parent/detail joins use result_id
and cannot assume both rows share a day partition.

| Retained built-in | Declared time axis | Required adaptation |
|---|---|---|
| vala.traces.spans | start_time_unix_nano | Timestamp(ns,+00:00) OTLP instants; preserve exact ordering/bounds. |
| vala.metrics.points | time_unix_nano | Typed point/start/exemplar instants. |
| vala.logs.records | time_unix_nano | Typed event/observed instants. |
| vala.drift.observations | created_at | Timestamp(us,+00:00); retain main window/aggregation semantics. |
| vala.eval.observations | created_at | Variant context/media; preserve writer/Run inputs. |
| vala.verification.results | ended_at | Source typed summaries plus Task assertion_result Variant. |
| vala.drift.result_features | wyrd_ingested_at | result_id join; measured window stays domain time. |
| vala.eval.result_items | wyrd_ingested_at | Variant actual/expected; result_id join. |
| vala.gateway.calls | started_at | request_payload/response_payload Variant, preserving payload-read authorization. |
| vala.system.audit_log | created_at | Actual decision timestamp and Variant detail, no chain. |

### REQ-018 — No development agent trace table

Keep vala.dev.agent_traces removed and remove its sole-owner CodeAxis behavior,
registration/docs/test dependencies. Maintain ten canonical built-ins. Preserve
generic Variant/admission proof using supported or dynamic tables; do not
restore a test-only capability. Main eager provisioning/startup checks operate
over the ten source-updated schemas; source idempotent ensure remains available.

### REQ-019 — Preserve main typed query parameters

Keep ordered QueryParam across shared Rust facade, HTTP/gRPC, forwarding,
native bindings, Python, and TypeScript. Bind values in source planning rather
than SQL interpolation; preserve main public argument positions. This source
omission is unchanged ancestry, not intentional API removal. Parameter support
does not change source deadline clamping or source plan/result semantics.

### REQ-020 — Atomic terminal problem protocol

Take source WyrdProblem terminal representation and protobuf error_problem_json
field 9 with old error field 6 reserved. Keep main request params and other
newer error identities. Spec, protobuf, conversions, server streams, client
reconstruction, SDKs, CLI, and MCP must agree. Partial frames followed by failure
cannot become success. Keep revision/body/tenant validation and no compatibility
shim. Python QueryTerminal.error_code returns the stable catalog code, and error_detail
comes from the problem detail. Descriptor generation follows current main build ownership.

### REQ-021 — Shared types and narrow first-class client projections

Adopt source wyrd-types, schema mapper, timestamp/Variant conversions, row
preflight, and logical JSON rendering. Keep main internal feature gates and
renewable credential pooling identity. Adapt Service declared-table preflight
and registration to the new mapper without dropping its before-Card-transaction
validation/ensure semantics. Registration Created/AlreadyExists is the source
outcome decided under its registration lock; callers must not infer it by a
describe-before-create race. A propertyless top-level table model stays invalid;
open column values can be Variant. Clients do not duplicate server/transport owners.

### REQ-022 — Main identity, credentials, SQL trust, and bootstrap

Retain main auth/credential/role/grant/revocation and human/platform separation,
API-key OTLP authentication-before-decode, verified-principal Gate entrance,
external Postgres credentials, and default local storage. New Bifrost SQL obeys
TenantConn/OperatorPool and main role/RLS conventions; no naked pool/connection
API. The narrow SECURITY DEFINER newest-WAL-fence function follows main's
operator raise/restore and bounded search_path/privilege conventions. Keep selected local HTTP/gRPC endpoints and safe persisted client config.
Do not revive obsolete workload/bootstrap grants or Argon2 credential behavior.

### REQ-023 — Retained product owners and ordered shutdown

Keep main Workflow/MCP/gateway/Operator/Trigger/verification/identity/principal
owners, routers, configuration, and tracking connected in state and boot. Drain
accepted Workflows while their dependencies remain available; query tools settle
before run terminal state. Account for tracked calls, drain shared Scribe outbox
while Bifrost accepts, then settle roles/storage and observation queue within
the existing process deadline. Preserve errors and lifecycle settlement rather
than merely restoring a shutdown call.

### REQ-024 — Main migration history plus source forward DDL

Preserve main applied historical migration bytes/checksums and retired audit
tables' drop history. Import only source's three genuinely new retention,
cut-notification, and WAL-fence migrations, adapting role/RLS/search_path/grants
to main. Put genuinely required historical DDL differences in uniquely versioned
forward migrations; comment changes do not justify checksum changes. Keep old
workload/grant deletions: they are unchanged source/base, not new source changes.
Fresh replay and forward migration from main with no old Bifrost objects must
produce equivalent target SQL while preserving identities/Cards/credentials.

### REQ-025 — Fresh-only Bifrost, no implicit destructive transition

Honor source fresh Iceberg-v3-only and no mixed-version/data-migration contract.
Do not rewrite stored fingerprints, relax schema/format refusals, add historical
read conversion, or delete state to make startup succeed. Use isolated fresh
test Bifrost state. Retained old Bifrost deployment/export/reset is outside this
task; main-owned control-plane data is never collateral cleanup.

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

1. Trace source query/write/client queue workloads and main capacity/release/profile/fixture lifetimes; retain inputs/assertions/metrics/report meaning, Host/scoped placement and mixed defaults.
2. Extend existing capacity dispatch with source workload modes and usable mise aliases, using focused parsing/dispatch/lifetime RED/GREEN; retain source baseline worktree and avoid per-case binaries/target directories.
3. Reconcile all 527 source records, 148 conflict paths and 76 auto-overlaps by canonical dispositions, plus L01–L64/S1–S4 and all task-source assertions.
4. Consolidate S01–S23, I18/I24/I26/I27 and individual AC-001–027 actual source/diff/command/results with invariant evidence; return missing items to the owning task rather than relabeling them passed.
5. Prepare exact-S matched baseline/report provenance and integrated-candidate review handoff, carrying the spec's non-fail-fast logging/telemetry, geometry and parity obligations intact.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S23 — Same benchmark workloads, consolidated compiled targets

**Behavior.** Source query-capacity, native/OTLP write and ingest/client queue
cases keep their inputs, assertions, metrics, report meanings and command
behavior in main's existing capacity target; main's mixed benchmark stays the
default. Host/scoped placement, mode-specific environment/lifetime and current
options remain correct. Main consolidated integration targets preserve source
coverage without resurrected one-case binaries. Maps REQ-026/027, AC-026/027,
INV-010/011/012.

**RED.** In the existing capacity target add focused parsing/dispatch and
lifetime regression assertions for the source workload selections, invalid
options and unchanged mixed defaults. Discover/record/run each exact selector
using P31 before changing dispatch; expected RED is missing/wrong mode or
mixed-only deadline/environment incorrectly required by a source experiment.
Discovering zero tests or only compiling the binary is not behavioral RED.
Source baseline workloads already correct get no-op evidence, not mutations.

**GREEN.** Extend the existing capacity entrypoint and release/profile owners
with the selected workloads, preserve current mise aliases/options, and remove
source per-workload bin declarations after consumer transfer. Rehome integration
cases to their existing main capability targets. Rerun focused new dispatch
assertions and affected query/ingest/server fixture scenarios. Full same-host
benchmark parity runs once at spec closeout, not during every local edit.

**REFACTOR.** Consolidate genuinely shared release/profiling/fixture code within
existing owners; keep isolated source measurements distinct from main mixed-SLO
experiments. No extra binary/target directory per workload, duplicate server
harness, synthetic load, raised test-thread count, or new optimization.

## Acceptance Criteria

- **TASK-006-C01.** S23 actual parsing/dispatch/lifetime tests prove source modes, invalid options and unchanged mixed defaults; cargo check/discovery alone is not behavioral proof.
- **TASK-006-C02.** Source standard/telemetry query, logs/telemetry/native write and client-queue workloads preserve exact inputs/assertions/metrics/report meaning and current command aliases in one existing capacity target.
- **TASK-006-C03.** Every source/conflict/overlap and historical L/S disposition resolves with reviewed provenance/evidence; no omitted or grouped untraceable path silently disappears.
- **TASK-006-C04.** Every REQ/INV/AC has a complete integrated evidence owner, all S01–S23 and static proof records are reconciled, and no inherited 93-command/blanket-PASS assertion substitutes for results.
- **TASK-006-C05.** Review handoff identifies the immutable integrated candidate, exact-S baselines, matched resources/settings and raw reports plus remaining final gate/geometry/parity execution obligations; only the independent final review can establish change completion.
- **TASK-006-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-006-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

wyrd-testing existing capacity/release/profile/workload/report owners; source workload port and current mise aliases; consolidated target registration/CI selection; Cargo/Hakari consumer metadata only where benchmark packaging earns it; packet provenance/source-test/scenario/AC ledger and review handoff. Earlier implementation owners only for a reviewed bounded integration correction, never an unrecorded replacement.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P31. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:workspace-hack`, `mise run check:ci-selection`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Final gate, geometry and parity executions belong to immutable change review under spec Acceptance and evidence closure. This task prepares the workloads, exact-S baselines and complete review evidence; it may not mark unexecuted final commands passed.

### Non-TDD proof

I26 graph/tooling/workload packaging and I27 inventory/registration/generated/document reconciliation use static or verification-only evidence. Benchmark dispatch/lifetime changes require full S23 TDD. Baseline/report preparation is measured verification, not a manufactured RED.

### Exact Selected Recipes

#### P31 — Consolidated benchmark target and new focused mode proof discovery

```bash
# Static target compilation, not behavioral RED or a benchmark result.
mise exec -- cargo check --locked -p wyrd-testing --bin capacity
# Add focused parsing/dispatch/lifetime cases in this existing owner, then
# discover their actual symbols. Record and run each exact selector before GREEN.
mise exec -- cargo nextest list --locked -p wyrd-testing --bin capacity
# Recorded command shape after discovery (substitute an actual discovered name):
# mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<actual-full-name>)'
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
