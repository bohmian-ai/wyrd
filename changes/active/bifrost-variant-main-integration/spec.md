---
id: SPEC-bifrost-variant-main-integration
revision: 5
status: approved
task_count: 7
---

# Integrate the Bifrost variant with current main

## Approval, objective, and value

Revision 2 records explicit human approval of every reconciled obligation in
this conversation: batches 1–4, 5–8, 9–12, 13–16, 17–20, 21–24 and 25–27 were
each approved (“agree” / “agree on all 4”). These approvals include the unified
resolutions and the user's explicit benchmark/test consolidation instruction.
The approved scope is one spec and one integrated delivery on the existing branch.
The user explicitly requested decomposition of original TASK-001 into six tasks
on 2026-10-11; this organization amendment preserves revision 2's full behavioral
authority, decisions, implementation constraints, and proof obligations.
No deployment, retained-data deletion, push, external fork publication, or merge
into main is authorized by this packet. The packet is the implementation
authority; reconciliation.md is supporting decision history, not a rival task.

Produce one working integration of the rewritten Bifrost with main's current
Wyrd product, security, verification, client, and tooling behavior. Users must
be able to emit, verify, retain, and query attributable evidence through the
real first-class clients and agent surfaces without losing main's fixes.

There is exactly one integrated delivery outcome, implemented through six
sequential scoped tasks. The consumer-complete breadth remains mandatory: an
engine that compiles but loses its surrounding consumers is not deliverable.
Task boundaries are reviewed internal checkpoints, never permission to omit a
consumer or release a partial outcome. Every task closes its affected consumers,
tests and artifacts; final integrated acceptance accounts for all contributions.

## Revision 3 — Bifrost is the variant

The user ruled on 2026-10-11: “the only source of truth for Bifrost architecture
is variant (S)… I don't want main's outdated and stale architecture for Bifrost
in this codebase”; “variant (S) is the owner of the bifrost logic”; “this is for
bifrost only”. This section overrides every conflicting sentence elsewhere in
this spec, its tasks, decomposition, contract, and dispositions.

**Bifrost scope.** `crates/vala/vala-bifrost-redux`; the Bifrost tables, queries,
row types, and tests in `crates/vala/vala-sql`; the Bifrost harness, journeys,
and benchmark workloads in `crates/wyrd/wyrd-testing`; the Bifrost server seams
(`otlp_decode.rs`, Bifrost gRPC/HTTP wiring); `wyrd_client::bifrost`; and
`architecture/bifrost-design.md`. Non-Bifrost main code is unaffected.

**Rule.** Inside Bifrost scope the implementation is S at
`cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. There is no main Bifrost mechanism,
state, SQL, test, or prose except the exceptions below. A conflicting “retain
main's …” or “compose with main repair” obligation in REQ-001–REQ-027 is struck.
Composition means adapting an exception to S's owners — never layering a main
mechanism over or beside an S one, never two implementations.

**Exceptions — main behavior kept, adapted to S's owners:**

| ID | Exception | Governing REQ | Status |
|---|---|---|---|
| X1 | `card_uid` observation correlation and Gate attribution of unbound writers | REQ-013, REQ-014 | user-approved |
| X2 | One shared non-blocking Scribe outbox for audit, gateway captures and Verifier results; no audit staging, publisher, or chain; audit content and six trusted destinations | REQ-010, REQ-011, REQ-012 | user-approved |
| X3 | Oracle and Scribe peer routing: Scribe capability and internal-write routing independent of Oracle | REQ-009 | user-approved |
| X4 | Role-owned heartbeat and one process membership poller | REQ-008 | user-approved |
| X5 | Ten built-ins, eager provisioning, no `vala.dev.agent_traces` | REQ-018 | user-approved |
| X6 | Ordered typed query bind parameters | REQ-019 | user-approved |
| X7 | Main identity at the Bifrost entrance: verified-principal Gate, API-key OTLP authentication before decode, `TenantConn`/`OperatorPool` SQL capability types | REQ-022 | user-approved |
| X8 | No-wait Oracle graph release (main c63e2ff46): a settled or dropped graph releases inline without waiting on nested children; a late child returns its bytes through the shared memory root and the query memory view's drop is the leak check; one cancellation token ends the leader's first-batch wait and an open stream alike, and a cancel observed before the first batch is recorded as `cancelled` | REQ-007 | user-approved |
| X9 | Task Verifier results: `vala.verification.results` carries main's nullable `assertion_result` Variant column | REQ-016 | user-approved |
| X10 | One Forge table authority (Q16, an S defect exposed by X2): the active `forge_tasks` row is the table's exclusive claim — claiming it acquires the table, its heartbeat renews it, every durable transition and pre-commit check fences on it, and its one terminal transition, committed by the settlement seam after the heartbeat stops, releases it; a strategy that decides its own outcome returns it for that seam to commit. Every Forge table operation, manifest rewrite included, runs as a task, so contention is only a refused claim. `vala.maintenance_leases` and `vala.assert_maintenance_lease_fence` are deleted, and `ForgeLease` is rebound to carry the task claim instead of a separate lease | — (S defect) | user-approved 2026-10-12 |

**Struck (reverted to S):** REQ-005 flush ownership repair; all of REQ-006;
REQ-007's settlement model; main deadline refusal codes; main Forge SQL,
leadership, reclaim, expiry, publication-retry and operation-state designs and
their tests. Commits 136e460d6, ba86fe129, f10f735dd, 53dfc2a1c and the Forge
gate part of 985aa12f8 are reverted with them. S's tests for S's behavior return.

**Migrations.** Nothing has shipped, so no migration history is preserved:
the Bifrost migrations in `crates/vala/vala-sql/migrations` are S's files,
edited in place. X2 and X5 are applied by editing those files (audit staging and
`vala.dev.agent_traces` are never created), not by drop or forward migrations.
Main-only Bifrost migrations and the integration's forward migrations are
deleted, along with tests that pin shipped-migration bytes. REQ-024 is struck;
non-Bifrost (`wyrd-sql`) migrations are unaffected.

**Benchmarks.** S's query-capacity, write-capacity and client-queue workloads
are restored with S's exact inputs, steps, assertions, metrics, report layout
and output paths. They compile into main's one consolidated capacity build, and
every S command remains callable by name, unchanged:
`mise run bench:bifrost:query-capacity [-- --fixture telemetry | --heavy]`,
`mise run bench:bifrost:write-capacity [-- --fixture logs|telemetry]`,
`mise run bench:bifrost:client-queue`. Main's `bench:capacity` stays beside
them. `changes/active/bifrost-variant/BENCHMARK-HANDOFF.md` runs as written.

**Acceptance (AC-028).** Bifrost scope contains no trace of main except X1–X9,
and no S mechanism except as X10 corrects it,
each adapted to S's owners. Evidence: a per-file diff of Bifrost scope against
S where every hunk maps to one exception (or to a non-Bifrost rename that the
exception forces); the struck mechanisms absent by symbol and SQL object; the
net Bifrost schema equal to S's apart from X1, X2, X5, X9 and X10; and the handoff benchmark commands running
unchanged with S's report shape.

## Immutable inputs and integration authority

| Input | Pinned commit |
|---|---|
| Main / integration base | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6` |
| Variant source, `origin/wyrd/bifrost-variant/TASK-003` | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05` |
| Common ancestor | `5ab92b0039404565e51f4a246863ce90cbd8d11f` |
| Integration branch | `wyrd/bifrost-variant/integrate-main` |

The existing remote integration branch contains pinned main plus the other
reviewer's research/benchmark documents and the initial packet. Research tip
`40113bc83ba49792d24b17c7552709c4566f2af0` and packet commit
`28b6f3b6da61874bd345464f13b735cdde41833e` are preserved. Use the existing
worktree and branch; do not recreate or reset them. The implementation integrates
the pinned variant while preserving provenance;
this packet alone does not perform that merge. Refreshing either input requires
recording and reviewing the new delta rather than silently widening scope.

Authority is explicit:

1. Current user instructions, including the corrections below.
2. Main's Wyrd doctrine, ownership, tenant/security, audit, and contributor rules,
   except the Bifrost decisions explicitly replaced by this approved change.
3. The variant's Bifrost rewrite and approved source contract, composed with the
   compatible main repairs and retained surrounding capabilities enumerated here.
4. This revision's exact resolutions of cross-boundary conflicts.
5. Tests and historical research, which are evidence and cannot redefine scope.

The user clarified: “whatever is in bifrost-variant wins (except for things weve
discussed (heartbeat, poller, etc)).” Therefore source snapshot retention and
deadline clamping win. Do not restore reader pins, uncapped main deadlines, or
the researcher's withdrawn proposal to reject oversized deadlines instead.

The user subsequently instructed: “no get rid of vala.dev.agent_traces for now.”
Keep that table removed even though the source specification includes it.
Its sole-owner CodeAxis behavior is excluded. The retained built-in set is ten.

An unchanged source/base fragment is not an intentional removal of a newer
main capability. Research confirmed this distinction for query parameters,
Workflow files, identity migrations, and provisioning. Preserve those main
capabilities at the source-engine seam; do not replace Bifrost's new owners.

Update the owning architecture and public documentation in the same integration
to remove contradictory old reader-pin, event-time, audit-publication, terminal,
schema, and shutdown descriptions. Preserve main's unrelated protocol prose.

## Required behavior — all 27 decisions

Each REQ corresponds one-to-one to the numbered recommendation accepted by the
user. [Evidence](evidence.md) identifies immutable sources, symbols, history,
direct conflicts, and silent seams. Private helper/file structure is not fixed.

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

**Struck in revision 3.** Scribe staging, persistence, flush and publication are S's, unchanged.

### REQ-006 — Compatible Forge durability and lifetime repairs

**Struck in revision 3.** Forge (expiry, publication, leadership, worker, scheduler, catalog operations) and its SQL are S's, unchanged.

### REQ-007 — Oracle-owned memory settles correctly

**Struck in revision 3; X8 in revision 4.** Oracle governed memory and executor ownership are S's. X8 is the remainder: a settled or dropped graph releases inline with no wait on nested children and no spawned release work; a late child returns its bytes through the shared memory root; the query memory view's drop poisons the governor on bytes that never return; one cancellation token ends the first-batch wait and an open stream, and a cancel observed before the first batch is recorded as `cancelled`.

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

**Struck in revision 3.** Bifrost migrations are S's files edited in place; see Revision 3, Migrations.

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
source manifests (superseded by Revision 5's fork rule: `5f401029…` on
`wyrd-production`, 22cd178 minus the unused Glue catalog). Neither old source 6965606 nor main 4e9f325 is sufficient. No
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

## Invariants and excluded outcomes

| ID | Required invariant |
|---|---|
| INV-001 | Source analytical architecture remains one engine; no alternate reader/storage/executor owner. |
| INV-002 | Tenant, Card, credential, peer fence/body, and object authority never widen on retries or adaptation. |
| INV-003 | Audit is one non-blocking authorization path; engine lineage never becomes permission audit. |
| INV-004 | ACK/replay/publication retain exact durable identities and uncertainty evidence. |
| INV-005 | Role discovery remains live through necessary drain; all owners settle or report counted/visible loss. |
| INV-006 | Observation subject/writer/application Run and Verifier/result identity remain distinct and exact. |
| INV-007 | Declared types/time axes and logical Variant values agree across writers/readers/schema/layout. |
| INV-008 | Partial/failed/cancelled query streams are not successful complete results. |
| INV-009 | Main control-plane (`wyrd-sql`) migrations and state survive; Bifrost migrations are S's edited files; old Bifrost format is not silently admitted. |
| INV-010 | No old dev table/CodeAxis, publisher, audit chain, deleted scripts, descriptor, or duplicate harness is restored. |
| INV-011 | Main public integrations and all first-class clients consume the same source-authoritative contract. |
| INV-012 | Checks are not bypassed, generated output is not hand-edited, and private mechanics remain implementer-owned. |

Excluded: deployment, main merge/push, destructive reset, retained-data migration,
mixed-version compatibility, new public peer services, new distributed scheduler,
new runtime/graph/transport layers, unrelated auth/storage/UI rewrites, blanket
lint suppression, synthetic host load, or changes to the inferred source-only
cache-timestamp/fleet-deadline concerns. These observations are evidence, not
new merge architecture instructions. A reproduced failure that prevents an
approved acceptance outcome still requires diagnosis and cannot be ignored.

## Acceptance and evidence closure

For every number n from 001 through 027, **AC-n** means REQ-n is satisfied in
the integrated candidate with its consumer closure, success/failure/edge proof,
and no violated invariant. These are independent acceptance obligations, not
permission to mark a group passed after only its happy path.

The six-task traceability map in decomposition.md assigns every AC to its
inspected owners, task-local ordered behavioral scenarios or explicit non-TDD
proof, and exact focused recipes. Original TASK-001 is retained verbatim under
source/ as the lossless historical record; its former one-task scheduling is
superseded, while all substantive obligations remain required. The
implementation evidence must record one row per AC with actual source/diff,
test/check result, and any bounded command correction. All 527 source changed
records, all 148 conflicted paths, and all 76 automatically merged overlapping
modifications receive a reviewed disposition; historical L01–L64/S1–S4 are
reconciled rather than replayed blindly. New discoveries extend the ledger.

Verification execution is mandatory for the implementer and final reviewer:
expect substantial pedantic Clippy fallout from the source import. Fix the
underlying code; do not add `#[allow]`, `#[expect]`, `cfg_attr` suppressions,
workspace lint exceptions, or weaken a check to silence integration failures.
Existing justified policy is not permission for new integration suppressions.

Run lanes and the gate to completion, collecting every runnable test/check
failure before making fixes. Preserve non-fail-fast runner behavior and exit
status; do not cancel on the first failure or use fail-fast options. If an
aggregate aborts early, ensure its remaining independent checks execute and
record prerequisite-blocked checks explicitly. Compilation/setup failures can
prevent dependent tests from running; never report those tests as passed.
Then bucket failures by shared root cause, fix those causes, and rerun affected
proof followed by the required final closeout.

Run every lane and gate with logging and production test telemetry enabled.
Use `WYRD_LOG=info,vala_bifrost_redux=debug`, `RUST_LOG` set to the same filter
for runners using it, and `RUST_BACKTRACE=1`. Retain complete output, exit
statuses, failing span/trace evidence and metric diagnostics. Environment
filters alone do not install telemetry: preserve the owning harness subscriber,
tracer and metrics recorder; fix a missing harness rather than diagnose blind.
Use repository-managed telemetry; no external collector credentials are required.

The final immutable change review, not the task's TDD iteration, runs with
those logging/telemetry settings:

```bash
mise run gate
mise run test:bifrost:journey:scribe:production-geometry
mise run test:bifrost:journey:forge:production-geometry
```

The extra geometry lanes prove production sizes excluded/reduced in the broad
aggregate. Do not repeat its included component lanes as final evidence. Live
cloud/provider checks are not claimed as performed; unavailable credentials do
not replace repository-managed proof. The approved opt-in benchmark parity
closeout is additionally required and is not covered by the correctness gate:

```bash
mise run bench:bifrost:query-capacity
mise run bench:bifrost:query-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity -- --fixture logs
mise run bench:bifrost:write-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity
mise run bench:bifrost:client-queue
```

These entrypoints select workloads in the same consolidated target on the
integrated tree; the source baseline packaging remains untouched. Produce
matched exact-source baselines first where missing, using the same host,
resources, fixtures and settings with no compilation running. Compare standard
and telemetry reads on hot/Parquet before logs/telemetry/native writes and
client/ingest behavior. Preserve current source verdicts and compare case qps,
p50/p95/p99, CPU/query, peak memory and raw report provenance. Repeat suspected
regressions, diagnose consistent degradation, and do not invent a new tolerance
or suppress validation. Old/experimental reports are not a cfe21dc90 baseline.
No new optimization is part of this parity work.

Required strongest proof includes real SDK→server→SDK/agent journeys; Rust-only
unit tests support pure ownership/error branches, real Postgres tests prove SQL
and tenancy, and Python/Node lifetime behavior runs in its owning runtime.
Trace a failing Bifrost/server test before diagnosis. Exact named selectors,
features, setup, expected RED and actual GREEN results belong in task evidence.

## Revision 5 — One object store for all Wyrd storage

The user ruled on 2026-10-12: “As long as GCS XML multipart supports what we
require then we can swap it”; “Im fine with consolidation, but everything must
be supported”; “this will ultimately be a win because we can simplify
dependencies … and have 1 source of truth for storage”. Cause: S's Bifrost
writes GCS through `object_store` (XML API) while main's `wyrd-storage` used
OpenDAL and the gcloud SDK (JSON API), and no one GCS emulator serves both.

**Rule.** `object_store` 0.14.2 is Wyrd's one storage client. `wyrd-storage`
owns construction of the one backend store (S3, GCS, Azure, local) that both
`StorageHandle` and `BifrostStorage` use; Bifrost passes only its client and
retry policy. Presigned URLs come from `object_store`'s `Signer`, multipart
control from `MultipartStore`. OpenDAL, `aws-sdk-s3`, `aws-config`,
`gcloud-storage`, `gcloud-auth` and the `azure_*` crates leave the workspace.

**Wire contract.** `GcsResumableV1`, `UploadPlan::GcsResumable` and
`UploadCompleteRequest::GcsResumable` are deleted. GCS uploads above the
multipart threshold use the GCS XML multipart API, the S3 multipart protocol
GCS implements. Because the protocol now serves both backends it is renamed
backend-neutral (user ruling 2026-10-12): `S3MultipartV1` → `MultipartV1`
(`multipart_v1` on the wire and in SQL), `UploadPlan::S3Multipart` →
`UploadPlan::Multipart`, `S3MultipartComplete` → `MultipartComplete`,
`S3CompletedPart` → `CompletedPart` — server-created upload id, per-part
presigned `PUT ?partNumber&uploadId`, server completion with the client's part
ETags, server abort. GCS XML multipart limits (5 MiB–5 GiB parts, 10,000 parts, 5 TiB)
equal the existing plan limits. GCS therefore gains server abort and sweeper
cleanup. Nothing has shipped, so `wyrd-sql` migration
`20260601000002_storage.sql` is edited in place to drop `gcs_resumable_v1`
and rename `s3_multipart_v1`.

**Preserved.** Single PUT and GET presigning on every backend; S3 per-part
presigning, completion and abort; Azure block-blob SAS (account key or user
delegation key from token credentials) and server block-list commit with the
existing zero-based six-digit block ids; typed not-found and throttling;
`HeadInfo` size, content type and SSE marker driving `require_encryption`;
replay re-mint; sweeper aborts; boot credential probe; the S3
`AbortIncompleteMultipartUpload` lifecycle warning; per-operation storage
metrics; emulator-only endpoint overrides. Credentials resolve through
`object_store`'s environment chains, the chain Bifrost already requires.

**Emulators.** RustFS and Azurite are unchanged. GCS runs
`tustvold/fake-gcs-server`, the emulator `object_store`'s CI uses; emulator
lanes sign with a throwaway service-account key generated per run and send
data-plane requests unauthenticated. No `fake-gcs-server` implements GCS XML
multipart (fsouza/fake-gcs-server#852), so GCS multipart is proven only
against real GCS in `test:storage:gcs:cloud` (user ruling 2026-10-12, “Do 1”);
the emulator proves GCS single PUT, HEAD and handle CRUD.

**Fork dependencies.** The user ruled on 2026-10-12 “remove all outdated
storage dependencies” and “I want to standardize a common name across all used
forks”. Every `bohmian-ai` fork Wyrd uses (iceberg-rust, iceberg-compaction,
datafusion, datafusion-distributed, arrow-rs) publishes the revision Wyrd pins
on one branch, `wyrd-production`; manifests still pin literal revisions.
`iceberg-compaction-core` declared `iceberg-catalog-glue` without using it,
which pulled OpenDAL, the AWS Glue SDK and `aws-config` into Wyrd; the pin
moves to `5f4010291869ed4fb71357aa3dec96746010319a` on `wyrd-production`,
which is 22cd178 with only that dependency removed.

## Open material decisions and revision history

All product/architecture/security/format resolutions are approved in the seven
batches; none remain open. The external compaction artifact prerequisite was
fulfilled by remote integration commit `914495674`: the manifest pins
`22cd178d14009468bc951a14d7acf63237af808a`, confirmed published on the fork’s
`wyrd/main`. The remote research records the fork’s build/test and no-Ring
checks; this packet does not claim those checks were independently rerun.
Combined Wyrd lock/Hakari resolution and native verification remain implementation
obligations, not unresolved design decisions. External fork publication is not
performed or implicitly authorized by this Wyrd packet.
Old populated Bifrost deployment is
explicitly excluded, not an unanswered migration decision. Private adaptation,
helper placement, fixture synchronization, and source test relocation are
implementation-owned within the approved outcomes. A material contradiction
or proposed scope/contract change routes through a new spec revision.

| Revision | Date | Status | Basis |
|---|---|---|---|
| 1 | 2026-10-11 | superseded | Initial transcription; incomplete task recipes and conflicting earlier research choices replaced by revision 2. |
| 2 | 2026-10-11 | superseded | Explicit approval of all seven batches (1–27), unified Task/registration/error/SQL resolutions, and source workload retention with main target consolidation. |
| 3 | 2026-10-11 | superseded | User ruling that S is the sole Bifrost implementation: exceptions X1–X7 approved, REQ-005/006/007/024 struck, Bifrost migrations edited in place (nothing shipped), benches restored by name in one build, AC-028. |
| 4 | 2026-10-11 | superseded | User approval of X8, the no-wait graph release with one cancellation signal and no spawned release work, and X9, main's Task Verifier `assertion_result` Variant column. |
| 5 | 2026-10-12 | approved | User approval of one `object_store` storage client for `wyrd-storage` and Bifrost, GCS XML multipart replacing GCS resumable, and removal of OpenDAL and the cloud SDKs (TASK-007). |

## Authority links

- [Repository standards](../../../AGENTS.md)
- [Agent rules](../../../architecture/agent-rules.md)
- [Wyrd protocol](../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Bifrost authority](../../../architecture/bifrost-design.md), updated under this change where source replaces main design.
- [Security posture](../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../architecture/references/languages/testing-workflows.md)
- [Canonical integration evidence](evidence.md)
- [Reconciliation decision history](reconciliation.md), superseded as execution authority by this approved revision.
- [Six-task decomposition and complete traceability](decomposition.md)
- [Shared implementation contract](implementation-contract.md)
- [Complete proof recipes](proof-recipes.md)
- [Original integration task, verbatim historical source](source/TASK-001-integrate-bifrost-variant-with-main.md)
