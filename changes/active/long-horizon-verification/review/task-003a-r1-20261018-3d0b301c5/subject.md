# TASK-003a cumulative remediation re-review

- Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001
- Branch: change/long-horizon-verification-task-001
- Cumulative base: 22335c2c40a5ad24c27bfa0d4686f4da49f922c5.
- Prior candidate / latest-fix base: 7cda4e93159ab328de2a74068d5e1978a2590818.
- Immutable candidate: 3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4.
- Full acceptance range: git diff 22335c2c40a5ad24c27bfa0d4686f4da49f922c5...3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4 (84 files, +5256/-2317, including prior reports).
- Latest-fix range: git diff 7cda4e93159ab328de2a74068d5e1978a2590818...3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4 (56 files, +3263/-483, including prior reports).
- Original task: changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md, approved original content from cumulative base. Its appended evidence is not authority.
- Approved current spec: changes/active/long-horizon-verification/spec.md revision 18, status approved.
- Prior audit/R1 directory: changes/active/long-horizon-verification/review/task-003a-20261018-7cda4e931/.
- Prior required inputs there: verdict.md, findings-validation.md, TASK-003a-R1-retain-pending-work-and-remove-stale-code.md (including appended implementation evidence). Read original task and prior findings for closure, not just the R1 diff.
- Initial worktree clean; HEAD is exact candidate. Only this new report directory may be written by reviewers. No source, spec, task, fixture, generated-output or earlier report changes. No commit/push/merge/deploy/cleanup.

## Controlling owner decisions

Revision 18 and R1 explicitly supersede earlier FIND-1/FIND-2 corrections. Retryable Wyrd/capacity/configuration/query/blocked-enqueue problems retain pending and trace; true owner inactivity/retirement or auth rejection records one terminal errored group execution and consumes pending atomically. Error-recording failure retains pending. No evaluation, verdict, result pointer or Operator. Pre-admission failure may omit authority/snapshot only in truthful terminal shape; runnable authority/input invariants remain. Existing latest-binding-execution ID then execution status supplies user visibility; no new history API. Read still requires authorization.

Reject group_end.after above one fixed year (365d = 8760h = 525600m = 31536000s) at write time for standalone and inline Triggers. Preserve accepted values exactly; remove silent SQL cap. Pre-release/reset policy applies, no upgrade compatibility machinery.

Accepted limits are not findings: ACK may precede complete query visibility; post-ACK/pre-pending crash can lose scheduling signal; outbox/retention loss; continuous arrivals postpone due time; distinct snapshots overlap and react by settlement order; durable SQL backlog growth; no strict global fairness/quota. No visibility barrier, source-terminal waiting, recovery ledger, hard finality, custom source ceiling or serialization/adoption gate. REQ-015/AC-009 benchmarking intentionally pending.

Older TASK-001 packets under other review directories remain superseded. The one prior TASK-003a directory above is explicitly required for this remediation round; do not revive older packets. Owner-directed cleanup remains mandatory but gets no retroactive FIND IDs.

## Common navigation map (starting point, not exhaustive)

| Owner/path | Changed seams | Callers/consumers/proof |
|---|---|---|
| wyrd-server verification/scheduler.rs | GroupHandoff, hand_off_groups, hand_off, refusal recording, separate cron/group loops | authority resolver, poller, queue; C13/P1 actual loop, retry/terminal record/atomic consume/ACK race/capacity |
| verification/authority.rs and groups.rs | typed owner rejection vs Wyrd configuration/store errors; preserve query auth denial vs transient read | shared cron resolve sibling; Scribe-derived due identities; terminal insertion; accepted query path |
| wyrd-sql queries/verifier_executions.rs; group pending migration plus cumulative prior migrations | claim retains actual inactive principal identity; pre-admission errored row; narrow nullable authority/snapshot; exact after; unused projection cuts | claim/settlement/manual/cron siblings, status and binding last_execution_id; C9/P2, SDK P3 |
| wyrd-spec card/trigger.rs | positive max365d shared parser and generated descriptions | standalone/inline Trigger decode, raw server registration; P7, P8, P3 |
| wyrd-spec reference.rs and card/verifier/mod.rs | manual InlineableRef deserialization and InputSlot visitor/tagged remote mirror | ALL inline refs and typed/source slots, authoring/schema consumers; focused decoder tests, whole spec test evidence |
| server pg_card_registration_route.rs, pg_verification_groups.rs; SQL pg_verifier_executions.rs | raw invalid write boundary, terminal no-snapshot shape, actual cron/ACK/query observations | current focused selectors and task evidence; formerly vacuous assertions |
| Rust verify_in_real_time.rs | real Python/TS child-process shared C1; terminal user readback, write-time rejection, renamed helpers/fixtures | actual runtime subprocess lifetime, support/late/coding/coexistence/owner-grant callers |
| Python/TS verify-in-real-time tests; MCP verification.rs | bound source pressure/retry/readback; webhook changes; renamed helpers | C11, refund, coding, media/discovery, language-owned lifetime |
| fixtures/cards/verify_in_real_time and fixtures/invalid | six atomic group fixture renames, webhook, long-wait365d; three negative fixture moves | all refs and valid-fixture sweep; coding-child must be Verifier, not vacuous Service test |
| cumulative original owners: scribe_outbox, runner, claims, runtime composition, sources/Visible/Earliest, contract snapshots | ACK-to-pending without request SQL; pinned visible evidence, no completeness/discovery; normal runner/capacity/settlement | original A1–A11 acceptance, verify/submit, schedules/operators, SQL/RLS, SDK/MCP projections |
| architecture/docs/generated TS | rev18 failure and max-after semantics; stale ceiling/only-automatic-Trigger removed | source-driven descriptions, authored Python snapshot typing, user query/readback docs |

## Review duties and evidence

Read .agents/skills/wyrd-task-review/SKILL.md role, AGENTS and agent-rules, spec-driven-development, maintainer-style and applicable design/doctrine/Bifrost/reference authorities. CodeGraph first when locating code. Audit complete cumulative acceptance plus prior FIND-TASK-003a-1..6/HUMAN-CLEANUP1..3 closure; source evidence required. Expand map through real producers, every shared writer/caller and siblings. Matching diagnoses/green tests are claims, not proof.

Candidate R1 appended evidence records all focused selectors/counts, static checks and diagnoses. Independently assess manual decode changes, typed-input error propagation, intentionally invalid fixture relocation and corrected coding-child kind assertion. Those shared changes affect more than grouping even if wire shapes appear unchanged. Assess their necessity/root cause and all affected acceptance sets. Treat scope extension as finding only with demonstrated unearned code or material behavior/regression/constraint consequence; pre-existing failures cannot simply be ignored.

Reviewers receive no intended verdict and no sibling conclusions. No heavy builds/tests by subagents without coordination: ask root for exact focused proof if needed. All tests via mise, all Postgres-backed work strictly sequential (supplied evidence records environment collisions when run concurrently). Root will coordinate focused reruns; no broad gate or benchmark. No synthetic host load. Use exact named selectors and record nonzero counts.

Report only at assigned path. Do not read current-round sibling reports. A fresh validator will later validate the union and stable closure ledger. New findings use next unused FIND ID after six; preserve old IDs for unclosed shared causes. Optional preferences do not block; missing substantive rustdoc on changed Rust items does by AGENTS. Accepted limits remain excluded.
