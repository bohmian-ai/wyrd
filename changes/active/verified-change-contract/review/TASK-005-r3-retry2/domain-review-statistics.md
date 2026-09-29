# TASK-005 r3: Drift statistics domain review

**Subject:** base `f8811ac5035c3aa165d34c38992f9889b3c9081f`, candidate `0ef7208565c69d96c44dfe5316394764678bd4e6`. Reviewed the cumulative task diff and the r2 remediation, rather than only the latest commits. The later `d638cc90` is a review record outside this candidate.

**Boundary:** baseline fitting and cancellation, PSI numeric/categorical bin counts and scoring, SPC subgroup and WECO scoring, Custom window mean, and the server's aggregate-to-scorer adaptation. Security, tenancy authorization, SQL transport, and generic run settlement are owned by other reviewers.

## Authority and source coverage

| Authority | Reviewed obligation | Source inspected |
|---|---|---|
| Approved `spec.md` revision 36, REQ-072/073/080/085/110/146 and AC-012 | Existing fitted profiles and scorers; exact windows; insufficient/invalid input inconclusive; bounded and cancellable fit | `vala-drift/src/{baseline,psi,spc,custom}`, `wyrd-server/src/verification/{drift,fitter}.rs` |
| Original `TASK-005`, scenarios 2–5 and acceptance criteria | Numeric/categorical bins, SPC trailing chunk, Custom weighted mean and threshold equality, readiness work | Same implementation and Rust/Python journey tests |
| `AGENTS.md` §§4, 6, 10–12; `architecture/wyrd-design.md` Drift implementation; `architecture/references/domain/drift-monitoring.md`; `architecture/wyrd-doctrine.mdx` | Server-owned statistics, preserved public method semantics, credible journey evidence | Cumulative diff, surrounding scorer code, tests and r2 evidence |
| Prior r1/r2 review and r2 remediation | Recheck previously open bounded fitting/cancellation and method edge proof | `review/TASK-005-r2/{verdict,findings-validation,TASK-005-R2-production-drift-closure}.md` |

## Findings and assessment

No material findings in this domain.

- **PSI:** Fitted numeric edges are persisted with JSON-safe open outer bins; the server's SQL assigns `(lower, upper]` bins on the same finite inner edges. Categorical SQL counts unknown labels in the denominator without inventing a fitted bin. `score_psi_counts` preserves the existing PSI formula, threshold equality, and the 100-row minimum as inconclusive (`psi/mod.rs:166–225`, `verification/drift.rs:150–195,431–459`). SQL and SDK journey tests exercise boundaries, unknowns, zero/short input, and subject/window filtering.
- **SPC:** `SpcScorer` consumes ordered subgroup means and includes a trailing partial chunk; `finish` makes fewer than one full chunk inconclusive. `WecoScan` retains only the maximum rule/trend lookback and its random-sequence parity test compares the old scan (`spc/mod.rs:145–335`, `spc/weco.rs:108–255`). The server SQL uses deterministic observation order and carries count plus numeric count, so a null numeric row does not become a pass (`verification/drift.rs:200–214,294–324,464–485`).
- **Custom:** The one-row SQL aggregate computes a row-weighted mean; empty, text/null, or nonfinite input yields no report; `score_custom_mean` uses strict `>` so equality is no drift (`custom/mod.rs:75–120`, `verification/drift.rs:215–221,328–350,406–418`). Rust and Python journeys exercise weighted input and both half-open window edges.
- **FIND-TASK-005-9 closure:** `fit_baseline_until` passes the cancellation probe through PSI/SPC. PSI checks at feature boundaries and every 65,536 rows while binning/counting; SPC checks before and after collection; the server awaits the blocking fit after cancellation before lease settlement (`baseline/mod.rs:55–115`, `psi/mod.rs:70–104,342–430`, `spc/mod.rs:48–135`, `verification/fitter.rs:206–294`). The focused cancellation test triggers after work starts and checks uncancelled output parity. One column conversion, one SPC limit fit, or one quantile sort remains uninterruptible; each starts from the capped decoded batch. This is a bounded verification limit, not a demonstrated failure of the required cancellation outcome.

## Verification limits

I inspected reported passing Vala, Wyrd and Bifrost lanes and the focused cancellation/Drift journey commands in the committed r2 evidence; I did not rerun broad suites. The source tests and journeys provide direct method-edge proof. This review does not claim a hard wall-clock maximum for one uninterruptible in-feature phase, and the 256 MiB decoded budget bounds input size rather than every temporary allocation.

**Overall: PASS.**
