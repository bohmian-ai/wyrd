# TASK-003 — FIX_REQUIRED

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
Candidate: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
First full range: `git diff 7f422c53ad54e370c24cb971a343ec5e06651109...45641bd5eadf8c86111d82fda6afd9605e218f84`.
Original task: `changes/active/benchmark-experiments/tasks/TASK-003-production-tracing.md` at candidate.
Governing spec: working revision 8 of `changes/active/benchmark-experiments/spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`, explicitly approved by the maintainer during this review session. This instruction supersedes stale draft frontmatter and earlier outstanding-approval evidence. Candidate Git contains revision 6; the approved revision-8 text is external review authority. Approval is resolved and is not a finding.

The earlier `TASK-003-authority-check/verdict.md` covered authority establishment only. It supplies no implementation acceptance or findings. This is the first complete implementation review, not a remediation round. No prior-finding closure or convergence assessment is applicable. HEAD and approved-spec hash remained unchanged during review; no implementation files were edited. Unrelated worktree edits were preserved.

## Scope, selection and independent reports

`scope.md` records the complete navigation map, selection reasons and subject. Fresh behavior discovery covers the full task, repository rules/rustdoc, owner reuse, filtering/sampling, tenant caller-to-result paths, tests and docs. Fresh system discovery covers materially changed export-loss, saturation, process availability and shutdown/recovery obligations. No additional domain/invariant/maintainer/repo specialist was required because existing owners remain in place; behavior review retains those applicable duties and system review covers lifecycle. Both reports are complete:

- `task-review-behavior.md`: FAIL, proposals BEH-001 and BEH-002.
- `system-review.md`: FAIL, proposal SYS-001.
- Fresh independent `findings-validation.md`: recommends FIX_REQUIRED, independently confirms/revises all three proposals from source and assesses acceptance coverage.

`claim-comparison.md` records proposal grouping and why no extra follow-up discovery was needed. Direct-SDK lifecycle mechanics and actual production lifecycle invocation are distinct obligations, not a reviewer conflict. No proposed finding was accepted by agreement alone. Validation found no new blocker requiring separate focused validation.

## Reconciled acceptance matrix

The full source/verification matrices are in the discovery reports and reconciled independently in `findings-validation.md`; they are incorporated here without carrying forward evidence from a prior round.

| Obligation | Source and proof | Result |
|---|---|---|
| REQ-017 independent filters, 0.01/default validation, remote-client isolation, zero/no-endpoint tenant copies | Existing sampler/routes and shared subscriber, server validation; rerun V1/V2/V4, quiet V5 and V6 | PASS |
| INV-001/003/010, AC-010 tenant authorization/catalog/scrubbing/nonblocking delivery preserved | Unchanged tenant owner precedes operator, real timeline/tenant-negative and cap/rejection paths | PASS |
| REQ-018 accurate failed-call metric, stock bounded nonblocking processor, result/delegation and unsampled correlated errors | Narrow exporter adapter and existing SDK mechanics; rerun V1/V3/V6 | PASS |
| REQ-018/AC-011 deployed orderly shutdown reaches SDK total and final drain | Global subscriber retains provider; application only drops guard; V3 invokes provider directly | FAIL — FIND-TASK-003-3 |
| REQ-017/019 operation-boundary inventory, optimization and normal/temporary/restore policy | Unchanged engine instrumentation, reviewed source inventory and representative timeline readback, owning documentation | PASS except shutdown promise covered by FIND-TASK-003-3 |
| AC-011 delivered quiet-policy regression selection | Expanded review selector passes, but new test is ignored and recorded V5 omits it | FAIL — FIND-TASK-003-2 |
| Mandatory changed Rust item documentation | New field/error/panic documentation omissions | FAIL — FIND-TASK-003-1 |
| Non-goals, existing ownership and dependency boundaries, no unrelated candidate changes | Full diff/manifest/source review by discovery and validation | PASS |

## Independently validated ledger

| Stable ID | Discovery / decision | Classification and obligation | Exact correction boundary |
|---|---|---|---|
| FIND-TASK-003-1 | BEH-001 / CONFIRMED | VIOLATION — mandatory changed-item rustdoc | Complete the new telemetry items' field/error/panic documentation; exact locations and truthful contracts in validated ledger. Batch with runtime correction; no new checker or broad documentation sweep. |
| FIND-TASK-003-2 | BEH-002 / CONFIRMED | VIOLATION — task no-new-ignore and required quiet-policy proof | Remove new ignore at SDK action timeline line 191, preserve existing lifecycle/assertions, include quiet story in V5. Existing shared-family Postgres and nextest isolation suffice. |
| FIND-TASK-003-3 | SYS-001 / REVISED | MISSING — REQ-018/AC-011 actual orderly SDK shutdown | Shared telemetry guard must explicitly shut down its provider on disposal despite retained subscriber/tracer ownership; preserve exactly-once explicit shutdown, no-provider behavior, logging and post-drain process order. Reuse stock bounded lifecycle. |

`findings-validation.md` preserves each finding's exact locations, violated obligation, producer-to-consumer and sibling-call tracing, observable consequence, materiality, correction burden, selected minimal correction and focused closure proof. These are the only retained findings; no optional routing/filter hardening is remediation. No new specification decision is required.

## Verification and limitations

`verification.md` records exact commands and results: V1 1/1, V2 1/1, V3 1/1, V4 16/16, V5 3/3 real SDK timelines/tenant-negative, V6 2/2 server rejection journeys and 1/1 cap test. Repository-managed migrations and base-to-candidate whitespace check passed. These results were rerun sequentially by the orchestrator against unchanged candidate source.

Implementation evidence reports format/lints, config tests, dependency/tenant gates and docs:check passed; those checks were not repeated here. No broad gate, full language suites, benchmarks or live process restart ran. Green checks do not exercise the missing deployed guard shutdown or waive manual repository gates. Full language-surface/integrated qualification stays with change review.

## Verdict and handoff

**FIX_REQUIRED** for **FIND-TASK-003-1**, **FIND-TASK-003-2**, **FIND-TASK-003-3**.

Self-contained remediation: `TASK-003-R1-production-shutdown-and-proof.md` in this directory, routed to `$wyrd-implement`. Review did not implement, commit, merge, push or deploy. The next review checks these closures, the prior-to-new candidate diff and affected guard/caller/test paths, carrying forward unchanged evidence under the skill's bounded remediation rules.
