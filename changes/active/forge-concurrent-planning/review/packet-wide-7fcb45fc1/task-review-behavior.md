# Packet-wide behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Prior review/remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Pinned RisingWave reference: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The checked-out worktree was later than the candidate, so every production
source citation below was read with `git show 7fcb45fc1:<path>` or
`git diff <base>..7fcb45fc1`. I did not use the implementer's completion
summary as proof.

## Proposed findings

### BEH-REREVIEW-001 — INCORRECT — prior `FIND-TASK-005-R1-3` remains open

- **Violated obligation:** REQ-014, INV-005/006/009, AC-006/009, TASK-005-R1
  Scenarios 1 and 3, and the R2 acceptance criterion require one ordering
  between cut acquisition and every destructive effect. The exclusive table
  authority must stay live until the effect's outcome is known; unknown
  acceptance may not become a window in which a new reader commits a cut.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:8190-8224,8270-8275`;
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:8037-8042`;
  `crates/vala/vala-bifrost-redux/src/forge/expire.rs:713-731,766-783`;
  `crates/vala/vala-bifrost-redux/src/forge/expire.rs:289-322`;
  acquisition consumer
  `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:275-305`.
- **Producer-to-consumer evidence:** `bounded_cleanup_delete` times out the
  proof/delete future at the lease TTL and explicitly says the delete may
  already have reached the object store, then returns `Uncertain`. Its caller
  drops `ExclusiveTableAuthority` and commits the authority transaction before
  retaining the prepared candidate. Snapshot expiry has the same reachable
  shape: `complete_expiry` says cancellation is not proof the remote commit was
  rejected, returns an unknown-acceptance error on timeout or cancellation,
  and its caller nevertheless drops and commits the exclusive authority. The
  acquisition function waits only for the authority lock; it does not refuse an
  unresolved prepared delete/expiry operation. A reader can therefore commit
  the old pointer/cut after the lock is surrendered while the previously
  submitted delete or catalog commit can still land.
- **Observable consequence:** a newly committed active reader can lose an
  object named by its cut, or can commit the pre-expiry pointer while the
  already-submitted expiry lands afterward. Durable prepared evidence prevents
  a *second* cleanup attempt; it does not stop the first in-flight effect from
  completing against the new reader.
- **Decision-complete correction:** keep the existing authority and prepared
  operation owners. A live process must not intentionally surrender exclusive
  authority while a submitted effect remains acceptance-unknown. If process
  loss or an unavoidable transport ambiguity releases the lock, the existing
  prepared destructive operation must make cut acquisition refuse until
  reconciliation establishes the effect's stable outcome. Do not add an epoch,
  IO gate, second persisted protocol, or downstream reader recheck.
- **Focused closure proof:** pause each snapshot-expiry and expired-object
  delete after external submission but before its response, force the timeout
  or cancellation path, then race cut acquisition. Prove no cut commits while
  acceptance is unknown; after reconciliation, prove the reader either obtains
  the post-effect pointer/cut or the definitively unmodified cut. The test must
  keep the external effect pending when it checks acquisition, rather than
  resolving it before releasing the authority.

### BEH-REREVIEW-002 — DRIFT — new packet finding

- **Violated obligation:** task-review PASS requires that no unrelated change
  enter the reviewed cumulative diff.
- **Exact location:** `changes/active/bifrost-variant/spec.md:1-686`, added by
  commit `406464a59` inside `e8d3cca13..7fcb45fc1`.
- **Evidence:** the file defines a separate `SPEC-bifrost-variant` change and
  has no requirement, acceptance criterion, remediation finding, or consumer
  in the Forge concurrent-planning packet. It was committed after the recorded
  final gate code commit `30d31ebac` and before this immutable candidate.
- **Observable consequence:** the Forge packet's candidate is not scoped to the
  work being accepted, so a PASS would also accept an unrelated 686-line draft
  specification without its own change review.
- **Decision-complete correction:** remove this file/commit from the Forge
  candidate and carry it only on its owning change branch. No Forge production
  or test change is required.
- **Focused closure proof:**
  `git diff --name-status e8d3cca13..<candidate>` contains no
  `changes/active/bifrost-variant/` path.

## Per-task acceptance matrices

### TASK-001 — leader and promotion

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-001 / INV-001: one renewable term; loss stops dispatch and leader maintenance | `forge/leadership.rs` separates renewal from promotion IO, revokes the exact `ForgeHeldTerm` on refusal/failure/timeout/replacement, and `held` excludes revoked terms; `scheduler.rs` and `gc.rs` run promotion/maintenance under that term's cancellation | `production_closeout::revoked_term_stops_promotion_dispatch_and_maintenance`; leader SQL and unit evidence recorded in TASK-001-R1 | PASS |
| REQ-002/008: only successful promotion counts; lost hints recover from hot evidence | Renewal is its own loop; acquisition signals a supervisor debt sweep; ordinary schedule remains term-local | Forge promotion and restart journeys recorded green | PASS |
| REQ-003 and empty restart | `ForgeHeldTerm` constructs a fresh `ForgeSchedule`; replacement drops the old schedule | original Task-001 schedule/zero-commit/failover evidence plus revocation journey | PASS |
| No process loss from a slow promotion or PostgreSQL renewal failure | Renewal is independent of promotion; failed renewal is logged and retried by the scheduler loop; server composes Forge loops under `restarting_worker` | restart-loop unit test and recorded final gate | PASS |

**Verdict: PASS.** `FIND-TASK-001-1` is closed.

### TASK-002 — pull and worker results

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-004/005: RisingWave-shaped pull, worker-owned physical planning, bounded capacity | `worker.rs::pull_claimed_tasks` and `free_pull_room`; current-head envelope and managed policy remain worker-side | production closeout pull/current-head journeys; 16/16 release benchmark record | PASS |
| REQ-006 / INV-003/004: accepted dispatch has one owner and reports without becoming fair-claimable | Post-insert shutdown removes dispatch bookkeeping, `close_dispatched` closes the exact row as `cancelled`, and one `NotStarted` report is emitted only on a successful close | shutdown edge proof recorded in TASK-002-R1; SQL guarded-close coverage | PASS |
| Mandatory exact shipped-fork comparison | Task evidence compares `74bdc45`, `6773e19`, `ef97aea`, and shipped `380a4d0`; all six required rows and every retained/deleted fork-only module name consumer, test, and disposition; workspace and lockfile pin `380a4d0717e...` | named fork tests, fork 151-unit result, and Wyrd managed rewrite/governor journeys recorded | PASS |
| AC-008 leader/worker capacity evidence | Task records two qualifying runs and the remediation records 16/16 release checks | release-mode `bench:bifrost:forge-capacity` result | PASS |

**Verdict: PASS.** `FIND-TASK-002-1` and `FIND-TASK-002-2` are closed.

### TASK-003 — maintenance and removal

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-007/008: ordered rewrite, expiry, cleanup and empty leader restart | `gc.rs::run_maintenance` uses term-owned schedule and revocation; rewrite precedes expiry and orphan cleanup | original maintenance/closeout journeys plus revoked-pass proof | PASS |
| Prepared cleanup identity survives refusal/uncertainty | `CleanupRetained` bypasses generic retry/terminal settlement; prepared reconciliation returns `Ok(false)` and replays the same frontier/attempt | refused-first/later/resumed/uncertain cleanup integration cases recorded green | PASS |
| REQ-010 deletion of planning-demand machinery | no production reader-epoch/frontier/IO-gate or planning-demand consumer was found; only protobuf `reserved "reader_cut"` remains | deletion grep and codegen/gate records | PASS |
| Destructive cleanup respects active reads | Definite paths require `ExclusiveTableAuthority`, but timeout/unknown-acceptance paths release it while the effect can still land | source trace in `BEH-REREVIEW-001` | FAIL (`BEH-REREVIEW-001`) |

**Verdict: FAIL.** `FIND-TASK-003-1` is closed, but TASK-003 shares the still-open destructive-authority failure.

### TASK-004 — compaction defaults and type

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-011/013: every table compacts and omission means `small-files` | canonical wire rustdoc, client config, schemas, docs, and journeys consistently state the default | Rust/Python/TypeScript contract and registration tests recorded green | PASS |
| REQ-012 canonical public values | `CompactionTypeWire` uses `kebab-case`; first-class projections accept `small-files` and `files-with-delete` and reject underscore spellings | schema assertions and three SDK test sets | PASS |
| Rust SDK can name the type through its public Bifrost surface | `wyrd_client::bifrost` re-exports `CompactionTypeWire`; thin `wyrd_sdk` projection has a compile/API test importing only there | `sdk_bifrost_names_the_compaction_type` | PASS |

**Verdict: PASS.** `FIND-TASK-004-1` and `FIND-TASK-004-2` are closed.

### TASK-005 — Iceberg filtering scenarios 0-3

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-015 Iceberg owns physical field IDs | cumulative source retains registered-layout projection through Scribe, Oracle, and Forge validation | original fresh signal/custom-table promotion evidence and Bifrost gate | PASS |
| REQ-016 typed pruning remains exact on hot/promoted/rewritten cuts | remediation changes around filtering owners are documentation/import-shape or shared lifecycle changes; no removal of binary predicate/Bloom/page-bound paths was found | original scenarios 0-3 plus recorded Oracle/Forge journeys and final gate | PASS |

**Verdict: PASS.** No filtering-specific finding.

### TASK-005-R1 / TASK-005-R2 — active table reads

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| One tenant-scoped atomic cut and RLS boundary | acquisition share-locks each authority row, obtains pointers/hot rows and upserts active reads in one SQL function under `TenantConn`; no app grant to `iceberg_catalog` was found | SQL membership/tenant-isolation records and principals integration | PASS |
| Leader stream owns Analytical lifetime; drop revokes followers/local drivers before claim release | `AnalyticalGraphLifecycle` is held by `AnalyticalAttemptOwnership` inside the stream's admission guard; no lifecycle task/handle exists in the supervisor; `LeaderStreamOwners::drop` drops admission before `ActiveReadClaim`; lifecycle drop drains admission, cancels, closes exchanges, drops grants, and abandons local attempt synchronously | owner-drop unit tests and held-cut ordering journey, including unread stream, recorded green | PASS |
| Human-approved drop semantics | lifecycle documentation and code allow already-in-flight remote IO to finish only after its grant/consumer is revoked; no result can reach the dropped leader | synchronous grant/local-owner assertions and ordering event proof | PASS |
| Initial acquisition and one retry retain the original deadline | planner passes the immutable `Instant`; catalog derives positive remaining duration immediately before each SQL call; PostgreSQL stamps `statement_timestamp() + remaining` | delayed/replayed acquisition and SQL expiry tests recorded | PASS |
| Every destructive effect remains serialized until its outcome is stable | definite effect paths carry borrowed `ExclusiveTableAuthority`, but expiry and expired-delete timeout/cancellation paths explicitly surrender it with acceptance unknown, and cut acquisition checks no prepared-operation barrier | source trace in `BEH-REREVIEW-001`; existing race tests resolve effects before acquisition and do not cover unknown acceptance | FAIL (`BEH-REREVIEW-001`) |
| Sibling maintenance is permitted | tenant-wide sibling-strategy ban is removed while exact orphan identity/result assertions remain | production closeout journey | PASS |
| SQL ownership is in `vala-sql` | production Redux has no raw `bifrost_tables` query; typed layout read is owned by `olap_catalog` | tenant-isolation check and source grep | PASS |
| Prohibited reader machinery remains deleted | only the protobuf reserved name remains; no production reader epoch, ancestry frontier, IO gate, reader-cut alias, or retention query cap found | source grep and codegen record | PASS |

**Verdict: FAIL.** `FIND-TASK-005-R1-1`, `-2`, `-4`, and `-5` are closed;
`FIND-TASK-005-R1-3` remains open.

### TASK-006 — Python API docstrings and runtime parity

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| Runtime/source/generated declarations agree for the recorded inventory | source stubs expose actual `Role`, keyword-only `SessionTurn`, Agent mutators, ordinary exception construction, and corrected card/model/prompt surfaces; generated public stubs match | top-level `test_public_api_parity.py`, typecheck, and codegen records | PASS |
| After-hook raises never panic and match help | shared callback owner maps `after_model`/`after_agent` aborts to `CallbackAborted`; `after_tool` creates one failed tool result and continues | three Rust abort tests and three top-level Python raise tests | PASS |
| Removed test did not weaken an approved property | deleted test enforced duplicating signature types in 74 doc lines; signatures/typecheck now own typing and TASK-006 expressly forbids that prose duplication | replacement runtime parity test and `py:typecheck` | PASS |

**Verdict: PASS.** `FIND-TASK-006-1` and `FIND-TASK-006-2` are closed.

### TASK-PACKET-R1 — standards and final gate

| Obligation | Implementation evidence at `7fcb45fc1` | Verification evidence reviewed | Result |
|---|---|---|---|
| Changed-symbol rustdoc and import/signature shape | remediation records the full cumulative changed-symbol audit; representative prior sites are documented and imports moved | all-feature lint and source/shape audit records | PASS |
| Clean cumulative diff | no whitespace errors in `git diff --check c1508b375..7fcb45fc1` | rerun during this review: exit 0 | PASS |
| Broad final verification | executable candidate `30d31ebac` passed `mise run gate`; `7fcb45fc1` adds only review evidence after the unrelated spec commit | recorded 1638-second gate and 16/16 release benchmark | PASS for Forge code |
| Candidate contains no unrelated work | separate Bifrost-variant draft is inside the immutable range | commit/diff trace | FAIL (`BEH-REREVIEW-002`) |

**Verdict: FAIL.** `FIND-PACKET-1` through `FIND-PACKET-4` are closed; a new
packet-scope drift finding remains.

## Prior-finding closure

| Stable finding | Closure result | Source-backed reason |
|---|---|---|
| `FIND-TASK-001-1` | CLOSED | renewal is independent and exact terms are revocable across scheduler, handlers, promotion, and maintenance |
| `FIND-TASK-002-1` | CLOSED | shutdown closes dispatched rows terminally and reports `NotStarted` once |
| `FIND-TASK-002-2` | CLOSED | exact shipped pin and every required comparison/inventory row are populated |
| `FIND-TASK-003-1` | CLOSED | retained prepared candidates bypass generic settlement and replay under the same identity |
| `FIND-TASK-004-1` | CLOSED | canonical values/default are hyphenated/`small-files` across public projections |
| `FIND-TASK-004-2` | CLOSED | Rust SDK names the canonical enum through `wyrd_sdk::bifrost` |
| `FIND-TASK-005-R1-1` | CLOSED | leader stream owns lifecycle; drop synchronously revokes follower grants and local drivers before claim release |
| `FIND-TASK-005-R1-2` | CLOSED | each acquisition derives remaining duration from one immutable deadline |
| `FIND-TASK-005-R1-3` | **OPEN** | live capability exists, but timeout/cancellation releases it while submitted effect acceptance is explicitly unknown |
| `FIND-TASK-005-R1-4` | CLOSED | sibling-strategy ban removed |
| `FIND-TASK-005-R1-5` | CLOSED | production durable layout SQL moved to `vala-sql` |
| `FIND-TASK-006-1` | CLOSED | runtime/source/generated parity inventory reconciled and top-level tested |
| `FIND-TASK-006-2` | CLOSED | after-hook abort paths return documented outcomes without `unreachable!` |
| `FIND-PACKET-1` | CLOSED | changed-symbol documentation audit and lints recorded clean |
| `FIND-PACKET-2` | CLOSED | changed-range import/signature shape audit recorded clean |
| `FIND-PACKET-3` | CLOSED | cumulative diff check rerun clean |
| `FIND-PACKET-4` | CLOSED | broad gate and release benchmark recorded green on the corrected executable code |

## Cross-task seams and added behavior

| Seam / journey | Evidence | Result |
|---|---|---|
| Leader revocation -> promotion, pull/report, maintenance | one term-owned cancellation reaches each owner; focused two-replica journey exercises the seam | PASS |
| Dispatch -> shutdown -> leader retry | exact dispatched close prevents fair claim and one `NotStarted` report returns eligibility | PASS |
| Prepared cleanup -> reader refusal -> later replay | retained frontier/attempt feeds the prepared reconciliation route without generic retry | PASS |
| Leader stream -> follower revocation -> claim release | structural drop order plus test-support event ordering; supervisor retains capacity residue only, not query lifetime | PASS |
| Active-read acquisition -> destructive external effect | unknown-acceptance paths reopen the forbidden gap | FAIL (`BEH-REREVIEW-001`) |
| Forge failure -> same-pod recovery | `restarting_worker` rebuilds scheduler/worker with bounded backoff; each loop's readiness guard retracts readiness before restart | PASS |
| New leader/worker/cleanup metrics | metrics are emitted at the owning acquisition/revocation/restart/refusal/settlement boundaries and do not alter public contracts | PASS |
| Packet scope | unrelated Bifrost-variant draft is committed in the candidate | FAIL (`BEH-REREVIEW-002`) |

## Verification limits

- I reviewed the complete cumulative name/diff inventory and the remediation
  delta, traced each prior finding through its current producer and consumer,
  and reran `git diff --check c1508b375..7fcb45fc1` (exit 0).
- I did not execute binaries from the later checked-out worktree as evidence for
  this immutable candidate. I reviewed the exact focused commands/results
  recorded in each remediation task and the final 16/16 benchmark plus
  `mise run gate` result.
- The existing destructive-race tests cover a reader while authority is held
  and a reader after a definite effect result. They do not cover a reader after
  authority release while external acceptance remains unknown, which is the
  precise remaining gap.

## Overall result

**FAIL**

The candidate closes 16 of the 17 prior findings. `FIND-TASK-005-R1-3`
remains reachable on timeout/cancellation, and the immutable range also carries
one unrelated change requiring a new packet finding.
