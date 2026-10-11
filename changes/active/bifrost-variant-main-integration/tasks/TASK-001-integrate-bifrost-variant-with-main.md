---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-017, REQ-018, REQ-020, REQ-021, REQ-022, REQ-024, REQ-025, REQ-026, REQ-027]
primary_requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-024, REQ-025]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-017, AC-018, AC-020, AC-021, AC-022, AC-024, AC-025, AC-026, AC-027]
scenarios: [S02, S03, S04, S20, S22]
depends_on: []
---

# Source engine integration

## Outcome and Value

A cohesive pinned-S analytical engine and compatible native dependency graph are available through the retained main server/client owners, with source retention/cache/WAL/v3 semantics, additive main-compatible SQL, and no destructive old-state transition.

Primary closure ownership: REQ-001 / AC-001, REQ-002 / AC-002, REQ-003 / AC-003, REQ-004 / AC-004, REQ-024 / AC-024, REQ-025 / AC-025. Supporting clause contributions: REQ-017, REQ-018, REQ-020, REQ-021, REQ-022, REQ-026, REQ-027. All INV-001–INV-012 apply. The task-specific criteria below are scoped contributions; integrated AC closure also includes the contributing tasks in [decomposition](../decomposition.md).

This task requires [shared implementation contract](../implementation-contract.md), [approved spec revision 2](../spec.md), canonical evidence and reconciliation. Current user direction explicitly authorizes this six-task decomposition without changing the approved behaviors. Use `$wyrd-implement`; implementation self-report is not independent task completion.

## Owners, Scope, Consumers, and Prohibited Changes

Own the cohesive native scans/Variant/timestamp/ObjectStore/v3/cache/WAL/executor import, source wire/type foundation, three new source migrations, and directed Cargo/lock/Hakari composition. Preserve main historical migration bytes and existing product/security/client owners. Mechanical adapters required to keep the imported contracts consistent and executable belong here, including producers/readers, public terminal conversions and retained parameter fields when their source types change; do not leave temporarily incompatible wire/schema consumers for a later task. TASK-003 and TASK-004 subsequently prove attribution/result semantics; TASK-005 closes their public journeys. This foundation is not permission to import source's retired publishers or old auth state wholesale.

No prerequisite task. Start from the recorded fresh integration plan commit, never the rejected archive.

All source-fidelity, SQL trust, fresh-only, no-deployment/no-push/no-main-merge, no-compatibility and no-gate-weakening prohibitions in the shared contract apply. Paths/symbols are inspected guidance, not a private implementation allowlist. Caller/test/fixture/export/generated/doc closure is part of this task, not work postponed to TASK-006.

This is task 1 of 6 for merging the bifrost variant worktree into the current main logic (this worktree)

### Source-backed reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| REQ-001 / AC-001 — Source analytical engine | S BifrostCatalog, OraclePlanner, ScribeIngress, ForgeRewriteCommit; catalog/oracle/scribe/forge/storage owners | S published/distributed journeys; P01/P06/P07/P22 | M consumers still assume replaced providers/schema/storage | Import cohesive S engine; adapt consumers, not old owners | S rewrite owns real native execution/storage state; no additional engine justified | S01/S03/S04/S06/S07/S20 |
| REQ-002 / AC-002 — Snapshot retention/deadlines | S OraclePlanner::request_deadline/pin_cut; OracleTableCuts::read; Forge expiry protections | P03/P06 source retention and non-blocking-cut proofs | M active-reader SQL and unlimited budgets conflict | Use S retention/clamp and delete superseded reader workflow | Existing S retention is replacement authority; no reader-pin layer | S02/S06 |
| REQ-003 / AC-003 — Read-cut cache | S CutCache::watch/reset; BifrostCatalog::read_cut_cached | P02; S catalog notification migration | M has no cache; stopped watcher must not serve stale cuts | Keep S listener/generation/fallback behavior | S owns actual invalidation state; no second cache/lifetime owner | S03 |
| REQ-004 / AC-004 — Scribe ACK/recovery | S shard group commit; NewestFence/FenceLookup; ScribePersistence recovery | P04/P22; SQL newest acknowledged fence | M lacks S compressed/group/fenced recovery changes | Import S WAL/replay/SQL as one durable protocol | Existing shard/recovery owners suffice | S04/S20 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-018 / AC-018 — Remove dev table | M canonical built-in inventory/provisioning; S dev/CodeAxis sole owner | Static I18; source dev tests -> supported/dynamic property proof | S approved dev inclusion conflicts with latest user override | Keep ten, no dev/CodeAxis; transfer still-reachable assertions | No test-only inventory or extra name-ban check | I18/S16/S20 |
| REQ-020 / AC-020 — Terminal problems | S WyrdProblem; tonic query_conversion; M from_problem/error catalog | P19/P22/P23; grpc/http/client/CLI/MCP collectors | Wire terminal changes must agree and preserve M other errors | Atomic S problem field with M params/security | Existing typed catalog/conversions; no compatibility parser | S18 |
| REQ-021 / AC-021 — Shared types/client boundaries | S wyrd-types schema mapper/Variant/timestamps; M Bifrost handle and Service declared_tables | P01/P19/P23; cards/service.rs preflight and SDK exports | Queue mapper removed; broad source exports regress M internals | Use S shared mapper and M narrow facade/preflight | Source shared type owner replaces duplicate mapper, no new transport | S01/S16/S19 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
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

1. Verify M/S/B pins and initialize the complete source/conflict/overlap inventory, including main-only retention obligations.
2. Import the cohesive source engine, shared types and native dependency group; preserve the supplied combined compaction pin and main unrelated versions, profiles, features, signing and storage owners.
3. Compose source time/table/wire foundations with the already-approved main exceptions needed for atomic contracts and mechanical consumer compilation; keep ten built-ins and existing public parameter positions.
4. Preserve main migration history and adapt only genuine source forward DDL to TenantConn/OperatorPool, roles/RLS/search_path/grants; prove the pinned-main control-plane upgrade.
5. Execute ordered source engine/cache/durability/fresh-v3 scenarios, retain source test assertions in current consolidated targets, and record schema/provenance/SQL evidence.

## Ordered Implementation Scenarios

Proof class: mixed. Changed executable seams require actual Behavior/RED/GREEN/REFACTOR; unchanged source behavior may use documented verification-only/no-op proof. Static/generated/configuration work does not require manufactured RED. Original scenario blocks below are retained verbatim, including mapped obligations, expected failures, required extensions and earlier-regression references. Their order is task-local; the shared contract explains cross-task regression references.

### S01-foundation — Source types and executable consumer foundation

**Behavior.** The cohesive source import registers/conforms/reads nested Struct/List/Variant and declared timestamps through existing real server/query owners, while preserving retained main public inputs, narrow types and mechanically affected producers/consumers. This supplies the engine contribution to S01/REQ-001/017/021; TASK-005 still owns the complete original combined Service/UID S01 journey.

**RED.** Use the exact source type/admission/layout selectors in P01 and real server published/recovery selectors in P22 below. Expect a wrong mapped type, declared time, logical Variant value or prohibited row acceptance; compilation and missing target/setup do not count as RED. Already-preserved source assertions receive verification-only proof.

**GREEN.** Import the source owners cohesively and mechanically adapt affected existing main consumers to their contracts, preserving all already-approved exceptions. Prove the source paths without waiting for a later task to fix an incompatible type/terminal contract. Run changed P01/P22 cases and current earlier proof.

**REFACTOR.** Remove superseded duplicate mappers/readers/import paths only under the source precedence and explicit exception rules. No engine redesign or replacement fixture infrastructure.

### S02 — Retained snapshots and source deadlines

**Behavior.** Replaced snapshots remain protected for source retention; read cuts do not acquire old per-reader pins or wait on expiry authority. Source valid deadline projection/default/max/clamp behavior is retained. Mapped obligations: REQ-002 / AC-002.

**RED.** Run P03 and retention/cut selectors in P06. Add cross-entrypoint assertions to an existing query journey in P19/P23 if the integrated adapter changes the requested budget. Expected RED is premature expiration, cut blocking, or main deadline semantics surviving in an adapter.

**GREEN.** Use source retention/read-cut/config/planner semantics unchanged; remove superseded main reader machinery. Keep source budget behavior consistent at client/wire entries. Rerun affected S01 reads.

**REFACTOR.** Delete obsolete reader bookkeeping and tests only after replacing reachable safety proof; do not add a rejection policy or new reader authority.

### S03 — Invalidated or stopped caches cannot return frozen cuts

**Behavior.** A cache returns tenant-qualified snapshot plus hot candidates only while its listener is live and the cut meets the source freshness bound; invalidation races cannot refill stale entries and orderly cancellation falls back to current database reads. Mapped obligations: REQ-003 / AC-003.

**RED.** Run P02. Extend an existing cut integration owner for warm-cache -> cancel/join listener -> changed pointer/hot set -> read if its real lifecycle is not already proved. Expected RED is a stale cut or cross-tenant/cache-generation leak.

**GREEN.** Retain source cache notification/generation/reset behavior and wire its watcher without membership lifetime coupling. Rerun affected S02 cuts.

**REFACTOR.** Use the one catalog cache owner; remove duplicate invalidation/lifetime copies.

### S04 — ACK and recovery retain one exact logical batch

**Behavior.** Group ACK follows durable sync/fence, resent batches land once, acknowledged corruption fails closed, and an unfenced corrupt final tail is handled exactly as source defines. Mapped obligations: REQ-004 / AC-004.

**RED.** Run P04. Extend source crash/fence proof only for the adapted main outbox frame/schema seam. Expected RED is ACK before authority, duplicate rows, discarded acknowledged data, or contradicted recovery evidence.

**GREEN.** Integrate source group/WAL/compression/fence/replay and scoped SQL together; retain identical retries. Rerun affected typed admission from S01.

**REFACTOR.** Keep one durable state transition path; preserve diagnostics and do not force database tests into the credential-free lane.

### S20 — Fresh v3 hot/recovery/rewrite reads stay equivalent

**Behavior.** Fresh typed/Variant data is readable hot, after staging recovery, promotion and repeated v3 rewrite with correct null/nested values and lineage; old physical format/schema is rejected without mutation. Mapped obligations: REQ-001 / AC-001, REQ-004 / AC-004, REQ-025 / AC-025.

**RED.** Run P06/P22 and P23 typed round trips. Extend existing fresh-table/lineage integration owner to present incompatible format/schema and assert refusal plus unchanged objects/metadata. Expected RED is decode/value drift, reupload/duplicate recovery, lineage loss, or silent old-format acceptance.

**GREEN.** Take source storage/native readers/writers/metadata validation and adapt only approved main context fields. Rerun S04/S05/S06/S17 affected stages.

**REFACTOR.** Keep one source read/write representation; no old-file conversion, reset, or dual durable format.

### S22 — Main SQL history advances without control-plane loss

**Behavior.** Fresh replay and main-applied/no-old-Bifrost forward migration yield same target SQL, unchanged historical checksums and retained identities/credentials/Cards. New retention/notify/fence functions obey main tenant/operator policy; obsolete audit/workload/grant tables/files stay retired. Mapped obligations: REQ-024 / AC-024, REQ-025 / AC-025.

**RED.** Run P27/P28. Extend existing migration integration owner for exact pinned-main applied ledger plus retained control-plane records before additive source migration. Expected RED is checksum mismatch, historical resurrection, bad new grants/RLS or changed retained records. No old-object reset is a GREEN option.

**GREEN.** Preserve main bytes and add/adapt only required unique forward DDL; retain source fresh contract. Rerun S02/S04/S12/S19 SQL-dependent behavior.

**REFACTOR.** Use current migrators/connection owners; no checksum bypass, raw pool or data-conversion subsystem.

## Acceptance Criteria

- **TASK-001-C01.** The source engine is cohesive; replaced providers/readers/storage/executor owners remain removed and current owners compile without placeholder or compatibility implementations.
- **TASK-001-C02.** S02/S03/S04 source behavior and S20 fresh-v3/no-mutation behavior have real scoped proof; S22 proves historical bytes, fresh replay and exact-main upgrade preserving Cards/identity/credentials.
- **TASK-001-C03.** All ten source-updated table foundations follow the spec time-axis matrix, with dev/CodeAxis removed; necessary cross-wire producer/consumer changes are atomic.
- **TASK-001-C04.** The graph uses the supplied compaction commit with no REST/Ring waiver and directed lock/Hakari regeneration, preserving unrelated main versions/features.
- **TASK-001-C05.** Every touched source/conflict/overlap and historical L/S item has a disposition; remaining untouched inventory is explicitly assigned, not marked implemented.
- **TASK-001-EVIDENCE.** Every assigned scenario records required assertions and actual exact-command RED/GREEN or justified preserved/no-op results, plus affected regressions; each relevant REQ/AC has source/diff evidence. Update per-path and historical dispositions for this write set and preserve all invariants.
- **TASK-001-REVIEW.** Fresh independent task review evaluates the immutable cumulative task, its necessary consumer closure and source-authority exceptions before accepting the checkpoint. Missing proof and unexplained source divergence are not PASS.

## Expected Write Set and Consumer Closure

vala-bifrost-redux catalog/oracle/scribe/forge/storage/table foundations; wyrd-types, queue and source converters; vala-sql migrations/queries/readiness; spec/tonic/protobuf contract foundations and mechanically affected producers/readers; main server/client imports; Cargo.toml/Cargo.lock/workspace-hack and required shared-storage helpers; owning source tests, artifacts and architecture.

Trace whole workflows through callers, producers/readers, schemas, exports, fixtures, generated outputs, tests, command registration and owning docs. Tests requiring Python/Node lifetimes run in those runtimes. Rehome source assertions by body and critical behavior rather than function-name matching; keep current main consolidated targets. Update documentation and generators with actual contract changes in this task.

## Verification and Evidence

Read the shared recipe/execution contract. Selected historical recipe groups: P01, P02, P03, P04, P06, P22, P27, P28. Run only exact selectors supporting the active scenario plus changed earlier regressions; multi-selector recipes may be narrowed without dropping required assertions. Correct relocated names/features/setup through actual discovery and record non-weaker proof. New scenarios extend listed owners; exact new selectors are chosen/discovered and executed before GREEN, rather than predetermined fictitious names.

Affected Rust/static lanes: `mise run fmt:check`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`, `mise run check:workspace-hack`, `mise run codegen:check`; `git diff --check`. Apply Python format/lint/typecheck and TS format/lint/typecheck/native-declaration checks only when those surfaces change; use owning generators and `codegen:check` for changed contracts. Run docs-site checks when docs-site content changes. Add only a narrow affected owner lane whose property is not proven by exact selectors, recording why. No task-local full language/journey suite or broad aggregate.

Record cross-task regression triggers now; later modifications must rerun affected accepted proof. Task-local acceptance never waives final specification-owned gate/geometry/parity.

### Non-TDD proof

I18 canonical removal, I24 historical SQL bytes and I26 graph/config/source preservation are static/regression-only contributions; source behavior already correct uses no-op proof. Runtime S02/S03/S04/S20/S22 and foundation selectors still prove executable behavior.

### Exact Selected Recipes

#### P01 — Declared schema/time conformance (source lib owners)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::time_column_drives_partition_and_sort)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::layout::tests::invalid_time_column_is_refused_naming_the_field)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::a_naive_timestamp_is_refused_for_a_zoned_column)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::row_group_bounds_read_the_time_column)'
```

#### P02 — Source cache reset/invalidation

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts)'
```

#### P03 — Source deadline projection and retention configuration

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::request_deadline_projects_unusable_budgets_to_the_default)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=config::tests::max_query_deadline_sits_between_default_deadline_and_retention)'
```

#### P04 — Source group/fence/crash recovery

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_syncs_once)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::group_commit_crash_between_sync_and_fence_replays)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::zeroed_block_after_the_newest_fence_ends_the_chain)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_before_an_acknowledged_commit_fails_closed)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_crash_injection::bad_record_without_a_fence_authority_fails_closed)'
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

#### P22 — Source logical Variant reads and recovered publication

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

#### P27 — Historical migration immutability

```bash
mise exec -- cargo nextest run --locked -p vala-sql --lib -E 'test(=tests::forge_and_oracle_migrations_have_unique_forward_owners)'

mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_migration::pg_tests::shipped_audit_staging_migration_is_immutable)'
```

#### P28 — Actual SQL replay/readiness/RLS/main upgrade owners

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::vala_migrations_apply_and_are_idempotent)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent) | test(=pg_migration::pg_tests::single_url_boot_and_separate_url_regression) | test(=pg_migration::pg_tests::tenant_policy_on_another_column_fails_readiness)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration --test-threads=1 -E 'test(=pg_schema_usage::pg_tests::tenant_login_registers_bifrost_table) | test(=pg_schema_usage::pg_tests::fresh_schema_is_tenant_only_and_keys_are_tenant_qualified)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_migration::pg_tests::human_connection_upgrade_preflight) | test(=pg_admin_principals::pg_tests::system_principal_upgrade_backfills_existing_tenants_idempotently) | test(=pg_admin_principals::pg_tests::tenant_principal_queries_are_confined_by_row_level_security)'
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

Candidate: merge `b356c948f` (M `8a8813a7` + S `cfe21dc9`) plus commits
`b38f9be37`, `597533616`, `cac9c4969`, `4f9a5c1d1`, `80b29eb2a`, `eaa129c4a`,
`5e7b177b0`, `1678da1ac` on `wyrd/bifrost-variant/integrate-main`. Status:
**IMPLEMENTED**, pending `$wyrd-task-review`.

### Acceptance

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001-C01 cohesive engine | S catalog/oracle/scribe/forge/storage owners imported; S-deleted owners stay deleted (`analytical_scan.rs`, `pruning.rs`, `fixed_ipc.rs`, `storage/cache.rs`, `storage/telemetry.rs`, `wyrd-queue/src/schema.rs`); M consumers adapted in `597533616`/`cac9c4969` (results via `ScribeOutbox::stage`, audit via staged `AuditEvent`, eval observations via admin write) | `cargo check --workspace --all-features --all-targets` clean; `mise run lints` (incl. release `wyrd-server` bin) | PASS |
| TASK-001-C02 S02/S03/S04/S20/S22 | S retention/deadline (S02), cut cache (S03), group commit/fences (S04), v3 rewrite (S20); REQ-022: cut read inside `TenantConn` (`oracle_reader_authority.rs`, `bifrost_catalog.rs::read_cut`), WAL fence via `OperatorPool` (`scribe_batch_commits::newest_wal_fence`, `ReplayFences`); REQ-024: `olap_minimal.sql` restored to M bytes, three S forward migrations without GRANT/REVOKE, `operator_access` policy, plpgsql SECURITY DEFINER `scribe_wal_newest_fence` | P02, P03, P04, P06, P22, P27, P28 below | PASS |
| TASK-001-C03 ten tables / time axes | `builtin_tables() -> [_; 10]`; no `vala.dev.agent_traces`/`CodeAxis` in crates, sdks, docs, architecture (grep empty); consumers moved to `created_at`/`ended_at`/`started_at`/`wyrd_ingested_at` (`4f9a5c1d1`: capacity load, `observe_ingest`, security posture, schema.svx) | P01; `observe_ingest::observations_store_the_client_emit_time` adapted | PASS |
| TASK-001-C04 graph | `iceberg-compaction-core` rev `22cd178d14009468bc951a14d7acf63237af808a`; lock/Hakari regenerated; `ring` only via M's `jsonwebtoken` (present in M lock) | `mise run check:workspace-hack` ("no operations to perform"), `mise run check:deps` | PASS |
| TASK-001-C05 dispositions | [dispositions.tsv](../dispositions.tsv): every one of 527 B→S records with class, disposition, owner task, note | Generated from `git diff --name-status -M B S`, `git merge-tree M S` and HEAD blobs | PASS |
| TASK-001-EVIDENCE | This section | Commands below | PASS |

### Dispositions ledger

[dispositions.tsv](../dispositions.tsv) is the canonical per-path ledger
(later tasks update rows they touch). Counts at this candidate:

- 527 B→S records (531 paths incl. rename sources). Overlap with B→M: 228
  paths = 138 conflicting + 85 auto-merged + 5 byte-identical.
  `git merge-tree --name-only M S` reports 148 conflicted paths; 10 sit at
  main-side rename destinations outside the B→S path set. The plan's figure of
  76 auto-merged overlaps came from an earlier count. This ledger counts every
  overlapping path that is neither conflicted nor identical (85) and gives each
  one a row.
- Dispositions: preserved 277, adapted 184, retired 51, main-retained 8,
  source deletion kept 7.
- "Retired" includes rehomed tests (wyrd-client `tests/bifrost/*`,
  vala-sql `tests/pg_*`, wyrd-server `tests/pg_*`) whose bodies now live in the
  consolidated integration binaries (S3).
- Assigned, not implemented here: audit (TASK-003: `audit_staging.rs`,
  `gate_audit.rs`, `audit/mod.rs`), results publisher and drift SDK journeys
  (TASK-004), Python/TS/Rust SDK source assertions in stories main deleted or
  rewrote, incl. `test_bifrost_e2e.py` Variant attributes and TS `otel-export`
  (TASK-005), `bifrost_query_capacity` logs/telemetry and other capacity
  workloads (TASK-006).

### Historical L01–L64 and S1–S4 reconciliation

Checked against the integrated tree. Every entry follows evidence.md's recorded
status. Items with a concrete current check:

- L02/L62: compaction pin is now the task-supplied `22cd178d` (supersedes
  6965606); ring absent from the compaction/iceberg cone.
- L03/L23/S3: vala-sql has one `tests/integration` binary (12 modules); source
  `pg_oracle_membership` content rehomed there.
- L07/S2: `scripts/checks/object-store-pin.sh` and `scripts/test-families.sh`
  absent and unreferenced.
- L08/S4: `wyrd-tonic/proto` holds only `wyrd.v1.proto`; capacity binaries are
  `capacity` and `bifrost_forge_capacity`.
- L10/L16: no `CodeAxis`/`agent_traces` anywhere.
- L12/L57/L51/L01: no audit publisher, staging or hash chain; `audit/` is
  `mod.rs` only.
- L20/L38/L56/S1: workspace Clippy passes under the pinned stable toolchain
  (1.98) with no added `allow`/`expect`; the source `type_complexity` allow and
  one unfulfilled `expect` were removed (`5e7b177b0`).
- L22/L60: M's reader-authority and `UnresolvedExpiry` cut refusal are retired
  by the approved REQ-002 (S retention window; spec: "Do not port main's
  barriers/refusals for new cuts"). The prepared-expiry lease authority
  safeguard is proven by P06 `retryable_catalog_failure_retains_prepared_authority`;
  Forge barrier work is TASK-002.
- L26: no `planning_demands` consumer; only historical migrations reference it.
- L34: `params: Vec::new()` carried on rehomed `BifrostQueryRequest` literals.
- L43: no Python `wyrd.verification` module or TS `Verification` client.
- L46: TS `index.d.ts`/`.d.cts` regenerated via `ts:build` (`80b29eb2a`);
  `codegen:check` clean.
- L58/REQ-022: main role model kept. The P22 payload-refusal case uses a seeded
  role, because every main builtin role holds `gateway_payload_read`
  (`1678da1ac`).

### Scenario proof (RED/GREEN)

S behavior was imported intact, so the selected recipes give verification-only
proof. RED observed during this task:

- **REQ-022 (static RED):** `mise run check:tenant-isolation` failed on
  `OracleTableCuts::read` (bare connection) and `newest_wal_fence` (bare
  connection, no marker). GREEN after `eaa129c4a`.
- **P22 RED:** `published::variant_sql_registry_covers_every_session` failed
  with "the sensitive Variant read settled (Interactive, [])".
  - Diagnosis:
    - Symptom: panic at `published.rs:850`.
    - Evidence: no "Oracle refused a query reaching gateway payload columns"
      line in the trace.
    - Cause: `builtin_roles.rs:44` grants `gateway_payload_read` to
      `workload` (`DEFAULT_CARD_ROLE`), so `authorize_payload_columns`
      (`oracle/mod.rs:3479`) returns `Ok`.
    - Fix site: the test caller, not the server.
  - An independent read-only diagnostician confirmed the cause and fix site and
    found no other affected callers.
  - First fix run: a role name with hyphens was refused with
    `WYRD_PERMISSION_500_ROLE_CORRUPT` (names must match `^[a-z0-9_]{1,64}$`);
    renamed.
  - GREEN, rerun with `--no-capture`: the trace shows the payload refusal.

| Recipe | Exact selectors | Result |
|---|---|---|
| P01 | `catalog::layout::tests::{time_column_drives_partition_and_sort, invalid_time_column_is_refused_naming_the_field}`, `scribe::execution_lanes::tests::{same_iceberg_type_spellings_cast_to_the_registered_type, a_naive_timestamp_is_refused_for_a_zoned_column}`, `scribe::parquet_writer::tests::row_group_bounds_read_the_time_column` | 5/5 PASS |
| P02 | `catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts` | PASS |
| P03 | `oracle::tests::request_deadline_projects_unusable_budgets_to_the_default`; wyrd-server `config::tests::max_query_deadline_sits_between_default_deadline_and_retention` | 2/2 PASS |
| P04 | `scribe::shards::tests::{group_commit_syncs_once, same_batch_resent_before_commit_lands_in_a_later_group_and_commits_once, group_commit_crash_between_sync_and_fence_replays}`, `scribe::pg_scribe_crash_injection::{zeroed_block_after_the_newest_fence_ends_the_chain, bad_record_before_an_acknowledged_commit_fails_closed, bad_record_without_a_fence_authority_fails_closed}` | 6/6 PASS |
| P06 | 3 lib (`forge::leadership` ×2, `forge::worker`) + 7 journey (`production_routes` ×2, `snapshot_expiration::retryable_catalog_failure_retains_prepared_authority`, `reader_expiry_ordering` ×2, `managed_rewrite` ×2) + vala-sql `pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt` | 11/11 PASS |
| P22 | `published::{variant_sql_registry_covers_every_session, struct_and_variant_share_physical_pushdown}`, `distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null`, `recovery::variant_staging_restores_and_publishes_once`, `scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds` | 5/5 PASS (first selector RED → GREEN) |
| P27 | `tests::forge_and_oracle_migrations_have_unique_forward_owners`, `pg_migration::pg_tests::shipped_audit_staging_migration_is_immutable` | 2/2 PASS |
| P28 | vala-sql `vala_migrations_apply_and_are_idempotent`; wyrd-sql 3-selector set; vala-sql `pg_schema_usage` 2-selector set; wyrd-sql upgrade/principal 3-selector set | 9/9 PASS |

Run exactly as written in "Exact Selected Recipes" with
`WYRD_LOG` and `RUST_LOG` set to `info,vala_bifrost_redux=debug` and `RUST_BACKTRACE=1`. Additional
regression run for the changed cut owner:
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(/^pg_oracle_membership::/)'`
6/6 PASS. Its statement-local binding and rebind-refusal sub-assertions were
removed because `TenantConn` now carries the tenant in its type.

### Static lanes

`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run check:workspace-hack`,
`mise run codegen:check`, `git diff --check`: all PASS on the final candidate.
TS lanes (`ts:lints`, `ts:format:check`, `ts:typecheck`, `ts:napi:check`) passed
with `80b29eb2a`.

### Limits and non-goals

- Unported source assertion: `newest_wal_fence_spans_tenants_on_one_stream`
  (deleted with main's `pg_audit_staging.rs`). It covered the SQL fence lookup
  across tenants. The adapted SECURITY DEFINER function is applied by P28; P04
  covers replay semantics with injected fences. The cross-tenant SQL lookup
  itself has no executable proof in this task's selected recipes.
- Not done here (owned by TASK-002 through TASK-006): Forge cut barriers and
  Oracle memory, audit, observations/results, public journeys, workload ports.
  None were implemented in this task.
- No deployment, push, merge to main, data migration, compatibility shim,
  suppression attribute, hand-edited generated artifact, or restored retired
  script/descriptor/publisher.
