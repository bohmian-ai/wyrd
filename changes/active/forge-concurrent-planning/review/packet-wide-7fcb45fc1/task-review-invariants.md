# Invariant review — packet-wide remediation candidate `7fcb45fc1`

## Review Findings

### Critical

None.

### Important

- **`INV-REREVIEW-001` / `FIND-TASK-001-1` — INCORRECT — [`crates/vala/vala-bifrost-redux/src/forge/leadership.rs:364`](../../../../../crates/vala/vala-bifrost-redux/src/forge/leadership.rs):** `accept`, `serve_pull`, and `serve_report` clone a live term under the slot read lock and release that lock before touching its schedule, so `set_held` can revoke the same term in between and an old leader can still mutate or return dispatch after revocation; keep the existing slot read guard across term validation and schedule access (with replacement/revocation serialized by the write guard), and add a barrier test that pauses after validation, revokes the term, then proves the request either completed before revocation or is refused without a schedule effect.

### Suggestions

None.

## Verdict

**FAIL / FIX_REQUIRED.** Sixteen of the seventeen prior findings are closed, but `FIND-TASK-001-1` is only partially closed because the leader-handler check/use race still permits work under a revoked term.

The caller explicitly excluded the separately authored `changes/active/bifrost-variant/spec.md` commit from this review; I treated it as outside the subject rather than packet drift.

## Prior-finding closure table

| Prior finding | Closure | Invariant evidence |
|---|---|---|
| `FIND-TASK-001-1` | **OPEN — INCORRECT** | Renewal is independent and long-running promotion/maintenance observes revocation, but peer handler validation is not serialized with `set_held`, leaving the race described above. |
| `FIND-TASK-002-1` | CLOSED | A dispatched row is durably distinguished by its plan, shutdown closes only the exact claimed/running attempt as `cancelled`, sends `NotStarted` only after that close wins, and expiry reclaim reconstructs cancelled dispatch reports without making the row fair-claimable. |
| `FIND-TASK-002-2` | CLOSED | The evidence compares pinned upstream, the prior fork, and the shipped narrowed fork across every mandatory strategy, noncommitting seam, memory/spill, cancellation, and inventory row. |
| `FIND-TASK-003-1` | CLOSED | `CleanupRetained` bypasses generic retry settlement, preserves the exact prepared attempt/candidate, gates ordinary work as unattended authority, and replays that identity after the reader releases. |
| `FIND-TASK-004-1` | CLOSED | `CompactionTypeWire` uses kebab-case, omission is consistently documented and implemented as `small-files`, and schema tests reject underscore spellings. |
| `FIND-TASK-004-2` | CLOSED | `wyrd_client::bifrost` re-exports `CompactionTypeWire`, so the Rust SDK projects the same contract without a direct `wyrd-spec` import. |
| `FIND-TASK-005-R1-1` | CLOSED | `LeaderStreamOwners` drops `AnalyticalGraphLifecycle` before `ActiveReadClaim`; lifecycle drop synchronously revokes followers and aborts local drivers, while only resource-envelope reclamation remains asynchronous, matching the approved decision that unconsumed remote IO may finish after revocation. |
| `FIND-TASK-005-R1-2` | CLOSED | Every acquisition and retry derives its remaining duration from the same absolute `Instant` deadline after obtaining the SQL connection, so PostgreSQL never rebases the analytical lifetime. |
| `FIND-TASK-005-R1-3` | CLOSED | Destructive paths require `&ExclusiveTableAuthority`, whose mutable `TenantConn` borrow prevents ending the authority transaction before the external catalog/object effect; uncertain effects retain prepared evidence for replay. |
| `FIND-TASK-005-R1-4` | CLOSED | The stale sibling-maintenance prohibition is absent and closeout now permits immediate maintenance after the reader releases. |
| `FIND-TASK-005-R1-5` | CLOSED | Table-layout SQL is owned by `vala-sql`; the Redux paths consume the typed query API rather than embedding a second `bifrost_tables` query. |
| `FIND-TASK-006-1` | CLOSED | Runtime objects, source declarations, generated stubs, exports, and parity tests now agree for the previously mismatched session, agent, role, error, and prompt-card surfaces. |
| `FIND-TASK-006-2` | CLOSED | Callback abort carries the held value through the chain: after-model/after-agent ends the run as `CallbackAborted`, while after-tool fails only that call and continues; Rust and top-level Python tests cover the branches. |
| `FIND-PACKET-1` | CLOSED | The remediation records a complete changed-symbol documentation audit and the relevant doc lints/source scan have no actionable residue. |
| `FIND-PACKET-2` | CLOSED | The changed-range shape scan reports only justified collision/associated-type paths and no prohibited function-local import. |
| `FIND-PACKET-3` | CLOSED | `git diff --check c1508b375..7fcb45fc1` exits zero. |
| `FIND-PACKET-4` | CLOSED | The packet records `mise run gate` passing on the final remediation lineage after the sanctioned test-only mock allowlist update. |

## Per-task acceptance matrices

### TASK-001 — leader and promotion: **FIX_REQUIRED**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| One PostgreSQL leader term; renewal independent of promotion IO | PASS | — | Renewal has its own scheduler branch and term-deadline timeout. |
| Exactly-once promotion settlement and successor recovery | PASS | — | Durable promotion recovery remains fenced and the recorded failover journey settles the parked publication once. |
| Revocation stops all leader dispatch and timer work | FAIL | **INCORRECT** | `term()` returns an unguarded `Arc`; schedule operations at lines 372, 448, and 471 can execute after concurrent revocation. |

### TASK-002 — pull and worker results: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Capacity-bounded, oldest-due, one-table pull | PASS | — | Schedule and worker capacity paths preserve the RisingWave-shaped pull contract. |
| Accepted dispatch has exactly one settlement owner through shutdown/reclaim | PASS | — | Dispatched attempts close terminally and are never released into fair claim; reporting follows the successful guarded close. |
| Current-head planning and stale/later report behavior | PASS | — | Worker plans from the current table head and leader report identity guards retain later arrivals. |
| Required exact fork comparison and capacity evidence | PASS | — | The evidence table is complete for narrowed fork `380a4d0`, and the recorded release benchmark passes 16/16 checks. |

### TASK-003 — maintenance and removal: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Ordered maintenance with per-table isolation | PASS | — | Rewrite, expiry, and cleanup remain table-scoped and ordered. |
| Active readers and running attempts protect destructive maintenance | PASS | — | Maintenance authority and reader gates precede every destructive path. |
| Refused/uncertain cleanup retains and replays exact ownership | PASS | — | Prepared evidence, attempt, candidate index, and self-exemption survive refusal and replay. |
| Superseded planning-demand state and callers removed | PASS | — | Removal evidence and consumer scan show no live planning-demand caller. |

### TASK-004 — compaction defaults and type: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Default compaction and omission semantics | PASS | — | Omission selects `small-files` across runtime, docs, and generated schemas. |
| Canonical public values across all SDKs | PASS | — | Values are `auto`, `full`, `small-files`, and `files-with-delete`; underscore variants are rejected. |
| Rust projection is reachable through the client facade | PASS | — | The enum is re-exported from `wyrd_client::bifrost` and exercised through the Rust SDK. |
| Staged inputs are merged once and not revisited | PASS | — | Existing scenario evidence remains intact and the remediation does not disturb its state transition. |

### TASK-005 — Iceberg filtering, Scenarios 0–3: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Field IDs and filtering layout remain exact through hot, promoted, and compacted tiers | PASS | — | The remediation does not alter the already-passing tier/filtering paths, and Scenario 4 remains superseded by R1 as revision 11 requires. |

### TASK-005-R1 — active-table reader cut: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Leader stream owns the analytical graph through claim release | PASS | — | Owner field/drop order makes follower revocation and local-driver abort happen before claim-row release. |
| One immutable deadline bounds every descendant/retry row | PASS | — | Retry uses remaining time from the original absolute deadline. |
| Reader cut and destructive effects share one serial table authority | PASS | — | The capability's connection borrow spans each external destructive result, with lease-TTL bounds and prepared replay for uncertainty. |
| Immediate sibling maintenance and SQL ownership | PASS | — | The stale test ban is gone and durable layout SQL resides in `vala-sql`. |
| No deleted reader-cut model remains | PASS | — | No epoch/frontier/IO-gate/cut field/alias/query cap remains outside the allowed protobuf reservation. |

### TASK-006 — Python API docstrings: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Public runtime, source stubs, generated stubs, exports, and help agree | PASS | — | The parity inventory covers the previously contradictory surfaces; the valid model export/docstring tests remain present. |
| Callback failure semantics are invocable and non-panicking | PASS | — | Abort paths return documented outcomes and are exercised in the Python runtime. |
| Python tests remain top-level runtime tests | PASS | — | New lifetime-dependent coverage imports public `wyrd` modules and runs under Python. |

### TASK-PACKET-R1 — standards and final gate: **PASS**

| Acceptance surface | Result | Classification | Evidence |
|---|---|---|---|
| Changed-symbol rustdoc and import/signature rules | PASS | — | Recorded complete audits have no actionable residue. |
| Clean cumulative diff | PASS | — | Independently rerun `git diff --check` succeeds. |
| Broad final verification | PASS | — | The recorded final `mise run gate` is green on the candidate lineage. |

## Cross-task seams and invalid-state audit

| Seam | Result | Invariant trace |
|---|---|---|
| Leader term → pull → dispatched attempt → report/reclaim | **FAIL** | Durable dispatch ownership is now coherent, but the producer can still emit a pull response after its leader term is revoked because handler use is not serialized with revocation. |
| Worker shutdown/restart → durable task ownership | PASS | Pre-effect dispatches close as cancelled and report once; database-unavailable read loops back off, while the server-level restart wrapper reconstructs a failed worker/scheduler on the same pod. |
| Active read → analytical descendants → claim release → maintenance | PASS | Stream ownership revokes descendants before claim release, and exclusive table authority then orders later destructive effects. |
| Prepared cleanup → reader refusal → same-candidate replay | PASS | The retained prepared row blocks unrelated work and preserves the only identity permitted to self-exempt on retry. |
| Wire contract → Rust/Python/TypeScript projections | PASS | Compaction spellings/defaults and Python callback/runtime contracts have one consistent source-to-consumer interpretation. |
| Tenant/clock/deletion constraints | PASS | New SQL paths remain tenant-bound, coordination eligibility uses PostgreSQL time, and deleted reader-cut concepts did not reappear. |

No additional reachable invalid state was confirmed in the other remediations.

## Open Questions

None; the leader-handler race has a bounded correction at the existing ownership boundary and does not require a design decision.

## Verification Notes

- Reviewed immutable source and diffs with commit-scoped `git show`, `git grep`, and `git diff` because the checkout is later than `7fcb45fc1`.
- Compared the cumulative subject `c1508b375..7fcb45fc1` and remediation range `e8d3cca13..7fcb45fc1`, using the prior ledger only as hypotheses.
- Independently ran `git diff --check c1508b375..7fcb45fc1` (exit 0).
- Did not rerun the 27-minute aggregate gate; its successful candidate-lineage result and focused remediation results are recorded in the review tasks, while this audit used source-level producer-to-sink tracing to falsify their invariant claims.
