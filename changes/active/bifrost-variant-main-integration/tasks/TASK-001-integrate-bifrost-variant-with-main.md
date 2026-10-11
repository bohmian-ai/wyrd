---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 1
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-024, AC-025, AC-026, AC-027]
---

# Integrate the pinned Bifrost variant with main

## Outcome and Value

Deliver one consumer-complete integrated candidate on
`wyrd/bifrost-variant/integrate-main`: source Bifrost architecture and behavior,
main surrounding product/security/verification/client behavior, and every
explicit compatibility repair in approved spec revision 1. This single task
owns all 27 REQ/AC obligations and all 12 invariants. There are no upstream task
dependencies and no partial delivery that excludes consumers or proof.

The [spec](../spec.md) is the decision authority. The [evidence](../evidence.md)
contains pinned source facts and the historical/path ledger. Earlier research
options superseded by the spec are not implementation instructions. In
particular: keep source retention and deadline clamp, remove dev.agent_traces,
do not resurrect reader pins or cut barriers, and keep UID-only attribution.

## Owners, Scope, Consumers, and Prohibited Changes

M means pinned main; S means pinned variant. Start from M
`8a8813a7458b7c98707c7b4f9b9d403f604ee0e6` and integrate S
`cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. Preserve branch provenance; inspect
three-way differences rather than treating endpoint differences as deletions.
Do not advance the input pins silently.

Vala owns the analytical engine and its SQL; Wyrd owns durable app/auth/security
and boot/lifecycle; wyrd-spec owns wire shapes; wyrd-client is the sole shared
SDK facade; language SDKs own only their runtime boundary. Existing concrete
owners remain discoverable and cohesive under AGENTS.md. Paths/symbols below
are inspected guidance, not a private implementation or file allowlist.

No deployment, push, main merge, destructive reset, old Bifrost data migration,
compatibility shim, new peer services, public broad internal client exports,
separate audit/capture/result publisher, test-only dev table, raw SQL connection
API, unrelated storage/signing/review-skill churn, or gate weakening is allowed.
Retain main's removed scripts/descriptor and all unrelated main-only behavior.

### Source-backed reuse and all-27 obligation map

Every row maps REQ-n and AC-n to an existing workflow, selected extension, and
proof. P identifiers resolve to exact commands below. I identifiers resolve to
explicit non-TDD proof. Test files mentioned as consumers are not claims that
an entire file or suite already proves the new seam.

| Obligation / capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification | Proof |
|---|---|---|---|---|---|---|
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
| REQ-016 / AC-016 — Result evidence | M ResultPayloadBuilder::build/summary; ResultRun; S typed result schemas | P15/P17; direct Task/continuous Eval/Drift results | S typed Drift/Eval summaries omit M Task and app Run meaning | S typed structs + existing Task details as Variant | Existing AssertionResult; no new Task type/table | S15 |
| REQ-017 / AC-017 — Declared time axes | S TimeAxis::of_wire; canonical tables; event_time bound conversion; M BifrostReader | P01/P15/P17/P23; all ten table producers/readers | M event-time predicates/types and same-day joins obsolete | Use spec matrix, source precision/conformance; result_id joins | Existing table owners/type mapper; no synthetic time alias | S01/S13/S15/S16 |
| REQ-018 / AC-018 — Remove dev table | M canonical built-in inventory/provisioning; S dev/CodeAxis sole owner | Static I18; source dev tests -> supported/dynamic property proof | S approved dev inclusion conflicts with latest user override | Keep ten, no dev/CodeAxis; transfer still-reachable assertions | No test-only inventory or extra name-ban check | I18/S16/S20 |
| REQ-019 / AC-019 — Typed SQL parameters | M Bifrost::sql/sql_as/stream; QueryParam; S OraclePlanner | P18/P19/P23; HTTP/grpc/SDK query bodies | S unchanged-base signatures omit newer params | Bind M typed ordered values in S plan and keep public argument order | Existing planner/transport; no interpolation/second client | S17 |
| REQ-020 / AC-020 — Terminal problems | S WyrdProblem; tonic query_conversion; M from_problem/error catalog | P19/P22/P23; grpc/http/client/CLI/MCP collectors | Wire terminal changes must agree and preserve M other errors | Atomic S problem field with M params/security | Existing typed catalog/conversions; no compatibility parser | S18 |
| REQ-021 / AC-021 — Shared types/client boundaries | S wyrd-types schema mapper/Variant/timestamps; M Bifrost handle and Service declared_tables | P01/P19/P23; cards/service.rs preflight and SDK exports | Queue mapper removed; broad source exports regress M internals | Use S shared mapper and M narrow facade/preflight | Source shared type owner replaces duplicate mapper, no new transport | S01/S16/S19 |
| REQ-022 / AC-022 — Identity/bootstrap | M ServerAuth/ExchangeApiKey/AuthContext::from_verified; DevServer; ResolvedDsns | P14/P24/P25/P26; issued credentials, OTLP, saved local config | S Gate/state/boot lack newer entrance/wiring | Retain M authorities/verified entrance/DSNs/endpoints | Existing auth/runtime/SQL owners; no raw pools or new credential model | S12/S19/S21 |
| REQ-023 / AC-023 — Product lifecycle | M WorkflowRuns::drain; RunTools; AppState; BoundServer | P09/P20/P21/P24; workflow/MCP/gateway/Operator routers | Whole-source state selection can disconnect M-only owners | Retain connected M owners and ordered drain using S Bifrost | Existing trackers/owners; no general orchestrator | S19/S21 |
| REQ-024 / AC-024 — Migration history | M MIGRATOR/verify_migrations; S three forward migrations | P27/P28; serving/operator readiness and role RLS | Historical checksum/source old grants cannot be transplanted | M bytes + S forward DDL with current tenancy | Existing migrators; no reset or checksum bypass | S22/I24 |
| REQ-025 / AC-025 — Fresh v3 contract | S create_physical_table/physical_table_matches; spec no data migration | P06/P28; dynamic/fresh built-ins and format refusal extension | Old tables/objects are not schema-compatible | Fresh isolated Bifrost; reject old schema/version without mutation | No conversion/dual reader; old-state deployment excluded | S20/S22 |
| REQ-026 / AC-026 — Native graph/tooling | S native Cargo pins/ObjectStore factory helpers; M deps.sh/Hakari/profiles | Static I26; scoped dependency/Clippy/codegen checks | Locks/lints/test scripts/signing drift between snapshots | Resolve combined graph; retain M deleted tooling and artifact storage | Only S required analytical deps/helpers; no unrelated signer rewrite | I26 |
| REQ-027 / AC-027 — Proof/artifact closure | M consolidated targets/fixture stories/CI selection; source Variant Arrow fixtures | P01-P28; static I27; served OpenAPI/runtime MCP | Source tests/stubs/lanes may disappear or omit new cases | Rehome coverage, regenerate source-owned outputs, docs agree | Reuse M harness; no extra checker/duplicate TS server | All scenarios/I27 |

## Approach

1. Verify pins and clean worktree; record all source-path and conflict dispositions.
   Perform the pinned three-way integration on this branch. Resolve manifests,
   wire/type contracts, migration ownership, and mechanically broken imports
   only far enough to compile focused proof; compilation/setup failures are not
   behavioral RED evidence.
2. Import source engine/dependency groups coherently. Resolve the declared time,
   retention/clamp, Variant, WAL and recovery semantics against the approved spec.
3. Adapt compatible main Forge, memory, flush, membership and peer-routing
   safeguards inside source owners, one ordered scenario at a time.
4. Reconnect main shared outbox, credential/UID entrances, observation activation,
   verification/gateway/result lineage, product owners and ordered shutdown.
5. Close all public consumers and language-runtime tests using main fixtures;
   rehome source proof, regenerate source-owned artifacts, and reconcile docs.
6. Run the focused proofs and applicable owner/boundary/static checks below;
   audit complete source and main preservation ledgers and record AC evidence.
   Handoff the immutable candidate for task/change review. Whole-candidate
   aggregates belong to the spec's change-review closeout, not this task recipe.

## Ordered Implementation Scenarios

Run these in order. Each scenario is one Behavior / RED / GREEN / REFACTOR
cycle, even where several exact selectors provide supporting seam evidence.
Run only its focused selectors, not all groups for every edit. If a preserved
source behavior already passes, record verification-only/no-op proof; do not
break production to manufacture RED. For a changed executable seam, extend a
listed existing owner test with the described assertion and run it before the
fix. A compiler/setup/zero-selection failure does not count as RED. Missing
private test support is implementation-owned and does not require a new spec.

If a new test is necessary, choose its actual name in the existing owner, verify
its package/target/feature/module with nextest discovery, and record/run its
exact focused command before GREEN. No invented future test/module names are
mandated here. This preserves implementation freedom while requiring exact
execution evidence. Any relocated listed test requires an equivalent exact
selector correction, recorded with the actual new symbol, never zero selection.

### S01 — Fresh registration and lossless typed rows

**Behavior.** A fresh user/model/Service table describes the registered schema and time axis, rejects undeclared/type-invalid rows before admission, and returns nested Struct/List/Variant and explicit timestamp values through the source contract. Mapped obligations: REQ-001 / AC-001, REQ-017 / AC-017, REQ-021 / AC-021.

**RED.** Use P01 and the source Python/TS file recipes P23; extend the registration/round-trip assertions at the combined Service and UID boundary. Expected RED is mismatched schema/time/Variant decoding or acceptance of a forbidden row, not an absent test module.

**GREEN.** Import source type mapping, preflight, declared layout, and projection in current owners; adapt main Service table preflight to the new mapper. Rerun the specific registering/writing/reading cases that changed.

**REFACTOR.** Remove duplicate mapper/conversion paths; preserve per-column types, strict validation and narrow client exports.

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

### S10 — One outbox delivers bounded identical private writes

**Behavior.** Audit/capture/results share one process writer, bounded destination frames and stable retry IDs. Terminal refusal consumes/counts its slice and later work progresses; private destinations/Verifier/system-owner rules and all public result-write refusals remain enforced. Mapped obligations: REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P11 and private validator in P13. Extend existing private/public journey owner to observe ACK/readback of all destination categories under source schema. Expected RED is lost category, changed retry frame, forbidden public write, or permissive internal attribution.

**GREEN.** Use main shared outbox and six-destination mTLS ingress with source frame/projection. Rerun S04/S09 exact retry and routing cases.

**REFACTOR.** Delete old independent publishers/staging owners; keep existing sink and permission surfaces.

### S11 — Audit records real credentials and never changes authorization

**Behavior.** Allowed/denied keyed Gate and Oracle requests retain credential or null exactly, decision-created_at and logical Variant detail, with no sequence/hash columns; delivery failure never changes the request decision. Trusted system rejection records once. Mapped obligations: REQ-011 / AC-011, REQ-010 / AC-010, REQ-012 / AC-012.

**RED.** Run P12/P13. Extend existing retention journey to originate actual credential-bearing ingress/query decisions rather than only stage fabricated events, and extend production blanket adapter proof. Expected RED includes current-main omitted credential_id, text/time mismatch, restored chain, or audit IO affecting authorization.

**GREEN.** Keep main event/stage/outbox authority, source explicit time/detail representation, and carry credential through actual builders. Rerun S08/S10 retention and private destination proof.

**REFACTOR.** Remove old hash/staging helpers and AuditUnavailable cases; do not change canonical CardRef content or batch dedup hashing.

### S12 — UID and API-key scope are preserved at native and OTLP entrances

**Behavior.** Bound signed scope and unbound tenant registry admit valid optional subject UIDs; missing correlation stays null; forbidden/malformed/wrong-kind/foreign UID rejects the entire frame. Stock API-key OTLP authenticates before decode and works across token TTL without becoming a global API-key route. Mapped obligations: REQ-013 / AC-013, REQ-022 / AC-022.

**RED.** Run P14 and native scope selectors in P12; extend the API-key journey to trace/log/metric entrances and revoked/unknown key refusals where those branches changed. Expected RED is CardRef-only behavior, unbound denial/allow-all, attribution drift, or missing verified Gate entrance.

**GREEN.** Port main authorization/registry/verified entrance into source admit/conform/ACK and SDK/OTLP input. Rerun S01/S10/S11 for scope and retained attribution.

**REFACTOR.** Keep authentication and registry IO at the trusted boundary; no lookup in CPU decode or global CardRef rename.

### S13 — ACK-derived Eval activation survives outage and next-day replay

**Behavior.** Observation ACK remains independent of enqueue availability, queue retries retain work, matching writer bindings activate once, and later-day replay uses original created_at to read the same subject record. Mapped obligations: REQ-014 / AC-014, REQ-017 / AC-017.

**RED.** Run P15/P16 and ACK decoder in P12. Extend existing replay/outage assertions with source UID/Variant created_at and different receipt day. Expected RED is all-subject activation, lost enqueue, duplicate run, or record lookup under the receipt day.

**GREEN.** Retain main outbox/bulk writer-aware queue and replay fence; adapt admitted ACK fields/time. Rerun S04/S12 with changed frame identities.

**REFACTOR.** No best-effort semaphore task replacement or per-binding SQL loop; preserve lock order and batching.

### S14 — Verification keeps leases, schedule clock and gateway authority

**Behavior.** The database owns claim/renew/schedule timing; expired/stale leases cannot revive and cancel their work; hydrated/cache baselines remain correct. Judge calls pass through gateway under the right authority and denial sends no provider request. Mapped obligations: REQ-015 / AC-015.

**RED.** Run P15/P16. Extend existing terminal/direct-judge matrix with explicit denied/allowed gateway assertions and retained writer/Run fields. Expected RED is direct provider bypass, missing renewal/cancel, missed delayed occurrence, or wrong attribution.

**GREEN.** Keep main queue/runtime/authority/cache/leases and gateway invocation; port source Variant/timestamp read decoding only. Rerun S13 and S11 decision/lineage proof.

**REFACTOR.** Keep existing verification owners; no duplicate runner, minted SYSTEM reader token or repeated per-run baseline lookup.

### S15 — Every implementation retains its result and application lineage

**Behavior.** Drift/Eval use typed source summaries, Task uses existing AssertionResult as Variant details, and exactly the matching implementation payload is populated. Application run_id stays distinct from verifier execution identity; detail joins survive a different ingestion day. Mapped obligations: REQ-016 / AC-016, REQ-017 / AC-017.

**RED.** Run P17 and result/Task selectors in P15. Extend existing result-layout journey for ended_at versus detail ingestion midnight. Expected RED is lost Task payload/fields, text decode, duplicated summaries, verifier UUID as app Run, or pruned detail rows.

**GREEN.** Compose main result/outbox and source schema/projectors; use result_id joins and source nullable semantics. Rerun S11/S14 under combined schemas.

**REFACTOR.** Use existing details and result contracts; no new Task struct/table or synthetic shared event time.

### S16 — Ten built-ins and protected payloads use declared schemas

**Behavior.** All ten built-ins are eagerly available with source time/types; dev table/CodeAxis are absent. Gateway Variant payload projection/predicate/subquery access still requires payload-read authority; metadata remains ordinarily readable. Mapped obligations: REQ-017 / AC-017, REQ-018 / AC-018, REQ-021 / AC-021.

**RED.** Extend current query journey P19 with all-ten description/empty reads and removed-table absence, then run P18 for renamed payload protections. Expected RED is an old schema, revived dev table, missing eager table, or payload authority bypass. Static I18 complements rather than replaces real reads.

**GREEN.** Update canonical table registrations, all consumers/time predicates and protected payload names. Transfer generic removed-table Variant assertions to supported/dynamic owners. Rerun S01/S12/S15.

**REFACTOR.** Remove sole-owner dev/CodeAxis definitions/docs/tests; do not add name-ban machinery or a test-only inventory.

### S17 — Bound SQL stays data across first-class clients

**Behavior.** Ordered parameters bind in source plan and forward unchanged through HTTP/gRPC/Rust/Python/TS, preserving argument positions and separating hostile text from SQL. Source deadline behavior is unchanged. Mapped obligations: REQ-019 / AC-019.

**RED.** Run P19/P23 and shared-client bind proof P18. Expected RED is dropped/reordered values, interpolation, wrong TS rows/params position, or changed result projection.

**GREEN.** Carry main request/facade/native parameter projection into source planning and update imported callers. Rerun S01/S02/S16 typed reads.

**REFACTOR.** Reuse shared transport/planner and existing typed QueryParam conversion; no alternate query client or parameter parser.

### S18 — Failure after rows remains a typed failed terminal

**Behavior.** Server/grpc/forwarder/client/SDK/CLI/MCP reconstruct the source WyrdProblem exactly; prior rows do not become complete success, old protobuf field remains reserved, and main newer non-Bifrost errors remain available. Mapped obligations: REQ-020 / AC-020.

**RED.** Extend existing streaming/error owners in P19/P22/P23 with an induced failure after an initial batch and assert typed problem/terminal behavior. Expected RED is missing problem fields, legacy terminal decoding, partial success, or widened worker context. Additional focused CLI/MCP runtime tests are discovered in their existing owners before their exact command is recorded.

**GREEN.** Integrate terminal contract and consumers atomically with main params/security. Rerun S09/S17 frame and request proof.

**REFACTOR.** Use derive-backed catalog reconstruction; no manual parallel code map or old-terminal compatibility shim.

### S19 — Main app owners and Service registration stay connected

**Behavior.** Service declared tables validate/ensure through source mapper before Card transaction, main Workflow/MCP/gateway/Operator/Trigger/auth owners retain mounts/config/authority, and shared outbox is boot-composed once. Mapped obligations: REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Extend existing Service registration and routed-owner journey owners after inspecting their concrete selectors; run changed-boot P24 and Workflow P20/P21. Expected RED is table preflight divergence, missing route/owner, duplicate outbox, or wrong credential/tenant composition. Record exact newly selected owner commands before GREEN.

**GREEN.** Adapt main state/boot and Service mapper use around source role/resource/storage construction. Rerun S10/S12/S14/S16 after constructors change.

**REFACTOR.** Retain existing app services/handles; no new umbrella owner, graph layer or generic bootstrap wrapper.

### S20 — Fresh v3 hot/recovery/rewrite reads stay equivalent

**Behavior.** Fresh typed/Variant data is readable hot, after staging recovery, promotion and repeated v3 rewrite with correct null/nested values and lineage; old physical format/schema is rejected without mutation. Mapped obligations: REQ-001 / AC-001, REQ-004 / AC-004, REQ-025 / AC-025.

**RED.** Run P06/P22 and P23 typed round trips. Extend existing fresh-table/lineage integration owner to present incompatible format/schema and assert refusal plus unchanged objects/metadata. Expected RED is decode/value drift, reupload/duplicate recovery, lineage loss, or silent old-format acceptance.

**GREEN.** Take source storage/native readers/writers/metadata validation and adapt only approved main context fields. Rerun S04/S05/S06/S17 affected stages.

**REFACTOR.** Keep one source read/write representation; no old-file conversion, reset, or dual durable format.

### S21 — Shutdown and installed local bootstrap preserve real callers

**Behavior.** Accepted Workflow tools settle before terminal run state; admission/reservations and abandoned blocking work remain tracked. Installed CLI provisions empty state, saves selected endpoints/credentials, and subsequent Rust/Python/TS/MCP clients work without developer credential leakage. Mapped obligations: REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run P20/P21/P25/P26. Expected RED is a detached query, forgotten preparation, removed app owner, stale saved gRPC endpoint, altered credential state, or inherited local config winning. Bootstrap starts unmigrated; do not pre-migrate that fixture.

**GREEN.** Retain main product drain/config/endpoint persistence around source Bifrost lifetimes. Rerun S07-S11 where teardown/routing changes.

**REFACTOR.** Use existing trackers, installed harness and isolated SDK fixture; no duplicate test server or new config surface.

### S22 — Main SQL history advances without control-plane loss

**Behavior.** Fresh replay and main-applied/no-old-Bifrost forward migration yield same target SQL, unchanged historical checksums and retained identities/credentials/Cards. New retention/notify/fence functions obey main tenant/operator policy; obsolete audit/workload/grant tables/files stay retired. Mapped obligations: REQ-024 / AC-024, REQ-025 / AC-025.

**RED.** Run P27/P28. Extend existing migration integration owner for exact pinned-main applied ledger plus retained control-plane records before additive source migration. Expected RED is checksum mismatch, historical resurrection, bad new grants/RLS or changed retained records. No old-object reset is a GREEN option.

**GREEN.** Preserve main bytes and add/adapt only required unique forward DDL; retain source fresh contract. Rerun S02/S04/S12/S19 SQL-dependent behavior.

**REFACTOR.** Use current migrators/connection owners; no checksum bypass, raw pool or data-conversion subsystem.

## Acceptance Criteria

Every AC-001 through AC-027 has the owner and proof in the all-27 reuse table.
Acceptance requires the full behavior of its matching spec REQ, including
listed refusal/edge cases. Record AC status individually, not one blanket PASS.
The scenario matrix is mandatory; I18/I24/I26/I27 static proof is described
below. Preserve all INV-001 through INV-012 in every scenario.

The task evidence also proves all source changed records/direct conflicts and
automatic overlaps have a disposition, all required main-only owners remain
connected, no prohibited resurrection occurred, and final contract/docs agree.
Source tests already passing can provide regression/no-op evidence; missing
cross-boundary assertions cannot be waived because supporting units pass.

## Expected Write Set and Consumer Closure

Likely owners include vala-bifrost-redux catalog/gate/oracle/scribe/forge/storage
and tables; vala-sql migrations/queries/readiness; wyrd-spec wire/error/schema;
wyrd-types/queue/runtime/client; wyrd-server state/boot/config/grpc/routes and
verification/gateway/workflow/card integration; tonic/proto conversions; all
three SDKs/bindings/public exports; CLI/MCP consumers; testing/fixture owners;
Cargo manifests/lock/Hakari; mise/CI selection; generated artifacts and owning
architecture/docs/examples. This is guidance, not an allowlist or permission
to rewrite unrelated modules. Keep main auth/role/storage/signing/UI/review
owners unless an approved seam actually requires adaptation.

Consumer closure means callers, schemas, producers/readers, tests, exports,
fixtures, generated output, current commands, and documentation are all updated.
A deleted old target is not restored to make a source test visible. Add imports
from owning re-exports, map schema fields by name, and retain runtime ownership.

## Verification and Evidence

### Exact proof recipe contract

P01-P28 below are source-inspected existing package/target/selector recipes,
not claims of execution or permanently fixed private test structure. A recipe
with imported source tests becomes executable after integration. Before running
it, confirm every exact selected symbol exists on the combined target; after
relocation use its actual discovered name and record the equivalent correction.
Do not count zero tests as success. Tests whose names express superseded reader
barriers must be adapted to source semantics rather than preserve those barriers.

All commands run from the integration repository root through mise. Postgres
wrapper owns isolated local DB setup and supplies serving/admin DSNs; never
substitute production credentials. PgFixture-based SQL/engine tests create and
migrate their own databases. Real server/SDK recipes use their inspected setup
migration task. Installed-bootstrap recipes intentionally do not pre-migrate.
Use default/minimal declared features, not all-features. Cargo-backed work is
sequential. On failure rerun with WYRD_LOG=info,vala_bifrost_redux=debug, verify
the subscriber, and read traces before diagnosing.

New runtime tests use their owning Python/Node process, never a Rust-emulated
foreign runtime. Source Rust client tests rehomed into main integration retain
test-support and get their actual module selectors after discovery; no invented
future selector is supplied. P23 defines bounded existing language files.

### Non-TDD and regression-only obligations

- **I18 (AC-018):** inspect canonical inventory/types/docs and removed-table
  callsites; no dev/CodeAxis owner remains. Runtime S16 covers supported table
  behavior. No manufactured RED for deleting unreachable sole-owner machinery.
- **I24 (AC-024):** compare historical bytes and migration versions to pinned
  main; inspect only genuine source migration additions. P27/P28 cover executable
  SQL transitions. Historical checklist corrections are static evidence.
- **I26 (AC-026):** dependency/feature/profile graph, lock/Hakari regeneration,
  script/descriptor deletions, lint policy and shared-storage consumer audit.
  Do not manufacture RED for configuration regeneration or retained simplification.
- **I27 (AC-027):** test registration/CI selection/source-generation/doc and full
  changed-path disposition audit. Source-owned generated artifacts use their
  generator; no hand-edited stubs. Runtime scenarios separately prove behavior.

For the affected write set, run applicable focused owner/static checks:
`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run check:workspace-hack`,
`mise run codegen:check`; Python `py:format:check`, `py:lints`, `py:typecheck`;
TypeScript `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check`;
`mise run docs:check` for actual docs-site changes; `git diff --check`.
Use `mise run codegen:regen` and the current Hakari commands to regenerate before
checking. Add only an affected owner lane whose required property is not proved
by its exact selectors; record why. Do not add full language/user-journey suites
or broad repository aggregates to this task. Those run once under spec closeout.

Record expected RED, actual GREEN/no-op, scoped regressions, features/setup,
exact final selectors and source/diff evidence per AC. Lack of runtime proof in
research is not a passing implementation result.

### Focused command catalog
