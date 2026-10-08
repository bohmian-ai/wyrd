# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Complete range: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Prior reviewed candidate: `852894689388124960993014a46934e73c0ed2a8`
- Remediation delta: `852894689388124960993014a46934e73c0ed2a8..5c3bb79b3598abd88a3a234611fc400096adc975`
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior verdict and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/verdict.md` and `TASK-008-CLOSEOUT-R1-capacity-closure.md`
- CodeGraph: not applicable; the repository has no `.codegraph/` directory.

This report audits repository standards only. It does not decide task acceptance
or perform the Ponytail audit.

## Authority status

The caller identifies `changes/active/verified-change-contract/spec.md` revision
58 as the approved specification. The immutable candidate's file instead says
`revision: 57` and `status: approved` at `spec.md:1-4`. The original task also
maps itself to revision 57 at `task-008-closeout.md:6-8`, and the prior
remediation maps itself to revision 57 at
`TASK-008-CLOSEOUT-R1-capacity-closure.md:5-10`.

The authoritative approved revision named by the caller is therefore absent
from the immutable subject. Under `wyrd-task-review` (missing authority blocks
repository-standards review) and
`architecture/references/languages/spec-driven-development.md` (approved spec
revision precedes tasks, tests, implementation, and evidence), compliance
cannot be finally determined. The tables below record the complete surface
routing and the revision-57 conformance that could still be established; they
do not substitute revision 57 for the requested revision 58.

## Authority coverage

| Changed surface in the cumulative candidate | Applicable repository authority | Coverage and result |
|---|---|---|
| Active specification, task packets, review artifacts, architecture diagrams, and implementation evidence | `AGENTS.md` §§1, 11-12 and change-artifact rules; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | **BLOCKED** — the requested approved revision 58 is not present. The remediation did correct the task's prior stale revision-49 frontmatter to revision 57 and review status. |
| Verification/Card contracts, IDs, stable errors, schemas, and generated protocol artifacts (`wyrd-spec`, `wyrd-tonic`) | `AGENTS.md` §§2-4, 9, 12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; reference router routes for positioning/vocabulary, architecture constraints, errors, and testing | **PROVISIONAL PASS at revision 57** — ownership remains in the foundational contract crates; PyO3 remains out of `wyrd-spec`; generated protocol proof is recorded. Final revision-58 compliance is unavailable. |
| Authentication, workload roles, credentials, principal projection, and tenant SQL | `AGENTS.md` §§2-4, 9, 12; `architecture/agent-rules.md` SQL/tenant/audit rules; `architecture/wyrd-security-posture.md`; Rust, error, and testing references | **PROVISIONAL PASS at revision 57** — typed identities, RLS-owned tenant paths, credential secrecy, and server-side authorization ownership are retained in the inspected source and recorded verification. Final revision-58 compliance is unavailable. |
| Shared client, Bifrost transport/queue/state/observation/verification clients | `AGENTS.md` §§2-6, 9-12; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; architecture-pattern, Rust, OLAP-serving, analytical-reliability, and testing references | **PROVISIONAL PASS at revision 57** — the shared Rust client remains the SDK-facing owner, async work is attached to real IO, admission/backpressure is bounded, and client code does not acquire server owners. Final revision-58 compliance is unavailable. |
| Server HTTP, MCP, gRPC, gateway capture, direct/queued verification, telemetry, audit, and runtime lifecycle | `AGENTS.md` §§2-6, 9-12; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`; agent-harness, errors, telemetry/observations, evaluation, OLAP-serving, and analytical-reliability references | **PROVISIONAL PASS at revision 57** — durable behavior stays server-owned, public shapes remain typed, permission/audit boundaries follow the current repository authority, and user-facing surfaces have journey coverage recorded. Final revision-58 compliance is unavailable. |
| Vala/Bifrost Gate, Scribe, Forge, tables, query evidence, migrations, and SQL run queues | `AGENTS.md` §§3-6, 9-12; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; Vala, OLAP-serving, Iceberg/DataFusion where reached, and analytical-reliability references; deployment/recovery authority | **PROVISIONAL PASS at revision 57** — the inspected changes remain in their Vala/server owners and preserve typed tenant/durability boundaries. Final revision-58 compliance is unavailable. |
| Rust, Python, and TypeScript SDK projections and journeys | `AGENTS.md` §§2-4, 7-12; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; PyO3, Python API/stubs, TypeScript, errors, telemetry, and testing references | **PROVISIONAL PASS at revision 57** — language SDKs project the shared client/server contract, runtime-dependent behavior is tested in its owning language, and public typing/codegen evidence is recorded. Final revision-58 compliance is unavailable. |
| Deployment manifests, operator configuration, and user/operator documentation | `AGENTS.md` §§1-2, 9, 11-12; `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/operations/deployment-and-release.md`; `architecture/operations/reliability-and-recovery.md` | **PROVISIONAL PASS at revision 57** — documentation and deployment files are in the cumulative range and recorded docs checks exist. Final revision-58 compliance is unavailable. |
| `wyrd-testing` release-process harness and consolidated `capacity` benchmark | `AGENTS.md` §§4-6, 11-12 and Rust documentation rules; `architecture/agent-rules.md`; Rust, maintainer, testing, OLAP-serving, analytical-reliability, deployment, and recovery references | **PASS for repository shape** — one registered capacity binary/task remains; `Benchmark` owns the lifecycle; `Deployment` owns measured steps; `StepKind` closes step identity; concurrency, deadlines, cleanup, resource windows, backlog evidence, and reports are bounded and documented. Acceptance of the measured result belongs to task reviewers. |
| Cargo manifests, lockfiles, `mise.toml`, CI selection, and boundary-check allowlist | `AGENTS.md` §§1, 4, 11-12; `architecture/agent-rules.md` feature/task/gate rules; implementation-execution and testing-workflow references | **PROVISIONAL PASS at revision 57** — no new remediation dependency or Cargo feature was added; the stale capacity target/task was deleted; focused commands use the repository toolchain. Final revision-58 compliance is unavailable. |

## Rule-by-rule evidence

| Applicable rule | Source evidence | Result |
|---|---|---|
| Review authority must be the exact human-approved specification revision; tasks and implementation cannot silently substitute another revision. | Caller names revision 58. Candidate `spec.md:1-4` is approved revision 57; `task-008-closeout.md:6-8` and the remediation frontmatter also name 57. | **BLOCKED** — STD-R2-001. |
| A task contract identifies its governing approved revision, mapped obligations, and current lifecycle status. | `task-008-closeout.md:1-18` is now `status: review`, `spec_revision: 57`, maps REQ-171/AC-040/AC-041, removes the obsolete planning blocker, and labels revision-49 prose historical. | **PASS at revision 57**; prior STD-001/FIND-4 is closed. |
| Stateful, dependency-backed, multi-step Rust workflows have one meaningful concrete owner and discoverable inherent methods. | `capacity/main.rs:191-574` places setup-to-report state and `prepare`, `run`, `measure`, `provision`, `scale_out`, `reconnect`, `step`, and `clean_up` on `Benchmark`; `main.rs:621-637` is a thin process boundary. `step.rs:303-513` keeps measured-step behavior on `Deployment`. | **PASS**; prior STD-002/FIND-5 is closed. |
| Closed sets use enums when exhaustiveness matters. | `capacity/step.rs:58-93` defines `StepKind` and its sole label mapping; `report.rs:394-407` selects required verdict steps by the enum rather than strings. | **PASS**. |
| Every materially changed Rust item has substantive rustdoc; fallible functions document errors and effectful async functions document cancellation/partial progress. | Representative lifecycle documentation is at `capacity/main.rs:227-240`, `:298-311`, `:346-359`, `:397-410`, `:455-481`, and `:500-509`; request/client/fixture/evidence boundaries also contain `# Cancellation`; tests contain `# Panics`. | **PASS**; prior STD-003/FIND-6 is closed. |
| Async is limited to real IO or intentional composition; concurrency and time are bounded. | Capacity async methods await server/client/database/network/timer work. `Lifetime` owns an absolute measurement and cleanup budget (`main.rs:124-189`); request work uses the shared semaphore and join ownership; backlog drain uses `DRAIN_LIMIT`. Pure judgment/report/resource arithmetic remains synchronous. | **PASS**. |
| Test-only capacity evidence must not move durable product behavior into a client or alternate engine. | Workload traffic uses the public Rust client and release server; database reads in `capacity/evidence.rs` are benchmark evidence only; production Gate/Scribe/Oracle/Forge owners remain unchanged by the remediation delta. | **PASS**. |
| Raw `PgPool` is banned from library code, while test/binary construction remains boundary-owned. | The remediation's `PgPool` use is in the `capacity` binary's evidence collector, not exported library behavior; no remediation library signature accepts a raw pool. | **PASS**. |
| One server-capacity authority remains and benchmark/tool tasks are explicit, opt-in, and repository-managed. | `wyrd-testing/Cargo.toml` retains only the `capacity` target for this benchmark family; `mise.toml` deletes `bench:bifrost:query-capacity` and retains the Postgres/RustFS-backed `bench:capacity`. Production comparison tools outside this family remain untouched. | **PASS**. |
| Tests may not be weakened, ignored, or replaced merely to clear a gate. | The remediation deletes the obsolete benchmark target but moves each required correctness/durability obligation to existing focused owners, adds unit proof for deadline/backlog/report/resource semantics, retains the cross-replica and SDK journeys, and adds no `allow`/new ignored escape in the remediation delta. | **PASS**, based on source and recorded commands. |
| Build/test commands use `mise`, exact `nextest` expressions for named Rust tests, and repository-managed service setup. | The remediation implementation record at `TASK-008-CLOSEOUT-R1-capacity-closure.md:302-338` records exact `mise exec -- cargo nextest` expressions, Postgres wrappers for DB/journey tests, `mise run fmt`, `mise run lints`, and the opt-in benchmark smoke. | **PASS**. |
| Dependency and feature cost stays in the narrowest owner; no unearned feature is added. | Remediation `Cargo.toml` changes only remove the obsolete binary registration; lockfile and dependencies are unchanged by the remediation delta. | **PASS**. |
| Secrets and diagnostics are redacted and temporary ownership is explicit. | The benchmark continues using `SecretString`, temporary directories for signing/peer/judge material, and retained abnormal logs by path; reports contain configuration/resource evidence, not credentials. | **PASS**. |
| Bifrost saturation evidence includes all server-owned backlog stages and measures comparable resource windows. | `capacity/evidence.rs:89-105` includes `bifrost_scribe_staging_live_members`; `capacity/step.rs:184-231` opens/closes CPU and `memory.peak` over the same measured interval; `Drain::judge` enforces the exact deadline. | **PASS**. |
| Reported evidence must preserve stable meanings without creating an extra product contract. | `capacity/report.rs:192-264` separates engine overhead, judge-provider wait, ingest drain, and backlog; `:371-536` owns one report; `:538-563` renders one shared table schema. No production metric or public API was added by remediation. | **PASS**. |
| Candidate must remain immutable throughout review. | `HEAD` was `5c3bb79b3598abd88a3a234611fc400096adc975` when the review began; only this assigned report path was written. | **PASS**, subject to the orchestrator's final hash recheck. |

## Prior repository-standard finding closure

| Prior source finding | Candidate evidence | Result |
|---|---|---|
| STD-001 / FIND-TASK-008-CLOSEOUT-4 — stale task authority | `task-008-closeout.md:1-18` now consistently describes revision 57 review and maps REQ-171/AC-040/AC-041. | **CLOSED for revision 57**. It does not cure the caller/candidate revision-58 mismatch. |
| STD-002 / FIND-TASK-008-CLOSEOUT-5 — free-function benchmark lifecycle | `Benchmark` owns the complete lifecycle and state; the old free `benchmark` and deployment-mutating free `connect` are absent. | **CLOSED**. |
| STD-003 / FIND-TASK-008-CLOSEOUT-6 — missing cancellation/partial-progress contracts | Every applicable capacity async boundary has a substantive cancellation section naming cleanup, durable residue, retry meaning, or discarded partial evidence. | **CLOSED**. |

## Material finding

### STD-R2-001 — The requested approved revision is absent from the immutable candidate

- **Violated rule:** `wyrd-task-review` “Establish the subject” and repository-standards completeness rule; `architecture/references/languages/spec-driven-development.md` authority order and specification contract.
- **Location:** `changes/active/verified-change-contract/spec.md:1-4`; `changes/active/verified-change-contract/tasks/task-008-closeout.md:6-8`; `changes/active/verified-change-contract/review/TASK-008-r1/TASK-008-CLOSEOUT-R1-capacity-closure.md:5-10`.
- **Evidence:** the caller explicitly identifies approved revision 58, but every governing artifact in candidate `5c3bb79b...` names revision 57. `git show 5c3bb79b...:changes/active/verified-change-contract/spec.md` confirms the discrepancy is part of the immutable commit rather than an uncommitted working-tree artifact.
- **Consequence:** the reviewer cannot map changed Rust, Python, TypeScript, server, contract, test, documentation, deployment, or tooling surfaces to the exact approved obligations the caller requested. Treating revision 57 as equivalent would let an older spec silently redefine review authority.
- **Testable correction/unblock:** provide an immutable candidate containing the exact human-approved revision-58 `spec.md`, with the task/remediation metadata reconciled to that revision when revision 58 governs them, or correct the review request if revision 57 is in fact the intended approved authority. Then rerun the complete task review against that fixed base/candidate pair. This review does not prescribe the content of revision 58.

## Verification reviewed and limits

Recorded evidence reviewed from the remediation artifact:

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity` — 14 passed.
- Postgres-wrapped `wyrd-testing --lib` — 58 passed.
- Exact Postgres-wrapped Rust SDK observe and direct/queued verification journeys — 1 passed and 2 passed.
- Exact two-replica claim/fairness runtime tests — 2 passed.
- `mise run fmt`, `mise run lints`, scoped all-target/all-feature Clippy, and `git diff --check` — recorded clean.
- Reduced two-replica `bench:capacity` smoke — exercised the full topology and cleanup; expected nonzero verdict under shortened windows.
- This reviewer independently ran `git diff --check` for the complete immutable range; it was clean.

Limits:

- No Cargo-backed command was rerun by this delegated standards reviewer; the
  review used the immutable source and recorded results so parallel reviewers
  did not contend for the shared target directory.
- The unmodified default benchmark was not recorded by this remediation and is
  a task-acceptance evidence question, not a separate repository-style defect.
- Most importantly, no verification result can establish conformance to an
  absent revision-58 authority.

## Overall result

**BLOCKED**

The remediation closes all three prior repository-standard findings under the
revision-57 authority and no new material repository-rule violation was found
in the remediation delta. The repository-standards review cannot complete,
however, because the exact approved revision named by the caller is not in the
immutable candidate (STD-R2-001).
