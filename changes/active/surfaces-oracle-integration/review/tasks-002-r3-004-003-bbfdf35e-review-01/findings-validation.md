# Wave 2 Structured Ponytail Validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Reviewed tasks: original `TASK-002`, its prior reviews and `TASK-002-R3`, `TASK-004`, and `TASK-003`

The candidate commit and tree matched the supplied identities before source
inspection and again after this report was written. The repository has no
`.codegraph/` directory, so caller tracing used `rg`, Git, and complete source
bodies. Concurrent untracked review output and the unrelated untracked
`changes/active/verified-change-contract/architecture/verifier/` directory are
not candidate evidence.

## Validation scope and authority resolutions

Current user authority narrows this validation to material production defects,
normal public workflows, realistic high-probability cases, and explicit
security, data-loss, durability, or acceptance invariants. Style-only defects,
rare unintended internal use, speculative checks, and future final-review work
are rejected even when a broader standards pass would normally mention them.
That instruction changes finding selection, not the reviewed task's explicit
durability and acceptance obligations.

After the owner challenged remediation complexity, the ledger was rechecked
against the existing owners. The connection semaphore cannot also be the
pending bound without reducing ordinary burst capacity to one quarter of the
pool and dropping healthy audit writes. `FIND-TASK-003-1` therefore keeps that
connection limit and uses one separate native semaphore for total pending work;
a custom counter or worker queue would be larger. `FIND-TASK-003-3` no longer
schedules `test:storage:matrix` separately because nightly's existing `gate`
already reaches it through `test:rust`.

The following conflicts are resolved before evaluating individual proposals:

- The owner expressly accepts the 22 historical AI co-author trailers without
  rewriting history. `FIND-TASK-002-18` remains closed.
- Approved specification revision 8 and the current `AGENTS.md` and
  `bifrost-design.md` audit rules supersede older Oracle audit-WAL/relay prose.
  The stale prose is drift; the tracked non-blocking `vala.audit_staging`
  commit is the required implementation model.
- `TASK-002-R3` expressly permits the Python SDK's package-local release
  profile for this cumulative candidate. The profile warning is not reopened
  as a finding.
- A task review cannot require the future final `$wyrd-change-review` as its own
  remediation. AC-009 is proved by that later workflow after task approval.
- REQ-064 and AC-022 do explicitly require final-candidate local evidence and
  credentialed live-cloud evidence. Their absence is an acceptance gap even
  though this read-only reviewer cannot manufacture it locally. No equivalent
  hosted nightly or performance run is required by AC-022: nightly topology is
  reviewed statically, the production-geometry lane is the performance proof,
  and live-cloud is the only mandated hosted pass.
- The checked-in `wyrd.v1.bin` is consumed only by `check:proto-drift`; normal
  compilation generates its descriptor in `OUT_DIR`, runtime reflection
  registers only `tonic_health::pb::FILE_DESCRIPTOR_SET`, and MCP does not read
  the snapshot. The Wave-1 claim that the orphan check protects those runtime
  consumers is false. The check passed independently in Wave 1, so wiring a
  dead snapshot check into every aggregate is not a material current defect.
- No detached-worker correction is retained. The stream reviewer retracted it,
  and independent inspection found no production path requiring a detached
  stage runtime or worker.

## Complete Wave 1 disposition

| Wave 1 source ID | Disposition | Final finding | Independent validation |
|---|---|---|---|
| `CLIENT-CONTRACT-001` | **CONFIRMED** | `FIND-TASK-002-19` | The source types and generated files have no runtime consumer, explicitly advertise two prohibited MCP tools and an obsolete query/layout shape, and are published in the generated schema inventory. The runtime catalog contains exactly the approved three tools and all live query callers use `BifrostQueryRequest`, so deletion is the correction. |
| `PERSIST-DATA-01` | **REVISED** | `FIND-TASK-004-1` | `BifrostDataRoot::prepare` is the shared production/test root owner. It creates four directories and write-opens only the root lock. An existing unusable stage/output child can pass until later durable role IO. That is an explicit boot/durability invariant and a realistic mounted-volume failure. Retain the writeability gap; reject the child-symlink extension as a rare operator-controlled configuration outside the 90% scope. |
| `PERSIST-DATA-02` | **REVISED** | `FIND-TASK-003-1` | Both `OracleAudit` methods call `stage`; every call spawns before awaiting the connection semaphore. `TaskTracker` and its retained `AuditEvent`s are unbounded across successive completed reads. Retain a finite admission correction on the existing owner, without a queue, WAL, relay, or knob. |
| `DRI-01` | **REVISED** | `FIND-TASK-003-2` | A generic Rust PR gets `os=[ubuntu]`; `rust-compat` correctly excludes the Linux entry already owned by `ci`, but `ci` runs only `mise run check`. The normal PR path therefore runs no Rust tests. Make that existing Linux owner run `test:rust` after `check` when it is not already running the full `gate`; no matrix or checker change is required. |
| `DRI-02` | **REVISED** | `FIND-TASK-003-3` | `nightly.yml` invokes `gate`, identity, and four Postgres checks. `gate` already reaches the storage-emulator matrix through `test:rust`, but omits real-server Card, CLI, WyrdState, Python harness/integration, and full TypeScript integration journeys. Invoke only those missing owners; do not add another aggregate or permanent topology checker. |
| `DRI-03` | **REJECTED** | — | The final-candidate local matrix is missing, but that is verification evidence rather than a source-proven production defect. Under the current scope it remains a verification limit for the orchestrator, not an implementation finding. |
| `DRI-04` | **REJECTED** | — | GitHub reports no candidate live-cloud run, but absence of a run is evidence debt rather than proof that the normal production implementation is wrong. Preserve it as a mandatory acceptance limit under REQ-064/AC-022, not a remediation finding. |
| `DRI-05` | **REJECTED** | — | Its claimed runtime consumers do not read the checked-in descriptor. The dedicated check passed in Wave 1 and TASK-002 records its focused pass. Adding dead snapshot ownership to `gate` would preserve complexity without closing a current production or acceptance failure. |
| `DRI-06` | **REJECTED** | — | It asks this task review to execute the future final `$wyrd-change-review`. That workflow follows task approval and owns the full revision-8 REQ/INV evidence map; requiring it here creates a review cycle rather than closing TASK-003. |
| `STREAM-LIFECYCLE-001` | **REJECTED** | — | Cancellation specifically during the private cleanup awaits is technically reachable, but ordinary Rust collection awaits settlement to completion, Python/TypeScript close paths drop rather than call it, and server deadline cleanup bounds the residue. The path is not a realistic common public workflow or a security/data-loss defect under the current 90% scope. Do not reopen `FIND-TASK-002-17`. |
| `STREAM-LIFECYCLE-002` | **REVISED** | `FIND-TASK-003-1` | Duplicate of the unbounded Oracle audit ownership in `PERSIST-DATA-02`. The detached-worker idea is not part of this finding and is not restored. |
| `REPO-001` | **REJECTED** | — | Nine routed passages are stale and contradict revision 8, but the production implementation follows the current canonical path. Under the current material-production scope, documentation-authority drift alone is not a retained implementation defect. It remains a documentation cleanup limit. |
| `REPO-002` | **REJECTED** | — | The cumulative `TASK-002-R3` authority expressly allows this package-local profile. Current owner authority resolves the repository-default disagreement; no history or manifest correction is authorized from this proposal. |
| `REPO-003` | **REJECTED** | — | The qualified spellings are real style-rule examples, mostly in tests and internal harness fields, but they do not alter production behavior. Under the current material-production/90% scope they do not reopen `FIND-TASK-002-8`. |
| `TASK-REV-01` | **REJECTED** | — | Duplicate of rejected `STREAM-LIFECYCLE-001`; the rare cancelled-cleanup interleaving does not meet the current threshold. |
| `TASK-REV-02` | **REVISED** | `FIND-TASK-004-1` | Duplicate of `PERSIST-DATA-01`; retain only the normal unusable-volume preflight gap, not symlink hardening. |
| `TASK-REV-03` | **REVISED** | `FIND-TASK-003-3` | Duplicate of `DRI-02`; retain missing nightly lane reachability, not absence of a hosted nightly run. |
| `TASK-REV-04` | **REJECTED** | — | Duplicate of `DRI-04`; preserve the required cloud pass as a verification limit rather than a source-remediation finding. |

## Final deduplicated finding ledger

### `FIND-TASK-002-19` — CONFIRMED — VIOLATION: generated public contracts advertise removed Bifrost surfaces

- **Wave-1 source:** `CLIENT-CONTRACT-001`.
- **Obligation:** TASK-002 generated-contract acceptance; spec REQ-057,
  REQ-059, INV-010, and INV-012 require the exact three MCP tools and coherent
  query/layout/error authority without stale public shapes.
- **Exact location:** `crates/wyrd-spec/src/vala/api.rs:271-306,428-456`;
  `crates/wyrd-spec/examples/gen_schemas.rs:60-68,208-233`; generated and
  golden `bifrost_{error_descriptor,permission_descriptor,query_param,sync_query_request,partition_column_spec,partition_transform}.json`; and
  `docs/src/content/docs/api/schemas.md:13`.
- **Caller and full-body evidence:** `BifrostErrorDescriptor` and
  `BifrostPermissionDescriptor` are used only by `gen_schemas` and say they are
  returned by prohibited `bifrost.list_errors` and
  `bifrost.list_permissions`. `QueryParam` and `SyncQueryRequest` are used only
  by schema generation and round-trip/schema tests. Every live HTTP, gRPC,
  client, CLI, SDK, and MCP query path uses `BifrostQueryRequest`; runtime MCP
  `descriptors()` returns exactly list, describe, and query. The two partition
  files have no Rust or generator owner, while `PhysicalLayoutWire` fixes the
  event-time layout. The docs inventory exposes all six stale shapes.
- **Observable consequence:** a client or agent following the checked-in
  machine-readable contract can implement nonexistent tools, omit required
  query controls in favor of `SyncQueryRequest`, or author the removed
  partition-column contract.
- **Decision-complete smallest correction:** delete the four zero-runtime
  source types and their tests/generator entries; delete all six stale generated
  and golden schema pairs; regenerate the docs inventory from the surviving
  schemas. Reuse `BifrostQueryRequest`, `PhysicalLayoutWire`, the derive-backed
  error catalog, and runtime MCP descriptors. Add no alias, compatibility
  route, tombstone schema, or cleanup framework.
- **Focused closure proof:** repository search finds none of the four type names,
  two removed MCP names, or orphan partition titles; `mise run codegen:check`,
  `mise run docs:check`, the owning `wyrd-spec` tests, and the existing MCP
  discovery journey pass with exactly three tools.

### `FIND-TASK-004-1` — REVISED — INCORRECT: root preflight does not prove managed children usable

- **Wave-1 sources:** `TASK-REV-02`, `PERSIST-DATA-01`.
- **Obligation:** spec REQ-055, INV-022, AC-018 and TASK-004 require every
  root-derived Scribe/Oracle path to be created and validated before any role
  activates, and an unusable or contradictory root to fail readiness.
- **Exact location:** `crates/wyrd/wyrd-server/src/boot/data_root.rs:71-108`;
  production caller `boot/mod.rs:428-491`; test-harness caller
  `crates/wyrd/wyrd-testing/src/server.rs:3760-3780`; later stage write
  `crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs:594-620`.
- **Caller and full-body evidence:** both callers use the same
  `BifrostDataRoot::prepare`; production calls it before external dependency
  composition and role reservation. The body runs `create_dir_all` on existing
  paths, then opens/locks only `<root>/.lock`. `BifrostVolumeGovernor::register`
  inspects metadata, retained bytes, and free capacity but does not prove stage
  or output writes. An existing read-only managed child therefore passes until
  later Scribe IO.
- **Observable consequence:** a Scribe role can activate and acknowledge WAL
  writes before discovering that staging/output is unusable.
- **Decision-complete smallest correction:** inside the existing
  `BifrostDataRoot::prepare`, use the standard library to create/write/sync/remove
  one fixed reserved probe file in each managed directory after taking the
  exclusive root lock. Preserve existing contents and clean the probe on error.
  The lock makes unique naming unnecessary. Do not add a
  second root type, config knob, background health checker, symlink policy, or
  generalized hostile-filesystem defense.
- **Focused closure proof:** a focused root test covers an existing unwritable
  managed child alongside the current create/restart/lock tests; the failure
  occurs from `prepare` before role composition.

### `FIND-TASK-003-1` — REVISED — VIOLATION: Oracle audit pressure accumulates an unbounded tracked task/event backlog

- **Wave-1 sources:** `PERSIST-DATA-02`, `STREAM-LIFECYCLE-002`.
- **Obligation:** `bifrost-design.md` requires every Wyrd-owned task set and
  queue to be bounded. Spec REQ-026/REQ-026A and revision 8 require tracked,
  non-blocking audit commits that do not starve reader renewal or queries; a
  failed commit is logged and counted.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/oracle/query_audit.rs:34-105,116-160` and
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:314-371`.
- **Caller and full-body evidence:** the only two `OracleAudit` entry points,
  read decisions and verified tripwires, both build one complete `AuditEvent`
  and call `stage`. `stage` unconditionally spawns a `TaskTracker` task; only
  inside that task does it await the connection semaphore. Neither
  `TaskTracker` nor the semaphore waiters bound retained tasks/events. Completed
  reads release query admission while their audit task remains, so query slots
  do not bound accumulation. The locked-chain-head journey deliberately
  demonstrates pool-sized pending accumulation but no ceiling.
- **Observable consequence:** a slow or locked tenant audit chain permits
  sustained reads to retain tasks and events until memory or shutdown debt
  exhausts the server, even though pooled connections remain available.
- **Decision-complete smallest correction:** retain `OracleQueryAudit`, its
  `TaskTracker`, and its existing connection-share semaphore. Add one Tokio
  semaphore, sized from the existing Vala pool maximum, for total pending work.
  `stage` uses `try_acquire_owned` on that pending limit before spawning and
  moves the permit into the accepted task; the existing semaphore continues to
  limit active database users to one quarter of the pool. Saturation spawns
  nothing, logs the same scrubbed failure, and increments
  `oracle_audit_commit_failures_total`. Add no custom counter, queue, worker,
  WAL, relay, durable fallback, or configuration knob.
- **Focused closure proof:** extend the existing locked-chain-head journey past
  the pending bound and assert pending ownership never exceeds it, overflow is
  counted, reads and a spare pooled connection still succeed, releasing the
  lock drains admitted commits, and shutdown reports bounded debt.

### `FIND-TASK-003-2` — REVISED — INCORRECT: generic Rust pull requests run no Rust tests

- **Wave-1 source:** `DRI-01`.
- **Obligation:** spec REQ-033/REQ-035 and TASK-003 require affected-code PR
  selection to execute a credible owning Rust lane and aggregate its result.
- **Exact location:** `.github/workflows/lints-test.yml:40-59,61-109,155-175`;
  classifier proof `.github/scripts/tests/test-detect-changes.sh`.
- **Caller/topology evidence:** a pull request produces only
  `os=["ubuntu-24.04"]`; `rust-compat` excludes that sole entry. For a generic
  Rust change `full_gate=false`, so `ci` runs `mise run check`, whose body is
  format/lint and not tests. The classifier test proves flags only; required-job
  aggregation accepts the resulting skipped empty Rust matrix.
- **Observable consequence:** a normal non-Bifrost Rust PR can break its crate
  tests while the stable `ci complete` check succeeds.
- **Decision-complete smallest correction:** keep the Ubuntu exclusion in
  `rust-compat`. In the existing Linux `ci` job, keep `mise run gate` for
  `full_gate=true`; otherwise run the existing `mise run check` followed by
  `mise run test:rust`. This uses the current Linux owner and avoids duplicating
  Rust tests when `gate` already contains them. Keep the existing required-job
  aggregator. Add no focused-lane planner, matrix entry, job, or new check.
- **Focused closure proof:** evaluate one generic-Rust PR fixture through the
  classifier and selected `ci` command, showing the Linux job reaches both
  `check` and `test:rust`; evaluate one `full_gate` fixture showing `gate` runs
  without a second `test:rust`; `mise run check:ci-selection` remains green.

### `FIND-TASK-003-3` — REVISED — MISSING: nightly omits required non-credentialed journeys

- **Wave-1 sources:** `TASK-REV-03`, `DRI-02`.
- **Obligation:** spec REQ-034, REQ-035A, REQ-064, INV-025, AC-008, and AC-022
  require complete non-credentialed correctness nightly and forbid treating a
  gated journey's default exclusion as evidence.
- **Exact location:** `.github/workflows/nightly.yml:14-95`; `mise.toml:102-137,
  918-952,1299-1318,1361-1412`.
- **Caller/topology evidence:** nightly reaches `gate`, identity, and four
  Postgres checks. `gate` reaches default Rust/Bifrost families, the local
  storage-emulator matrix through `test:rust`, and Python and TypeScript unit
  lanes, but not the existing real-server owners for Card, CLI, WyrdState,
  Python testing/integration, or full TypeScript integration. Those
  ignored/integration-marked paths are not selected elsewhere in nightly.
- **Observable consequence:** nightly can remain green while preserved public
  Card/state/CLI/language journeys fail.
- **Decision-complete smallest correction:** turn the existing nightly `gate`
  job into a native matrix over `gate`, `test:cards:integration`,
  `test:cli:journey`, `test:wyrdstate:journey`, `py:test:testing`,
  `py:test:integration`, and `ts:test:integration`, and run the selected value
  through `mise run`. Keep the existing setup steps; `gate` already owns the
  storage-emulator matrix. Keep live-cloud and production geometry in their
  separate workflows. Add no job-specific copies, new aggregate, or permanent
  YAML parser.
- **Focused closure proof:** inspect the resolved nightly command closure once,
  run each named owner locally with nonzero selection, and validate the edited
  workflow syntax. A hosted nightly pass is useful operational evidence but is
  not a separate task-review finding.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-002-1` through `-6`, `-9` through `-16` | Remain closed; complete caller/source inspection found no regression in their corrected boundaries. |
| `FIND-TASK-002-7` | Closed by the R3 documentation work; no material production defect was proposed in this wave. |
| `FIND-TASK-002-8` | Not reopened. Wave 1 found qualified spelling examples, but current user scope rejects style-only findings without production consequence. |
| `FIND-TASK-002-17` | Not reopened under the current material-production/90% scope. Completed failed-drain re-entry is closed; cancellation specifically during cleanup is a bounded rare path. |
| `FIND-TASK-002-18` | Closed by explicit owner acceptance of the 22 historical trailers without history rewrite. |

No earlier stable TASK-003 or TASK-004 finding IDs exist in the preserved
review packet, so their retained findings start at `1`. The next new TASK-002
finding after the existing maximum `18` is `FIND-TASK-002-19`.

## Verification performed and limits

- Independently inspected the complete base-to-candidate changed-file
  inventory and every Wave-1 report, then read each proposed correction owner's
  complete body and all production/public callers relevant to reachability.
- Confirmed the current runtime MCP catalog contains exactly the approved three
  tools and that the stale schema source types have no live consumer.
- Confirmed the two production/test data-root callers share
  `BifrostDataRoot::prepare`, and root preparation precedes production external
  composition and role reservation.
- Confirmed both Oracle audit entry points share the unbounded spawn-before-
  permit path, and the existing locked-chain test demonstrates accumulation.
- Confirmed the generic-Rust PR matrix empties itself and the nightly command
  closure omits the retained journey owners.
- Queried GitHub Actions read-only: no candidate-branch run and no successful
  candidate-SHA cloud run exists.
- Relied on Wave-1 passing reruns for format, codegen, docs, CI selection,
  proto drift, boundary checks, and the completed-settlement/direct-write unit
  tests. Those checks do not exercise the retained source gaps.
- Did not rerun `mise run gate`, long Postgres/Bifrost/language journeys,
  live-cloud jobs, nightly, or performance. Their absence is not used as a
  source defect. The missing final local and live-cloud results remain explicit
  orchestrator acceptance limits under REQ-064/AC-022.
- No proposed correction requires a new product, public API, architecture,
  security, compatibility, cross-service, concurrency-semantics,
  resource-ownership, or persistent-data decision. Each correction stays in
  an existing owner or required evidence lane.

## Recommended overall outcome

**FIX_REQUIRED** — five deduplicated material findings remain:
`FIND-TASK-002-19`, `FIND-TASK-004-1`, and `FIND-TASK-003-1` through
`FIND-TASK-003-3`. Each directly affects a normal public/production path or an
explicit high-impact durability/resource invariant; none is retained solely
for style, documentation, review process, or missing evidence. None requires a
specification revision, and the immutable review inputs are not blocked.
