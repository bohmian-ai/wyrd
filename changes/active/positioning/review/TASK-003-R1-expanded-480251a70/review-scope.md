# TASK-003 expanded remediation review

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
Prior candidate: `634e19cbc517293ed2458f30ce73401714f3447d`.
Candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`.
Remediation diff: `git diff 634e19cbc 480251a70` (70 paths).
Cumulative context: `git diff 5775c3795...480251a70`.
No ancestry rewrite; two descendant commits, `2add347cd` and `480251a70`.

## Authority and evidence pins

User explicitly identifies these working documents as approved authority. Their content is read-only and pinned with `git hash-object` (hashes need not be stored Git objects):

| Input | Content hash |
|---|---|
| `changes/active/positioning/spec.md`, approved revision 6, including REQ-017/AC-016 | `7f9bdb2a67e6b113b02a58526925b75f91028112` |
| `changes/active/service-identity-and-telemetry/spec.md`, approved revision 17 | `6561fb1a747687f56a051edfeff23cc1f0e2375f` |
| Original `changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md` | `6846032793910170e76c0cc0ec9de2f50cb6ad6f` |
| Prior remediation `changes/active/positioning/review/TASK-003-first-634e19cbc/TASK-003-R1-complete-schedule-reader-setup.md`, including evidence | `6f3484e808b5015efa78abdf0c2b52863082fea5` |

Original task revision-5 metadata and no-runtime mechanics remain original acceptance context; the maintainer's explicitly approved revision 6 and current architecture govern the added SDK scope. Do not reject the authorized additions merely by applying superseded spec5 non-goals. User confirms architecture update was approved; read the owning committed sections, not design notes as normative authority. No authority is edited during review.

The committed source candidate is 480251a70, not a working-tree snapshot. Uncommitted Python/TS telemetry-off tests are supplemental evidence only, not candidate contents. Unrelated dirty positioning/mockup/TASK-004 files and verification-model packet are excluded. No staging/commit/source changes are authorized. Check HEAD, authority/task hashes and changed source cleanliness before verdict. Candidate change returns BLOCKED.

## Prior context and bounded expansion

Prior substantive review: `../TASK-003-first-634e19cbc/{verdict.md,findings-validation.md,task-review-behavior.md,standards-review.md,maintainer-review.md,domain-review-evidence.md,verification.md}`.
Prior findings: FIND-TASK-003-1 (schedule reader setup) and FIND-TASK-003-2 (mandatory rustdoc). `../TASK-003-R1-subject-check-634e19cbc/verdict.md` was a subject-only block, not an unsuccessful implementation review. This is the first substantive remediation review; no convergence assessment is yet required.

Carry forward source-backed prior coverage of unchanged schedule execution/windows, retained investigation/tenant refusal, routing/dispositions and runtime engine ownership. Reopen request setup/handler and guide rendering for 2add347cd; reopen refund execution evidence, all changed SDK lifecycle/authoring and affected consumers for 480251a70, because their dependencies and public contracts materially changed. Do not restart four-tutorial discovery wholesale.

## Common navigation map (starting point, not exhaustive coverage)

| Changed contract | Owners / producers | Callers, consumers and tests |
|---|---|---|
| Flush with no writer; closed still refuses | `wyrd-client/src/observe/lifecycle.rs::BifrostLifecycle::flush`, `state.rs::WyrdState::flush` | Rust observe tests, Python/TS wrappers, telemetry-only refund programs; compare `started`, start/shutdown and drain settlement |
| Default configure installs managed telemetry; opt-out; frozen failures; TS async | Python `src/state/mod.rs::configure`, internal installer and `python/wyrd/otel.py`; TS `src/index.ts::WyrdState.configure`, `#startTelemetry`, `src/otel.ts` | ASGI middleware, support-desk examples, configured-context/local-dev/observe/action/gateway integrations, unit telemetry/state tests and stubs; Rust setup intentionally unchanged |
| Same-name Trace marker lowers to canonical trace slot | Rust `authoring/verifier.rs::{Trace,SLOT_KEYWORD,InputSlot}`, public authoring/verify reexports; Python `verify/__init__.py::Trace`; TS `definitions.ts::Trace`, native cards input lowering | Model/mapping authoring paths and schema traversal; Rust/Python/TS authoring tests; actual refund model serialized into verify inputs; stored contract/native trace check unchanged |
| Original work/evidence | `examples/refund-agent/refund_agent.{py,rs,ts}`, configure → actual spans → flush → model verify | Python building-block journey, Rust examples refund journey, TS refund journey, investigation expected captures and exact retained attribution |
| Request state ownership / rendered source | `examples/finance-release/api.py` lifespan and Depends; FinanceFlow; Aside blank lines | Actual handler journey, shell-links test, code embeds/request guide and generated full text |
| Prior finding closure / reader setup | schedule guide per-language CLI and Zod; five Rust documentation items | Pinned R1 evidence; clean-reader setup, generated text, unchanged scheduler programs/tests |
| Public/generated surfaces | stubs, TS generated declarations, error catalog remedy, setup prompt/MCP guide, ASGI/observe/recipes and agent guide | Actual public exports/callers and docs/contract generation; query change compared with stored-verdicts.sql |

## Specialist selection before discovery

| Boundary | Selection and source-backed reason |
|---|---|
| Shared state and process-global telemetry lifecycle, startup/flush/shutdown interleavings and opt-out | invariant-rev: newly composed configure and existing global provider/phase owners materially change lifecycle behavior; prior review did not cover this |
| Cross-tier Rust/PyO3/TS authoring/public export/stub/error/generated/rustdoc and docs rules | repo-rev: broad 70-path SDK+contract+docs changes exceed prior standards coverage; include prior rustdoc closure |
| New marker/lowering and installer composition ownership/reuse | maintainer-rev: inspect complete workflows against existing input/schema and provider owners, not just leaf reuse |
| Failure propagation / application-owned providers / optional packages / async startup / shutdown | system-rev: configure now installs process-global telemetry and can fail application startup; unchanged app-owned provider and sibling state availability must be traced |
| Trace evidence type lowering and actual-work judgments | domain-rev (evidence): new marker changes what inputs become trace queries, and changed refund setup affects sufficiency. Preserve exact canonical slots, grants and native absence judgments |
| Comprehensive first-review behavior agent | omitted: this is remediation. Fresh ponytail validation owns closure and changed caller/reader acceptance; five scoped discovery specialists cover material expansion, prior behavior matrix supplies unaffected coverage |
| Separate security/RBAC, SQL/storage/persistent-data specialists | omitted provisionally: no permission/authentication implementation, migration, storage engine or SQL capability change. Evidence reviewer covers trace-slot/tenant semantics; system/invariant reviewers must flag any actual changed sensitive boundary |

Each reviewer reads its assigned boundary at recorded commits, expands the common map independently, uses applicable root/nested rules and only owning architecture sections, and writes its exact assigned report in this directory. No .codegraph directory exists. Do not spawn extra agents or mutate source. No overlapping Cargo commands across agents; orchestrator owns checks. No intended verdict is supplied.

After discovery, compare proposals by invariant, use focused follow-up only for unresolved material conflict/path, then fresh ponytail validation writes `findings-validation.md`. Preserve original FIND IDs for closure; next unused new finding is FIND-TASK-003-3. A new validator-origin blocker requires separate source validation. Reports only; no patches, diffs or archives in review directory.

## Available verification

Pinned original/R1 files contain supplied earlier execution evidence, including isolated-reader TS/Python commands and limitations. Current user supplies Rust fmt/clippy, client 290/spec 766 lib tests; Python touched unit/lint/type checks; TS 95 units/lint/format/type checks; codegen/docs tests/command check; actual refund journeys all three SDKs, touched Python integrations/local-dev and 7 TS integration files. These are reported results, not independent reruns. Full docs:check was not run. Uncommitted opt-out tests must not be counted as committed candidate tests. Fresh orchestrator checks will be recorded separately.

No verification/gateway/Oracle/Scribe engine hot-path implementation appears changed; SDK configure/declaration/explicit flush are lifecycle/authoring operations. Reviewers must reassess from actual call paths. No benchmark exception is granted if a designated hot path is materially touched.

## Concurrent working followups

After subject establishment, external working edits appeared in `examples/refund-agent/refund_agent.py` (direct model verification) and `docs/AGENTS.md` (direct-model authoring instruction). They are excluded from committed candidate 480251a70, which remains unchanged. Reviewers read candidate source and committed instructions through Git and distinguish these subsequent working followups; no dirty fix is counted as accepted candidate content. Fresh workspace docs checks disclose this limitation in verification.md. The approved spec/task hashes remain pinned and unchanged.
