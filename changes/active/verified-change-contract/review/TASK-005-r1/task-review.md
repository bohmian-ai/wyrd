# TASK-005 implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`
- Candidate immutability check: `HEAD` was the candidate before and after inspection.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-110: preserve the approved PSI/SPC/Custom contract and reject invalid pairs/configuration | `wyrd-spec/src/card/drift.rs:380-553`; aggregate scorers in `vala-drift` | New contract and scorer unit tests; `test:vala` and `codegen:check` are recorded as passing | PASS |
| REQ-072: resolve exact baseline Data, read registered Parquet as Arrow, fit and persist exact-version state | `verification/fitter.rs:216-317`; `components/cards/service.rs:1194-1204`; `drift_baselines.rs:172-197` | Rust/Python journeys fit genuine Parquet; SQL lifecycle tests; missing/invalid artifact is not journey-proven | PASS |
| REQ-073/074/134: non-blocking pending/building/ready/failed status with exact Data identity and structured error; Custom has no row | migration `20260601000032`; `drift_baselines.rs:112-126,325-370`; `components/cards/service.rs:340-359` | Rust/Python status polling and SQL lifecycle tests | PASS |
| REQ-080/082: exact subject/series and immutable `[start,end)` server window, manual direct and scheduled binding path, insufficient input inconclusive | `verification/drift.rs:93-265,446-534`; generic run queue/runtime reuse | One Rust journey exercises direct methods and one due Custom schedule; required boundary/activation journeys are incomplete (TASKREV-002), and a never-created observation table mishandles empty Custom input (TASKREV-001) | FAIL |
| REQ-085: scored details precede summary; no-report inconclusive writes summary only and no features | Existing generic result publisher reused; `VerificationReport::Drift(None)` path | Rust empty-window journey after prior writes; generic publisher tests recorded under `test:wyrd` | PASS |
| REQ-113: reuse current Card/Bifrost/runtime/engine owners and avoid a second scheduler/result path | Drift adapter is composed into `VerificationRuntime`; existing Oracle, result publisher, and Vala scorers are reused | Source inspection and broad recorded lanes | PASS |
| REQ-152 / INV-015 / AC-033: PostgreSQL owns fit claim, lease, retry, and due clocks | `drift_baselines.rs:34-137`; all transitions use `statement_timestamp()` | `pg_drift_baselines.rs` covers due/retry/expiry/tenant isolation | PASS |
| INV-004: unavailable, invalid, insufficient, or errored evidence never becomes a passing verdict | PSI/SPC small input produces report-level inconclusive; Custom invalid/empty normally produces `Drift(None)` | Unit and written-table empty-window coverage exist, but the reachable TableNotFound empty window becomes terminal errored rather than inconclusive (TASKREV-001) | FAIL |
| INV-010: Bifrost remains authoritative for observations/results | Fixed typed plans read `vala.drift.observations`; generic publisher writes canonical result tables | Rust/Python real-server journeys query canonical tables | PASS |
| INV-012: preserve existing Drift scorer semantics | Aggregate-input entry points share report/formula code in `vala-drift` | Aggregate-vs-raw unit comparisons exist | PASS |
| AC-012: production method journeys, Parquet authoring paths, status, server aggregates, results/details, and negative flows | Rust journey covers PSI/SPC/Custom and result rows; Python journey covers Pandas/Polars/Arrow PSI plus SPC | No TypeScript Drift journey, and the Rust/Python journeys omit multiple required method edges (TASKREV-002) | FAIL |
| AC-013: real binding journey proves due-window selection, activity gate, no backfill, shared Trigger isolation, fanout and inspectable delivery | Rust journey forces one binding due and sees two dispatches | It does not prove inactive/unready/missed suppression, no backfill, two Services sharing a Trigger, worker claim/delivery, sibling retry independence, restart, or manual-binding dispatch (TASKREV-002) | FAIL |
| AC-020: supporting real Postgres/Oracle/Scribe seams plus focused unit tests; lower tiers do not replace journeys | SQL baseline integration, DataFusion plan unit coverage, Rust/Python journeys | Broad lanes are recorded green, but exact focused evidence is incomplete (TASKREV-004) and journey closure is missing (TASKREV-002) | FAIL |
| AC-024: exact verification table schema and tenant/time/result pruning behavior | This task uses the existing result schemas and fixed observation table | Existing/base Bifrost schema lanes are recorded under `test:bifrost`; no TASK-005 regression found in the changed schema use | PASS |
| AC-028: Card status plus Rust/Python/TypeScript/MCP manual direct/binding run projections and negative lifecycle cases | Baseline status projected to Rust-generated schemas and TypeScript interface; existing generic run APIs reused | TASK-005 adds no TypeScript/MCP Drift execution journey and no manual-binding Drift journey (TASKREV-002) | FAIL |
| Task Scenario 3: numeric/categorical bins, boundaries, unknowns, zero bins, subject/tenant/window exclusion, pass/fail/insufficient | Fixed PSI plans and count scorer exist | Unit tests cover aggregate math; journey covers drift only and does not close the listed server cases (TASKREV-002) | FAIL |
| Task Scenario 4: SPC ordering, boundaries, pass/fail, partial/empty windows with unchanged public algorithm | Fixed ordered subgroup plan and existing scorer reuse | Aggregate-vs-raw unit test and one authored-size drift journey; adaptive, rule, pass, partial, and no-data server paths are not journey-proven (TASKREV-002) | FAIL |
| Task Scenario 5: Custom weighted raw mean, equality, invalid/empty/time-edge/subject isolation/direct/binding | Fixed aggregate and shared `score_custom_mean` | Unit equality and one direct drift/empty journey; remaining real-server cases are absent, and never-created-table empty input is incorrect (TASKREV-001/002) | FAIL |
| Task Scenario 6: generic result/manual/cron/retry/restart/authorization/tenant paths | Existing generic runtime is reused; Rust journey covers direct, one cron failure, authorization and cross-tenant refusal | Partial ACK, restart, missed/inactive/no-backfill, manual binding, and delivery-state paths are not exercised through Drift (TASKREV-002) | FAIL |
| Non-goals: no client aggregation/raw download/user SQL/profile MemTable/Drift scheduler/Alert table/fabricated no-report result/SPC replacement | Diff inspection found none of those mechanisms | N/A | PASS |
| Scope discipline: no unrelated change enters the cumulative task diff | Most files are task owners or diagnosed gate fixes | Four skill-policy files change review/implementation process, outside TASK-005 behavior (TASKREV-003) | FAIL |

## Proposed findings

### TASKREV-001 — INCORRECT — a Custom run against a tenant with no physical observation table errors instead of completing inconclusive

- Violated obligation: Scenario 5, REQ-080, INV-004, AC-012, and `architecture/logic/drift.md` require an empty Custom window to persist `completed/inconclusive` with null details and zero feature rows.
- Location: `crates/wyrd/wyrd-server/src/verification/drift.rs:376-386,631-634`.
- Evidence: `aggregate` explicitly maps `BifrostError::TableNotFound` to `Ok(Vec::new())`, documenting that a tenant which has never written an observation owns an empty window. `custom_mean` rejects an empty batch list as `Err("the Custom aggregate is not exactly one row")`. `try_verify` maps that error to terminal `drift_invalid`, so the generic runner settles an errored run without a result. The journey at `sdks/wyrd-sdk-rust/tests/drift_verification.rs:479-506` first writes 120 observations, so its later empty historical window never reaches this branch.
- Observable consequence: the valid workflow “register a ready Custom Verifier, invoke it before the subject has emitted any Drift observation” produces a terminal engine error rather than the required durable inconclusive result.
- Required testable correction: make the no-physical-table outcome method-appropriate empty aggregate input (for Custom, `None`) without changing genuine malformed aggregate handling. Add a real direct Custom run in a fresh tenant before any Drift write and assert `completed/inconclusive`, null details, zero feature rows, and no dispatch.

### TASKREV-002 — MISSING — the required Drift method and activation journey matrix is not delivered

- Violated obligation: task Scenarios 3-6, AC-012, AC-013, AC-020, AC-028, and `architecture/logic/drift.md:421-447` require real Rust/Python/TypeScript client-to-server journeys for PSI numeric/categorical, SPC, and Custom, including their edge, activation, ACK/retry, restart, authorization, and tenant boundaries.
- Location: `sdks/wyrd-sdk-rust/tests/drift_verification.rs:428-741`; `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py:174-269`; `mise.toml:346-352,411-420` and the unchanged TypeScript journey command immediately following it.
- Evidence: the Rust binary has one happy drift window for all methods, one Custom historical empty window, one forced-due schedule, and registration/auth/tenant refusals. Python exercises PSI for three authoring paths and SPC only. No TypeScript Drift journey exists or is included in the TypeScript lane. Across these journeys there is no real-server proof of strict ingest-time edges; PSI pass/minimum/zero-bin cases; SPC adaptive size/pass/partial/no-data/rule branches; Custom equality/non-numeric/unequal batches/never-written input; shared raw input for two bindings; inactive/unready/missed schedule suppression and no backfill; two Services sharing a Trigger; manual-binding dispatch; partial ACK/retry; restart reclaim; Notify delivery; or independent sibling retry. Unit and generic scripted-runtime tests cannot replace the expressly required user journeys.
- Observable consequence: the candidate can regress any of those cross-boundary contracts while its recorded lanes remain green; TypeScript users have no acceptance proof for production Drift at all.
- Required testable correction: extend the smallest existing language journey owners (do not create a new harness) so the required PSI/SPC/Custom and activation cases run through public Rust, Python, and TypeScript clients against the real server/Bifrost path. Reuse generic TASK-004 fixtures for fault/restart/delivery controls, but drive them through the real Drift adapter and assert the method-specific observable outcomes enumerated by the task and drift authority.

### TASKREV-003 — DRIFT — task candidate changes repository workflow skills unrelated to production Drift

- Violated obligation: task scope and the completion requirement that no unrelated change enter the diff.
- Location: `.agents/skills/wyrd-implement/SKILL.md:56-76`, `.agents/skills/wyrd-task-review/SKILL.md:78-84`, and identical `.claude/skills/...` mirrors.
- Evidence: commit `ff27b0eb` adds a repository-wide failure-diagnosis and reviewer policy to four skill files. TASK-005 owns Drift fitting, scoring, durable status, runtime adaptation, and its evidence; it does not authorize changing how every future Wyrd implementation and task review is governed. Existing AGENTS.md testing/failure rules already govern this task, so these edits are neither required production behavior nor a necessary correction to the diagnosed Forge test.
- Observable consequence: accepting TASK-005 would silently change future repository-wide implementation/review policy, and the candidate even changes the review instructions used to judge itself.
- Required testable correction: remove the four skill-file changes from the TASK-005 cumulative candidate. If the policy is desired, deliver it as a separately approved documentation/workflow change.

### TASKREV-004 — VIOLATION — the recorded evidence does not include exact focused runs for every newly named statistical, SQL, and journey test

- Violated obligation: `TASK-005-production-drift-verifier.md:178-179` and AGENTS.md §11 require every specifically named Rust/Python/TypeScript test to be run with its exact focused command after the final target/selector is fixed.
- Location: task implementation evidence under `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`.
- Evidence: the record names broad `mise` lanes, the complete Rust Drift capability lane, one focused Python file, and one focused Oracle planner test. It does not record exact selectors for the five new `vala-drift` aggregate-input statistical tests, four `pg_drift_baselines` SQL tests, the Rust Drift journey test, or the added/re-pinned CLI/runtime named tests. The statement “and the focused commands above” cannot establish runs that are not listed.
- Observable consequence: the required selector-level proof may have selected no test or may not have been rerun after the final target changed; the immutable candidate lacks the evidence class its task makes mandatory.
- Required testable correction: run and record the repository-native exact focused command for every newly named statistical, SQL, and journey test against this cumulative candidate, including the Postgres setup wrapper where required. Do not substitute another broad lane.

### TASKREV-005 — VIOLATION — SPC aggregate output is accumulated without the required bounded rule window

- Violated obligation: the approved Drift authority requires SPC aggregate rows to stream through a bounded eight-point rule window instead of accumulating an unbounded vector; the task requires production aggregation while preserving the scorer.
- Location: `crates/wyrd/wyrd-server/src/verification/drift.rs:342-367,608-665`.
- Evidence: `aggregate` collects every decoded `RecordBatch` in a `Vec`; `spc_chunks` then appends every subgroup mean to another `Vec`. A permitted manual window spans up to 31 days, and subgroup count grows with stored observations. The implementation therefore retains all SPC aggregate batches and all means until the query completes, contrary to the explicit bounded-processing design.
- Observable consequence: a high-volume but valid SPC window can consume memory proportional to every subgroup and fail or pressure the server even though the WECO evaluator only needs its bounded consecutive-rule state.
- Required testable correction: keep PSI/Custom bounded aggregate handling intact, but consume SPC query frames incrementally in chunk order through a bounded scorer state owned by `vala-drift`, preserving current zone/trend/alert semantics and trailing-chunk behavior. Add a focused test that feeds more than one output batch and proves identical report semantics while the retained state remains bounded by the rule window rather than subgroup count.

## Failure-diagnosis review

- CLI/runtime fixture diagnosis: the changed registration validation and shipped engine explain the reported errors; replacing fake/non-Parquet fixtures with genuine Parquet plus readiness assertions is at the named fixture boundary. No contrary finding.
- Forge compaction diagnosis: the fixed sleep could cancel a catalog-committed attempt before its accounting settlement. Polling durable task/operation state is at the test synchronization boundary and preserves the equality assertion. No contrary finding.
- The added repository skill-policy commit is not needed to make either diagnosed correction and remains unrelated drift (TASKREV-003).

## Ponytail audit

The production structure generally reuses the existing owners: one `BaselineFitter`, one `DriftEngine`, the existing Oracle seam, existing Vala scorers, and the generic runtime/result publisher. No new scheduler, result store, client aggregation layer, arbitrary SQL surface, or compatibility path was added. The material excess is the unrelated four-file skill policy. The material under-build is the missing language/journey matrix; unit tests are not a smaller substitute for the explicitly required cross-boundary proof. The SPC buffering is also not the approved lazy solution: retaining all rows is more state than the existing bounded rule semantics need.

## Verification notes and limits

- Reviewed the complete base-to-candidate diff and the task's recorded evidence.
- The task records successful broad lanes: `test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, `test:bifrost:journey:sdk`, `test:wyrdstate:journey`, `test:storage:matrix`, `codegen:check`, `check:tenant-isolation`, `fmt`, `lints`, `git diff --check`, and `ts:typecheck`.
- This review did not rerun the broad suites. Missing focused-command evidence is a finding, not inferred success.
- `.codegraph/` is absent, so source navigation used repository text/diff inspection.

## Overall result

**FAIL**

The candidate implements the central fit/status/query/score path, but it does not satisfy the task exactly: a reachable empty Custom workflow is incorrect, the required multi-language method/activation journeys are materially incomplete, SPC violates the bounded-streaming design, required focused evidence is absent, and unrelated workflow-policy changes entered the immutable range.
