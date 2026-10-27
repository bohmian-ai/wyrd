# TASK-004 first remediation review — FIX_REQUIRED

## Immutable subject and authority

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
Prior candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
Cumulative reviewed candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
Review range: `git diff a020500e8ea0c39311d508d6b5bfa85ebe90368c c9d72729b85187f2399725e0ed9496c4c4a39298`.

Approved specification revision 8 remains `changes/active/benchmark-experiments/spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. Explicit maintainer approval persists; stale draft wording is not reopened. Acceptance context retains original TASK-004, prior R1 verdict/validated ledger/reports, and `../TASK-004-R1/TASK-004-R1-complete-cohort-timing-evidence.md` with candidate evidence. No rebase occurred. Previously untracked prior reports were committed as evidence, without changing the original source subject. HEAD and source remained the pinned candidate; review made no implementation edits and preserved pre-existing untracked benchmark records.

## Round scope, selection and independent results

This is the first remediation review, not another first-review audit. `scope.md` records the complete navigation map and carried acceptance. Review covers the six existing finding closures, the full fix diff and affected driver/counter/metadata/auth/artifact paths. Source-backed passing prior evidence carries for unchanged HDR arithmetic/bounds/validity, timer and wait-union meanings, listener/privacy/recorder, normal tracing, warmup/resource invalidity, legacy reading, summary publication and dependency/platform/non-goal boundaries. Changed dependencies of those accepted rows were checked; no unrelated whole-task rediscovery occurred.

Selected fresh reviewers:

| Role / report | Result and coverage |
|---|---|
| invariant-rev / `task-review-invariants.md` | FAIL; F1/F2/F3 closure and material driver concurrency, counters/cohorts/scans and artifact projections |
| security domain-rev / `domain-review-security.md` | PASS; F4 verified activation through HTTP/Gateway/gRPC/native, F2 receiving/auth seams and ordinary refusal preservation |
| fresh ponytail-rev / `findings-validation.md` | FIX_REQUIRED recommendation; independent source/materiality/correction validation, full residual ledger, all six closures and changed-item rules |
| fresh focused followup-rev / `followup-review.md` | RESOLVED/CONFIRMED; validator-discovered new Pending field rustdoc omission, separately source-validated under F5 |

The two discovery specialists were selected for materially changed driver/window behavior and multiple verified-context paths. The original five-role team was not recreated. No separate behavior/repo/maintainer/system specialist was needed for carried evidence and remaining validator coverage. All selected reports are complete.

`claim-comparison.md` records why initial discovery had no unresolved conflict and why one focused follow-up later became necessary: the validator could not validate its own newly discovered field-doc blocker. The follow-up confirms only that exact explicit gate, without a new finding identity or broad audit. F4 receiving activation PASS and F2 outgoing OTLP omission are distinct links, not contradictory verdicts. Reviewer agreement did not replace source proof.

## Prior closure and remaining validated ledger

| Stable finding | Closure disposition / independent decision | Remaining boundary |
|---|---|---|
| FIND-TASK-004-1 | PARTIAL / REVISED | CPU freeze and late request/row/wire accounting accepted; measured retained/capture bytes still use post-drain Metrics, and three open-loop owners lack actual end rechecks before admission |
| FIND-TASK-004-2 | PARTIAL / CONFIRMED | Finite context, native/HTTP/Gateway consumers, final cohorts/exact keyed reconciliation/scans accepted; raw OTLP constructor sends auth only and drops scoped metadata |
| FIND-TASK-004-3 | PARTIAL / REVISED | Full snapshots, target persistence/failure invalidity, reconciliation/ranks and shared max conversion accepted; Bifrost L2 direct scalar conversion omits its required selected maximum |
| FIND-TASK-004-4 | CLOSED | Verified-context activation, public/refused isolation, actual Gateway ancestor/task/stream-end paths and ordinary timing/auth/audit preserved |
| FIND-TASK-004-5 | PARTIAL / REVISED | Original listed omissions fixed; newly added Pending tuple field lacks separately mandated field rustdoc, independently confirmed by follow-up |
| FIND-TASK-004-6 | CLOSED | Installed checked conversion replaces prohibited truncation/sign-loss suppression; supported quantile behavior retained |

Retained findings are **FIND-TASK-004-1**, **FIND-TASK-004-2**, **FIND-TASK-004-3**, **FIND-TASK-004-5**; no new stable IDs. Full source locations, obligations, producer/consumer and sibling traces, supported scenarios, consequences, materiality, correction burden and narrow proof are preserved in `findings-validation.md` and incorporated into the self-contained remediation.

The validated corrections reuse existing owners: stop-boundary versus final metrics in Window and its byte consumers; actual-stop guards in existing schedules; scoped headers in the shared raw OTLP request constructor; q=1 at the existing selected L2 conversion; one tuple-field doc. No new clock, queue, transport, collector, report framework or specification decision is needed. Accepted full artifact/cohort/auth fixes must not be reimplemented.

## Reconciled acceptance matrix

| Obligation / gate | Evidence | Result |
|---|---|---|
| Bounded HDR, strict bounds, zero/reset/overflow/components and actual engine timer/wait meanings | Prior accepted source/owner tests; current telemetry and metrics reruns | PASS carried/current |
| Local opt-in read-only bounded snapshot, privacy/recorder and ordinary tracing | Prior accepted authority/source; current scoped source/tests | PASS |
| Authenticated receiving classification and ordinary refusals | Current shared Pending/activate source, actual Gateway/gRPC traces, HTTP/Gate/shared regressions | PASS; F4 closed |
| CPU/T freezes before drain; client request/row/wire late accounting and final latency | Current measure/driver conversions and exact new regressions | PASS for corrected portions |
| Every required byte work numerator and actual admission uses that stop | Post-drain closing Metrics and missing actual deadline rechecks | FAIL; F1 remainder |
| Actual offered metadata through every sender | Correct native/HTTP/Gateway consumers; raw OTLP auth-only constructor | FAIL; F2 remainder |
| Final admitted cohorts, incidental separation and scan matching | Current expected/recorded/selection source and cohort/scan tests | PASS for corrected portions; OTLP producer depends on F2 |
| Retained full snapshots, all outcomes/process/method/times, ranks/reconciliation and write-failure invalidity | Actual shared collection/runner writer; reconstruction regression | PASS |
| Required selected maximum | Shared conversion works; Bifrost L2 bypass omits q=1 reading | FAIL; F3 remainder |
| V1/V2 summary-only rendering, unsupported versions, collision-safe publication | Prior accepted behavior and current evidence/record source | PASS |
| Mandatory smoke dispatch | Prior accepted dispatch structure; no current smoke executed, raw OTLP metadata prevents current reconciliation | Structure carried; failure recorded under F2 |
| Changed-item rustdoc and checked conversion | Original docs fixed; independently confirmed Pending field omission; checked numeric source and lints | F5 FAIL; F6 PASS |
| Existing owners, portable/dependency/non-goal boundaries, no unrelated change | Complete fix diff and affected caller coverage | PASS except explicit retained gate |

The full reconciled matrix and carried-forward evidence are in `findings-validation.md`. Known incidental/probe counting and missing dedicated Gateway runtime coverage were not promoted to new obligations without a source-proven material failure.

## Verification, limits and convergence

`verification.md` records independent sequential reruns: telemetry 3/3, client trace/header 6/6, server metrics 9/9, Gate verified activation 1/1, six exact new/changed regressions 6/6, affected tooling 34/34 with repository Postgres/migrations, HTTP receiving activation 1/1, scoped format/Clippy, dependency and whitespace checks. All passed. Existing RED history is implementation evidence; review did not mutate source to reproduce it.

These tests do not exercise raw OTLP `authorized` metadata, measured retained/capture bytes crossing stop, a due arrival actually waking past stop, or the L2 direct maximum conversion. Source proofs therefore remain compatible with green focused checks. No release smoke, real capacity run, complete language journey suite or platform matrix ran for this bounded remediation. Earlier smoke establishes old-candidate unchanged boundary availability, not changed producer metadata at this candidate. Broader integrated verification remains separate.

This is only the first remediation review, so the skill's two-unsuccessful-remediation convergence assessment is not yet applicable. Most original corrections are accepted; residuals are incomplete sibling producer/consumer fixes, a required scalar projection and one remediation-introduced mandatory doc omission. The next bounded scope is these four residual closures and affected paths, not a restart of the original audit.

## Verdict

**FIX_REQUIRED** — **FIND-TASK-004-1**, **FIND-TASK-004-2**, **FIND-TASK-004-3**, **FIND-TASK-004-5**.

Closed: **FIND-TASK-004-4**, **FIND-TASK-004-6**. Self-contained remediation: `TASK-004-R2-finish-stop-counters-and-otlp-cohorts.md`, routed to `$wyrd-implement`. Review did not implement, commit, merge, push or deploy.
