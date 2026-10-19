Review ID: long-horizon-verification-r13-plan-2026-10-17
Target: changes/active/long-horizon-verification/spec.md revision 13 and tasks/TASK-001 through TASK-004
Gate: Plan
Verdict: Revise
Architecture axis: FAIL
Executability axis: FAIL
Critical: 0
Major: 1
Task coverage: All implementation tasks
Rehearsal: FAIL

## Evidence and scope

Read-only review of the approved specification and all four tasks. Tasks 1–3
were assessed as parallel branches; task 4 follows their integration. Only this
review artifact was written. Task status and executor preflight were not gates.

Authorities used, in precedence order:

- Current user instructions: reviewed target, parallelism, and restricted write location.
- Approved spec revision 13: grouping, attribution, source grammar, accepted outbox loss, scheduled samples, bounds, reactions, and capacity acceptance.
- Root `AGENTS.md` (no nested AGENTS found) and `architecture/agent-rules.md`: crate ownership, SQL/RLS, audit, portability, SDK parity, and journey proof.
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`: protocol/owner boundaries and updating superseded decisions in the cohesive implementation. The approved group activation, check-tool, reaction, and samples changes are intentional replacements, not findings merely because current architecture differs.
- `architecture/bifrost-design.md`: registered schema authority, built-in provisioning, visibility, admission, recovery, and shared-role startup.
- `architecture/wyrd-security-posture.md`, `architecture/operations/README.md`, and `architecture/operations/deployment-and-release.md`: accepted authority, tenant isolation, readiness, retained-state migration, and version overlap.
- The spec/task-linked reference router, evaluation, architecture patterns, analytical reliability, agent harness, spec-driven development, implementation execution, and testing workflows: reuse, reproducibility, bounded failure, material escalation, and proof tiers.
- Reviewer rubric, execution-readiness, production-risk, public-workflow, and review-format references: startability threshold, all-task rehearsal, deployed-system pass, and report structure.

CodeGraph was consulted before source traversal. Source, manifests, test roots,
and `mise.toml` were inspected statically; planned tests were not treated as
existing executed proof. No external API/version assertion was needed.

Reviewed input SHA-256 values:

| File | SHA-256 |
|---|---|
| `spec.md` | `ae484136e024603eda38e97391c5296941dc7683451e01eda4bd82acf0c74415` |
| `TASK-001-grouped-verification.md` | `288d425a4c396942d244f79548572b3a13c5adb14b69d46b3e23792eacdca2ed` |
| `TASK-002-scheduled-binding-simplification.md` | `cb106379bfd7bd38eb167374515741b2ab07bbe698e60ea7e05dfdef24b1626e` |
| `TASK-003-bounded-outbox-admission.md` | `987e03c61be30f709ef69a024b2b7b5b280108efcbcd72bc547b06ecc97c1fee` |
| `TASK-004-capacity-qualification.md` | `9bca47a78184fe70d2f44ec01621262ab6789cc0314924cb48a3f62a6e25b671` |

## Findings

### PR-001 — Major — Existing verification-table upgrade has no supported transition

- **Location:** TASK-001, Source-backed Reuse Map (analytical evidence), Expected Write Set and Consumer Closure:183–190, and Verification and Evidence; spec REQ-001/014 and AC-006/008.
- **Type:** `IMPACT_GRAPH_GAP`.
- **Issue:** The task adds `group_id` to managed input/result tables and `owner_card_uid` to inputs, but neither it nor the parent fixes how existing registered Bifrost tables transition. Updating definitions and adding queue SQL migrations cannot perform that transition. The planned journeys start with fresh tables and do not prove an upgrade.
- **Impact:** On an existing tenant, the changed built-in definition fails fingerprint validation during startup reconciliation. The replacement process cannot activate its colocated roles, affecting ordinary ingest/query/verification as well as grouping. Choosing a drained upgrade, compatible evolution, or a destructive reset changes rollout, recovery, and evidence preservation; this is a material decision, not private helper design.
- **Repository evidence:** `crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs:52` and `results.rs:56` define the existing fields. `catalog/bifrost_catalog.rs:260–277` assumes immutable registrations and node-lifetime schema caches; `ensure_builtin:912–941` calls registration, and `create_table_locked:1022–1025` refuses a changed fingerprint. Physical schema equality is independently enforced at `:1155–1187`. `crates/wyrd/wyrd-server/src/components/platform/builtins.rs:112–117` propagates the first tenant failure; `src/boot/mod.rs:504–510` performs that reconciliation before any role activates. `architecture/operations/deployment-and-release.md:144–176` requires source-schema compatibility, preservation, and explicit overlap/contract ordering. No existing schema-evolution precedent or fresh-install-only authority was found.
- **Required artifact edit:** Add a packet-local retained-table upgrade contract to TASK-001: select supported deployment/overlap ordering; identify the catalog/Iceberg transition owner, stable table/field identity and historical-null behavior, cache and retained hot/WAL compatibility obligations, and interrupted-upgrade retry boundary. Add a named populated-old-state → upgrade → restart → old/new read/write preservation test with setup and critical assertions. Keep private migration mechanics open; do not weaken schema checks or discard evidence. If the chosen transition changes an approved material constraint, obtain the corresponding spec revision.

## Architecture and execution readiness

The existing owners otherwise fit the feature: clients project shared contracts;
server registration pins sources; Bifrost holds analytical evidence; the current
queue owns immutable group snapshots, serialization, leases, and settlement;
existing Operators deliver reactions. No parallel queue, membership store, or
provider runtime is necessary. PR-001 leaves persisted-schema deployment
architecture unresolved, so both readiness axes fail.

Independent **system-rev** pass: `/root/system_rev`; root validated its finding
against the catalog and startup source. Deployed path:

`SDK/HTTP/MCP → verification control → ScribeOutbox → local/peer Scribe ACK → existing SubmissionOutbox → Postgres queue/runner → result outbox + fenced settlement → Operator delivery`.

Group discovery uses the binding cursor and bounded Oracle queries, then queues
a pinned snapshot. Scribe outage retains identical retry slices within finite
accounting and eventually refuses new source work; audit staging loss is counted
without changing authorization. Abrupt death may lose unacknowledged source
work, explicitly accepted by REQ-005. Failed/truncated discovery defers the
affected work; the scheduler's per-tenant error loop continues
(`verification/scheduler.rs:147–154`). Worker loss recovers through existing
leases/fencing; retries retain snapshot values. Lost analytical results must
not erase required group coverage/comparison control state: REQ-003/004/010
already requires that extension within the queue/settlement owner.

TASK-001 Scenario 5 and TASK-003 Scenarios 1–3 specify these interruption and
recovery proofs. PR-001 identifies a different failure boundary: schema
reconciliation aborts the replacement process before any role activates. Its
retained-state recovery proof is absent.

Non-gating handoff notes:

1. TASK-003 accounting must include the existing Scribe-ACK → SubmissionOutbox handoff (`verification/submissions.rs:29–54`, `scribe_outbox.rs:921–930`); bounding only the first channel leaves retained work unbounded.
2. Enforce the stated active-work bound at claims: `verification/runner.rs:227` creates the runner claim loop without permits, and `claims.rs:140–165` keeps spawning work. Hosted step concurrency alone does not bound execution tasks. TASK-003 already owns this outcome.

## Adversarial analysis

| Counter-hypothesis | Disposition |
|---|---|
| Existing Run/trace identity or another evidence pipeline suffices. | Rejected: independent caller grouping and explicit recorded-source inputs require the approved metadata/reference extensions; existing owners remain sufficient. |
| Missing private snapshot/index/reservation APIs make the tasks unstartable. | Rejected: source alignment, immutable retries, serial settlement, all-or-none refusal, and audit behavior constrain compatible implementations. No generic missing-stub finding. |
| Parallel tasks 1–3 need behavioral dependencies because files overlap. | Rejected: normal-load grouping, current scheduled activation, and ordinary-call pressure each have standalone proof. Shared-file integration is mechanical; combined J-005 is explicitly required later. |
| Adding nullable fields is ordinary existing schema evolution. | Disproved by immutable catalog registration, exact physical-schema validation, and pre-role startup reconciliation: PR-001. |
| Unit report tests can establish the performance target. | Rejected: TASK-004 additionally requires real opt-in qualification with raw bucket/sample/noise evidence, separate legacy/extension verdicts, and all five cases. |

## Traceability and task coverage

PASS means statically decision-complete, not implemented or tested. TASK-004's
PASS is conditional on its declared dependencies; it is not dispatchable now.

| Compilation dimension | TASK-001 | TASK-002 | TASK-003 | TASK-004 |
|---|---|---|---|---|
| Outcome and scope | PASS | PASS | PASS | PASS |
| Owners and impact | FAIL: PR-001 | PASS | PASS | PASS |
| Interfaces | FAIL: upgrade contract | PASS | PASS | PASS |
| Control flow | FAIL: upgrade/recovery | PASS | PASS | PASS |
| Acceptance | FAIL: retained-state transition | PASS | PASS | PASS |
| Tests | FAIL: populated upgrade absent | PASS | PASS | PASS |
| Verification | FAIL: upgrade proof absent | PASS | PASS | PASS |
| Adaptation | PASS | PASS | PASS | PASS |
| Rehearsal | FAIL | PASS | PASS | PASS |

Cold rehearsal identity: independent `/root/cold_rehearsal`, supplemented by
root source tracing and the independent system pass. Inputs were the approved
spec, every task, normal repository authorities, current source/tests/manifests,
and task-runner definitions; no hidden planning rationale. Initial TASK-001
rehearsal passed the new-install workflow, then failed the populated-table
challenge. Both independent agents confirmed PR-001. No task revisions were made.

| Task / requirements | Located impact and proof closure; first steps rehearsed | Remaining choice / disposition |
|---|---|---|
| **001:** REQ-001–006, 008–011, 013–014; AC-001–006/008 | Located Verifier/Reference/Trigger/request contracts; Observe/Target; registration resolver; managed input/result schemas; scheduler, runner, hosted tools and result builder; SQL ExecutionInput/claims/complete; SDK exports, authored stubs, generated declarations, MCP and served OpenAPI. Start with `grouped_support_resumes` in existing three-language verify-in-real-time owners, then the other four named journeys and MCP/OpenAPI assertions. Existing Service credentials, local providers, real server and fixture clock/cursor seams support the new-install flow. | Source union, snapshot encoding and indexes are local. Existing-table transition is material and unresolved: **FAIL**, PR-001. |
| **002:** REQ-012; AC-007 | Located `VerifierSpec::validate_schedule` (`validate.rs:878`), binding inputs (`mod.rs:672`), registration `check_activation`, SQL `ExecutionInput::Scheduled`, and `DriftEngine::checks` (`drift.rs:690`, slots-presence omission gate). Start the two named scheduled-binding journeys; remove redundant selectors/optional samples, update all fixtures/projections including capacity fixtures, retain real fitted/sample/window/Operator branches. | Deserialization field errors and internal representation are bounded mechanics: **PASS**. |
| **003:** REQ-006; AC-005/006 pressure portions | Located generic Outbox/stage, writer retries/drain, audit/gateway consumers, verification producer staging, SubmissionOutbox and claim loop. Start stalled-sink bound/audit unit tests, reserve whole-call capacity before staging, then the named ordinary-call SDK/HTTP/MCP pressure/replay/progress journeys. Add grouped J-005 after integration. | Accounting/reservation API and configuration placement are local under fixed finite ownership and refusal semantics: **PASS**. |
| **004:** REQ-015; AC-009 | Located explicit `capacity` binary target and main/load/step/evidence/report modules, `Scrapes::overhead` (`evidence.rs:73`), telemetry buckets and existing report regressions. Start the four named evidence/report tests; add isolated warmed baseline/group windows, merged replica deltas, strict p99 resolution/noise, group sweeps and overload evidence; run real benchmark. | Bucket detail, window scheduling and disclosed noise method are local under fixed thresholds/cases: **PASS**, after 1–3. |

Verification recipes were traced to existing Rust `integration` targets,
Python integration modules, TS Vitest files, MCP `verification::pg_tests`,
and the served OpenAPI target. `mise exec` exact selectors, repository Postgres
wrapper/migrations, `py:setup`, `ts:build`, and `ts:build:testing` provide a
non-weaker path. New names are explicitly planned additions; ordinary selector
or setup corrections do not block readiness. `mise.toml:516–565` owns the
opt-in capacity launcher, release builds, RustFS/Postgres setup, one/two
replicas, and absolute 30-minute lifetime; it remains outside gate/verify lanes.

Integrated closeout must combine grouped pressure J-005, schedule coexistence,
shared contract/staging changes, all required journeys, and benchmark evidence.
TASK-004 waits for 1–3; no artificial dependency among 1–3 is required.

## Static limitations

No build, test, migration, SDK runtime, server, or benchmark was executed; no
performance or recovery result is claimed. The schema failure follows the
current registration/startup control flow, not a performed upgrade. Specific
WAL/staged-data corruption is not asserted; compatibility needs the missing
transition proof. Fresh-only deployment was not assumed.

The review validator was run separately. The shell's `python` shim was
unavailable, so validation used `/usr/bin/python3` without changing tooling.
