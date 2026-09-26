# TASK-005 round 2 repository standards review

## Subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`.
- Base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate and HEAD `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`.
- Reviewed the complete cumulative diff and source, not only the remediation commits. `.codegraph/` is absent.
- Governing material: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, `architecture/references/README.md`, focused references listed below, approved `spec.md` revision 36, original TASK-005, and the prior r1 verdict. The user directed revision 36 after the r1 `SPEC_REVISION_REQUIRED` verdict, so the absence of an r1 implementation-remediation task from that prior verdict is expected.

## Authority coverage

| Changed surface | Applicable authority | Coverage and rule result |
|---|---|---|
| Spec/task/review packet and `architecture/logic/drift.md` | `AGENTS.md` §§1, 2, 12, 14–16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `references/languages/spec-driven-development.md`, `implementation-execution.md` | **FAIL REPO-R2-001**: current ready task still binds revision 35 and prescribes the displaced typed-plan path. Revision 36 is approved and recorded in task evidence, so the cumulative code remains reviewable. |
| Pure Card Drift validation, typed baseline status, generated Card schemas | `AGENTS.md` §§2, 3, 4, 8, 9, 16; `wyrd-design.md`; `references/doctrine/positioning-and-vocabulary.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md` | **PASS**: `wyrd-spec` stays IO/async/PyO3-free; typed status and validation live with contracts; generated schema and TS projection agree with the typed source; recorded `codegen:check` passed. |
| SYSTEM issuance, verification, scoped permission, query service, scheduled consumer | `AGENTS.md` §§2, 3, 4, 9, 16; `agent-rules.md` audit/tenant boundaries; `wyrd-security-posture.md`; `wyrd-design.md`; `bifrost-design.md`; `references/architecture/patterns.md`, `domain/olap-serving.md`, `languages/agent-harness.md` | **PASS**: `issuance.rs:541–606` mints a verified, single-purpose table UID permission; `wyrd-auth-verify/src/lib.rs:796–835` rejects mixed or widened SYSTEM claims; `verification/drift.rs:540–628` calls the public query service through `ScheduledQueryCaller`; Oracle/Gate keeps authorization and read auditing. Tenant isolation and client-tier gates are recorded green. |
| Fitter, Drift engine, Vala scoring, generic runtime | `AGENTS.md` §§3–6, 10, 11, 16; `agent-rules.md` struct/async/doc/testing rules; `bifrost-design.md`; `references/languages/rust-core.md`, `domain/drift-monitoring.md`, `domain/evaluation.md`, `domain/analytical-operations-reliability.md`, `domain/arrow-analytical-interop.md` | **PASS**: owning structs compose dependencies; Parquet decode uses a bound and cancellation; SPC state is capped by rule lookback; fit and run share permits; new Drift test module has inner rustdoc. Recorded Vala/Wyrd/focused tests and lints passed. |
| Baseline Postgres state, Forge namespace, migration and SQL tests | `AGENTS.md` §§2, 3, 9, 11; `agent-rules.md` TenantConn, OperatorPool, RLS, SQL mapping; `bifrost-design.md`; `references/domain/analytical-operations-reliability.md`, `domain/iceberg.md`, `domain/olap-serving.md` | **PASS**: `drift_baselines` has tenant-qualified keys and forced RLS; fitter uses tenant connections and operator discovery; Forge Rust and both CHECK allowlists include `vala.verification`; focused and SQL lanes are recorded green. |
| Rust, Python and TypeScript SDK/test-server journeys; CLI fixtures; mise runner | `AGENTS.md` §§7, 8, 11, 12, 16; `agent-rules.md` test tier/runtime ownership; `references/languages/python-api-and-stubs.md`, `typescript-guide.md`, `testing-workflows.md`, `pyo3-boundaries.md` | **PASS**: each first-class SDK drives a real bound server; Python lifetime tests stay in Python and TypeScript lifetime tests in TS; `mise.toml`/runner includes all three Drift journeys; generated stubs match harness surface; exact focused commands and all required lanes are recorded in the task evidence. |

## Material finding

### REPO-R2-001 — stale ready task revision and approach

- **Rule:** `architecture/references/languages/spec-driven-development.md:12–23,134–172`: a ready task identifies the approved spec revision and may not contradict it; a spec revision that invalidates a ready task calls for supersession or a replacement task. `AGENTS.md` §1 makes the current design authoritative over planning drift.
- **Location:** `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md:1–6,17–43`; approved `spec.md:1–3,975–991,1983–1992` and `architecture/logic/drift.md:154–161`.
- **Evidence:** the task still says `status: ready`, `spec_revision: 35`, and directs “fixed typed Oracle plans” through a local Oracle. The approved revision 36 directs fixed SQL through the ordinary Gate/query service and a scoped SYSTEM read token. The task's later “Remediation r1 Evidence” acknowledges revision 36 and the replacement path but does not update its governing task contract.
- **Consequence:** a later implementer or reviewer following the ready task's frontmatter and Approach would work from an obsolete approved revision and a superseded query boundary. The candidate code is still reviewable against revision 36 because the approved spec and the task's evidence identify that boundary unambiguously; this is a packet-compliance defect, not a missing-source `BLOCKED` condition.
- **Testable correction:** align the active task metadata and its Step 3/owner description with approved revision 36, or mark this task superseded and provide a revision-36 replacement. Preserve the original task obligations and its r1 evidence; do not change the approved behavior. A source check of the task frontmatter/Approach against revision 36 closes this finding; no runtime test is needed.

## Verification limits and result

The implementation packet records successful post-change `fmt`, `lints`, Vala/SQL/Wyrd, nine Bifrost sub-lanes including all three SDK journeys, SDK/WyrdState journeys, storage matrix, codegen, tenant isolation, client tier, skills sync, and every named focused command. I checked `git diff base..candidate --check` and confirmed no `.agents` or `.claude` changes remain. I did not repeat the long-running lanes. This report judges repository compliance; independent reviewers own task acceptance and sensitive-domain correctness.

**Overall: FAIL** — one bounded packet authority finding, `REPO-R2-001`.
