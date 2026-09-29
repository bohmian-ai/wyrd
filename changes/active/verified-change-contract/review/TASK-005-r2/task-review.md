# TASK-005 task implementation review, round 2

## Subject and method

- Base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d` (checked-out HEAD).
- Approved `changes/active/verified-change-contract/spec.md` revision 36; original `tasks/TASK-005-production-drift-verifier.md` still labels revision 35, but its F6 implementation authority is superseded by revision 36. Prior review: `review/TASK-005-r1`.
- Inspected the complete cumulative changed-file list and focused source/diff for contracts, fitting, scoring, query/auth, runtime, SQL, and SDK journeys. `.codegraph/` is absent. This is a source/verification-evidence review; no lane was rerun here.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-110; valid method/signal/profile/condition pairs, unchanged SPC contract | `wyrd-spec/src/card/drift.rs`, `card/verifier.rs`; `vala-drift/src/spc` | Contract tests; scorer parity tests | PASS |
| REQ-072, REQ-073; exact Parquet Data baseline, pending work, durable bounded fit and retry | `verification/fitter.rs`; `wyrd-sql/queries/drift_baselines.rs`; registration in `components/cards/service.rs` | `pg_drift_baselines`; Rust/Python/TS baseline journeys; decoded-budget and permit tests | PASS |
| REQ-074, REQ-134; nonblocking, exact status and structured errors | `components/cards/resolve.rs`, baseline query/status projection | Rust/Python/TS status journeys; failed-fit journey | PASS |
| REQ-080; fixed subject/series/managed-time aggregates and inconclusive insufficient input | `ObservationWindow` and `DriftEngine` in `verification/drift.rs`; Vala aggregate entry points | SQL unit execution, three SDK method-edge journeys | PASS |
| REQ-082; direct/manual bounded run via generic runner | `verification/runner.rs`, generic run routes | Rust direct journey; `pg_verification_routes` | PASS |
| REQ-085; canonical summary/details, null details and zero features without report | generic result publisher; `EngineOutcome::Completed(Drift(None))` | Rust/Python/TS unscored journeys; `unscored_drift_publishes_only_the_summary` | PASS |
| REQ-113, INV-010, INV-012; existing Vala scorers, Bifrost authority, no Drift scheduler | `vala-drift` score entry points; server Drift adapter; generic runtime | Vala parity tests; SDK/server journeys | PASS |
| REQ-152, INV-015; PostgreSQL time for coordination | `wyrd-sql` baseline claims and verifier runs | `pg_drift_baselines`, `pg_verifier_runs` | PASS |
| INV-004; unready, empty, invalid, failed cannot pass | readiness gate, `Drift(None)`, retry/terminal mapping | Rust/Python/TS negative journeys; runtime terminal tests | PASS |
| AC-012; real three-language fit and scoring journeys | Rust/Python/TS Drift journey files | Reported `test:bifrost` and exact focused commands in task evidence | PASS |
| AC-013; complete Service binding journey, including shared Trigger across two Services and manual binding dispatch | generic scheduling and dispatch; one-Service scheduled Drift case in Rust journey | Generic skip/restart/ACK tests; one-Service/two-Operator Rust case; **no two-Service shared-Trigger Drift journey or binding-invocation-to-dispatch proof found** | FAIL (`TASKREV2-001`) |
| AC-020; Postgres control, claims, retries, clock, isolation | `wyrd-sql` baseline/run queries | `pg_drift_baselines`, `pg_verifier_runs`, runtime tests | PASS |
| AC-024; analytical schema/result columns and queries | unchanged Bifrost system tables; Forge namespace fix | Existing Bifrost/schema journeys; Rust result/detail queries | PASS |
| AC-028; exact Card/binding/run status surfaces | Card resolve and generic verification routes | `pg_verification_routes`; Rust Card/status journey | PASS |
| AC-033; shared permit/admission and restart | `BaselineFitter::pass`, generic runner permits/runtime | `baseline_fits_share_the_verifier_permits`; generic runtime tests | PASS |
| Revision-36 SYSTEM Drift read; token, exact table scope, ordinary audited/peer query path | `wyrd-auth/src/issuance.rs`; `wyrd-auth-verify`; `Reader`/`ScheduledQueryCaller::authenticated` | `system_drift_reader_reads_only_the_observation_table`; peer-Oracle journey | PASS |
| Task non-goals: no raw scorer download, client aggregate, user SQL, private scheduler, Alert, fabricated no-report details, SPC replacement | cumulative diff and owning modules | static diff inspection | PASS |
| Required focused verification evidence | exact selectors in task's remediation evidence | report records each named focused run as passed | PASS |

## Proposed finding

### TASKREV2-001 — MISSING — AC-013 binding journey coverage

- **Violated obligation:** AC-013 requires a real Service binding journey with two Services sharing a Trigger Card and isolated runs per binding, and manual binding invocation following its Operator path. The original task's Scenario 6 requires scheduled activation and dispatch proof.
- **Location/evidence:** `sdks/wyrd-sdk-rust/tests/drift_verification.rs:463-547,974-1028` registers one Service and makes one binding due, then checks two dispatch rows. The Python and TypeScript Drift journeys exercise direct runs only (`test_drift_journey.py:399-496`; `drift-verification.test.ts:269-384`). `pg_verifier_runs.rs:868-925` proves skip reasons at the SQL seam, and `pg_verification_routes.rs:318-360` proves manual binding admission, but neither drives a shared referenced Trigger across two active Services through scheduling, scoring, result identity, and dispatch. No test covering that journey was found in the repository.
- **Observable consequence:** A cross-Service Trigger/binding projection or scheduled routing defect can pass the current task gates while mixing, omitting, or duplicating Drift runs. A manual binding might enqueue successfully yet fail to dispatch its configured Operators.
- **Required testable correction:** Extend the existing Rust SDK-to-server Drift journey to register two active Services sharing one referenced Trigger, make the same due occurrence eligible, and assert two separately attributable results with each binding's dispatches; invoke one binding manually and assert its failed result dispatches while a direct invocation does not. Reuse existing server/SDK journey harness and generic runtime. No new scheduler or public surface is needed.

## Prior findings and verdict

FIND-TASK-005-1, -3, -4, -5, -6, -7, -8, and -9 are closed by inspected source and recorded focused proof. FIND-TASK-005-2 is substantially closed across PSI/SPC/Custom method edges and the three SDKs, but retains the AC-013 gap above. The revision-36 security decision supplies F6's missing authority.

**FAIL.** One bounded journey-proof obligation remains. No optional refactor is proposed; `Oracle::query_plan` predates this task and is outside this acceptance audit.
