# Invariant Review — packet-wide candidate `7ac45dec9`

## Review Findings

### Critical

- **INV-REV-001 — INCORRECT — an abandoned analytical query can release its active-table claim before its descendants stop doing IO.** [`planner.rs:138`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/oracle/planner.rs:138) spawns `release_active_reads` as soon as `ActiveReadClaim` is dropped. The claim owns no analytical cancellation or join handle. Independently, [`analytical_supervisor.rs:1455`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:1455) states that dropping `AnalyticalAttemptGuard` cancels and releases an attempt but only `finish()` joins its drivers, and its `Drop` calls the non-joining `abandon_attempt` at line 1549. Therefore a caller-drop/error path can commit the SQL release while a local or remote analytical driver is still unwinding and can still issue metadata or data IO. Forge is then permitted to expire/delete the objects that driver selected. The caller-drop journey does not close the race: [`distributed.rs:4745`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:4745) releases the paused follower before waiting for the active row to disappear. This violates revision-11 Scenario 4/6 ownership and the explicit invariant that no reader performs IO after claim release. **Required correction:** make abandonment transfer the claim together with cancellation/drain ownership to one background settlement that first cancels and joins every descendant, then releases the rows. Add a journey that keeps a follower paused after root cancellation, proves the active row remains and destructive Forge paths refuse, then acknowledges follower shutdown and observes release.

### Important

- **INV-REV-002 — INCORRECT — acquisition latency and the permitted reacquisition extend a row beyond the query's absolute deadline.** [`oracle/mod.rs:1887`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1887) computes one `duration = absolute_deadline - Utc::now()` before pool acquisition and stores it in `ActiveReadOwner`. [`planner.rs:181`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/oracle/planner.rs:181) reuses that same duration for the permitted complete reacquisition. The SQL then sets `abandon_after = statement_timestamp() + p_deadline_ms`, and the conflict path refreshes it, at [`20260910000025_oracle_reader_authority.sql:293`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:293). Connection wait, authority-lock wait, authorization, and first materialization therefore move `abandon_after` later than the query's own absolute deadline; a reacquisition moves it later again. A crashed Oracle can consequently block destructive maintenance beyond the configured query deadline. **Required correction:** bind the remaining interval at each SQL acquisition from the original absolute deadline without using a host clock to authorize coordination; the resulting PostgreSQL timestamp must never move later on reacquisition. Cover delayed lock/pool acquisition and delayed metadata-`NotFound` reacquisition with assertions against the original deadline.

- **INV-REV-003 — REGRESSION — a refused prepared cleanup candidate is deliberately retained, then sent through a transition that cannot own it.** [`worker.rs:7934`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/worker.rs:7934) converts every pre-delete proof failure into `ExpiredCleanupOutcome::Refused`. [`forge_tasks.rs:1255`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/src/queries/forge_tasks.rs:1255) correctly leaves that candidate in `state='prepared'` for same-identity replay. But [`worker.rs:7804`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-bifrost-redux/src/forge/worker.rs:7804) then returns `ForgeError::Reconciliation`, whose generic settlement calls `retry_failure` at line 8505. [`forge_tasks.rs:654`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/vala/vala-sql/src/queries/forge_tasks.rs:654) can update only `claimed` or `running`, so the reachable prepared row produces `retry failure lost attempt ownership`. The path occurs when a fresh active read or another refreshed root invalidates the proof after candidate preparation. It emits a false settlement failure and abandons immediate same-identity replay, relying on a later recovery/claim expiry. **Required correction:** model retained prepared outcomes explicitly at the worker boundary so they bypass generic retry settlement and re-enter prepared reconciliation under the same durable identity. Add a focused prepare/read-arrival/refusal/replay test.

- **INV-REV-004 — INCORRECT — the public compaction wire values do not match approved revision 11, and the public default documentation is reversed.** REQ-012 accepts `small-files` and `files-with-delete`; [`vala/api.rs:151`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/wyrd-spec/src/vala/api.rs:151) instead applies `rename_all = "snake_case"`, exposing `small_files` and `files_with_delete`. The same source says an omitted description compacts `full` at lines 143-146 and calls `Full` the default at line 163, while REQ-013 makes the omitted default `small-files`. This is not older task text that can override the approved spec; it is a mismatched public Rust/Python/TypeScript/schema contract and contradictory rustdoc. **Required correction:** make the wire spellings exactly `auto`, `full`, `small-files`, and `files-with-delete` across the contract owner and generated projections, and correct all omitted-default documentation to `small-files`. Regenerate schemas/stubs and exercise the hyphenated values through every first-class SDK journey.

- **INV-REV-005 — INCORRECT — TASK-006 documents public signatures and callback behavior that the runtime does not implement.** [`error.pyi:37`](/home/thorrester/Documents/GitHub/wyrd-forge/sdks/wyrd-sdk-python/python/wyrd/stubs/error.pyi:37) declares keyword-only `details` and `remediation`, while its own docstring says the runtime is a bare exception that retains positional arguments only. More seriously, the Python-native docs say all `after_*` callbacks may raise to abort ([`python.rs:84`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/skald/skald-agent/src/python.rs:84)), but the wrappers convert Python exceptions into `CallbackOutcome::Abort` and [`loop_runtime.rs:357`](/home/thorrester/Documents/GitHub/wyrd-forge/crates/skald/skald-agent/src/loop_runtime.rs:357), line 405, and line 492 route `after_model`, `after_agent`, and `after_tool` aborts to `unreachable!`. The task's own “Contract Disagreements Found” records these and other public stub/runtime differences while claiming the “no docstring conflicts with signature” criterion passed. This fails the task's accuracy criterion and material stop condition; green codegen/typechecking only proves internally valid generated text, not runtime agreement. **Required correction:** reconcile each inventoried disagreement at its source. Documentation-only corrections may describe actual behavior; any desired runtime/signature change needs separate contract authority. Add top-level Python runtime tests for the corrected signatures/help and callback exception behavior.

### Suggestions

None. All reported items affect an approved acceptance criterion or a safety/lifecycle invariant.

## Per-task Verdicts and Acceptance Matrices

### TASK-001 — leader and promotion: PASS

| Acceptance area | Classification | Independent result |
|---|---|---|
| One durable leader term and fence | — | SQL-backed term/fence ownership is single-writer and downstream mutations revalidate the term. |
| Promotion settlement exactly once | — | Promotion state/evidence and file-list settlement share fenced transactions; replay paths converge. |
| Failover and late/stale result handling | — | Stale term and stale attempt identities are rejected without adopting their result. |
| RisingWave comparison | — | Compared with pinned `e23ddf952c3e6ebc03cc254789e84d1179cfacae`: Wyrd retains one-track scheduling shape and adds only the approved durable leader/promotion and Scribe recovery ownership. |

No source-local invariant failure was found for this task.

### TASK-002 — pull and worker results: PASS

| Acceptance area | Classification | Independent result |
|---|---|---|
| Pull capacity and one task per table/track | — | Captured running count, capacity subtraction, and selected work preserve the intended governor bound. |
| Oldest-due selection and current-head decision | — | Pull order and worker-side head refresh preserve fairness and current catalog authority. |
| Stale/late worker reports | — | Term, attempt, and assignment identity prevent stale settlement. |
| Release-mode capacity proof | — | Recorded release-mode evidence is specific to the capacity lane; source paths do not substitute debug-only behavior. |
| RisingWave comparison | — | Compared with pinned `e23ddf...`: the pull/schedule lifecycle remains RisingWave-shaped; central governor charging and governed spill placement are the approved Wyrd difference. |

No source-local invariant failure was found for this task. The prepared-cleanup failure below belongs to maintenance execution/recovery rather than pull capacity.

### TASK-003 — maintenance and removal: FIX_REQUIRED

| Acceptance area | Classification | Independent result |
|---|---|---|
| Planning-demand state and callers removed | — | No live planning-demand caller/state remains; the forward migration removes the obsolete table. |
| Destructive maintenance uses table authority and active-read refusal | — | Snapshot expiration, expired cleanup preparation, and orphan protection serialize through table authority/current roots. |
| Prepared cleanup recovery | **REGRESSION** | INV-REV-003: a refused prepared candidate is retained but then passed to a retry transition that only owns claimed/running rows. |
| RisingWave comparison | — | Compared with pinned `e23ddf...`: maintenance scheduling/settlement structure is preserved; Oracle/hot-object deletion protection is an approved Wyrd difference. |

### TASK-004 — compaction defaults and type: FIX_REQUIRED

| Acceptance area | Classification | Independent result |
|---|---|---|
| Compaction enabled and default planning behavior | — | Absent enablement is on; undeclared type resolves to small-files and the group floor prevents lone-file churn. |
| Public type contract | **INCORRECT** | INV-REV-004: snake-case wire values conflict with approved hyphenated revision-11 values. |
| Public/default documentation | **INCORRECT** | INV-REV-004: contract-owner rustdoc still describes `full` as the omitted default. |
| RisingWave comparison | — | Compared with pinned `e23ddf...`: file grouping follows the upstream mechanism; always-on compaction and the approved Wyrd default are explicit packet decisions, not an empty comparison. |

### TASK-005 — Iceberg filtering across tiers: PASS

| Acceptance area | Classification | Independent result |
|---|---|---|
| Stable field IDs and filter propagation across hot/promoted/rewritten tiers | — | IDs and pruning expressions remain stable through Scribe promotion and Forge rewrite. |
| Scenario 0–3 query equivalence/pruning | — | Source/test evidence exercises the surviving revision-11 requirements across the three tiers. |
| Original Scenario 4 | — | Superseded by TASK-005-R1 under revision 11; it is not independently required. |
| RisingWave comparison | — | Compared against the pinned Iceberg fork/RisingWave commit; no unapproved query/filter semantic difference was found in the surviving scope. |

This verdict covers the surviving original task. The replacement active-reader work is separately FIX_REQUIRED below.

### TASK-005-R1 — active-table reader cut: FIX_REQUIRED

| Acceptance area | Classification | Independent result |
|---|---|---|
| Atomic tenant-scoped cut and narrow catalog definer | — | Acquisition records the cut and read rows under tenant authority; catalog-pointer definer scope is narrow and `wyrd_app` has no `iceberg_catalog` grant. |
| Claim covers every descendant until IO is impossible | **INCORRECT** | INV-REV-001: caller-drop releases asynchronously without joining analytical drivers. |
| PostgreSQL expiry equals the query's deadline | **INCORRECT** | INV-REV-002: a stale pre-acquisition duration and conflict refresh extend abandonment beyond the absolute deadline. |
| Every destructive Forge path is gated | — | Expiration, cleanup preparation, and orphan selection use maintenance authority plus active-read/current-root protection. This does not repair early release. |
| Physical/metadata cleanup convergence | **REGRESSION** | INV-REV-003: retained prepared refusal falls into an impossible generic retry transition. |
| Deletion closure | — | No live reader epoch, ancestry frontier, IO gate, reader-cut field, retention-derived query cap, or compatibility alias remains; only the protobuf reservation remains. |
| RisingWave comparison | — | Compared with pinned `e23ddf...`: Oracle/hot-object deletion protection is an approved Wyrd difference; no additional live reader-frontier mechanism remains. |

### TASK-006 — Python API docstrings: FIX_REQUIRED

| Acceptance area | Classification | Independent result |
|---|---|---|
| Public argument/field documentation coverage | — | The sweep is broad and `TableConfig` is documented on runtime and typing surfaces. |
| Runtime/signature/default accuracy | **INCORRECT** | INV-REV-005: source stubs and native docs contradict runtime signatures and callback behavior. |
| Generated ownership and named checks | — | Generated files remain generator-owned and recorded checks establish formatting/type/codegen consistency, not semantic runtime agreement. |
| Material stop condition | **VIOLATION** | The task records real public signature/default disagreements but nevertheless marks its conflicting-doc criterion PASS. |
| RisingWave comparison | — | Not applicable to Python documentation; no comparison row is required by this task's domain. |

## Cross-task Invariants

| Seam / journey | Result | Evidence |
|---|---|---|
| Oracle claim → analytical descendants → Forge destruction | **FAIL** | INV-REV-001 breaks the ownership chain at asynchronous `Drop`; the destructive gates are correct only while the row remains. |
| Absolute request deadline → SQL abandonment | **FAIL** | INV-REV-002 converts one early host-derived duration into later PostgreSQL timestamps and refreshes it on reacquisition. |
| Cleanup prepare → fresh proof → settlement/recovery | **FAIL** | INV-REV-003 retains prepared state but invokes a transition whose state predicate excludes it. |
| Public compaction request → catalog property → Forge policy | **FAIL** | Internal property/policy spelling is coherent, but INV-REV-004 breaks the approved public boundary and documents the wrong default. |
| Tenant isolation | **PASS** | New query paths use tenant-bound connections/RLS; catalog pointer access is the narrow definer; no production `iceberg_catalog` grant to `wyrd_app` was found. |
| PostgreSQL coordination time | **FAIL** | Eligibility/fencing uses PostgreSQL time, but INV-REV-002 means the value supplied to PostgreSQL no longer denotes the query's remaining deadline. |
| Removal/deletion closure | **PASS** | Searches and source traces found no live obsolete reader/planning structures beyond the explicitly permitted protobuf reservation and migration history. |
| Python source docs → generated editor/runtime surfaces | **FAIL** | INV-REV-005 shows generated consistency without behavioral accuracy. |

## Open Questions

- None required to establish these findings. The recorded stale sibling-route assertion in `production_closeout.rs` was inspected, but this review did not establish that its orphan-only scenario can actually produce a sibling expiration/cleanup route, so it is not reported as a defect.

## Verification Notes

- Immutable subject verified at `7ac45dec99535c881b7a936c66623044f15d8823`; base `c1508b375` was excluded.
- Reviewed the revision-11 spec and revision history, task index and all seven task artifacts, governing architecture/rules, full changed-owner source paths, lifecycle/failure consumers, and deletion-closure searches.
- RisingWave comparison used only `/home/thorrester/Documents/GitHub/risingwave` at pinned commit `e23ddf952c3e6ebc03cc254789e84d1179cfacae`; the scheduler/pull/compaction comparison was non-empty.
- Reported green `verify:bifrost`, principals integration, formatting, lints, and diff checks were treated as evidence, not acceptance. I did not rerun the packet-wide lanes. The failures above are source-proven lifecycle/contract defects not falsified by those green results; the missing race/runtime tests are identified with each correction.

## Overall Invariant Verdict

**FAIL.** TASK-001, TASK-002, and the surviving TASK-005 scope pass. TASK-003, TASK-004, TASK-005-R1, and TASK-006 require fixes before the cumulative packet can be accepted.
