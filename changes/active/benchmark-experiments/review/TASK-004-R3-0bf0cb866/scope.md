# TASK-004 second remediation review subject

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
Prior candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
Candidate: `0bf0cb8662a221233a963ed147d57ef0cbb6720f`.
Review range: `git diff c9d72729b 0bf0cb866`.

Read original TASK-004, R1 and R2 remediation, prior verdicts and validated
ledgers. Carry forward unchanged acceptance; review residual F1/F2/F3/F5,
maintainer-directed real-path smoke, and affected paths. F4/F6 were accepted,
but classification/auth changes explicitly reopen F4's affected boundary.
Do not expand into TASK-005 qualification or its planned instrumentation audit.

Authority is the maintainer-approved revision-8 contract plus the explicitly
directed revision-9 smoke correction, preserved in working-tree spec.md.
The stale draft header is not a new approval blocker. Record the working-tree
authority identity; source is pinned to the candidate. TASK-005 uncommitted
revisions are dependency context, not implemented obligations for this task.

Working-tree spec SHA-256 at review start:
`007d27e3a0e22a4aa912108da6a7478849a3f2f56c953fdea9e1491c65e96dce`.
TASK-005 dependency SHA-256:
`1c61c344c800e6630226e4d091fd5ee2c18a4b861eaf9e971f7eda08946e6c64`.

PR 215 enters at merge 9a540b7b8 (parents 7746a4073 and 1c52b78bb).
This review checks its integration with benchmark source, renamed identities,
and materially affected timing/auth producers and consumers. It does not
restart the separate schema-change implementation review. Use first-parent
changes before/after that merge and the cumulative range to distinguish origins.
Read merge conflict decisions and the imported change's existing review evidence.

## Navigation map (starting points, not a coverage limit)

| Surface | Owners / affected paths | Proof / consumers |
|---|---|---|
| Stop and work | capacity.rs Window::measure/until_stop/ResourceWindow; query isolated/competing; gateway run/capacity; verification run | stop byte deltas, late/drain accounting, scheduled admission tests, Repetition conversions |
| Raw OTLP | query/run.rs authorized / Sender::send; client scoped benchmark_headers | spans/logs receiving classification, exact population matching |
| Maximum | query/competing.rs read_queries/server_percentile | selected cohort max rank/bound |
| Smoke orchestration | experiment.rs Mode/Declaration/run/judge_criterion/Record; capacity levels.rs/main.rs; engine declarations/workloads; mise bench tasks | actual fixtures/steps/phases, validity/judging/persistence/report, baseline exclusion, full prepass |
| Production classification | telemetry timing.rs label_request/Labels/record; HTTP authenticate, principal extractor, Gateway ingress, Gate auth/mod and OTLP gRPC | trust boundary, refusal behavior, disabled collection, existing authentication/audit/tenant ownership |
| Merge seams | benchmark fixtures/SQL/catalog names, telemetry capture resources projection, production timing call sites | existing PR215 journeys and benchmark callers; no broad unrelated schema re-review |

## Reviewer selection

1. Fresh remediation discovery ponytail reviewer: residual closure, changed
   benchmark paths, shared measurement invariants, smoke acceptance and proof,
   changed-item repository rules. Report `remediation-review.md`.
2. Fresh maintainer reviewer: substantial replacement orchestration and its
   existing owners/callers, including lifecycle and reporting reuse. Report
   `maintainer-review.md`.
3. Fresh security domain reviewer: materially changed classification/auth
   receiving boundary and affected production callers after merge. Report
   `domain-review-security.md`.
4. Fresh independent validation ponytail reviewer after discovery: validates
   union, closures, affected coverage and smallest correction. Report
   `findings-validation.md`.

No separate repo/system/general first-review team: changed rules and ordinary
benchmark failure lifecycle belong to selected reviewers. Add a focused
follow-up only for an unresolved material path/conflict or validator-new blocker.
Reviewers receive this same map, authority/task/evidence paths and immutable
diff, not the implementor's conversational summary or an intended verdict.

Review artifacts only in this directory. No source edits, commits or pushes.
