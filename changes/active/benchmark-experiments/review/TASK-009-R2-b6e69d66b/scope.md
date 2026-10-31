# TASK-009 remediation review scope

Original task: `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md`, revision 2; approved `spec.md`, revision 24. Original range `6f38db16c524417301ffc4a5dc0b478daf997ed1..f0ea3b4023e544d0f95e9b5e38d398105dbe2e33` remains reviewed by `../TASK-009-R1-f0ea3b402/verdict.md` and its ledger, without replacement.

This round: `git diff f0ea3b402 b6e69d66b`; cumulative candidate `b6e69d66b5657881f7d605a0e308b2985464c420`. `a8590dcc89b7afcf723039048a1dde1e3ecffcd5` records the prior review; `b6e69d66b` supplies remediation. HEAD initially equals the candidate. Existing untracked `tasks/unified-verification-flow.html` is excluded and preserved.

Review prior FIND-TASK-009-1..4 closure, remediation Implementation Evidence, changed paths and their affected consumers only. Carry forward all unaffected acceptance/source evidence from the prior verdict and specialist reports; no new runtime admission implementation exists in this delta. User accepts the earlier L1 records, explicitly defers final gate/measurements/records/PR until after this review, and prohibits gate, benchmarks, commits and pushes now. Distinguish bounded remediation acceptance from whole-task completion and pending original closeout.

## Navigation and authority

- `verification_runtime.rs::queued_origins_share_ack_gated_admission`: hydrate support and observed-service from declared fixtures; WyrdState -> Run::observe -> Observe::submit/verify -> existing publisher/admission; held execution-input barrier, scheduled/group timing fixture controls, status and immutable readback. Inspect fixture declaration/binding, hydration lifecycle, and underlying submit dispatch as needed.
- `scheduler.rs::InFlight::drop`: rustdoc only; preserve release and poison recovery.
- `architecture/wyrd-design.md` group selection/generation and schedule population paragraphs; `architecture/bifrost-design.md` selector publication/admission paragraph. Compare scheduler/groups preparation and SQL prepare/accept generation semantics; no new runtime semantics expected.
- Original task evidence and remediation evidence: accurate SHA attribution and pending V13–V16; prior report-only addition preserved.
- Root AGENTS, spec-driven-development, maintainer-style, agent_docs index/Rust/testing guidance, architecture authority map and owning paragraphs, approved revision-24 supplement apply. Discover nested AGENTS for touched ancestors; SQL owner rules apply if tracing SQL.
- CodeGraph exists and initial exploration covered queued journey, Observe::submit, prepare_group, accept_grouped, hand_off_groups, InFlight. Use CodeGraph before additional code navigation.

## Reviewer selection

One fresh remediation discovery `ponytail-rev` writes `task-review-behavior.md`. One separate fresh independent validator follows discovery and writes `findings-validation.md`. No shared-state/domain/system/maintainer/repo specialist selected: runtime delta is method documentation only; behavioral change is the existing SDK journey and its fixtures, plus authority/evidence reconciliation. Mandatory rules remain within discovery/validation. Add focused source validation only for a new blocker or material unresolved conflict. Reports live only in this directory; no source changes or stored diffs.
