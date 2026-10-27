# TASK-004 first remediation review

## Immutable subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
- Prior candidate / remediation base: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
- New cumulative candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
- Review diff: `git diff a020500e8ea0c39311d508d6b5bfa85ebe90368c c9d72729b85187f2399725e0ed9496c4c4a39298`.
- Cumulative original context: `git diff 0b500d677531ba501924b1c6d412fe14ff9aa5cd...c9d72729b85187f2399725e0ed9496c4c4a39298`.
- Approved spec: `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. Explicit approval persists from the preceding review sessions; stale draft wording is not an open approval question.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md`.
- Required prior context: `../TASK-004-R1/verdict.md`, `findings-validation.md`, discovery reports and verification; `TASK-004-R1-complete-cohort-timing-evidence.md` with candidate implementation evidence. Prior untracked review reports are now committed in the candidate; distinguish that report addition from executable remediation.

No ancestry change occurred. This is the first remediation review after the first complete TASK-004 review. Preserve stable FIND-TASK-004-1 through -6 IDs. No implementation work is authorized by this review; the user has a separate implementor.

## Scope and carried-forward acceptance

Review prior-finding closure, the complete remediation diff, and affected producer/consumer/caller paths. Carry forward source-backed passing acceptance in the prior validated ledger for unchanged local bounded HDR/concurrency/bucket arithmetic, engine timer meanings and wait unions, recorder/listener exposure, ordinary tracing, legacy v1/unknown-schema behavior, existing summary publisher, dependency/platform constraints and non-goals. Recheck only dependencies affected by classification, checked rank conversion, resource clocks, new summary/evidence fields or artifact writes; record reasons for any expanded scope. Do not repeat whole-task discovery.

## Navigation map (starting point, not complete coverage)

- F1: shared `capacity.rs::Window::measure`, `close`, `until_stop`, resource freeze; query isolated and run Admission/Outcome late fields; Bifrost competing close; Verification closed/drive/observe; Gateway closed/run and L2 schedule/late conversion. Trace actual admission stop, CPU/work/T and bounded final cohort through Repetition.
- F2: client `with_benchmark_labels`, `benchmark_headers`, actual unary/native/HTTP/OTLP sends and retries; query Read/WriteOp/Admitting phase, probes/background context; Gateway Case/Period/cohort/raw reqwest producer; Verification admission/Op. Shared Population Expected/expect/scan selection matches finite case/outcome/phase through final drain. Evaluate recovery grace changes against existing authority. Incidental groups need separation to prevent masking; do not invent arbitrary per-request identities or a blanket logical-call/physical-attempt equality requirement.
- F3: Population snapshots and validate(window), Repetition reconciliation/timings, Reading rank/max, experiment TimingEvidence/keep_timings and runner write/error path. Trace before/after artifacts and selected summary readings for actual engine steps and phases, including after source teardown. Preserve target-only buckets and summary-only rendering.
- F4: timing Pending/activate_request and labels lookup; HTTP trace span/setup, authentication bearer/browser/API-key successes/refusals; Gateway ingress verified context; gRPC principal extraction; Gate verified AuthContext/native write. Trace actual request span inheritance and existing timing/auth/audit/tenancy owners. Scope known proof gaps from source and explicit obligations, not just the completion account.
- F5/F6: changed-item rustdoc, checked rank conversion and installed num-traits dependency. Inspect new/changed items introduced by remediation as well as listed prior omissions; no old-code documentation sweep.
- New/changed tests are recorded in remediation evidence: stop-before-drain, query late completions, admission-cohort crossing, recovery grace, retained evidence reconstruction, scan matching, scoped headers, HTTP verified activation and Gate verified writes. Tests are evidence to evaluate, not task authority.
- Root AGENTS and applicable Rust/maintainer/architecture/testing guidance govern; no nested AGENTS applies to changed source paths. CodeGraph exists and precedes code navigation.

## Selected reviewers and exact reports

This round adds two narrowly scoped discovery specialists because the fixes materially change driver concurrency/windows and multiple verified-context activation paths. It does not recreate the original five-role first-review team.

1. Fresh `invariant-rev` for F1/F2/F3 closure and fix-induced effects on actual clocks, cohort/scan reconciliation, evidence retention/rank/summary windows. Exact report `task-review-invariants.md`.
2. Fresh security `domain-rev` for F4 closure and affected metadata trust/auth paths, with F2 propagation context available. Exact report `domain-review-security.md`.
3. After selected discovery/follow-up completes, fresh independent `ponytail-rev` validates every closure/proposal, F5/F6 and changed-item rules, complete remediation coverage and carried evidence. Exact report `findings-validation.md`.

No separate behavior/repo/maintainer/system discovery selected: prior source-backed coverage carries, while the validator covers the remaining changed owners/rules and specialists cover materially changed sensitive behavior. Add a role only for a material unresolved question. A newly discovered blocker from validation requires separate focused source validation before retention.

During independent validation, a possible new omission under existing F5 was discovered: the newly added `timing.rs::Pending` tuple field may lack mandatory field rustdoc. Select one fresh focused `followup-rev` to independently check only that claim, its exact changed source and governing rule, material gate and smallest correction. Exact report `followup-review.md`. This does not reopen old documentation or add a new finding identity. The validator cannot validate its own new blocker; retain it only if separately source-confirmed.

## Execution and evidence

Reports only in this new directory. No source edits, implementation, formatting mutations, commits, pushes or deployments. Preserve pre-existing untracked benchmark folders. Orchestrator owns sequential Cargo checks; reviewers must request focused runtime checks rather than competing. Review will rerun exact new regressions and narrow affected owner lanes/auth seams, using repository Postgres lifecycle. No full capacity suites, release smoke or broad aggregates are planned for this bounded remediation; their integrated obligations remain. Known limits in evidence are neither automatic waivers nor automatic findings: require source-proven failure of an original obligation/material supported regression. No intended verdict or finding quota.
