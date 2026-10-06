# Behavior Review — packet-wide candidate `7ac45dec9`

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base (excluded): `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- RisingWave comparison checkout: `/home/thorrester/Documents/GitHub/risingwave` at verified HEAD `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The candidate remained at the required commit throughout this pass. The only
worktree additions were review artifacts under the assigned review directory.

## Proposed findings

### BEH-001 — `VIOLATION` — a closeout journey still forbids maintenance revision 11 permits

- **Violated obligation:** Revision 11 / REQ-007 makes replaced snapshots
  eligible at the next maintenance opportunity once no active read or other
  root retains them. The implementation evidence also records removal of the
  equivalent stale sibling-route ban from `production_routes.rs` as a required
  red-gate correction.
- **Location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:496-544`,
  especially the assertion at lines 539-544.
- **Evidence:** `assert_orphan_evidence` loads every durable Forge strategy for
  the tenant and fails if either `snapshot_expiry` or `expired_cleanup` ran.
  That is the same pre-revision-11 assumption removed from
  `tests/integration/forge/production_routes.rs` by `05cceaf35`. The helper's
  query is tenant-wide rather than operation- or table-scoped, so a legitimate
  expiry/cleanup task for the same tenant also trips it.
- **Observable consequence:** The production-geometry journey can fail when
  the scheduler correctly performs immediate post-reader expiry/cleanup. It
  therefore rejects approved behavior and cannot be trusted as a regression
  gate for the packet.
- **Required correction:** Remove the sibling-strategy prohibition. Keep the
  assertions that the orphan route owns a terminal `orphan_gc` lineage
  operation and records physical deletion. If route isolation is required,
  scope evidence to the exact orphan operation/object rather than forbidding
  independent approved maintenance.
- **Focused closure proof:** Run the exact production-closeout journey that
  calls `assert_orphan_evidence`, with a fixture that also owes an eligible
  snapshot expiry, and prove orphan evidence remains exact while the sibling
  route is allowed.

### BEH-002 — `MISSING` — TASK-002 never closes its mandatory pinned fork comparison

- **Violated obligation:** `TASK-002` lines 118-145 and `tasks/README.md`
  require every Full, SmallFiles/FilesWithDelete, Auto, noncommitting,
  governor/spill, and cancellation/loose-output row to contain the exact
  pinned sources, selected Forge source, focused test and result, plus the
  disposition and live consumer of every retained fork-only module.
- **Location:**
  `changes/active/forge-concurrent-planning/tasks/TASK-002-pull-and-worker-results.md:118-145`
  and its implementation report at `:340-367`; current dependency pin at
  `Cargo.toml:243` / `Cargo.lock:4745-4748`.
- **Evidence:** The implementation report contains only a six-row
  RisingWave-to-Forge scheduler summary. It never fills the required fork
  review rows, never inventories retained fork-only modules or their live
  consumers, and does not give exact commands/results for those rows. The
  separate `evidence/TASK-002-fork-review.md` reviews the earlier `6773e19`
  tree and explicitly says at that point that the real-rewrite governor proof
  was still needed. The immutable candidate actually pins `ef97aea`, after
  further filtering and expiry fork changes, so that document is not a
  complete comparison of the shipped dependency.
- **Observable consequence:** The packet lacks the required proof that the
  dependency now shipped preserves only the three approved Wyrd differences
  and that all other physical selection/result behavior still matches the
  pinned RisingWave implementation. Green Wyrd tests do not substitute for
  the explicitly required source comparison and fork-deletion audit.
- **Required correction:** Complete the comparison against RisingWave
  `e23ddf95` / nimtable `74bdc45` and the actually shipped fork `ef97aea`.
  For every required row, name exact sources, Forge consumer, focused executed
  command/result, retained/deleted disposition, and the test that would fail
  if retained fork-only behavior were removed. Do not change production code
  unless the comparison exposes a real mismatch.
- **Focused closure proof:** A non-empty, current-pin comparison matrix meeting
  the task's stated columns, backed by the referenced focused tests and a
  source inventory showing no unexplained fork-only production consumer.

### BEH-003 — `INCORRECT` — TASK-006 claims complete public Python documentation while its public stubs contradict runtime

- **Violated obligation:** TASK-006 requires every public callable's arguments
  to be accurate at runtime and typing surfaces, no docstring/signature
  conflict, and discovered typing drift to be corrected at its source.
- **Location:**
  `changes/active/forge-concurrent-planning/revision/TASK-006-python-api-docstrings.md:193-224`;
  representative current contradictions at
  `sdks/wyrd-sdk-python/python/wyrd/stubs/error.pyi:7-50`,
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:15-64`, and
  `crates/skald/skald-agent/src/python.rs:496-525`.
- **Evidence:** The task's own final evidence lists the disagreements as
  "reported, not resolved" while marking the acceptance matrix PASS. The
  public `SessionTurn` stub documents a positional `role`, keyword
  `tool_call_id`, property `tool_call_id`, and no `Role.System`; the PyO3
  source exposes keyword-only `role`/`content`, `call_id`, a `call_id`
  accessor, and the runtime role includes the system value. The public error
  stub advertises keyword construction and populated attributes that the task
  itself records the native exception does not provide. The same section
  enumerates further missing runtime methods and contradictory defaults.
- **Observable consequence:** `help()`, generated typing surfaces, and runtime
  behavior do not describe one API. IDE/type-checker-approved code can fail at
  runtime, while valid runtime calls can be rejected by typing. This directly
  contradicts the task's outcome and acceptance criteria.
- **Required correction:** Reconcile each inventoried public stub/runtime
  disagreement at the owning hand-authored source and regenerate public stubs.
  Where correction would change the intended public runtime contract rather
  than repair a projection, route that item to contract authority instead of
  claiming the documentation sweep complete. Add top-level Python tests for
  representative constructor/property parity (`SessionTurn`, `Role`, and
  `WyrdError`) and rerun the task's codegen, lint, format, and typecheck lanes.
- **Focused closure proof:** A source-derived inventory with no unresolved
  public signature/default/name disagreement, plus runtime imports/calls that
  exercise the corrected representative surfaces.

## TASK-001 — leader and promotion

**Result: PASS**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One elected leader, fenced replacement, volatile schedule | `forge/leadership.rs`, `forge/leader.rs`, `vala-sql/queries/forge_leader.rs` | leader failover and empty-state closeout journeys; SQL leader tests | PASS |
| Scribe publication is distinct from successful Iceberg promotion | `forge/scribe_promotion.rs`; notification enters `ForgeSchedule::notify_commit` only after catalog commit | promotion notification integration coverage | PASS |
| Lost hints/restart recover only hot promotion debt | `forge/scheduler.rs::sweep_promotion_debt` reads durable `file_list` debt without reseeding ordinary counters | restart recovery journey | PASS |
| RisingWave due rule, zero-commit inactivity, manual force, disabled-count default | `CompactionTrack` / `ForgeTableSettings` | leader/settings unit tests | PASS |
| No durable ordinary compaction schedule | fresh `ForgeSchedule` per term; planning-demand table dropped later by TASK-003 | diff/source trace | PASS |

No task-local behavioral finding proposed.

## TASK-002 — pull and worker results

**Result: FAIL**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Capacity pull uses running parallelism, cap four, five-second cadence, ack ordering | `forge/worker.rs`, role-scoped boot sizing, shared plan queue | pull/capacity journeys and reported qualifying release runs | PASS |
| Oldest-due selection, timeout, one current task/table, stale reports ignored | `forge/leader.rs::DueIndex`, dispatch/report transitions | randomized index equivalence and report tests | PASS |
| Worker plans current Iceberg head; empty plan succeeds; no leader file list | `forge/worker.rs`, `forge/managed/policy.rs` | current-head production-route coverage | PASS |
| Publication remains Forge-owned; shared governor and governed spill are retained | managed executor/publisher and shared resource pool | real rewrite refusal/release/spill coverage inspected in production routes | PASS |
| Mandatory pinned nimtable/current-fork comparison and fork-only consumer disposition | Required table at task lines 118-145; shipped fork is `ef97aea` | implementation report omits the required current-pin matrix | **FAIL — BEH-002 (MISSING)** |
| AC-008 capacity evidence | release-mode report records two qualifying runs at `69d2efe8c` | recorded metrics include the revised 1/2/3 worker gates | PASS (not re-run) |

## TASK-003 — maintenance and removal

**Result: PASS**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Manifest rewrite precedes expiry; rewrite opt-in, expiry default-on; per-table errors isolate | `forge/gc.rs::run_maintenance` | integration maintenance ordering/skipping tests | PASS |
| Destructive work uses table authority and reader/promotion gates | `forge/table_authority.rs`, SQL maintenance authority, expiry/cleanup owners | reader ordering, promotion barrier, cleanup retry coverage | PASS |
| Empty leader restart does not seed cold maintenance | volatile membership in `ForgeSchedule` | closeout empty-restart journey | PASS |
| Planning-demand live schema and consumers removed | forward migration `20261003000100_drop_forge_planning_demands.sql`; no live query caller remains | SQL/grep/source trace | PASS |
| Exact cleanup deletes/absent-confirms object before terminal `file_list` removal | candidate settlement transaction in `forge_tasks.rs` | terminal-row cleanup test | PASS |

The recorded "retry failure lost attempt ownership" warning is a real awkward
error path, but source tracing shows a `prepared` cleanup row remains
same-owner reclaimable immediately (and takeover-reclaimable after its claim
expiry). I therefore do not promote it to a correctness finding without an
observable loss or violation of the approved retry outcome.

## TASK-004 — compaction defaults and type

**Result: PASS**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every table compacts by default | catalog/settings default enablement | default-table dispatch journeys | PASS |
| Optional compaction type crosses Rust/Python/TypeScript/server/catalog surfaces | wire enum, SDK projections, service validation, catalog property | language journeys, schema/codegen checks | PASS |
| Re-registration omit/repeat succeeds; change conflicts stably | server and catalog under-lock checks | server integration and SDK journeys | PASS |
| Default type is `small-files`; threshold is 75% of resolved target | `forge/settings.rs`, `forge/managed/policy.rs` | policy unit and real rewrite tests | PASS |
| Small-files needs two files and does not revisit finished files | `min_group_file_count = 2`; size threshold selection | real-file pair/lone/finished test | PASS |

No task-local behavioral finding proposed. A stale parenthetical in the task
evidence says an omitted catalog property means Forge `full`, but the current
source, revised spec, tests, and surrounding evidence consistently implement
`small-files`; this is evidence prose drift rather than shipped behavior.

## TASK-005 — Iceberg filtering across tiers, scenarios 0–3

**Result: PASS**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Iceberg owns physical field IDs, including managed/nested fields | catalog table creation, Scribe physical projection, Forge validation | fresh signal/custom promotion and mismatch tests | PASS |
| Hot reader consumes declared Bloom layout, including binary `trace_id` | closed `ScanLiteral::Bytes` contract and hot scan pruning | mechanism-isolated hot-tier journey evidence | PASS |
| Iceberg binary page bounds remain conservative and do not disable sibling predicates | pinned fork reader changes and Oracle Iceberg scan | mixed-predicate/page-index tests | PASS |
| Promoted and rewritten files preserve field IDs and pruning behavior | promotion/rewrite projection paths | hot/promoted/rewritten journeys | PASS |
| No legacy migration or declared-ID restoration path | deleted declared managed IDs; no compatibility path | diff/source trace | PASS |

Scenario 4 is correctly superseded by TASK-005-R1 and is not judged against
the older task text.

## TASK-005-R1 — active-table reader cut

**Result: FAIL**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One tenant-scoped SQL acquisition records claims and returns pointers/hot rows | migration 025 and `OracleActiveTableReads::acquire` | SQL atomicity/tenant tests and statement-count journey | PASS |
| PostgreSQL derives abandonment from the remaining query deadline; no fence-liveness/6h rule | `p_deadline_ms`, `statement_timestamp()`, `abandon_after` | exact PostgreSQL-time expiry test | PASS |
| Cut and claim stay inseparable through local/distributed terminal settlement | private `ClaimedSqlCut`; stream settlement joins distributed/analytical descendants before awaited release | held-cut journey and source trace | PASS |
| Dropped owner initiates nonblocking release; failed/no-runtime release leaves deadline protection | `ActiveReadClaim::drop` | dropped-owner/post-pin journey | PASS |
| Every destructive path uses maintenance authority and active-read gate | snapshot expiry, expired cleanup, orphan cleanup owners | held-query journey and Tier-2 route tests | PASS |
| Terminal physical cleanup removes matching terminal `file_list` row only after delete/absence | candidate settlement transaction | focused terminal-row test | PASS |
| No reader epoch/ancestry/IO gate/retention cap or catalog grant remains | deleted reader-protection graph; narrow catalog-pointer definer; no `wyrd_app` catalog grant | tenant isolation and codegen checks | PASS |
| Tests encode immediate post-reader eligibility rather than forbidding valid sibling maintenance | `production_closeout.rs::assert_orphan_evidence` retains the old ban | gate can reject approved revision-11 behavior | **FAIL — BEH-001 (VIOLATION)** |

## TASK-006 — Python API docstrings

**Result: FAIL**

| Requirement / criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `TableConfig` seven arguments documented consistently in runtime help and generated stub | runtime wrapper/source stub/generated projection | representative help output and codegen check | PASS |
| Every public callable and typed structure has accurate argument/field semantics across runtime and typing surfaces | broad docstring sweep | implementation evidence itself lists unresolved public contradictions | **FAIL — BEH-003 (INCORRECT)** |
| No public docstring conflicts with signature/default/runtime contract | current `SessionTurn`, `Role`, and error stub/runtime surfaces disagree | no runtime parity test; typecheck cannot detect native-runtime mismatch | **FAIL — BEH-003 (INCORRECT)** |
| Generated files remain generator-owned | hand-authored sources feed assembled `.pyi` outputs | `codegen:check` reported green | PASS |
| No unrelated behavior/signature change hidden in documentation sweep | changed runtime Python was docstring-only; PyO3 changes were rustdoc-only | AST comparison recorded in task | PASS |

## Cross-task seams, journeys, and regressions

| Seam | Evidence | Result |
|---|---|---|
| Leader notification → volatile due index → local/peer pull → worker current-head plan → report | task 1/2 owner trace and journeys | PASS |
| Worker publication → maintenance membership → expiration/cleanup | task 2/3 owner trace | PASS |
| Oracle active-read acquisition → held cut → Forge destructive refusal → immediate post-release cleanup | R1 SQL, Oracle stream, Forge authority, held-query journey | PASS in production source; closeout assertion drift is BEH-001 |
| Field IDs/filtering across hot → promoted → rewritten tiers | Scribe/catalog/Oracle/Forge journeys | PASS |
| Registration compaction type across first-class SDKs and server/catalog | Rust, Python, TypeScript journeys | PASS |
| Public Python documentation ↔ generated typing ↔ native runtime | current stubs and PyO3 surface | FAIL — BEH-003 |
| Pinned RisingWave behavior ↔ shipped fork delta | RisingWave checkout verified pinned; task comparison is incomplete for current fork | FAIL — BEH-002 |
| Test-fix integrity (`05cceaf35`, `484c3b4f6`, `3a51a24d5`) | PostgreSQL-clock replacements and AttemptGeneration-only age floor match their production owners; assertions otherwise retained | PASS, except the second stale sibling ban left in closeout (BEH-001) |

## Verification assessment

- Reviewed the full `c1508b375..7ac45dec9` changed-file inventory and traced
  the material Forge, Oracle, SQL, SDK, task-evidence, and journey owners.
- Verified candidate identity and pinned RisingWave checkout identity.
- Inspected the recorded green results for `verify:bifrost`,
  `test:principals:integration`, formatting, lints, and `git diff --check`.
- Did not rerun the broad lanes; the packet records a recent all-green run and
  the proposed findings are source/evidence contradictions those lanes do not
  disprove.
- The current fork comparison remains a verification gap by construction
  (BEH-002). Python typecheck/codegen success cannot prove native-runtime parity
  where the hand-authored source stub itself is wrong (BEH-003).

## Overall behavior-review result

**FAIL**

Proposed finding union: `BEH-001`, `BEH-002`, `BEH-003`.
