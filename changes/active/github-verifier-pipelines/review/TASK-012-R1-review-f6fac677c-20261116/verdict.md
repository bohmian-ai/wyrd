# TASK-012 R1 verdict

FIX_REQUIRED

Five independently validated findings remain: FIND-012-1, FIND-012-2, FIND-012-6, FIND-012-7 and FIND-012-8. FIND-012-3/4/5 are closed. R1 fixes the tested queued-settlement, live-state, workspace and guard lifetimes, but active settlement, terminal copies and media representation accounting remain incomplete; G1 adds a cancelled-load metadata leak. The mandatory documentation gate also remains open.

## Immutable subject and round

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Branch: task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Original base: c24e1db8f6641cca9912376a4587e63e2fe74dd1.
Prior candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46.
Candidate: f6fac677cd71208871fa5c21e4035085026a99c4.
Reviewed delta: git diff 901639300 f6fac677c. Continuous ancestry was confirmed; original-base-to-candidate remains cumulative context. HEAD remained at the candidate through final verification.

Approved spec: changes/active/github-verifier-pipelines/spec.md revision 15.
Original task: changes/active/github-verifier-pipelines/tasks/012-integrate-code-mode.md.
Prior acceptance/finding context: review/TASK-012-first-review-20261116/verdict.md and findings-validation.md.
Remediation contract: that directory's TASK-012-R1-preserve-settlement-and-resource-custody.md, including committed Obligations, Evidence and diagnoses.

This is the first remediation review. It covers prior-finding closure, the remediation delta and affected producers/consumers/invariants/tests. Source-backed unaffected acceptance carries forward; no full integration rediscovery was performed. Reopened scope and reasons are in scope.md: OBL-3/6/7 custody, changed-item documentation, G1 cold-loading and G2 sandbox test proof. A two-unsuccessful-remediation convergence assessment is not yet required.

Current explicit directions govern: no benchmarks, synthetic host load, full journey suite, gate, multi-pod proof or architecture/wyrd-design.md edits. TASK-013/014 own combined-server qualification. PostgreSQL owns coordination time, accepted server-clock gateway admission remains accepted, and the original struck proofs remain excluded. The directly approved G1 cold-load decision narrowly supersedes older no-cold-read wording; this review does not create another approval gate or reverse it.

## Independent selection, coverage and comparison

scope.md records the common navigation map, materially affected contracts, producers/consumers, selected specialists and omitted-role rationale. findings-validation.md independently checks that selection and all carried coverage. Every selected report is complete:

| Selected review | Scope | Result |
|---|---|---|
| task-review-invariants.md | State/frozen/outbox/read, workspace and media custody | FAIL |
| system-review.md | Active host-work settlement, discard/deadline/queued terminal and failure propagation | FAIL |
| domain-review-concurrency.md | G1 shared cache load state, callers, cancellation, tenant/invalidation/recovery paths | FAIL |
| maintainer-review.md | Changed owners, full-workflow reuse and abstraction burden | PASS |
| standards-review.md | Actual changed runtime/Vala/server/testing items, rules, docs and test diagnoses | FAIL |
| findings-validation.md | Fresh independent source/materiality/correction and acceptance/coverage validation | FIX_REQUIRED recommendation |

Prior domain-security assertion evidence carries for accepted caller, grants, audit, nested restrictions, publication identity and session isolation: R1 changes internal custody, not those decisions. G1 tenant SQL and admission consumers were reviewed anew. Prior domain-records assertion evidence carries for durable acceptance, unknown state, fences, group settlement, ACK order and migrations: those mechanics are unchanged, while changed custody/timeout seams were reviewed by invariants/system specialists. Unchanged SDK, generated, dependency, deployment and UI surfaces do not require new reviewers. No additional required specialist was omitted.

comparison.md groups proposals by shared invariant. Maintainer ownership acceptance does not conflict with quantitative/concurrency findings reserved to specialists. No focused follow-up was needed: there was no unresolved material report conflict, untraced cause or validator-origin blocker. Every unique proposal received fresh independent source validation. The validator revised media scope to a bounded media-only peak promise and rejected G1's proposed new LRU/capacity/refusal policy in favor of local cancellation-safe retirement. Neither discarded recommendation creates remediation work.

## Validated ledger

findings-validation.md preserves complete producer-to-consumer proof, proposal dispositions, obligation mapping, materiality, correction burden, exact source and focused closure. Only independently CONFIRMED/REVISED findings are retained:

| ID / status / class | Source and violated obligation | Observable failure and minimum correction |
|---|---|---|
| FIND-012-1 / CONFIRMED / INCORRECT | SYS-R1-1; OBL-3/R1-1; Vala blocks/tools.rs:142–150,190–205,274–280; engine.rs:883–889,1125–1133,1943,2066; runner.rs:762–772 | Pop-before-await hides active settlement from Drop and concurrent waiters. Original expiry/discard can accept progress while host work remains. Preserve active completion ownership at ToolCallsOwner/HostWork across cancelled/concurrent waiters; retain existing tracker and engine/runner consumers. |
| FIND-012-2 / CONFIRMED / INCORRECT | INV-R1-01; OBL-7/REQ-017/R1-2; state.rs:157–163,820–821,924–946; runner.rs:540–541,1595–1613,1648 onward | Terminal selection/recovery returns an uncharged owned outcome, held across persistence and result delivery. Extend existing charged-read and outbox handoff custody before copying and through actual result ownership, preserving first-decision and ACK semantics. |
| FIND-012-6 / REVISED / INCORRECT | INV-R1-02; OBL-7/inherited REQ-027/R1-6; hosted/evidence.rs:770–788; Vala agent.rs:352–358,442–521; existing Skald binding/render/loop consumers | N+E guard survives but ordinary media binding/render/template/seed/rebuild copies exceed it. Admit conservative media-only peak demand at the existing binding seam using actual occurrences/provider shapes and existing guard; no generic Skald accounting project. |
| FIND-012-7 / CONFIRMED / VIOLATION | STD-R1-1; mandatory changed-item rustdoc; outbox.rs:602,766,771; tools.rs:169; snapshots.rs:403; state.rs:363; verification/mod.rs:619 | Remaining changed panic/error/associated-item/field/test docs violate the explicit merge gate. Document retained declarations in place, batched with behavior, without new machinery or a style-only round. |
| FIND-012-8 / REVISED / REGRESSION/INCORRECT | CONC-012-R1-1; preserved gateway bounds/cancellation; snapshots.rs:109,162–183; invocation.rs:640–644 | Last caller's deadline/cancellation bypasses map removal; genuine tenant cells can accumulate after requests end. Retire the matching shared load when its last participant ends, preserving peers, identity/generation, cold loading and error retry. No new capacity/refusal policy. |

FIND-012-8 is a new independently validated regression in the human-directed G1 addition. G1 itself is not retroactively a reviewer finding. Earlier IDs are preserved, not renumbered. Findings sharing a lifetime cause are consolidated at their owning boundary instead of repeated by caller.

## Reconciled acceptance and prior closure

| Obligation / prior finding | Accepted evidence and remaining exception | Result |
|---|---|---|
| R1-1 / FIND-012-1 / OBL-3 | Queued future transfer and simple held-executor deadline/discard proof pass; active-drain cancellation/concurrent-observer hole remains | FAIL |
| R1-2 / FIND-012-2 / OBL-7 | Live/recovered incremental growth, frozen precharge/outbox transfer and Charged publication/restore pass; terminal consumer copies remain | FAIL |
| R1-3 / FIND-012-3 | Count-before-materialize staging, count/admit-before-stream extraction; source ordering plus Busy/no-destination/balanced-custody assertions | PASS / CLOSED |
| R1-4 / FIND-012-4 | Full transient physical replacement is charged; same-size refusal preserves old bytes; success final charge is once; failed-close retention inspected | PASS / CLOSED |
| R1-5 / FIND-012-5 | Custody returns scratch only after confirmed deletion; failed/abandoned/partial-open and healthy reuse assertions plus source | PASS / CLOSED |
| R1-6 / FIND-012-6 | Guard crosses binding and call completion/cancellation; quantitative simultaneous provider encodings remain undercharged | FAIL |
| R1-7 / FIND-012-7 | Original listed docs fixed; changed remediation items retain mandatory omissions | FAIL |
| R1-G1 / affected OBL-8 | Approved shared tenant read-through, cold CLI rerun, quota/tenant/invalidation behavior pass; cancelled load retirement fails FIND-012-8 | FAIL for that regression |
| R1-G2 / affected OBL-12 | Independent memory/time ceilings retain enforcement and post-failure serving assertions; production unchanged; source-backed diagnosis and fresh focused pass | PASS |
| OBL-6 | Prior fresh-session/suspended-envelope/immutable-snapshot acceptance carries, and changed workspace encoding/replacement/cleanup closes | PASS |
| OBL-7 preserved mechanisms | Prior one-root/coalescing/group retry/fence/ACK/backpressure evidence carries and changed seams inspected; FIND-012-2/6 remain | FAIL for listed exceptions |
| OBL-1/2/4/5/9/10/11/13, unaffected OBL-8/12, AC-014 | Carry first verdict/validation and behavior/security/records/standards assertion-level evidence; no invalidating changed authority/dependency beyond assigned reopenings | PASS with prior execution limits |
| Mandatory repository rules / exclusions | Complete cross-tier coverage; no new dependency, schema, public control or forbidden platform change; docs exception remains; all non-goals excluded | FAIL for docs; non-goals PASS |

The validator's matrix supplies the assertion-level details for these summaries. Missing terminal/media/active-settlement/load-retirement cases are not inferred from green test names.

## Verification, limits and handoff

verification.md records 18 fresh focused test passes, including the four prescribed regressions, new engine/state/outbox/workspace/media selectors, repository-Postgres deadline and cleanup cases, G2 and CLI init with ignored tests enabled. Format, both exact prescribed scoped clippy checks and immutable-range whitespace pass. Two initially misnamed workspace selectors selected zero tests and were corrected; only the successful exact pg_tests selectors count. The first clippy attempt exhausted filesystem space; removing only regenerable wyrd_testing incremental cache allowed the retry to pass. No implementation source was changed.

Allocation order and fallback custody were inspected statically. The newly retained failing paths are source-proven and lack focused runtime coverage in this immutable candidate; current green tests do not disprove them. Implementation-recorded broad test counts are preserved separately, not claimed as this review's executions. No gate, full journey, benchmark, synthetic load, SDK rebuild, migration replay or multi-pod qualification was run here; prior execution limits carry. No missing source, authority, immutable subject or required independent reviewer blocks this verdict.

One bounded remediation task, TASK-012-R2-complete-settlement-and-copy-custody.md beside this verdict, packages only the five retained corrections. Route directly to wyrd-implement. No new product, wire, security, persistent-data or resource-policy decision is required. The next review checks this delta's successors and these closures/affected paths while carrying FIND-012-3/4/5 and unaffected acceptance forward. No source, architecture, Git identity, commits, pushes, merge or deployment was changed; the pre-existing task-index and TASK-011 edits remain untouched.
