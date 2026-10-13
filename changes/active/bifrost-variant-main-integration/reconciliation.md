# Unified Bifrost integration proposal and independent reconciliation

Status: **RECONCILED — DECISIONS APPROVED**. All 27 obligations were explicitly
approved in seven human-review batches. [spec.md revision 2](spec.md) and the
[single completed task](tasks/TASK-001-integrate-bifrost-variant-with-main.md)
now provide implementation authority; this file is supporting decision history.

The text below preserves the independent review as written against packet
commit 28b6f3b6. Its references to a proposed review, incomplete recipes, absent
evidence.md, and required packet amendments describe that historical state.
Those amendments are now incorporated into revision 2, including consolidated
benchmark/test targets and assertion_result. The external compaction artifact
is now supplied: `22cd178d14009468bc951a14d7acf63237af808a`, published on
the fork’s `wyrd/main` and pinned by integration commit `914495674`. Historical
prerequisite and owner-question statements below are superseded; combined Wyrd
graph verification remains implementation work. No implementation, merge, deployment, fork
publication, tests or benchmarks were performed by finalizing the packet.

The recommended outcome remains **one spec and one integration task on the
existing `wyrd/bifrost-variant/integrate-main` branch**. Preserve the Variant
engine, compose compatible main repairs at its seams, and retain main's product,
identity, audit, verification, public-client and tooling owners. The two reviews
mostly agree. The material corrections are Task payload naming, retained
benchmark workloads with main consolidated packaging, explicit dependency-fork closure, Oracle memory lifecycle wording,
registration outcome, and SQL history/privilege treatment.

## 1. Provenance and authority

| Input | Immutable reference / inspected location |
|---|---|
| M: main production source | `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6` |
| S: Variant production source | `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05` |
| B: common ancestor | `5ab92b0039404565e51f4a246863ce90cbd8d11f` |
| Other review's dry merge | `574f4069dcb10f1f3dc50478d0419486ff77fb2c`; conflicted blobs, not runnable source |
| Existing integration branch research tip supplied to this researcher | `40113bc83ba49792d24b17c7552709c4566f2af0` |
| Worktree HEAD observed during reconciliation | `28b6f3b6da61874bd345464f13b735cdde41833e`, subject `integration`; adds only the spec and task, 848 lines, no production change |
| Worktree | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-integrate-main` |

Production facts below refer to `git show M:path` or `git show S:path`, not an
assumed integrated executable. There is no root `.codegraph/` in the inspected
worktree. Git and source inspection were used. No tests or builds were run.

Evidence set A is the preceding 27-point recommendation and these seven reports:
`/tmp/wyrd-merge-inventory.md`, `engine.md`, `audit-identity.md`, `membership.md`,
`tables-evaluation.md`, `surfaces-gates.md`, and `migrations.md` (the latter six
also have the `wyrd-merge-` prefix). Its exact proof supplements are
`/tmp/wyrd-plan-proof-{engine,security,lifecycle,evaluation,migrations,surfaces,agent-surfaces}.md`.
The local draft packet is [spec.md](spec.md) and
[TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md).

Evidence set B comprises **all eight** files in
[merge-research](../bifrost-variant/merge-research/RESOLUTION-MAP.md):
`RESOLUTION-MAP.md`, `redux.md`, `server-sql.md`, `shared-spec.md`, `sdks.md`,
`testing.md`, `infra-docs.md`, `silent.md`, together with
[BENCHMARK-HANDOFF.md](../bifrost-variant/BENCHMARK-HANDOFF.md) and
[pre-merge README](../bifrost-variant/benchmarks/pre-merge/README.md).
Their earlier “open decision” paragraphs must be read beneath the later human
decisions recorded in `RESOLUTION-MAP.md` §2; they are not eight competing plans.

Latest direct user instructions retain source retention and positive-deadline
clamping, retain discussed main repairs, and remove `vala.dev.agent_traces`.
Set B additionally records human decisions on 2026-10-11: keep source benchmarks;
the latest direct clarification preserves their workload behavior while requiring
main’s consolidated benchmark/test target packaging;
combine the Arrow-60 and no-REST/Ring compaction fork; audit `created_at` is
`decided_at`; use `assertion_result` Variant for Task; source registration
outcome under its lock; WAL fence definer uses main operator raise/restore;
Python terminal `error_code` is the catalog code; Eval key time is `created_at`.
Recommend honoring these recorded decisions without asking the same questions
again. A Git commit alone does not establish fresh semantic approval.

Disagreement categories used below:

- **Compatible composition:** different owners or boundaries can satisfy both.
- **Stale evidence:** an earlier option is superseded by later user authority or
  confuses endpoint differences with actual branch edits.
- **Material semantic difference:** two observable contracts cannot both hold;
  select the user-backed recommendation explicitly.
- **Mechanical proof gap:** selected behavior is clear but code/test/build
  integration remains unproved.
- **Genuine uncertainty:** external fact or unresolved authority, separated from
  ordinary implementation work.

## 2. All 27 recommendations reconciled

Each row preserves the original numbered obligation, maps it to set B, and
states the canonical recommended result. “Settled” means no new user choice is
needed to specify it; it does not mean implementation or proof has passed.

| # / obligation | Set B finding and alignment | Source-backed resolution / correction | Classification / decision status |
|---|---|---|---|
| 1. Coherent source engine | `redux` and `shared-spec`: source Variant, native scans, ObjectStore, conformance, sizing, v3 lineage and replacement owners win. | Take S catalog/Oracle/Scribe/Forge/storage/types graph together. Do not revive `analytical_scan`, old pruning/fixed IPC/storage cache to fix imports. M attribution and result lifecycle are adapted into surviving owners. | Compatible composition; settled. |
| 2. Retention and deadlines | Map D1/D2 and `redux`: retention, read-only cuts, no reader pins or expiry 503 barrier. A initially proposed oversized rejection, then withdrew it. | S `OraclePlanner::request_deadline` uses `.min(config.max_deadline)`; keep it. S retention migration drops active readers. Preserve invalid-input catalog errors only where they do not contradict an accepted/clamped S request. | Stale earlier A recommendation; settled by user. |
| 3. Cut cache | `redux`/`infra-docs`: cached one-roundtrip cuts and invalidation stay. A examined shutdown counterevidence. | S `CutCache::watch/reset/get/insert` clears/disables on cooperative cancellation and prevents old-generation fills; draining queries fall back to SQL. Keep its process-token watcher, independently of the membership poller. Add actual LISTEN cancellation proof where missing. | Compatible composition plus mechanical proof gap; settled. |
| 4. WAL durability | Both retain one fsync/group, ZSTD, fence-based tail recovery. B locates cross-tenant newest fence test in a deleted audit file. | Keep S shards/replay/fence protocol atomically; move useful fence proof into M SQL integration target. ACK only after durable authority; no duplicate batch; acknowledged corruption fails closed; recovered published object is not reuploaded. | Agreement; settled, runtime unproved. |
| 5. Concurrent flush | A identifies `OwnedMembers/owns_any` repair and scoped faults; B's per-file focus is less explicit. | Keep M `staging_runtime` snapshot membership and `persistence` flush wait semantics in S implementation. Flush includes pre-snapshot rows held by another publisher and excludes later claims; continuous audit must not steal another table's injection. | Additional A preservation requirement; settled. |
| 6. Forge safeguards | Both port lease-bound expiry/unknown-outcome retention. A also covers term linearization, scheduler-exit revocation, same-owner quiescence and bounded conflict retries. | Compose M Rust+SQL safety fixes with S retention/v3 rewrite. No SQL reader pin or unresolved-expiry reader barrier. Prepared/submitted uncertainty stays durable; no immediate reclaim after panic; no scheduling effect after term revocation. Preserve the accepted M definite-conflict retry simplification rather than overwrite S owner files. | Compatible composition; settled. |
| 7. Oracle memory | B says “main no-drain semantics”; A says Oracle memory drain. These sound contradictory but name different lifetimes. | M resources `OracleQuery...::release` returns the slot while a late child retains its view; M admission shutdown waits Oracle-owned memory/headroom, excluding Forge. Keep S lock-free governor and no removed exchange registry. Drop S `LiveConsumers/drain_children` per-query loop; keep owner-level shutdown accounting/wakes. | Wording correction, compatible composition; settled. |
| 8. Membership | A gives final `ClusterTask` and single process poller; B boot/state retains those fields but gives less lifecycle detail. | Retain M `6aaccebf9` final repair, not only earlier `21d02b174`: role heartbeats survive readiness withdrawal; one process poller serves routes through outbox/role drain; join/abort/Drop terminate tasks. | Agreement with stronger A closure; settled. |
| 9. Independent Scribe peers | A establishes both tips already execute Scribe fragments without Oracle; B preserves source assignment shape. | No new protocol services named `ScribePeer/OraclePeer`. Keep M `ScribeRoute/ScribePeers` using registry+boot TLS directly, configured peer decode ceiling and timeout/retry rotation; S old CaptureWriter incorrectly requires a local Oracle for remote writes. | Compatible composition; settled. |
| 10. One outbox | Both remove audit staging/publisher/hash chain and source capture/result publishers. | M `ScribeOutbox/Outbox<ScribeSink>` remains the sole process writer. Destination groups, bounded frames, content batch IDs, identical retries, terminal consumption/counting and nonblocking permission audit survive S projections. | Agreement; settled. |
| 11. Audit schema/credentials | Map D4: `created_at=decided_at`, Variant detail, no seq/hash. A additionally finds actual Gate/Oracle builders omit credential. | M 11 decision content fields plus explicit timestamp: 12 before managed envelope. Use captured decision time, not ingestion/SQL staging. Preserve nullable credential through actual entrance→builder→outbox→retention. Audit CardRef is legitimate content, not retired row correlation. | Compatible composition, additional demonstrated defect; settled. |
| 12. Private write trust | A preserves six destinations and verifier/system attribution; B primarily covers capture/outbox seams. | Keep M `grpc/capture_peer` six-table validator: audit, two captures, three results. Reserved owner only audit; exact Verifier attribution only results. Public raw result/capture writes remain forbidden. S frame/Variant changes must not reduce destination coverage. | Additional A closure; settled. |
| 13. Card UID | Every B group agrees UID rename crosses queue, conformance, OTLP, ingress, SDKs and tests. | M bound signed scope and unbound tenant registry authorize all supplied UIDs before admission; carry trusted attributed scope over peer. Keep at-most-32 lookup and observation-target kind. S `LogicalIdentity` excludes correlation from logical data but includes authorized correlation in batch identity. No blanket CardRef deletion. | Agreement; settled. |
| 14. ACK-derived activation | B observation signature/key-time fix; A supplies writer-owned activation, retry outbox and bulk SQL/replay/lock-order details. | Keep M `ObservationRunOutbox` and `enqueue_observation_batch`; decode acknowledged UID and freeze original `created_at` as input event time. Replay on another UTC day activates once. Do not replace with S best-effort tasks or activate another writer's binding. | Compatible composition with stronger A closure; settled. |
| 15. Verification runtime | B takes M runner and removes provider owner; A identifies lease/cache/authority/schedule/gateway lineage closure. | M claim hydration/renewal/cache/fault/delayed database schedule clock and gateway-mediated judges survive; adapt only S Variant/time readers. Retain tokenless trusted read and writer/application Run inputs. | Agreement; settled. |
| 16. Task results | A proposes Task-only Variant `details`; B recorded human decision explicitly selects `assertion_result`. | **Change draft REQ-016/S15/table row to nullable `assertion_result` Variant using existing `AssertionResult`.** S `drift_report` and `eval_summary` are typed **Struct**, not whole Variant columns; only open Drift features are Variant. Only matching payload populated. Keep application run_id distinct from execution/result linkage. | Material semantic difference, resolved by recorded human decision; do not ask again. |
| 17. Time axes | B D2 and smaller decision retire managed event time; A supplies full table and parent/detail join behavior. | S explicit axes/types win; drift/eval client time survives as created_at; record uses caller-declared axis or ingest default; verification parent uses ended_at. Do not add automatic stamp to arbitrary user column or restore range error specific to removed managed event time. Joins by result_id cannot assume same-day parent/detail ingestion. | Settled source contract; old open event-time question stale. |
| 18. Remove dev table | Map §2.6 and latest direct user instruction agree, despite older A appendices temporarily retaining it. | Ten built-ins, no agent_traces or sole-owner CodeAxis. Amend source dev requirement explicitly, preserve generic Variant properties on supported/dynamic tables, retire unreachable CodeAxis proof. Main eager application provisioning calls S idempotent ensure for final table set; no rewrite of S ensure mechanism. | Stale earlier evidence; settled by user. |
| 19. Query params | Both preserve M params/argument slots and bind into S planner. | B→S does not remove params; it retains ancestor signatures. Keep ordered `QueryParam`, protocol params and binding before optimization/classification. Preserve TS `(query, params?, rows?)`, Python keyword model, Rust params slices. | Compatible composition, three-way correction; settled. |
| 20. Terminal protocol | Both take S WyrdProblem, field9, old6 reserved; B human picks Python catalog error_code. | Atomically update spec/proto/conversions/stream/forwarding/client/SDK/CLI/MCP. Partial data then failure remains failure with problem details. Preserve M newer non-Bifrost errors; no audit-unavailable resurrected. Python error_code returns catalog code, error_detail problem detail. | Settled recorded public decision; consumers need proof. |
| 21. Shared types | Both move mapper to wyrd-types and retain M internal gates/public API. | S RowPreflight/Variant/timestamp/JSON encoder win; M Service declaration `declared_tables` imports new mapper and retains ensure-before-Card-transaction. Keep credential-pool stable identity and native whole-row validation. Top-level propertyless object stays invalid, open column values can be Variant. | Agreement plus source-confirmed uncertainty closure; settled. |
| 22. Main identity/SQL | Both preserve new auth/ambient credentials/API-key OTLP/external DB role model. B human selects WAL definer pattern. | Keep verified Gate entrance, revocation, config homes/selected endpoints. S new functions/table use M TenantConn/operator policy. Keep narrow SECURITY DEFINER newest-fence function with raise/restore/search_path, not raw unrestricted pool export. Restrict effect/data returned to stream fence. | Compatible composition; privilege proof still required. |
| 23. Main app owners | A covers accepted Workflow/MCP/gateway/Operator/verification and shutdown tracking; B main state selection aligns. | Keep main-owned files and state/boot/router consumers, first drain accepted workflow tools, then tracked calls/outbox while roles accept, then role and observation settlement. S ObjectStore has no old Wyrd close API; remove obsolete close/assertions, preserve real owner accounting. No need to make abort async solely for removed storage owner. | Compatible composition; endpoint deletion claims corrected. |
| 24. Migration history | B proposes recomputing historical OLAP digest and dropping grants/revokes; A preserves applied M bytes. | Preserve M historical OLAP bytes/checksum: S edit is commentary, not target DDL. Import only three new source migrations with main policies. Keep M workload/grant/audit deletions. Use documented M role/definer model, not blindly remove access restrictions. Fresh and main-applied/no-old-Bifrost upgrade must converge. | Material persistence difference; safest recommended A history, no historical rewrite required. |
| 25. Fresh-only | A confirms S approved no historical-data conversion/mixed rollout; B SQL rewrite notes unshipped state but not migration closure. | Isolated fresh Bifrost/v3 state only, preserve M control plane. No fingerprint overwrite, old-file conversion, implicit reset, or production deletion. Old populated Bifrost cutover remains outside task. | Agreement after scope clarification; deployment facts not code-plan blocker. |
| 26. Graph/tooling | A says coherent source graph and main simplifications; B identifies neither fork pin satisfies Arrow60+noREST/Ring and human D5 picks combined commit. B D3 retains source benches. | Pin combined compaction commit after publication/reachability proof; no Ring exception. Keep source analytical forks and toolchain, M unrelated deps/pedantic/checks/profile/descriptor deletion. **Retain source query/write/client-queue workload behavior and commands, consolidated into main’s owning capacity binary**, per the latest user clarification. No source per-workload binaries return. Keep main signing workflow; sync LocalSigner alone need not justify SignerFuture churn. | Material omission in A/draft; resolved benchmark decision, fork prerequisite remains. |
| 27. Proof/artifacts | Both rehome source tests to M targets/current stories, regenerate rather than handmerge. B lists dropped TS time-column case and fixture isolation. | Preserve late-failure Py/TS cases and native Variant context/media assertions. Include all five source capability stories and exact discovered Rust selectors, `wyrd.types` public export proof, toolchain lint inheritance, served OpenAPI/runtime MCP. Final gate once, geometry and benches outside it as described below. | Agreement plus mechanical proof gaps; settled. |

## 3. Canonical decisions that deserve explicit packet text

### 3.1 Registration and error semantics

Keep source `BifrostCatalog::register_dataset -> RegisterTableResponse` and its
Created/AlreadyExists outcome decided while holding the registration lock.
The two trees already share public response types. Preserve main permission
staging at the surface; remove source catalog `append_registration_audit` and
audit parameter. A describe-before-create adapter cannot infer the correct
outcome under concurrent matching registration. No new registration layer is
needed. Update `bifrost/service.rs` and callers to use the actual source outcome.

Python terminal `error_code` changes from an enum spelling to the stable catalog
code, as already recorded by the user in set B. Keep `error_detail` from the
problem detail. No legacy spelling alias, fallback success, or dual protocol.
Field 6 remains reserved and `error_problem_json=9` carries the terminal problem;
matching server and clients are required under the fresh-only contract.

### 3.2 Typed results and access

`ResultsTable::drift_report_fields` and `eval_summary_fields` at S explicitly
construct Struct children; `features` is the Variant child. Preserve that shape
and add only nullable Task `assertion_result` Variant containing the existing
`wyrd_spec::vala::eval::AssertionResult` serialization. No generic `details`
column, new task table, new payload type, or duplicated summaries.

S table documentation says sensitive summaries need elevated payload permission.
Main doctrine keeps sensitivity descriptive for ordinary traces/eval/results;
only gateway request/response payload projection has the specific payload-read
authority. Preserve main policy and protect the renamed Variant gateway payload
columns in projections, wildcards, predicates and subqueries. Change misleading
source docs, not general access policy. No new product decision is required by
a stale table comment.

### 3.3 Oracle release, shutdown and storage removal

Per-query slot release must not wait for descendants to finish shrinking; their
shared view keeps memory attributable and wakes the owning Oracle. The last
view detects truly abandoned bytes. Oracle process drain waits its memory plus
infallible headroom, while unrelated Forge memory is excluded. Keep S lock-free
ledger/native executor. Do not recreate its removed exchange registry or impose
an alternate LiveConsumers spin loop. This resolves the apparent “drain”
disagreement without weakening either late-child ownership or shutdown.

S removes Wyrd `StorageLifecycle`, payload cache and close/abort machinery in
favor of ObjectStore. Main membership/outbox lifetimes remain, but obsolete
storage methods/assertions cannot dictate an async abort shape. Prove actual
query/staging/task settlement rather than recreate a no-longer-owned lifecycle.

### 3.4 Time, UID, and batch identity

Use the complete table matrix in draft REQ-017, changing Task payload to
`assertion_result`. Drift/eval rows author `created_at` at client emit; audit
authors it at decision; parent results author `ended_at`; OTLP instants are
timestamp(ns,+00:00). Detail rows default to `wyrd_ingested_at`. Dynamic records
use the registered declared axis or this ingest default. No client event-time
stamp is invented for arbitrary tables, and no admission-window machinery for
the removed managed column is restored.

Keep authorized UID in correlation identity, separate from schema/logical-data
identity. Carry the source once-computed digest when conformance leaves its
logical shape intact. Retrying a staged outbox slice must use identical content
and batch ID. The benchmark's hoped-for SHA cost reduction after removing old
card_ref is a measurement hypothesis, not acceptance proof or permission to
skip authorization/Variant validation.

### 3.5 SQL history and access

Do not recompute a historical migration digest from a source comment change.
Set B's computed `b09fc1...` and instructions to merge the old OLAP file conflict
with preserving applied main SQL. Keep M bytes and its invariant digest; S's
actual new behavior lives in forward 20261009/10/11 migrations.

`forge_snapshot_supersessions` needs main `tenant_isolation` and
`operator_access` policies. The newest WAL fence spans tenants for one stream,
so keep its narrow SECURITY DEFINER operator raise/restore under main's explicit
access boundary and fixed search_path. Prove restoration including failure and
correct cross-tenant fence scope. Do not infer “no roles exist”: main uses
externally provisioned distinct serving/tenant roles; Wyrd stopped creating
hardcoded `wyrd_app`. Drop references to that obsolete role, and align EXECUTE
access with the actual main serving model. Blanket removal of every REVOKE is
not a sufficient security plan. Use main existing authorized SQL owner/capability
patterns and static tenant check; no unrestricted raw pool API.

## 4. Remaining direct and silent conflict closure

This is the resolution checklist, not a list of undecided architecture choices.
All 148 direct conflict paths and 76 automatically merged overlapping edits
must get a disposition against the 527 source change records (531 paths counting
rename origins). Counts describe pinned inputs, not the current integration
branch's documentation additions.

| Conflict family | Required disposition and consumers |
|---|---|
| Engine deletions versus main additions | Source owners replace scan/pruning/fixed IPC/cache. Preserve main gate registry authorization in actual conform/ingress path, not parallel legacy resolution. Main attribution file is an independent newer addition; endpoint deletion alone is not S post-base deletion. |
| Queue/mapper API | Replace BatchBuilder/append_json_row/finish_ipc with RowPreflight/PreparedRows; queue schema imports become wyrd-types. Reach describe/eval fixture helpers, Service registration, TS telemetry_schema, Python record and shared client table configs. Remove managed_candidates/include_event_time across schemas/SDK docs. |
| Observation time | Remove event_time_micros argument/stamp in auto-merged facade/handle/observe; rewrite observe_ingest and capacity load SQL using declared axis. ACK uses admitted created_at, not receipt_micros. Preserve main writer and replay SQL. |
| Terminals | Main-new query/collect still references old terminal enum/helper and default encoder. Import source problem conversion and Variant JSON factory there; adjust SDK unit fixtures and actual CLI/MCP collectors. |
| Outbox/peer | Main-new scribe_outbox maps removed ObjectStorePutFailed; classify current source errors without turning terminal contract failure into endless retry. Keep six private destinations and configured maximum frame handling. Source peer denials stage nonblocking audit while preserving new assignment/body digest. |
| Boot/state | Preserve one outbox/poller and main auth/gateway/workflow/operator configuration; add source cut watcher/resources/storage. Drop audit_outbox/judge_providers/publication/old storage settlement fields. Verify no retained owner was disconnected by whole-source chunk selection. |
| Forge/SQL | Apply term/quiescence/lease+uncertainty repair to source protocols and SQL together; remove only reader barriers. `fixture.staging` callsites move to current fixture API. Reconcile request timeout fields and catalog retries under selected semantics. |
| Resources | Source lock-free accounting plus main Oracle headroom/wakes and late-view behavior. Reconcile analytical reclaim/supervisor/admission jointly; no stale exchange or per-query child-drain owner. |
| SQL bootstrap | New grants to wyrd_app fail; supersession policy missing refuses readiness; unraised fence definer fails RLS context. Correct these before runtime claims. Preserve main old history/drop migrations and sync superuser_pool. |
| Python config/runtime | Add time_column through from_json_schema and every _layout_json invocation; keep main (table,schema), client-only constructors, params then keyword model. Retain timestamp/Pydantic conversion; source open column Variant does not make propertyless root valid. |
| TypeScript config/runtime | Keep main options object, ambient client/config isolation, params position, structured refusal constructors. Merge timestamp/Zod override in shared schema path used by record too; remove duplicated native config functions and reasonless lint allows. |
| Test orphans | Port five source Rust modules into main client integration target; no stale standalone pg_bifrost_e2e target. Source server/SQL test modifications follow main relocated module. Register recovery and time/timestamp cases; update CI regex/paths and ensure no zero selection. |
| Test behavior updates | Native Variant values replace JSON text assertions in main observation stories. Py/TS late-failure source cases survive retired journey names. Removed agent_traces generic proof transfers to retained/dynamic shapes; unreachable CodeAxis-only tests retire. |
| Harness/benches | Port source workload Case/Fixture and exact cases into the consolidated owner because retained benchmark behavior requires them; do not resurrect its independent binary. Keep main capacity but adapt LocalServer::start Placement, cpu_usec and shared Capture frequency API. Use source profile library once; remove retired verification_capacity residual files/duplicate profile copy and source per-workload bin declarations after their cases reach capacity. |
| Graph/generated | Combined compaction fork first; maintain native fork/version closure and main auth deps. Resolve lock deliberately, regenerate Hakari/schema/stubs/napi/errors; descriptor remains generated to OUT_DIR and untracked source bin stays deleted. |
| Docs/process | Merge architecture source Bifrost + main UID/audit/security/verification, ten table count and declared axes. Retain main .claude symlink/review skills/deleted scripts. Historical research is retained as evidence, not rewritten as current authority. |

Three-way corrections are mandatory: B→S has no Wyrd SQL migration changes and
no Workflow module changes. Old `20261002000000_workload_role.sql` and old Vala
schema grant files are unchanged in S and deleted in M. Keep deletions; do not
renumber/resurrect them. Main newer query parameters/eager application setup are
also absent from S ancestry, not intentionally rejected by the Variant rewrite.
The earlier historical L01–L64/S1–S4 ledger must be re-evaluated under the latest
retention, no-dev-table and outbox decisions. Latest benchmark authority retains
source cases and command behavior while preserving main single-target packaging;
old L25 cannot justify deleting the still-required source workloads.

## 5. External prerequisite: exact compaction fork closure

The combined fork is a genuine prerequisite, not permission to weaken
`check:deps`. S Wyrd pins
`696560602183f9eb12e42d7296a1a932e3dc8bed`; M pins
`4e9f325d993d900bcdec2fa74239c6a6173f49cd`. Set B reports a clean candidate merge
tree `ab95e99b`, but a Git tree is not a fetchable commit or validated API graph.

Read-only remote inspection during this reconciliation confirmed the fork heads
still advertise those separate variant/narrow revisions. The local checkout
`/Users/stevenforrester/Documents/GitHub/iceberg-compaction` is on
`a75947c0eb797250f760197c60f1863a3e445fdf`; it is not the combined candidate.
Remote main `d4b7c4f19eef46478d0a7794bb0fd1312997f5d7` is not a substitute:
its root Cargo.toml uses DataFusion54.1/Parquet58.4 and core still depends on REST.
These were directly read from that immutable remote revision. No fork edits or
publication occurred.

Required deliverable: one reachable immutable `iceberg-compaction` commit
containing the source Arrow60/DF55.1/target Iceberg compatibility and main narrow
managed/no-REST-core changes. Verify no consumed core API removed, one native
Arrow/Parquet/DataFusion/object_store stack, AWS-LC without rustls Ring feature,
and no unearned catalog/planning dependency returning. Pin its full SHA in Wyrd,
then resolve lock/Hakari and prove graph plus engine build/rewrites.

Recommend preserving source Cargo.lock as the starting **file**, incorporating
main exact newly needed dependency versions through controlled metadata
resolution; no wholesale `cargo update` or `generate-lockfile`. Use mise's
toolchain for metadata/Hakari, rerun locked metadata and dependency check. In a
main-based merge the source is Git **theirs**: set B's `checkout --ours
Cargo.lock` was written from a source-based perspective and must not be copied
literally. Use explicit pinned-file provenance.

Fork preparation/publication responsibility and the resulting SHA are the only
unavailable external input identified as blocking the final native build.
Unaffected contract/test/documentation work can progress before it is supplied.
No automatic waiver or temporary Ring exception is proposed.

## 6. Unified implementation sequence: one outcome, one task

These are ordered work within TASK-001, not independently deliverable subtasks.

1. Preserve current branch and user changes; confirm source/main pins and real
   remote ancestry. Do not create/reset another branch because the name already
   exists. Record current documentation-only delta. Merge S into the existing
   main-based integration worktree only when implementation is authorized;
   keep both parent histories and an explicit path/conflict ledger.
2. Resolve source type/wire/manifest/dependency owners and combined fork pin,
   main historical SQL plus source forward policies, and declared ten-table
   schemas first. Set Task assertion_result and catalog terminal contracts
   before consumer work. Compile sufficiently to run meaningful focused tests;
   compiler/setup failures are not behavioral RED.
3. Integrate native source WAL/staging/cache/storage/scan/Forge/distributed code
   together, then compatible main flush/lease/term/quiescence/headroom/heartbeat
   repairs. Preserve source retention/clamp/atomic registration and nonblocking
   cuts. Prove actual role/memory lifetime rather than restore deleted owners.
4. Reconnect main outbox/peer/credential/UID/auth entrances, writer-bound ACK
   activation, verification leases/scheduling/gateway judging and application
   Run lineage. Carry source time/Variant representations through these owners.
5. Close Rust/Python/TS facade/binding/export/rendering consumers and main
   Service/Workflow/MCP/CLI/bootstrap paths. Rehome tests and benchmarks using
   current fixtures, retain only benchmark dependencies earned by preserved workloads, and
   reconcile source/main architecture/docs/codegen/lints/CI registrations.
6. Execute scenario-focused proof, inspect all direct/silent/main-only seams,
   finalize per-AC evidence and immutable review candidate; change-review then
   runs final aggregate, production geometry and performance parity below.

Existing owners remain authoritative: `BifrostCatalog`, `OraclePlanner`,
`CutCache`, Scribe staging/persistence/shards/replay, Forge leadership/worker,
source resource governor, `ClusterTask/ClusterRegistry`, `ScribeOutbox/ScribeRoute`,
`ObservationRunOutbox/VerifierRunQueue`, `EvalEngine/DriftEngine`, `WorkflowRuns`,
shared `Bifrost`, `RowPreflight`, `wyrd-types` and existing SDK wrappers. No new
transport, parser, graph store, lifecycle umbrella, abstraction or validator is
required. Private helper and test-fixture shape remain implementation-owned.

## 7. Proof plan and evidence discipline

Test consolidation follows each existing owner’s integration/capability target;
it does not mean one giant repository-wide test binary. Source per-file targets
are not resurrected. The existing task has 22 ordered Behavior/RED/GREEN/REFACTOR scenarios, but its
P01–P28 catalog is absent. Keep the scenario model, correct selected contracts,
and complete exact recipes. Preserved already-correct source behavior gets
regression/no-op evidence; do not manufacture RED. Changed behavior must fail
with the intended assertion before implementation, not compilation or zero-test
selection. Record exact final test name/target/features after discovery, using
the managed PostgreSQL wrapper and owning Python/Node runtime.

The following source-confirmed samples are actionable anchors, not executed
results. Fuller A proof supplements supply additional exact tests for each
owner; archive those recipes durably rather than leave links only to `/tmp`.

```bash
# Source cache and group-fsync invariants; supporting unit proof.
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=catalog::cut_cache::tests::cut_cache_serves_only_fresh_listened_cuts) | test(=scribe::shards::tests::group_commit_syncs_once)'

# Main term and quiescence invariants composed into source owners.
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=forge::leadership::tests::schedule_use_is_linearized_with_term_replacement) | test(=forge::leadership::tests::relinquished_term_refuses_every_handler) | test(=forge::worker::tests::only_a_joined_loop_enables_same_owner_reclaim)'

# Main Oracle-only shutdown memory/headroom, not per-query child spin.
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::oracle_shutdown_drains_only_oracle_memory) | test(=oracle::admission::tests::oracle_shutdown_drains_oracle_infallible_headroom)'

# Real membership lifetime and delayed peer availability.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks) | test(=query::oracle_only_pod_retains_audit_staged_before_its_drain)"'

# Independent routing and identical retry slices; no DB needed.
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support \
  -E 'test(=scribe_outbox::tests::a_peer_route_needs_no_oracle) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe)'

# Historical bytes; source comment edits are not new historical authority.
mise exec -- cargo nextest run --locked -p vala-sql --lib \
  -E 'test(=tests::forge_and_oracle_migrations_have_unique_forward_owners)'

# Main typed parameter transport.
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'
```

Before claiming any selected test ran, confirm on integrated source with
`mise exec -- cargo nextest list --locked -p <package> --test <actual-target>`
or `--lib`, supplying target-required features. Source moved modules and ignored
tests must retain their actual final names/classification. No invented selector
is treated as a passed test.

Required changed behavioral closure:

| Scenario group | Behavior / RED / GREEN / REFACTOR outline | Strongest required evidence |
|---|---|---|
| Fresh types/time/registration | Behavior: schema and atomic registration outcome agree; invalid row/time/root refuses before writes. RED: extend current registration/typed roundtrip story to conflicting concurrent registration or dropped layout argument. GREEN: compose source locked outcome/type owner into main surfaces. REFACTOR: remove duplicate mapper/config constructors. | Real Rust/Python/TS model/timestamp/time-column/nested-null journeys, Service before-Card preflight, fresh v3 read/rewrite. |
| Source durability/retention | Behavior: ACK/replay/fence/retention/current snapshot safety and cache invalidation remain exact. RED: extend existing crash/expiry/cache owner with failing mixed seam. GREEN: source protocol plus bounded M uncertainty/flush. REFACTOR: delete old readers without deleting reachable safety proof. | Crash and publication recovery, concurrent flush, pre/post-submit unknown expiry, read cuts continue, repeated v3 lineage. |
| Main role/resource lifetime | Behavior: late memory stays owned, role heartbeat/discovery lasts until needed work settles. RED: actual late-child/delayed-Scribe scenario. GREEN: M lifetime fixes adapted to lock-free source owners. REFACTOR: no exchange registry or duplicate poller. | Real role drain journeys plus focused memory ownership/term/quiescence branches. |
| Trust/outbox/activation | Behavior: six destinations, scope, credential audit, writer-bound ACK/replay are preserved. RED: originate actual keyed decision and wrong-writer/next-day/outage scenarios. GREEN: main trusted owners plus source projection/time. REFACTOR: remove source publisher/staging, no authorization lookup in CPU path. | Native/OTLP negative journeys, builder-to-retention audit, all private categories and public refusal, bulk SQL replay/outage. |
| Results/terminal/public clients | Behavior: Task assertion_result and application lineage exact; partial rows followed by typed failure stay failed. RED: dropped Task value or legacy terminal/text decode. GREEN: full source wire/rendering plus main params/gateway judges. REFACTOR: one existing payload contract/conversion owner. | Rust/Python/TS native Variant/late-failure story; actual CLI/MCP invocation, served OpenAPI, gateway denied judge/payload reads. |
| SQL/control plane/bootstrap | Behavior: main ledger/data unchanged with new source DDL and fresh v3; installed callers work. RED: exact main-applied/no-old-Bifrost transition plus distinct roles. GREEN: retain history and adapt forward SQL/operator definer. REFACTOR: no checksum bypass/raw pool/reset. | Real managed Postgres migration+RLS/operator tests, retained identity/card/credential checks, installed CLI/config/all-runtime journeys. |
| Static/generated/proof registration | No manufactured RED: metadata/lock/Hakari/Clippy/codegen/CI/docs disposition proof. | All changed records/conflicts, relocated test discovery, type/export checks, source-generated outputs, retained workload dispatch in the single owning benchmark target compiles/runs. |

Known proof gaps must be closed, not mislabeled as existing coverage: real
credential-bearing Gate/Oracle builder→retained audit (current retention stages
fabricated events); actual warm LISTEN cancellation→fresh database read; exact
main-applied forward transition preserving identities/Cards/credentials;
CLI/MCP late runtime failure where acceptance claims it; legitimate private
frame over tonic's default 4MiB where configured limit changes; source TS
time-column focused registration. Nextest source confirmation does not prove
the final integrated target runs these tests.

Task iteration uses focused recipes and affected static/owner lanes only.
Whole-candidate closeout uses **`mise run gate` once**, plus
`mise run test:bifrost:journey:scribe:production-geometry` and
`mise run test:bifrost:journey:forge:production-geometry`: main gate excludes or
reduces those full production sizes. Do not rerun every component lane after
green gate merely because set B listed them chronologically. Trace failed
Bifrost/server tests before diagnosis; do not use synthetic load. Live cloud
and live provider smoke are not claimed by local gate.

## 8. Retained benchmarks and performance order

The latest direct user clarification resolves the apparent conflict in D3:
**preserve source benchmark workloads and behavior, and use main’s consolidated
compiled target convention**. Source `query_capacity`, ingest/client queue and
write sweeps stay unchanged in the reference Variant worktree. The integration
ports them into the existing owning `capacity` binary, retaining usable mise
command entrypoints and parameters. No new per-workload binaries are introduced;
main’s capacity mix and outcomes also remain. Earlier recorded “keep benchmarks”
means workload retention, not a separate-binary exception.

The bounded source/owner mapping is:

| Existing source behavior | Inspected source owner | Integrated main owner / required closure |
|---|---|---|
| Query capacity, standard/telemetry hot and Parquet, selective/small aggregate/concurrent writes/full queue/heavy case | S `src/bin/bifrost_query_capacity/main.rs`: `Tier`, `SWEEP`, `PARQUET_RETENTION_SECONDS`, `SETTLE_TIMEOUT`; `run::Bench`, `workload::{Case,Fixture}`, `telemetry::Telemetry` | Extend M `src/bin/capacity/main.rs` entrypoint and existing `capacity` Cargo target to dispatch this workload without replacing the default mixed benchmark. Preserve six query concurrency points, production paths, exact answer checks, hot versus published environment and recorded report fields. |
| Write sweep, native/OTLP telemetry/logs | Same S query-capacity owner, `WRITE_SWEEP=[1,4,16,64]`, `OTLP_SWEEP_ROWS=2_000_000`, `logs::Logs`, `run::Signal`, `--writes` | Route existing write task arguments to the same consolidated capacity target. Preserve native/OTLP row counts, fresh nonreplayed range per step, Parquet/one-shard settings, fsync/stage/profile outputs and current verdicts. Do not substitute mixed-SLO load for isolated source write measurements. |
| Client/ingest queue, WyrdState drift sustained/headroom/delayed ACK/in-flight sweep | S `src/bin/bifrost_client_queue/main.rs`: `Cli`, `benchmark`, rates500/1000, ACK50ms, `fixture::Tenant`, `step::{Bench,Plan,Record}`, `proxy` | Existing capacity target gains this bounded workload choice; retain seconds/max-in-flight options and client queue semantics, same production public client/server and source assertions. No standalone ingest/client queue binary; existing mise aliases may select the mode. |
| Main mixed verification/ingest/query capacity | M `capacity/main.rs`: `Cli`, `Lifetime`, `benchmark`; `load::{Op,Work,TenantClients,mix}`; `step::{Deployment,Plan,Record,StepKind}`; `evidence::Queue` | Keep default `bench:capacity` behavior, all mixed golden-signal verdicts, controlled ramp/sustained/scale-out and existing 30minute command lifetime intact. Source workload modes are distinct selections in the same compiled target, not changes to the default experiment. |
| Shared release/profiling lifetime | M `release_server::{LocalServer,OperatorRun,Envelope}` plus S `Placement::{Host,Scoped}`, `cpu_usec`, `profile::{Capture,Profile}` | Reuse and reconcile existing release/profile owners. Preserve Host placement for source whole-host comparisons versus scoped main pods. Adapt caller signatures once; avoid duplicate profile libraries and per-workload server harnesses. |

M currently has one `capacity` target; its command `Cli` is the mixed benchmark,
not an existing source-workload dispatcher. A small explicit workload selection
in that existing owner is the selected extension. Its private module/control-flow
shape is implementation-owned. The mode must be selected before requiring
mixed-only `WYRD_CAPACITY_STARTED/DEADLINE` inputs: preserve source task lifetime
and setup semantics rather than silently enforce main’s 30minute mixed window
on the heavy source experiment. Existing mise commands can act as aliases,
reusing the same release/profile Cargo target output, PostgreSQL wrapper and
required storage/cloud setup. Report directories are evidence outputs, not a
reason to compile separate binaries. Keep source report layout or provide an
explicit equivalent mapping with parity comparator inputs preserved. Do not
add new workload targets, new target-dir per case, synthetic host load, raised
test-thread count or hyperthread saturation experiment.

Performance parity is distinct from correctness gate, which does not run these
opt-in benchmarks. Follow handoff order on the same host with no compilation:

1. Obtain exact **S=cfe21dc90 read baseline** first, standard hot/Parquet then
   telemetry hot/Parquet. The committed read reports use older or unrecorded
   code and one Parquet run uses 12MiB experimental chunk; they cannot establish
   parity against S's 24MiB default. Preserve the existing source worktree.
2. Run identical read sweeps on candidate; compare each case qps, p50/p95/p99,
   CPU/query, peak memory, latency and crate CPU Paretos. Repeat a suspected
   regression before attributing it; investigate with traces/profiles.
3. Only after reads agree, compare logs writes, telemetry writes then native
   writes, each with an exact-source same-host baseline. Run37 logs is recorded
   at cfe21dc90; other missing source baselines must be produced. Retain existing
   harness checks (30MiB/s wire/core and 200k fastest-step rows/s), with host
   context rather than treating hardware-specific results as universal.
4. Confirm logical-digest reuse after card_uid integration via profile. Do not
   demand an invented fixed SHA percentage or suppress validation to hit a
   forecast. Report an unobserved predicted gain honestly.

Commands to preserve and run under matched parameters:

```bash
mise run bench:bifrost:query-capacity
mise run bench:bifrost:query-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity -- --fixture logs
mise run bench:bifrost:write-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity
mise run bench:bifrost:client-queue
```

These source command entrypoints become available after mapping to the consolidated target;
they are not currently present on the main-based worktree. Save code SHA,
host/resources/settings/exact command and raw reports so target output is not
overwritten evidence. Existing AMD9950X Linux measurements are not directly
comparable to this macOS environment. No new numeric regression tolerance is
invented; repeated coherent worse results need diagnosis, not a convenient
single noisy pass. Future optimization items (codec/per-query overhead/100M
fixture/single-pass OTLP/ZSTD tuning/gateway benchmarks) remain outside this
integration outcome; parity proof precedes those changes.

## 9. Packet amendments and user decision list

The existing spec's `approved` header transcribes the preceding request; this
**reconciled proposal has not been reviewed**. Preserve that historical packet,
then revise it after user review rather than pretend reconciliation itself
approved the new text. No production merge or fork push follows from this report.

Required amendments before implementation handoff:

1. REQ-016, table matrix, S15 and associated AC/proofs: replace Task `details`
   with recorded `assertion_result` Variant; describe Drift/Eval Struct accurately.
2. REQ-026/027, invariants/non-goals: explicitly retain source benchmark workloads, inputs,
   assertions, metrics and mise entrypoint behavior, packaged into main’s existing
   consolidated capacity target. Keep main retired checks/descriptor/review-process
   simplifications; remove source per-workload bin declarations. This is latest
   user authority, not an exception to main consolidation.
3. Add external combined-fork deliverable/owner/full SHA acceptance and stop
   condition; remove “no open material decisions” wording insofar as it implies
   a valid pin already exists. Fork closure is external prerequisite, not an
   unmade product choice or reason to stop unaffected work.
4. Make source atomic registration outcome, Python catalog error_code, source
   clamp/declared-time and WAL definer operator raise/restore explicit. Preserve
   M historical bytes rather than recompute merged old checksum.
5. Refine Oracle memory and S ObjectStore shutdown wording as §3.3; choose actual
   owners and invariants rather than obsolete storage API/async signature.
6. Complete missing P01–P28 focused catalog and durable evidence.md. The task
   currently ends at “Focused command catalog” and references absent recipes;
   the spec references absent evidence.md. No readiness claim is justified.
7. Add performance closeout before new optimization, accurate baseline gaps,
   benchmark context and main/source full changed-path ledger. Archive A reports
   and proof recipes or equivalent immutable evidence; `/tmp` alone is fragile.

There is **one operational question needing user input if an owner is not already
assigned elsewhere**:

| Question | Recommendation | Alternative / impact |
|---|---|---|
| Who will prepare/publish the combined compaction fork and supply its immutable SHA: an authorized follow-on agent or the existing fork owner? | Have the fork owner provide a commit combining source Arrow60 compatibility with main no-REST/Ring narrowing; integration pins and verifies it. | Explicitly authorize a bounded external fork change and publication separately. A local patch/unpublished tree is not reproducible release evidence; an exception to Ring is not recommended and contradicts recorded D5. |

No re-decision is requested for retention/pins, event-time removal, benchmarks,
Task column, atomic registration, Python code, audit timestamp, CodeAxis/dev
table removal, or WAL definer: authority already specifies them. Review the
unified recommended plan, and correct any inaccurately recorded decision; do
not turn every implementation proof gap into a product question.

No retained-data question blocks this fresh-only integration. If deployment
over old Bifrost data is later requested, inventory and cutover/preservation
authority are required before any reset or migration. Separate inferred
source-only cache-read timestamp and fleet max-deadline/retention consistency
concerns remain observations, not an unsolicited redesign; a reproduced failure
blocking selected acceptance must still be diagnosed.

Only this reconciliation file was added. No spec/task/source files were edited,
and no tests, merge, commit, push, fork changes or benchmarks were performed.
Runtime correctness, combined fork availability and same-host performance remain
unproved until the described implementation and verification run.
