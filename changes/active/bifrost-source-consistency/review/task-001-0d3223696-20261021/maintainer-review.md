# Independent maintainer review

Result: **FAIL**

Subject: base `1c5edf72958af8f47c63eb8ce39eb316672e170a` to candidate `0d3223696f091fb16bb2e0ec36040a249d39b61f`, complete cumulative three-dot range. HEAD still equals candidate. This is the maintainer discovery report, not the final independently validated ledger.

Authority: `AGENTS.md` §§3–6, 9, 11, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/{maintainer-style,spec-driven-development,rust-core,testing-workflows}.md`; architecture patterns; applicable Bifrost source authority, admission, recovery and terminal sections; Wyrd design Bifrost/coordination boundaries and doctrine service/surface boundaries; OLAP serving and analytical reliability references. Approved source-consistency spec revision 2 governs the older revision-1 task language. Read the original task, its implementation/verification evidence, and the cumulative diff directly. Used CodeGraph before ordinary source navigation.

## Changed-surface coverage

| Changed surface | Owners, callers and proof inspected | Maintainer assessment |
|---|---|---|
| `catalog/{bifrost_catalog,cut_cache}.rs` | `PinnedSealedTable`, cut materialization, `evict_cut`, `CutCache::evict`, fill-generation race test; Oracle `build_physical_root` and `list_live_routes` consumers | Extends the existing notification cache; generation bump prevents a pre-eviction fill reinstalling a discarded cut. Revision meaning is documented at SQL and materialized boundaries. |
| `oracle/{mod,planner,profile}.rs` | `classify_one_build`, `build_physical_root`, `discover_live_routes`, `list_live_routes`, `refreeze_roster`, `pin_cut`, profile counter and test; surrounding attempt/admission/root ownership | Selection retry stays in existing Oracle planning owner; profile is retained across repins. No caller-specific group reader or second planner introduced. Residual source documentation belongs to M-002. |
| `oracle/{live,exec}.rs` | `LiveCaptures`, its constructor/drop/Debug/test adapter, route cloning/pruning/partition resolution, signed fragment construction, `LiveFragmentRead::into_stream`, `HotObjects::Staged`, `hot_parquet_scan`; DataFusion plan/stream ownership and unit fixture consumers | New RAII value has real retained identity and cleanup dependencies; staged lease remains shared through the existing Parquet plan rather than a new scanner. Execution loss consistently propagates to failure. |
| `oracle/{follower,tail_discovery}.rs` | resolver trait/Arc forwarding, authenticated context, `ScribeTailResolver::resolve`, `LiveTailSource::open`, role/schema/closure/scan-id checks, staged fixture/lease tests, discovery result and release contract | Query identity reaches the existing authenticated resolver and signed assignment. Existing schema and integrity checks survive. M-001 concerns missing method error documentation. |
| `scribe/{memtable,shards}.rs` | memory read guards/selection, `LiveCapture::{take,read,partition_bytes,is_empty,publication_revision}`, `CapturedRead`, `ReadableSource::project`, `ScribeShardRuntime::memtables`; writable freeze/immutable state consumers and rollover hook | Existing selection/projection is extended and old production snapshot/listing entry points are removed or restricted to their unit consumers. Capture owns references and lease, not duplicate row data. |
| `scribe/{hot_source,tail_rpc}.rs` | registry publication marks, atomic advance/revisions, restoration, release/discard, `capture_staged`, notification and lease paths; `CaptureTable`, retained capture, expiry, binding/stream checks, transport capture/release; local scope/rollover/retention/authority/expiry tests | Registry remains the source authority. Table capture retention is shared by Scribe, with independent shard locks and removal values dropped outside locks. No alternate WAL/file-list reader was added. |
| `scribe/{claim_publication,file_list_writer,persistence,staging_runtime,mod}.rs` | full changed methods, publish and committed-claim recovery sequence, fenced artifact insertion/replay, node revision query, runtime restoration, boot before readiness, `tail_service`; existing retirement/lease drain and staging consumers | Publication, SQL outcome, revision advancement and replay stay with their established owners. New methods express distinct retained-owner operations rather than utility structs. |
| Vala SQL migration/query/integration test | revision trigger/RLS/function, `TableCut`, `CutDbRow`, `group_table_cut`, `OracleTableCuts` read shape; file-list per-node/replay/rollback/tenant proof | Durable meaning remains at catalog boundary; typed revisions are carried through the existing cut. SQL behavior/security/durability acceptance is separately domain-reviewed. |
| `wyrd-spec` API/digest and tonic proto/conversions | `ScribeProviderCut.capture_id`, digest v10 and substitution test, protobuf capture/release messages, private conversions and roundtrip fixture | Internal source identity is propagated consistently without an SDK/public query input. Digest domain changes with signed bytes. |
| Server `grpc/scribe_tail.rs`, `oracle/{tail_discovery,peer_service}.rs` | capture/release handlers, binding/UUID decoding/status mapping, channel caching, remote/local listing, admission/release, signed query claim extraction; source/resolver callers | Existing server transport and discovery owner is extended. Capture retention is visible in methods but handler error/cancellation documentation is incomplete (M-001). |
| Runtime tests and fixtures | `scribe_persistence_path`, `pg_grpc_ingest_smoke`, test-cluster observation, published source-transition/invalid-capture journeys, distributed terminal matrix and exported fixture helpers, peer-network security fixture | Reuses existing real-server journey topology and helpers. Row checks sort without deduplication. Terminal matrix changes match execution failure behavior. No new test harness or language-runtime emulation. |
| Capacity target/report/workload, `mise.toml`, redux manifest | selective inclusive limit with existing strict limits retained, sample fixture and boundary assertions, direct verification histogram boundary, output precision; UUID v4 capability | Existing report owners and dependency are reused. No extra benchmark harness; random capture capability is the reason for UUID v4. Performance qualification remains failed per recorded evidence, not assessed as a maintainer-style defect here. |
| Span-permit correction and verification cache prose | `every_permit_returns`, sustained-outage caller/test, blocking-copy lifetime explanation; cache comment change | Test waits on the actual permit authority under a deadline instead of taking an instantaneous count. Separate test-only handoff is explicitly authorized by approved spec. |
| Current architecture/product docs and generated LLM prose/diagram data | Bifrost/Wyrd design changed passages, OLAP/reliability references, Bifrost/evidence docs, `bifrost.ts`, `llms-full.txt` | Current descriptions remain contradictory about publication duplication and loss terminal semantics (M-002). No authored/generated SDK declaration was changed. |
| Planning/workflow prose | `.agents/skills/wyrd-{plan,spec}`, spec-driven reference, approved source-consistency spec/task/evidence, long-horizon supersession, two carried verifier draft specs | Maintainer explanation guidance is coherent; carried draft artifacts declare themselves drafts and grant no runtime authority. Their presence is explicitly covered by the source-consistency handoff, not mistaken for this task implementing those features. Stale revision-1 task metadata is subordinate to revision 2, as recorded in subject. |

The table covers all 58 changed paths by owning surface, including non-runtime changes and the carried test correction.

## Existing alternatives and new machinery

| Candidate machinery | Existing alternative checked | Missing behavior and reuse judgment |
|---|---|---|
| `LiveCapture` / `CapturedRead` | `Memtable::readable_sources`, existing projection/bounds, shard snapshot, registry staged leases, follower hot-Parquet scan | Old execution-time snapshot could not be addressed across fragments or kept coherent with rollover/publication. The candidate extends those exact selection/projection/lease mechanisms and removes old production acquisition. A retained value is necessary; another scanner is not added. |
| `CaptureTable` / `RetainedCapture` | Existing Scribe tail service lifecycle; Oracle `RunningQueryRegistry` and participant-cut owners; existing staged authority registry | Oracle registry contains admitted leader control facts, not Scribe sources acquired before admission. Hot-source registry owns generation authority rather than query identity. Sharing a Scribe-local capture table through `ScribeImpl::tail_service` is the missing responsibility, not a competing query scheduler. |
| `LiveCaptures` | Existing Oracle retained physical root, live route clone/pruning, fragment cancellation and participant lifetime | Local plan references already determine whether execution can still consume a route; the RAII value attaches asynchronous capture release to that same lifetime. Moving the rows into the running-query registry would create an additional lifetime rather than reuse the plan's. |
| Publication revisions/marks | Existing `ClaimPublisher`, `ScribePublicationReconciler`, registry authority transitions, SQL cut reader and notification cache | Notifications provide freshness but no committed handoff proof. Per-node revisions and unresolved-publication marks are approved behavior. Their updates are installed on existing commit/restore/authority methods. |
| Discovery release/channel helper | Existing `RegistryTailStreamDiscovery` channel cache and tonic transport | Existing discovery already owns roster dialing. Release reuses channels; the extracted cache helper avoids evicting other roster channels when releasing a subset. No parallel transport stack. |

No independently supportable blocking duplicate-workflow or speculative-abstraction finding emerged from these comparisons. Formatting or naming preferences are not findings.

## Proposed findings

### M-001 — Changed fallible capture adapters omit the mandatory error contract

Classification: **VIOLATION**. Governing rule: `AGENTS.md` §16 requires substantive rustdoc, including `# Errors` for every new/materially modified fallible method; maintainer-style documentation guidance requires enough information to modify failure and cancellation safely. This explicit documentation gate is blocking under the task-review skill.

Exact changed locations:

- `crates/wyrd/wyrd-server/src/grpc/scribe_tail.rs:49`, `ScribeTailGrpc::list_active_streams`.
- `crates/wyrd/wyrd-server/src/grpc/scribe_tail.rs:83`, `ScribeTailGrpc::release_capture`.
- `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:158`, `FollowerSourceResolver for Arc<T>::resolve`.
- `crates/vala/vala-bifrost-redux/src/oracle/live.rs:138`, new `NoCaptureDiscovery::discover` test adapter.

Evidence: the gRPC methods have only the short descriptive comment, despite binding/UUID decode failures and mapped capture deadline/state errors. Listing now awaits a retaining operation; cancellation after retention leaves cleanup to expiry. The changed Arc forwarding method has only a delegation sentence despite propagating resolver errors. The new test implementation returns `Result` without documenting that this fixture always succeeds. Adjacent `wire_binding`, `uuid_field`, `capture_active_streams`, and the `FollowerSourceResolver` trait itself show the required contract format.

Concrete cost: a maintainer reading the changed adapter cannot discover its refusal classes or the retaining/cancellation side effect from the method contract; the implementation also fails the repository's explicit documentation completion requirement. This is one bounded documentation gap, not a request for code redesign.

Smallest correction: complete the rustdoc on these changed methods. State malformed input and mapped deadline/state failures for listing, binding/UUID/state failures for release, that forwarded errors are unchanged for the Arc method, and that the test discover implementation never returns an error. Document listing cancellation after retention using the existing release/expiry contract; do not change runtime behavior.

Closure proof: inspect the actual method bodies against their rustdoc and run only the owning format/static documentation checks selected for the remediation. No new runtime test is justified by this prose-only correction.

### M-002 — Current contract documentation still permits deleted behavior and misstates capture-loss terminals

Classification: **INCORRECT**. Governing obligations: source-consistency `REQ-005`/`AC-004` expressly require removal of superseded read-path allowances and current documentation; `INV-005` and the approved technical boundary require accepted capture loss to fail. Maintainer-style requires the documented contract to match the operation.

Exact locations and evidence:

- `architecture/references/domain/olap-serving.md:93` still says accepted publication overlap can duplicate a row across published/live sources, although this same changed file's new paragraph promises exact coverage. This is the precise old allowance the task requires deleting.
- `architecture/bifrost-design.md:357` still says any known live source lost before rows degrades. Its newly added paragraph at approximately lines 380–386 correctly says an accepted capture lost before execution fails. The two current architecture statements conflict.
- `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:1782` still tells callers they may degrade a live Scribe `Unavailable` before any row. `oracle/query_stream.rs:891` similarly describes pre-row live loss generally, without restricting it to listing loss. These are the old policy's direct source comments, reached by the changed live-fragment path, and are required deletion/update closure rather than unrelated old debt.
- The newly changed `architecture/wyrd-design.md:1699`, `docs/src/content/docs/understand/bifrost.md:61`, `docs/src/content/docs/understand/runs-observations-and-evidence.md:107`, and diagram `docs/src/lib/diagrams/bifrost.ts:300` promise `degraded` whenever the holding Scribe is unreachable. The generated public LLM text repeats this at lines 5004/5114. That is false after a successful retained capture.

Reachable producer-to-consumer path: successful discovery yields a capture and route; `LiveScribeExec::fragment` signs that capture; `LiveFragmentRead::into_stream` at `oracle/live.rs:809–813` maps any open failure to an execution error, even before the first row, and lines 839–842 do the same for stream errors. Only failed listing is recorded as live degradation by `Oracle::list_live_routes`. The changed `distributed::live_query_terminal_failure_matrix` explicitly expects Failed for `UnavailableBeforeRows`, and the new invalid-capture journey proves the same boundary. Thus both internal maintenance guidance and public terminal explanations contradict reachable behavior.

Observable consequence: a caller following current docs cannot tell whether an unreachable live owner permits a usable degraded published-only result or invalidates the execution; a maintainer following the remaining duplication allowance or dispatcher comment can restore the exact incomplete behavior this change removes. This is a contractual explanation error with an explicitly required cleanup outcome, not a wording preference.

Smallest correction: reconcile these current descriptions to the already implemented two boundaries: inability to list a selected Scribe yields known listing degradation; loss of an accepted retained capture fails the dependent execution even before its first row; publication movement itself never permits a missing or duplicate selected row. Remove the old overlap allowance and pre-row execution-degrade instructions. Regenerate `llms-full.txt` through its source-driven workflow. Preserve historical packets/review evidence and all runtime behavior. Clarify residual `discover_live_routes`/`LiveTableRoutes` comments that imply routes retain no sources when updating this same description, without refactoring their owners.

Closure proof: compare every corrected statement with listing and live execution branches and their existing terminal tests; run `mise run docs:check` for the public documentation/generated text. No new runtime harness or journey is needed for this documentation correction.

## Verification and limits

This pass is static. No Cargo, benchmark, journey, SQL migration, or documentation build was rerun. The task records all focused/static checks passing and final capacity still failing the approved absolute targets; those claims do not override the source findings or establish performance acceptance. Confirmed `05186c1e4..candidate` changes only the task evidence file, so the identified runtime source is unchanged since its recorded capacity run. Neither the 14% table-aggregate result nor the assertion that it is noise was independently measured here.

No source edits, commits, merges, pushes, dependency changes, or unrelated cleanup were made. Only this assigned report was written.
