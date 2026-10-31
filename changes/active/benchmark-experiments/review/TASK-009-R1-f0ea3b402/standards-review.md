# Repository standards review

Result: **FAIL** — one mandatory changed-item rustdoc omission (REPO-001).

Subject: `6f38db16c524417301ffc4a5dc0b478daf997ed1..f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`; HEAD was verified as the candidate after inspection. This is the first review of TASK-009 revision 2 under approved specification revision 24. Scope is repository standards across all 35 changed paths, not task acceptance, benchmark qualification, or an ownership/complexity redesign. No prior evidence was carried forward.

CodeGraph was consulted first (`codegraph explore "benchmark experiment aggregate_for_run compare_experiments"`); the immutable Git diff and surrounding current source supplied the detailed navigation. The complete cumulative diff, changed tests, all applicable nested instructions, and the source/diagnostic evidence below were inspected. No source was modified and no Cargo command or benchmark was run by this reviewer.

## Authority coverage

Abbreviations: R = root `AGENTS.md`; SDD = `changes/spec-driven-development.md`; RS = `agent_docs/rust.md`; MS = `agent_docs/maintainer-style.md`; ER = `agent_docs/errors.md`; AS = `agent_docs/agent-surfaces.md`; T = `TESTING.md`; WD = `architecture/wyrd-design.md` (Verifier, client model, coordination clock); BD = `architecture/bifrost-design.md` (verification records, Scribe staging, Oracle caches, audit ordering, WAL shutdown); SEC = `architecture/wyrd-security-posture.md` (Postgres tenant binding and audit integrity); OPS = `architecture/operations/reliability-and-recovery.md` (Scribe failure boundaries). `architecture/README.md` supplies the authority map and normative-document rules. SQL/PY/TS/DOC below refer to each directory's `AGENTS.md`. No other nested `AGENTS.md` exists in a changed path's ancestry.

| Changed path | Applicable authorities | Inspected surface |
|---|---|---|
| `architecture/bifrost-design.md` | R, architecture README, WD/BD | Queued-origin marker and ACK admission text |
| `architecture/wyrd-design.md` | R, architecture README, WD/BD | Shared ACK pipeline and transactional transitions |
| `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md` | R, SDD, approved spec rev24 | Evidence appendix and failure diagnosis |
| `crates/shared/wyrd-client/src/observe/mod.rs` | R, RS, MS, ER, WD | Target borrows bound spec |
| `crates/shared/wyrd-client/src/state.rs` | R, RS, MS, ER, WD | Hydrated state lookup, borrowed BoundVerifier |
| `crates/shared/wyrd-runtime/src/outbox.rs` | R, RS, MS, T, WD/BD | Shared dispatch/wake clock and inline regression |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs` | R, RS, MS, BD | Table-specific test barrier at existing admission seam |
| `crates/vala/vala-bifrost-redux/src/scribe/mod.rs` | R, RS, MS, BD | IngestStall state and test-only arming methods |
| `crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs` | R, RS, MS, WD/BD | Managed record documentation |
| `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs` | R, RS, MS, T, SEC/BD | Exact invoke-only audit assertions |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs` | R, RS, MS, ER, WD/SEC | Existing request owner stages Submitted admission |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs` | R, RS, MS, ER, WD/BD/SEC | Acknowledged construction, successful-row filtering, handoff |
| `crates/wyrd/wyrd-server/src/verification/drift.rs` | R, RS, MS, ER, WD/BD/SEC | Two bounded reservations at existing engine owner |
| `crates/wyrd/wyrd-server/src/verification/groups.rs` | R, RS, MS, ER, WD/BD/SEC | Existing selector preparation and tenant transaction |
| `crates/wyrd/wyrd-server/src/verification/results.rs` | R, RS, MS, ER, WD/BD | Origin-derived queue marker; separate input/output/summary |
| `crates/wyrd/wyrd-server/src/verification/scheduler.rs` | R, RS, MS, ER, WD/BD/SEC | In-flight lifetime, producer staging, connection ownership |
| `crates/wyrd/wyrd-server/src/verification/submissions.rs` | R, RS, MS, ER, WD/BD/SEC | Shared tenant writer, typed exhaustive admission, lane tests |
| `crates/wyrd/wyrd-server/tests/integration/pg_card_declaration_route.rs` | R, RS, MS, T, WD/SEC | Updated due-claim caller |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs` | R, RS, MS, T, WD/BD/SEC | Real server/PG tests; lock, cursor, xid, queue helpers |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs` | R, RS, MS, T, WD/BD/SEC | Cancellation tests, own-ACK test, remote-polling test/helpers |
| `crates/wyrd/wyrd-sql/migrations/20261028000000_verification_group_generation.sql` | R, SQL, WD/SEC | Additive generation column on existing RLS table |
| `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs` | R, SQL, RS, MS, ER, WD/SEC | Production capability signatures, transactions, SQL clocks, admission data and decoding |
| `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs` | R, SQL, RS, MS, T, SEC | Real TenantConn concurrency, rollback, generation and cross-tenant tests/helpers |
| `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs` | R, RS, MS, T, BD | Exact combined standing-cache charge expectation |
| `crates/wyrd/wyrd-testing/src/gateway_capture.rs` | R, RS, MS, T, WD | Paced upstream's shared buffered body and answer path |
| `crates/wyrd/wyrd-testing/src/server.rs` | R, RS, MS, ER, T, WD/BD | Shared outbox-settlement deadline and public test barrier |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs` | R, RS, MS, T, BD/OPS | Stop/join before corrupting and snapshotting WAL |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs` | R, RS, MS, T, BD/SEC | Cluster audit barrier before exact event counts |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs` | R, RS, MS, T, WD/BD/SEC | New real-client ACK journey; existing serialized PG lane |
| `docs/public/llms-full.txt` | R, DOC, AS, WD, docs generator/tasks | Generated deployment/error entries match existing sources |
| `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py` | R, PY, MS, T, WD/BD | Public imports, top-level marked journey, exact retained-row count |
| `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs` | R, RS, MS, T, WD/BD | RetainedOutput/RetainedRequirements and canonical output readback |
| `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs` | R, RS, MS, T, WD/BD | Capture-aware staging capacity and selector-row count |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts` | R, TS, MS, T, WD/BD | Formatting-only assertion change |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts` | R, TS, MS, T, WD/BD | Capture-aware staging capacity and exact row count |

## Rule results

| Applicable rule | Source evidence and assessment | Result |
|---|---|---|
| Narrowest architecture owner; Wyrd nouns only; normative architecture describes required current behavior | Architecture edits update the existing queue/input owners, consistently with `results.rs::inputs` and Scribe-to-submissions handoff. No legacy noun, compatibility surface, execution history, or performance claim was added to architecture. | PASS |
| Contracts in spec; behavior in owning server; clients project behavior | No wire contract, Card kind, SDK export, public error, or binding implementation changed. Client `BoundVerifier` borrows the already hydrated `VerifierSpec`; observe passes the same type directly. Queue behavior remains in existing SQL/server owners. | PASS |
| Dependency/tier boundaries, features, portability, synchronization | No manifests, new features/dependencies, OS gates, process sandboxes, or ad hoc runtimes. Changes use existing std/tokio/SQL ownership. New source tracking is bounded by reservations; no unbounded task spawning was added. | PASS |
| Stateful capability has an owning struct; synchronous pure preparation | Existing scheduler/poller/writer own dependencies; admission is a closed enum. The small `stage` function moves caller-owned reservations/data; it retains no independent lifecycle. `admission_lane`, borrowed target conversion and in-flight bookkeeping remain synchronous. | PASS |
| Typed identifiers and errors; no unchecked production parsing/IO | Admissions use BindingId/ExecutionId/GroupId/CardUid; decode errors remain typed SQL errors; public service errors still use existing Wyrd mapping. New `expect` sites in SQL name the ready-Verifier version invariant. No new production unwrap, secret, raw-error payload, or custom error code. | PASS |
| SQL capability types must be inspected in actual production signatures/fields | Changed queue methods take `&mut TenantConn<'_>`; scheduler retains `WyrdPostgres` and `OperatorPool`; submission writer opens one tenant connection. No production PgPool, bare PgConnection or caller transaction was introduced. Raw pool/transaction helpers added in server tests deliberately observe or block the real database through the fixture's superuser pool; they are not domain production capabilities. | PASS |
| Tenant query transaction ownership, RLS, no manual tenant predicate | Queue admission/prepare/claim methods do not commit, rollback, nest transactions, or issue transaction control. Server callers own commit/drop. INSERT derives `wyrd.current_tenant()` and binding EXISTS stays under RLS; no manual tenant filter or RLS bypass. Foreign-tenant negative test drives both candidate variants. | PASS |
| SQL clocks, additive immutable migrations, named error family | Cursor/window anchors and due/created/updated timestamps use PostgreSQL statement time; the in-process outbox uses Instant. Migration only adds fresh-default UUID generation; existing migration blobs, table RLS, keys and permissions are unchanged. No new SQLSTATE message parsing. | PASS |
| SQL columns mapped by name, never position | `claim_due_group` at `verifier_executions.rs:2396` extends an existing tuple decode from four to five values, including two UUID positions. This does not follow SQL/RS's by-name rule. It is recorded as a nonblocking standards observation below: current selected columns and tuple positions match, and no failing candidate path is established. | FAIL (nonblocking) |
| Rust imports at module top; bare imported names in signatures | New function-local imports at `pg_verification_runtime.rs:5766` and `verification_runtime.rs:7633` violate RS. Other touched legacy fully qualified test signatures retain their local convention. The new imports are a localized nonblocking standards observation; they establish no behavioral regression or mandatory gate failure. | FAIL (nonblocking) |
| Every new/materially changed Rust item has substantive rustdoc; fallible functions Errors; panicking tests/helpers Panics | New production admission types/fields/methods and added SQL/server test helpers/tests generally carry meaningful workflow docs and relevant Errors/Panics sections. Changed cache, WAL, query, SDK retained-output helpers and test constants retain/update meaningful docs. The new `Drop::drop` lifecycle operation is entirely undocumented; see REPO-001. | FAIL |
| Server tracing scrubs arguments; tenant identity not emitted on the shared queue span | Changed queue writer uses `skip_all`; methods record operation/binding identity selectively. Public service staging still uses existing typed request/response and authorization path. No handler, route, permission bypass, or new secret logging introduced. | PASS |
| Tests belong at the right tier and real dependencies are isolated from fast lanes | New SQL tests are in `pg_verifier_executions`; server tests in `pg_*` files; new SDK journey lives in the existing server journey target and uses the lane's existing `ignore` convention (TESTING.md explicitly runs these with ignored tests enabled). No new binary or harness, test class, weakened skip, allow, or timeout increase. | PASS |
| Python/TS remain thin; tests use public surface and typed failures | Only assertions/capacity/format change; Python remains top-level pytest integration and public `wyrd` imports; TS remains existing Vitest integration. No Node/Python lifetime emulation in Rust, native API change, export/stub mismatch, or new unbounded concurrency. | PASS |
| Generated artifacts changed through generator; no committed schemas | llms-full additions match existing deployment page/error source. `mise.toml` docs:generate owns both API generation and llms generation, and docs:check regenerates then checks drift. Available green gate includes this lane; no generator/source contract change was required for refreshing this stale artifact. No schema or generated SDK declaration was manually changed. | PASS |
| Test/assertion/timeout repairs require diagnosis and diagnostician report; gates never bypassed | External diagnostic reports exist for all stale-gate expectation categories; source agrees with the mechanisms. Repairs preserve exact counts/cardinality and durable boundaries. Scheduler cancellation tests were adapted to the remaining scheduler-owned commit (skipped cursor), while new tests exercise deferred ACK admission. Shared outbox repair has its own diagnosed source race and focused regression. Details below. | PASS |
| Commands, Git identity, immutable review and report-only scope | This reviewer performed static inspection and `git diff --check` only, wrote this report only, verified HEAD, and did not build, benchmark, commit, push, configure Git, or alter source. Candidate's externally supplied command evidence is assessed below; final task delivery acceptance belongs to behavior review. | PASS |

## Failure repair evidence

The diagnostic directory is `/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/cdd11ca1-bd0f-4d66-883d-a1d0f87d1205/scratchpad/` (external evidence, not committed authority).

| Repair | Diagnostic report and source validation | Assessment |
|---|---|---|
| Invoke audit assertions | `diag-gw.raw` final report traces independent request lanes. Changed test filters only `gateway.invoke`, drains between requests and retains exact denied/allowed/denied cardinality. SEC audit integrity and BD audit ordering permit concurrent requests. | Diagnosed correction; no audit behavior weakened |
| Cache restart memory expectation | `diag3-report.md` identifies both standing cache budgets. BD Oracle cache accounting charges both at composition; changed exact equality sums metadata+data budgets. | Diagnosed correction |
| Forwarded query audit count | `diag-query.raw` final report identifies the remote leader's outbox. Existing `cluster.await_audit_retained` is now used before baseline and exact success/cancel counts. | Diagnosed synchronization, no sleep/timeout increase |
| WAL replay exact-byte assertion | `diag-owner_inspection.raw` final report identifies recycled suffix trim during old-owner shutdown. Test stops/joins before corrupting and snapshotting, preserving the exact failed-replay byte assertion. BD WAL recycling/shutdown and OPS Scribe failure boundaries support this. | Diagnosed fixture lifecycle correction |
| Captured staging capacity and row count | `diag4-report.raw` final report explains reserve(2) per direct judgment plus gateway capture: capacity 6 reserves only 4, capacity 8 reserves 6. Python lacks that capture so keeps capacity 6. Every language's exact expected input count adds the group's own canonical selector row. | Diagnosed corrections; refusal/recovery assertions retained |
| Rust custom Verdict retained data | WD/BD explicitly place requirements and steps in execution_outputs, not the payload-free results table. Changed RetainedOutput/RetainedRequirements helper queries that existing canonical owner, retaining exact requirement data checks. `5ead5a633` records this stale three-table expectation repair. | Source-proven stale assertion correction |
| Outbox lost wake | `codex-diag.md` proves dispatch/next_wake sampled separately and could strand a just-due lane. It explicitly does not prove this caused the captured incident. Candidate passes one Instant to both and adds a focused inline regression; shared callers include audit, capture, spans, input/result writes, and queue admissions. | Root-cause repair is independently source-supported; task appendix overstates incident attribution but does not invalidate the repair |

Available gate evidence: `gate2-sha.txt` names `e9e3d95cb57249a37238235259437dd8a64fab98`; `gate2.log` ends `Finished in 1210.73s`, `gate=0`, with Python integration 331 passed. This is genuine earlier-source evidence, not proof that the final two profiling changes were retested. The orchestrator separately reports the exact outbox lost-wake selector passed on candidate. No tests were rerun here. No benchmark validity decision is made here.

## Material finding

### REPO-001 — VIOLATION: the new lifecycle destructor has no rustdoc

- **Rule:** root AGENTS Code: “Every new or changed Rust item has rustdoc”; “Missing rustdoc blocks merge.” RS Rustdoc explicitly includes private helpers and requires the invariants and side effects that matter.
- **Location:** `crates/wyrd/wyrd-server/src/verification/scheduler.rs:148`, new `impl Drop for InFlight::drop`.
- **Source evidence:** lines 147–154 have no rustdoc on `drop`. The operation removes its source from shared in-flight bookkeeping under the mutex, recovering a poisoned lock. `InFlight`'s type-level docs describe final-owner release but do not document the new method itself; this explicit changed-item gate applies independently of visibility or compiler lint coverage.
- **Consequence/materiality:** the candidate fails a mandatory repository merge gate at the lifecycle operation that makes due work eligible for preparation again. This is a documentation-gate finding, not a claim that the destructor currently loses work. No probability argument or green compiler check waives that gate.
- **Smallest testable correction:** document this method's workflow role and removal side effect, including the final-owner release invariant and poison recovery, without altering source behavior or introducing a helper/check. Validate by inspecting this item and the bounded changed-item rustdoc set. No new runtime test is warranted for this documentation-only correction; batch it with any otherwise required remediation.
- **Burden:** one concise method rustdoc block; no public API, architectural, concurrency, persistence, or test-harness decision.

## Nonblocking rule observations

The two FAIL/nonblocking rows above are explicit local rule departures, not proposed optional refactors or material findings. For positional SQL decoding, current query/type order is consistent; no source-proven candidate failure was found. For the test-local imports, no functional or credible maintenance failure beyond local convention was established. Neither is promoted to a blocker under the skill's materiality threshold. No other minor recommendations are proposed.

Standards review is complete for every changed path. Overall **FAIL** due to REPO-001 only. Full acceptance, final-source benchmark/gate attribution and delivery obligations remain with the orchestrated task review.

Skipped: runtime execution, Cargo, full benchmarks, and Git mutations. Risk: the recorded green aggregate precedes the final client/upstream changes; it must not be represented as a final-candidate run.
