# Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Base (excluded): `c1508b375`
- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Approved authority: `changes/active/forge-concurrent-planning/spec.md`, revision 11
- Pinned RisingWave reference: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`

The candidate identity was checked before source validation. The only working-tree
changes observed were review artifacts under this review directory.

## Validation method and consolidation

I read the approved specification and revision history, task index and task
artifacts, repository authorities selected by the navigation map, the complete
discovery report set, and the cited production owners, callers, sibling
consumers, migrations, and tests. The ledger below retains only reachable
violations required by the approved packet or repository rules.

Discovery IDs that describe the same producer and failed invariant are
consolidated:

- `CONC-001`, `CONC-002`, and `SYS-001` share the same missing revocable leader
  lifetime and are one correction.
- `INV-REV-002`, `SYS-002`, and `DATA-DUR-002` are one deadline-refresh defect.
- `INV-REV-001` and `FUP-001` are one abandoned analytical ownership defect.
- `DATA-DUR-001` and `FUP-002` are one authority-lifetime defect.
- `INV-REV-004`, `D-SDK-002`, and `MNT-001` are one canonical compaction
  contract defect.
- `BEH-003`, `INV-REV-005`, `D-SDK-003`, `MNT-002`, and `MNT-003` share the
  incomplete Python runtime/declaration audit, while `D-SDK-004` remains a
  distinct reachable callback-lifecycle contradiction.
- `MNT-004` is subsumed by `STD-001`; `MNT-005` is subsumed by `STD-002`.

No proposed finding required a new product, public API, tenancy, security,
compatibility, or persistent-data decision. The retained corrections stay
inside the approved behavior. None therefore requires `SPEC_REVISION_REQUIRED`.

## Final validated finding ledger

### FIND-TASK-001-1 — CONFIRMED — INCORRECT

- **Discovery sources:** `CONC-001`, `CONC-002`, `SYS-001`.
- **Violated obligation:** REQ-001 and INV-001 require one active scheduling
  leader and require loss of leadership to stop dispatch and leader-timer work.
  The 30-second election lease is independent of publication duration.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/scheduler.rs:168-171,206-224,337-350,366-391`;
  `crates/vala/vala-bifrost-redux/src/forge/leadership.rs:168-205,235-344`;
  `crates/vala/vala-bifrost-redux/src/forge/gc.rs:152-190`;
  `crates/vala/vala-sql/src/queries/forge_leader.rs:105-127`.
- **Producer-to-consumer evidence:** the only heartbeat loop awaits hinted
  promotion and the full sequential promotion-debt sweep. Either may await
  catalog, object-store, and SQL IO past `LEADER_TERM`. PostgreSQL can then
  elect a successor while the old process still has `held: Some`. `term()`,
  notify, pull, and report consult only that cached value. Separately,
  `run_maintenance` clones the held `Arc` once; clearing the leadership slot
  does not revoke the pass, whose loops check only process shutdown.
- **Observable consequence:** two replicas can dispatch concurrently, and a
  replaced leader can continue a stale maintenance pass over the dead term's
  membership. Downstream task/table fences reduce corruption risk but do not
  satisfy single-leader scheduling or timer ownership.
- **Decision-complete minimal correction:** separate PostgreSQL renewal from
  promotion IO at the existing `ForgeLeadership` owner and make each
  `ForgeHeldTerm` revocable. Clearing or replacing the held term must revoke the
  exact term, and promotion, peer handlers, and maintenance must observe that
  revocation before dispatch and before each durable effect. Reuse the current
  term token, schedule owner, and cancellation machinery; add no second lease,
  scheduler, or durable schedule.
- **Focused closure proof:** hold a real hinted/debt promotion beyond the lease
  while a standby contends, prove the standby acquires a larger token and the
  former leader refuses notify/pull/report. In the same two-replica journey,
  revoke a term while maintenance is paused and prove the old pass performs no
  later rewrite, expiration, or cleanup effect.

### FIND-TASK-002-1 — CONFIRMED — INCORRECT

- **Discovery source:** `CONC-003`.
- **Violated obligation:** REQ-004/006 and INV-003 require accepted
  leader-dispatched work to remain owned by the pull/report protocol and never
  become an independently fair-claimable durable task.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:2703-2759,2627-2655,3025-3061,3587-3606`;
  `crates/vala/vala-sql/src/queries/forge_tasks.rs:159-236,593-644`.
- **Producer-to-consumer evidence:** `admit_dispatch` commits `insert_claimed`,
  inserts the dispatch bookkeeping, and only then calls `start_claim`. Shutdown
  observed there calls generic `release_claim_at_shutdown`, which uses `retry`
  and changes the row to `retryable`. `claim_fair` can then hand it to an
  unrelated worker while the leader still considers the table in flight. The
  existing `close_dispatched` transition exists specifically to prohibit this
  state but is bypassed on this edge.
- **Observable consequence:** one accepted dispatch can acquire a second owner
  outside leader result reporting, followed by a later leader retry of the same
  table.
- **Decision-complete minimal correction:** at the post-insert/pre-episode
  shutdown edge, reuse `close_dispatched` and the existing dispatch report path.
  Close the exact accepted attempt without making it `retryable`, remove its
  dispatch bookkeeping once, and report `NotStarted` (or the already-established
  equivalent pre-effect outcome) exactly once. Do not add a second admission
  state or queue.
- **Focused closure proof:** pause after `insert_claimed`, signal shutdown, and
  prove the row is never fair-claimable, dispatch bookkeeping closes, and the
  leader becomes eligible through one report only.

### FIND-TASK-002-2 — CONFIRMED — MISSING

- **Discovery sources:** `BEH-002`, `FUP-003`.
- **Violated obligation:** `tasks/README.md:45-56` and TASK-002's mandatory
  pinned nimtable/fork review require a completed current-pin row for Full,
  SmallFiles/FilesWithDelete, Auto, the noncommitting seam, governor/spill, and
  cancellation/loose outputs. An empty row blocks completion.
- **Exact location:**
  `changes/active/forge-concurrent-planning/tasks/TASK-002-pull-and-worker-results.md:118-145,340-367`;
  `changes/active/forge-concurrent-planning/evidence/TASK-002-fork-review.md`;
  `Cargo.toml:243`; `Cargo.lock:4745-4748`.
- **Evidence:** the implementation report fills a scheduler/worker comparison,
  not the mandatory six-row physical-planner/fork table. The separate analysis
  compares `74bdc45` with old fork pin `6773e19`, contains outstanding proof
  statements, and does not reconcile the shipped `ef97aea` pin or every retained
  fork-only module with a live consumer and failure test.
- **Observable consequence:** the packet does not establish that the shipped
  fork retains only the three approved Wyrd differences.
- **Decision-complete minimal correction:** this is evidence-only unless the
  comparison finds a concrete mismatch. Complete the existing mandatory table
  against exact revisions `74bdc45`, `6773e19`, and shipped `ef97aea`, naming
  the Forge consumer, executed focused result, and retained/deleted disposition
  for every row and fork-only module. Do not add a new comparison artifact or
  change production code merely to satisfy the table.
- **Focused closure proof:** every required row is non-empty and source-checked;
  every retained fork-only behavior names a production consumer and a test that
  fails if it is removed.

### FIND-TASK-003-1 — CONFIRMED — REGRESSION

- **Discovery source:** `INV-REV-003`.
- **Violated obligation:** prepared expired-cleanup candidates must retain their
  exact durable identity after refusal/uncertain deletion and replay safely,
  without entering a transition that no longer owns them.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:7752-7827,7934-7957,8505-8544`;
  `crates/vala/vala-sql/src/queries/forge_tasks.rs:654-677,1186-1252,1255-1269`.
- **Producer-to-consumer evidence:** a fresh active read or another refreshed
  root can make `prove_cleanup_candidate` return an error. The worker normalizes
  that to `ExpiredCleanupOutcome::Refused`; settlement intentionally leaves the
  row `prepared`; `drain_expired_cleanup` then returns
  `ForgeError::Reconciliation`. For an ordinary fair-claimed cleanup,
  `settle_execution_failure` invokes `retry_failure`, whose predicate accepts
  only `claimed`/`running`, producing `retry failure lost attempt ownership`.
  Dispatched attempts avoid that exact SQL error but still rely on outer report
  handling while retaining the prepared row.
- **Observable consequence:** a safe refusal is turned into a slot-fatal
  settlement failure and immediate same-identity replay is abandoned until a
  later recovery/lease path.
- **Decision-complete minimal correction:** represent the existing retained
  prepared outcome explicitly at the worker settlement boundary. A refusal or
  uncertain result that deliberately leaves `prepared` must bypass generic
  retry/terminal transitions and return to the existing prepared-candidate
  reconciliation path under the same task/attempt identity. Add no new durable
  state or retry ledger.
- **Focused closure proof:** prepare a candidate, introduce a fresh active read
  before delete proof, observe refusal without a settlement conflict, release
  the reader, and prove the same identity replays and converges.

### FIND-TASK-004-1 — CONFIRMED — INCORRECT

- **Discovery sources:** `INV-REV-004`, `D-SDK-002`, `MNT-001`.
- **Violated obligation:** REQ-012 accepts `auto`, `full`, `small-files`, and
  `files-with-delete`; REQ-013 makes omission resolve to `small-files`. Public
  contracts, SDKs, generated schemas, and documentation must agree.
- **Exact location:**
  `crates/wyrd-spec/src/vala/api.rs:143-168,411-419`;
  `crates/shared/wyrd-client/src/bifrost/table.rs:60-65,233-248`;
  Python/TypeScript compaction projections and tests; generated Bifrost request
  and description schemas.
- **Producer-to-consumer evidence:** `CompactionTypeWire` uses
  `#[serde(rename_all = "snake_case")]`, so the public wire and SDKs expose
  `small_files`/`files_with_delete`, while the approved contract names
  hyphenated values. The same canonical rustdoc says omission and `Full` are the
  default, while `ForgeTableSettings::default()` correctly uses `SmallFiles`.
  Code generation faithfully multiplies both contradictions.
- **Observable consequence:** public callers cannot send the approved values,
  and Rust/schema consumers are told that omission rewrites all live files when
  production uses grouped small-file selection.
- **Decision-complete minimal correction:** correct the canonical
  `wyrd-spec` enum serialization and omitted-default rustdoc, then update the
  existing Rust, Python, and TypeScript projections through their normal source
  owners and regenerate schemas/stubs/declarations. Preserve the internal
  Iceberg property spelling; add no alias for the unshipped snake-case wire.
- **Focused closure proof:** schema assertions and first-class Rust, Python,
  and TypeScript registration journeys accept/report the two hyphenated values,
  reject the old underscore spellings, and document omission as `small-files`.

### FIND-TASK-004-2 — CONFIRMED — MISSING

- **Discovery source:** `D-SDK-001`.
- **Violated obligation:** REQ-012 requires the compaction type through the
  first-class Rust SDK, not only the shared implementation crate.
- **Exact location:**
  `crates/shared/wyrd-client/src/bifrost/table.rs:233-248`;
  `crates/shared/wyrd-client/src/bifrost/mod.rs:51-69`;
  `sdks/wyrd-sdk-rust/src/lib.rs:27`; `sdks/wyrd-sdk-rust/Cargo.toml:15-18`;
  `crates/wyrd/wyrd-testing/tests/bifrost/forge/live_rewrite.rs:9`.
- **Evidence:** `TableConfig` publicly accepts and returns
  `CompactionTypeWire`, but `wyrd_client::bifrost` does not re-export it.
  `wyrd-sdk-rust` re-exports `wyrd-client` and has no direct `wyrd-spec`
  dependency. The claimed journey imports the enum directly from `wyrd_spec`,
  bypassing the SDK contract.
- **Observable consequence:** an ordinary `wyrd-sdk-rust` user cannot name the
  argument type without adding an implementation-level dependency.
- **Decision-complete minimal correction:** re-export the existing canonical
  enum from the existing public `wyrd_client::bifrost` surface, allowing the
  thin SDK re-export to expose it. Do not add a wrapper enum or SDK dependency.
- **Focused closure proof:** a `wyrd-sdk-rust` compile/API test and the Rust
  registration journey import the enum only through `wyrd_sdk::bifrost`.

### FIND-TASK-005-R1-1 — CONFIRMED — INCORRECT

- **Discovery sources:** `INV-REV-001`, `FUP-001`.
- **Violated obligation:** REQ-014, INV-005/009, TASK-005-R1 Scenario 2, and its
  stop condition require no metadata/file IO after the cut-and-claim owner
  releases and require every analytical descendant to stop first.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:80-153`;
  `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:401-504,608-657`;
  `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2459-2483,2683-2773`;
  `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:1183-1247,1342-1370,1549-1556`;
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:4740-4757`.
- **Producer-to-consumer evidence:** terminal stream settlement joins
  descendants before awaiting `ActiveReadClaim::release`, but dropping the
  stream drops the claim and independently spawns row deletion. Analytical
  drop only signals cancellation; the supervisor must still join attempts,
  drivers, exchanges, grants, and envelope children. No ownership edge makes
  claim deletion wait for that background settlement. The existing journey
  releases the paused follower before checking row disappearance, so it cannot
  prove the contested ordering.
- **Observable consequence:** Forge can observe no active row and expire or
  delete a selected object while an abandoned analytical descendant is still
  stopping and can still perform IO.
- **Decision-complete minimal correction:** on analytical abandonment, transfer
  the existing `ActiveReadClaim` into the existing supervisor-owned graph
  settlement. That settlement must cancel and join descendants before it
  releases the rows. Preserve nonblocking caller drop and the deadline fallback;
  the interactive/no-descendant path may keep direct spawned release.
- **Focused closure proof:** hold a follower stopped-but-not-joined after the
  leader stream is dropped; prove the active row remains and all Forge
  destructive paths refuse, then allow follower settlement and observe release.

### FIND-TASK-005-R1-2 — CONFIRMED — INCORRECT

- **Discovery sources:** `INV-REV-002`, `SYS-002`, `DATA-DUR-002`.
- **Violated obligation:** revision-11 REQ-014/AC-009 require each row to expire
  at the query's own deadline. Oracle binds the remaining duration for each
  acquisition and PostgreSQL derives `abandon_after` from statement time.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1887-1920`;
  `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:173-219`;
  `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:293-305`.
- **Producer-to-consumer evidence:** `prepare_query_attempt` computes one
  duration before pool/authority acquisition and copies it into
  `ActiveReadOwner`. The allowed metadata-`NotFound` reacquisition reuses that
  duration. SQL writes `statement_timestamp() + p_deadline_ms` and refreshes it
  on conflict, so acquisition latency extends the first row and reacquisition
  extends it again.
- **Observable consequence:** a crashed query can block destructive maintenance
  beyond its only authorized deadline.
- **Decision-complete minimal correction:** retain one immutable local deadline
  already owned by the attempt and derive its positive remaining duration
  immediately before every acquisition, including the one reacquisition. Bind
  only that duration; PostgreSQL remains the authority that stamps the row.
  Reject acquisition when no duration remains. Add no cap or alternate clock.
- **Focused closure proof:** delay first acquisition/materialization, force the
  one metadata reacquisition, then abandon without release and prove from
  PostgreSQL time that protection ends at the original deadline, not one later
  interval after reacquisition.

### FIND-TASK-005-R1-3 — CONFIRMED — INCORRECT

- **Discovery sources:** `DATA-DUR-001`, `FUP-002`.
- **Violated obligation:** REQ-007/014, INV-005/006/009, AC-006/009, and
  TASK-005-R1 Scenarios 1 and 3 require one ordering between cut acquisition and
  the destructive effect: a committed reader blocks preparation and commit, or
  destruction completes first and the reader receives the later pointer.
- **Exact location:**
  `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:275-305`;
  `crates/vala/vala-sql/src/queries/forge_operations.rs:891-959`;
  `crates/vala/vala-sql/src/queries/forge_tasks.rs:1186-1252`;
  `crates/vala/vala-bifrost-redux/src/forge/expire.rs:195-257,674-724`;
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:7842-7964`;
  `crates/vala/vala-bifrost-redux/src/forge/orphan_gc.rs:1041-1156,1395-1453`.
- **Producer-to-consumer evidence:** Oracle share-locks the maintenance row while
  selecting the pointer and inserting active rows. Every Forge path takes the
  exclusive lock and checks readers only in a transaction that commits before
  the external catalog mutation or object deletion. A reader can therefore
  acquire the released lock, commit the old cut and row, and start IO before the
  already-authorized destructive effect. Forge's table lease does not serialize
  Oracle. Existing tests cover reader-before-check, not reader-after-check and
  before-effect.
- **Observable consequence:** snapshot expiration can commit beside a reader of
  the pre-expiry cut, and cleanup can delete after a new active row committed.
- **Decision-complete minimal correction:** keep the existing per-table
  maintenance authority as the sole ordering mechanism, but extend its
  exclusive ownership continuously from the final active-read check through
  the corresponding catalog/object effect's known outcome. The SQL owner must
  own that scope; do not recreate reader epochs, an IO gate, an advisory lock,
  a per-query session, or a second persisted coordination protocol. Preserve
  current uncertain-effect evidence and idempotent recovery.
- **Focused closure proof:** deterministically pause snapshot expiration,
  expired cleanup, and orphan cleanup after their current last reader check but
  before effect; race acquisition and prove exactly one ordering, including the
  post-expiry pointer in the destruction-first case and no delete while a
  committed active row exists.

### FIND-TASK-005-R1-4 — CONFIRMED — VIOLATION

- **Discovery source:** `BEH-001`.
- **Violated obligation:** revision 11 permits immediate sibling expiration and
  expired cleanup once no reader/root remains. Tests may not forbid approved
  behavior to make a route-specific assertion pass.
- **Exact location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/forge/production_closeout.rs:496-544`.
- **Evidence:** `assert_orphan_evidence` queries every Forge task strategy for
  the tenant and fails if any `snapshot_expiry` or `expired_cleanup` task ran.
  The journey deliberately executes up to twelve full maintenance passes, so a
  valid sibling route for either table is reachable. The equivalent stale ban
  was removed from `production_routes.rs` in `05cceaf35` for this revision-11
  reason.
- **Observable consequence:** the production closeout journey can reject a
  correct immediate-maintenance result and is not a trustworthy packet gate.
- **Decision-complete minimal correction:** delete the tenant-wide
  sibling-strategy prohibition. Retain exact orphan operation identity,
  terminal phase, physical deletion, survivor, and query-result assertions.
  If route attribution needs tightening, scope it to the exact orphan
  operation/object already under test rather than banning independent work.
- **Focused closure proof:** run the closeout journey with an independently
  eligible snapshot-expiry sibling and prove exact orphan evidence still passes.

### FIND-TASK-005-R1-5 — CONFIRMED — VIOLATION

- **Discovery source:** `MNT-006`.
- **Violated obligation:** AGENTS.md section 15 assigns durable PostgreSQL
  access to the SQL layer, and repository structure already owns
  `vala.bifrost_tables` reads in `vala_sql::queries::olap_catalog`.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/table_authority.rs:45-72`;
  existing owner `crates/vala/vala-sql/src/queries/olap_catalog.rs:76-90`.
- **Evidence:** the new Redux wrapper embeds a raw `vala.bifrost_tables` query
  and decodes durable UID bytes before delegating the rest of the authority
  work to `vala-sql`. This duplicates the existing table-registry SQL owner and
  splits schema/decoding knowledge across layers.
- **Observable consequence:** registry schema or identity changes can update
  the SQL owner while leaving the Redux copy stale; every authority caller must
  compose two persistence owners for one lookup.
- **Decision-complete minimal correction:** delete the Redux raw query and reuse
  `vala_sql::queries::olap_catalog::get_by_fqn` (or the existing SQL authority
  owner if it already provides the exact typed projection), then construct the
  authority identity from that typed row. Keep `TableAuthority` only as the
  transaction-scoped Forge orchestration handle; add no repository trait.
- **Focused closure proof:** existing authority and tenant-isolation tests pass,
  and no raw `bifrost_tables` query remains outside the SQL owner.

### FIND-TASK-006-1 — CONFIRMED — INCORRECT

- **Discovery sources:** `BEH-003`, `INV-REV-005`, `D-SDK-003`, `MNT-002`,
  `MNT-003`.
- **Violated obligation:** TASK-006 requires every public Python callable and
  typed structure to have accurate runtime/source-stub/generated-stub
  documentation and requires discovered typing drift to be fixed at its source.
- **Exact location:**
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:15-64,247-380`;
  `sdks/wyrd-sdk-python/python/wyrd/stubs/error.pyi:7-90`;
  generated `wyrd/agent/__init__.pyi` and error projection;
  runtime `crates/skald/skald-agent/src/session.rs:30-60`,
  `crates/skald/skald-agent/src/python.rs:387-558`, and
  `crates/shared/wyrd-utils/src/py.rs:13-21`.
- **Producer-to-consumer evidence:** the source stub omits `Role.System`; gives
  `SessionTurn` positional `role/content`, `tool_call_id`, `Role`-typed getter,
  and `to_dict`; runtime requires keyword-only arguments, exposes `call_id` and
  string role, and provides the `model_*` methods. The Agent stub omits runtime
  mutators/callback registration methods. Error stubs advertise keyword-only
  structured constructors while the runtime classes are ordinary PyO3
  exceptions; their own docstring says direct construction only stores
  positional `args`. Generated files merely copy these incorrect sources. The
  task evidence itself lists these disagreements while marking the criterion
  PASS.
- **Observable consequence:** type checking approves calls runtime rejects,
  rejects supported calls, and hides public methods; editor help and runtime do
  not describe one API.
- **Decision-complete minimal correction:** reconcile the complete
  task-recorded public drift inventory at each existing hand-authored source,
  regenerate outputs, and describe actual runtime behavior where no runtime
  contract change is authorized. Remove invented exception constructors and
  expose the actual Agent/session surface. Do not hand-edit generated stubs or
  redesign the generator.
- **Focused closure proof:** top-level Python runtime/typing parity tests cover
  `Role.System`, `SessionTurn` construction/properties/model methods, Agent's
  public mutators, and direct versus builder-created `WyrdError`; codegen and
  typecheck then pass with no unresolved inventory row.

### FIND-TASK-006-2 — CONFIRMED — INCORRECT

- **Discovery source:** `D-SDK-004` (also part of `INV-REV-005`).
- **Violated obligation:** TASK-006 documentation may not promise behavior the
  implementation lacks.
- **Exact location:**
  `crates/skald/skald-agent/src/python.rs:84-89,797-835`;
  `crates/skald/skald-agent/src/callbacks.rs:81-167`;
  `crates/skald/skald-agent/src/loop_runtime.rs:357-365,405-414,492-502`.
- **Producer-to-consumer evidence:** Python exceptions become
  `CallbackOutcome::Abort`; the chain functions return `ChainResult::Abort`;
  `after_model`, `after_agent`, and `after_tool` consumers then execute
  `unreachable!`. Native help tells users each callback may raise to abort.
- **Observable consequence:** following public help can panic the agent run
  instead of producing the documented callback-aborted result.
- **Decision-complete minimal correction:** because TASK-006 authorizes
  documentation/projection repair but not an unapproved runtime contract
  change, remove the unsupported raise-to-abort claim from native help and
  source/generated stubs and document the behavior actually supported by the
  callback owner. If an existing authoritative callback contract elsewhere
  requires abort, correct the shared `loop_runtime` owner once rather than add
  Python-wrapper guards; otherwise do not broaden this task into callback
  redesign.
- **Focused closure proof:** top-level Python tests raise from each after-hook
  and assert the documented result; no test may accept an undocumented process
  panic as successful behavior.

### FIND-PACKET-1 — CONFIRMED — VIOLATION

- **Discovery sources:** `STD-001`, `MNT-004`.
- **Violated obligation:** AGENTS.md section 16 and `architecture/agent-rules.md`
  require substantive rustdoc on every new or materially modified Rust item,
  including private items/tests, `# Errors` on fallible functions, `# Panics`
  where applicable, and cancellation/partial-progress behavior for durable
  async workflows.
- **Exact representative locations validated:**
  `catalog/bifrost_catalog.rs:823-850` (`ensure_builtin`, no `# Errors`);
  `catalog/wire.rs:14-23` (`reject_reserved_field_names`, no `# Errors`);
  `forge/expire.rs:188-206` (durable fallible async workflow missing errors and
  cancellation/partial-progress contract);
  `forge/orphan_gc.rs:1000-1005` (no method rustdoc or `# Errors`);
  `oracle/query_stream.rs:401-504` (`build_frames`, no rustdoc while ownership
  prose remains on its neighbor);
  `wyrd-server/src/grpc/forge_peer.rs:91-150` (three new fallible methods with
  no `# Errors`);
  `scribe/staging_runtime.rs:175-181` (new associated `Target` item without
  rustdoc);
  `wyrd-spec/src/vala/assignment_authority.rs:270-291` (materially changed
  fallible digest helpers without rustdoc/`# Errors`), plus the changed test and
  constant examples recorded by `STD-001`.
- **Evidence:** the examples are new or semantically changed in the reviewed
  range. This is not a linter preference; the repository explicitly classifies
  missing changed-item rustdoc as `BLOCK_BEFORE_MERGE`.
- **Observable consequence:** failure, panic, cancellation, and partial-progress
  contracts are absent at the exact lifecycle-sensitive owners changed by the
  packet.
- **Decision-complete minimal correction:** document the existing items in
  place and re-audit every changed Rust symbol in the range. Move misplaced
  ownership prose to the item it describes. Add no lint suppression, wrapper,
  or documentation-only helper.
- **Focused closure proof:** a changed-symbol documentation audit is empty,
  followed by format, lints, and the owning focused tests.

### FIND-PACKET-2 — CONFIRMED — VIOLATION

- **Discovery sources:** `STD-002`, `MNT-005`.
- **Violated obligation:** `architecture/agent-rules.md` requires module-scope
  imports and bare dependency names in signatures, fields, impl heads, and
  bounds, except the documented narrow trait-import case.
- **Exact representative locations validated:**
  `forge/settings.rs:96-110` (fully qualified impl/signature and function-local
  import); `oracle/planner.rs:88-125` (fully qualified field/signature types);
  `oracle/exec.rs:3183-3188` (qualified signature plus local imports) and
  multiple function-local test imports; `scribe/persistence.rs:2190-2192`;
  plus the external journey and other qualified signatures listed by
  `STD-002`.
- **Evidence:** these imports/signatures were introduced or materially changed
  in the candidate. Test modules may have their own top-level imports; imports
  nested in individual test functions are not that exception.
- **Observable consequence:** module dependency manifests are incomplete and
  equivalent contracts are spelled in multiple forms, increasing search and
  refactor cost across already-large owners.
- **Decision-complete minimal correction:** move each changed local import to
  the owning module or test-module import block and use bare names consistently
  in changed signatures/fields/impls. Preserve only an actual `Trait as _`
  narrow-use exception. Add no aliases solely to evade the rule.
- **Focused closure proof:** a complete changed-range scan finds no disallowed
  local import or qualified signature, then format and lints pass.

### FIND-PACKET-3 — CONFIRMED — VIOLATION

- **Discovery source:** `STD-003`.
- **Violated obligation:** AGENTS.md section 12 requires a clean
  `git diff --check`.
- **Exact location:**
  `changes/active/forge-concurrent-planning/revision/TASK-005-R1-implementation-reference.md:464`;
  `changes/active/forge-concurrent-planning/tasks/TASK-003-maintenance-and-removal.md:307`.
- **Evidence:** `git diff --check c1508b375..7ac45dec9` was rerun and exited 2
  for a new blank line at EOF in both files.
- **Observable consequence:** the immutable candidate contradicts its recorded
  clean-diff claim and fails a required completion gate.
- **Decision-complete minimal correction:** remove only the two extra EOF blank
  lines.
- **Focused closure proof:** `git diff --check c1508b375..<new-candidate>` exits
  zero.

### FIND-PACKET-4 — CONFIRMED — MISSING

- **Discovery source:** `STD-004`, confirmed by the focused follow-up.
- **Violated obligation:** AGENTS.md section 11 selects `mise run gate` for an
  intentionally broad change and for shared CI/build/test-infrastructure
  changes.
- **Exact location/evidence:** the range spans SQL, Vala runtime, server,
  contracts, three SDKs, generated artifacts, docs, and test infrastructure and
  changes `mise.toml` with the Forge capacity benchmark. The packet and supplied
  verification record `verify:bifrost` and specialized lanes but no
  `mise run gate` result.
- **Observable consequence:** repository-wide composition outside the scoped
  Bifrost lanes is unproved.
- **Decision-complete minimal correction:** after source remediation, run the
  single required aggregate `mise run gate` and record the result. Do not rerun
  its component lanes as additional final commands; retain only specialized
  proof shown to be outside the aggregate.
- **Focused closure proof:** one green aggregate result on the corrected
  immutable candidate, with any out-of-aggregate specialized lane named
  explicitly.

## Rejected discovery claims

No proposed finding was rejected outright. Apparent disagreements were resolved
as follows:

- The concurrency/security pass statements that dropped analytical claims are
  safe were rejected as conclusions, not retained as findings: source shows no
  ownership edge from `ActiveReadClaim::drop` to supervisor settlement, and the
  journey releases the paused follower before checking claim disappearance.
- The behavior/concurrency pass statements that an active-read check alone
  protects every destructive path were rejected as conclusions: the authority
  transaction ends before each external effect.
- The invariant review's compaction spelling/default issue was retained after
  direct comparison with revision-11 REQ-012/013; the implementation's
  repository-wide snake-case convention cannot override the approved explicit
  values.
- Discovery findings merged into a stable `FIND-*` above are duplicates or
  narrower manifestations, not rejected claims.

## Task impact mapping

| Task | Validated findings | Result forced by ledger |
|---|---|---|
| TASK-001 leader and promotion | `FIND-TASK-001-1`; `FIND-PACKET-1`, `FIND-PACKET-2`, `FIND-PACKET-4` | FIX_REQUIRED |
| TASK-002 pull and worker results | `FIND-TASK-002-1`, `FIND-TASK-002-2`; cross-task standards/verification findings | FIX_REQUIRED |
| TASK-003 maintenance and removal | `FIND-TASK-003-1`; `FIND-TASK-005-R1-3`, `FIND-TASK-005-R1-4`; cross-task findings | FIX_REQUIRED |
| TASK-004 compaction defaults and type | `FIND-TASK-004-1`, `FIND-TASK-004-2`; cross-task findings | FIX_REQUIRED |
| TASK-005 Iceberg filtering scenarios 0-3 | No filtering-specific finding; cross-task repository findings still affect the cumulative candidate | PASS task behavior, packet not acceptable |
| TASK-005-R1 active-table reader cut | `FIND-TASK-005-R1-1` through `FIND-TASK-005-R1-5`; cross-task findings | FIX_REQUIRED |
| TASK-006 Python API docstrings | `FIND-TASK-006-1`, `FIND-TASK-006-2`; cross-task findings | FIX_REQUIRED |

## Validated decision

The deduplicated ledger is non-empty. Every retained item has a bounded
correction within existing approved owners and behavior. The cumulative
candidate therefore requires remediation; no specification revision is needed.
