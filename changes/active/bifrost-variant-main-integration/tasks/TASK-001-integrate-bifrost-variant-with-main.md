---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 2
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
explicit compatibility repair in approved spec revision 2. This single task
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

The existing remote branch and its research/benchmark handoff are preserved;
do not recreate/reset it. The approved revision supersedes conflicting research
options. The compaction fork prerequisite is supplied: use published commit
`22cd178d14009468bc951a14d7acf63237af808a` on the fork’s `wyrd/main`,
already pinned by remote integration commit `914495674`. Preserve this pin when
importing source manifests; resolve the combined lock/Hakari graph and verify
compatibility and the no-REST/Ring boundary without a waiver.

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

**Behavior.** Drift/Eval use typed source summaries, Task uses existing AssertionResult in nullable assertion_result Variant, and exactly the matching implementation payload is populated. Application run_id stays distinct from verifier execution identity; detail joins survive a different ingestion day. Mapped obligations: REQ-016 / AC-016, REQ-017 / AC-017.

**RED.** Run P17 and result/Task selectors in P15. Extend existing result-layout journey for ended_at versus detail ingestion midnight. Expected RED is lost Task payload/fields, text decode, duplicated summaries, verifier UUID as app Run, or pruned detail rows.

**GREEN.** Compose main result/outbox and source schema/projectors; use result_id joins and source nullable semantics. Rerun S11/S14 under combined schemas.

**REFACTOR.** Use existing AssertionResult and result contracts; no new Task struct/table or synthetic shared event time.

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

**RED.** Extend existing streaming/error owners in P19/P22/P23 with an induced failure after an initial batch and assert typed problem/terminal behavior. Expected RED is missing problem fields, legacy terminal decoding, partial success, or widened worker context. Run P29 for actual MCP/CLI and HTTP framing; extend its compiled CLI/MCP journeys for late failure because the framing unit alone is insufficient.

**GREEN.** Integrate terminal contract and consumers atomically with main params/security; Python error_code is the catalog code and error_detail is the problem detail. Rerun S09/S17 frame and request proof.

**REFACTOR.** Use derive-backed catalog reconstruction; no manual parallel code map or old-terminal compatibility shim.

### S19 — Main app owners and Service registration stay connected

**Behavior.** Service declared tables validate/ensure through source mapper before Card transaction, main Workflow/MCP/gateway/Operator/Trigger/auth owners retain mounts/config/authority, and shared outbox is boot-composed once. Mapped obligations: REQ-021 / AC-021, REQ-022 / AC-022, REQ-023 / AC-023.

**RED.** Run Service preflight and composed OpenAPI P30, changed-boot P24, and Workflow P20/P21; extend these existing proof owners for the Variant table and connected main owner seams. Expected RED is table preflight divergence, missing route/owner, duplicate outbox, or wrong credential/tenant composition. Record exact newly selected owner commands before GREEN.

**GREEN.** Adapt main state/boot and Service mapper use around source role/resource/storage construction; consume source registration outcome under its lock, never infer Created/AlreadyExists by a prior describe. Rerun S10/S12/S14/S16 after constructors change.

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

P01-P30 are source-inspected package/target/selector recipes; P31 explicitly
records discovery for new benchmark mode assertions rather than inventing names.
These are
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
- **I26 (AC-026):** combined-fork availability and dependency/feature/profile graph, directed lock/Hakari regeneration,
  script/descriptor deletions, lint policy and shared-storage consumer audit.
  Do not manufacture RED for configuration regeneration or retained simplification.
- **I27 (AC-027):** test registration/CI selection/source-generation/doc and full
  changed-path disposition audit. Source-owned generated artifacts use their
  generator; no hand-edited stubs. Benchmark dispatch has TDD in S23/P31;
  target metadata and test registration are static proof, not substitutes. Runtime scenarios separately prove behavior.

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

### Required verification execution

Expect substantial pedantic Clippy fallout while importing the rewrite. Fix
underlying code rather than adding `#[allow]`, `#[expect]`, `cfg_attr` lint
suppressions, new workspace exceptions, or weakened checks. Run each lane to
completion with non-fail-fast behavior and collect all runnable failures before
fixing them. Bucket the results by shared root cause; record setup/compile
blockers separately from executed failures and rerun affected proof after fixes.
The same execution requirements apply to the spec-owned final aggregate.

Set `WYRD_LOG=info,vala_bifrost_redux=debug`, `RUST_LOG` to the same filter,
and `RUST_BACKTRACE=1` for lane/gate invocations. Preserve production test
telemetry subscribers, tracers and metric recorders; keep full logs, exit
statuses, failing span/trace evidence and metric diagnostics. A missing
subscriber/recorder is a harness gap to fix. Do not cancel a lane after its
first error; if an aggregate stops early, run its remaining independent checks
and identify dependent checks blocked by prerequisites. Never turn incomplete
execution into a passing report. See the spec for required final closeout.

### Focused command catalog

All recipes below were inspected at the immutable M/S inputs. Setup/build
commands are prerequisites, not claims of successful checks. Run only the
selectors supporting the active scenario, followed by its affected regressions.

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

#### P05 — Main flush snapshot and concurrent ownership

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::tests::owns_any_tracks_only_snapshot_members)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::flush_waits_for_an_outside_claim_over_members_durable_at_drain)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::flush_ignores_an_outside_claim_taken_after_its_snapshot)'
```

#### P06 — Forge term/quiescence, uncertainty and source non-blocking retention

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::schedule_use_is_linearized_with_term_replacement)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::leadership::tests::relinquished_term_refuses_every_handler)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::failed_scheduler_ends_its_term_before_restart)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::production_routes::interrupted_startup_waits_for_its_prepared_lease)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::snapshot_expiration::retryable_catalog_failure_retains_prepared_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-sql --test integration -P journey --run-ignored=all -E 'test(=pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::replaced_snapshots_expire_only_after_the_retention_window)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::reader_expiry_ordering::cut_read_completes_while_snapshot_expiry_holds_the_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::managed_rewrite::standard_variant_layouts_round_trip_per_file)'
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

#### P12 — Native UID/ACK/audit projection and actual Gate adapter

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::audit::projection::tests::projects_canonical_content_with_the_decision_time) | test(=tables::eval::observations::tests::acknowledged_keys_mirror_scribe_stamping) | test(=scribe::execution_lanes::tests::card_scope_rejects_before_writer_admission) | test(=scribe::execution_lanes::tests::optional_and_scoped_card_correlations_stamp_signed_uids)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::tests::gate_enforces_bifrost_record_write)'
```

#### P13 — Real retained audit and six-destination private validator

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_retention::retained_history_carries_both_credential_shapes) | test(=audit_retention::system_owner_security_rejections_retain_once)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::capture_peer::tests::requests_are_confined_to_tenant_capture_destinations)'
```

#### P14 — OTLP API-key and UID runtime entrances

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=trace_export::pg_tests::stock_exporter_authenticates_with_api_key_after_token_lifetime)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::unbound_writer_attributes_spans_to_registered_cards_only) | test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

#### P15 — Evaluation/Task/trace/result layout real journeys

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::eval_runs_follow_the_writing_owner) | test(=eval_verification::integrated_enqueue_outage_preserves_ack_and_recovers) | test(=eval_verification::sealed_replay_on_a_later_day_activates_once) | test(=eval_verification::continuous_eval_runs_the_terminal_matrix) | test(=eval_verification::direct_task_judgment_records_one_correlated_result) | test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence) | test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling) | test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

#### P16 — Verification queue, schedule, lease and judging dependency proof

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::observation_outbox_retains_through_an_outage_and_flushes_at_shutdown) | test(=pg_verification_runtime::schedulers_create_one_run_per_occurrence_across_ticks_and_restart) | test(=pg_verification_runtime::renewal_keeps_a_long_run_and_a_taken_token_cancels_it) | test(=pg_verification_routes::direct_execution_calls_the_judge_provider)"'

scripts/postgres/with-test-postgres.sh -- bash -lc 'WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_runs::observation_runs_follow_the_writer) | test(=pg_verifier_runs::concurrent_schedulers_create_one_run_per_occurrence) | test(=pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences) | test(=pg_verifier_runs::leases_renew_once_a_third_has_passed_and_never_revive) | test(=pg_verifier_runs::reclaimed_lease_fences_the_stale_token)"'
```

#### P17 — Result authoring and schema-by-name

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::results::tests::direct_task_writes_one_summary_row) | test(=verification::results::tests::eval_result_writes_ran_and_skipped_items_then_summary) | test(=verification::results::tests::sampled_out_eval_writes_zero_count_summary_only) | test(=verification::results::tests::reordered_table_fields_keep_values_bound_to_names)'
```

#### P18 — SQL bind forwarding and protected gateway payloads

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::gateway_payload_objects_are_authorized_convergent_and_stable_when_expired)"'
```

#### P19 — Focused first-class Rust query/observation/verification stories

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=query_bifrost::parameterized_sql_returns_the_callers_rows) | test(=query_bifrost::bound_sql_text_is_treated_as_data) | test(=query_bifrost::stream_yields_arrow_batches_and_a_terminal) | test(=query_bifrost::caller_without_bifrost_read_is_refused)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=observe_a_run::run_observations_read_back_by_run_id) | test(=observe_a_run::card_scoped_key_cannot_write_another_cards_observations) | test(=observe_a_run::unsealable_byte_budget_is_refused_at_connect)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E "test(=scheduled_drift_alerts_operator::failed_schedule_alerts_its_operator_on_the_connection_origin) | test(=verify_in_real_time::exact_verifier_grant_verifies_only_that_verifier) | test(=verify_in_real_time::another_tenant_cannot_verify_the_assistant)"'

mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

#### P20 — Accepted Workflow owner accounting

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists) | test(=components::workflow::runs::tests::drain_waits_for_blocking_work_its_caller_abandoned)'
```

#### P21 — Real Workflow shutdown and forwarded query settlement

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -P journey -E "test(=pg_workflow_runs::lifecycle_races_retention_and_shutdown)" && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

#### P22 — Source logical Variant reads and recovered publication

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::variant_sql_registry_covers_every_session)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=published::struct_and_variant_share_physical_pushdown)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::pg_hot_variant_filter_on_an_unshredded_key_reads_null)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=recovery::variant_staging_restores_and_publishes_once)'"

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
```

#### P23 — Focused Python/TypeScript public stories and imported source capability files

```bash
# Runtime prerequisites once, before the focused tests below.
mise run py:setup
mise run ts:build
mise run ts:build:testing

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_query_bifrost.py::test_bound_sql_text_is_treated_as_data tests/integration/test_query_bifrost.py::test_stream_yields_arrow_batches_and_a_terminal tests/integration/test_observe_a_run.py::test_run_observations_read_back_by_run_id tests/integration/test_observe_a_run.py::test_card_scoped_key_cannot_write_another_cards_observations tests/integration/test_scheduled_drift_alerts_operator.py::test_failed_schedule_alerts_its_operator_on_the_connection_origin tests/integration/test_verify_in_real_time.py::test_exact_verifier_grant_verifies_only_that_verifier'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/query-bifrost.test.ts tests/integration/observe-a-run.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts'

# Source files become available after import; retain their inspected names.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/bifrost/test_variant_tables.py tests/integration/bifrost/test_every_iceberg_column_type.py tests/integration/bifrost/test_register_a_table_from_a_model.py tests/integration/bifrost/test_three_timestamp_types.py tests/integration/bifrost/test_table_time_column.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/variant-tables.test.ts tests/integration/every-iceberg-column-type.test.ts tests/integration/register-a-table-from-a-model.test.ts tests/integration/three-timestamp-types.test.ts tests/integration/table-time-column.test.ts'

# Rehomed Rust client cases: discover actual final symbols in existing target.
mise exec -- cargo nextest list --locked -p wyrd-client --features test-support --test integration
```

#### P24 — Changed boot, credentials, signing boundary and platform initialization

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::auth::tests::build_auth_handles_mints_tokens_the_request_verifier_accepts) | test(=boot::auth::tests::build_auth_handles_rejects_an_invalid_signing_key)'
mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=dsn::tests::explicit_serving_dsns_stay_separate_and_derive_the_catalog) | test(=dsn::tests::platform_url_falls_back_to_database_url)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::the_signing_key_file_requires_a_restrictive_regular_bounded_file)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=platform_admin_e2e::an_operator_initializes_the_deployment_through_the_shipped_binary) | test(=platform_admin_e2e::operator_provisions_a_usable_tenant_without_touching_the_database) | test(=platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions)"'
```

#### P25 — Installed CLI bootstrap (unmigrated fixture)

```bash
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
```

#### P26 — Installed local callers in owning language runtimes

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
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

#### P29 — Mounted MCP, compiled CLI and late HTTP terminal

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::agent_investigates_from_multiple_evidence_pivots) | test(=query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution) | test(=query::pg_tests::delegated_agent_query_is_authorized_as_the_subject)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey -E "test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'

mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::routes::tests::http_query_stream_preserves_late_failed_terminal)'
```

#### P30 — Service before-activation preflight and served OpenAPI

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_card_registration_route::service_declared_tables_are_ensured_before_activation)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -P journey -E "test(=pg_openapi_contract::bifrost_operations_publish_typed_problem_refusals) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code) | test(=pg_openapi_contract::no_operation_documents_an_audit_caused_refusal)"'
```

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

## Authority Links

- [Approved spec revision 2](../spec.md)
- [Durable evidence and recipes](../evidence.md)
- [Reconciliation decision history](../reconciliation.md), subordinate to spec.
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd protocol](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md), updated coherently.
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
