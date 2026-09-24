# TASK-005 r4: statistics domain review

**Subject:** cumulative base `f8811ac5035c3aa165d34c38992f9889b3c9081f`, candidate `6d93b75825926ac27c9fd7de90879b19d67e39b3`; approved `spec.md` revision 36 and original `TASK-005`. The review includes prior r1–r3 findings and remediation and the complete cumulative diff, not only `93a2e3c9..6d93b758`.

**Boundary:** PSI numeric/categorical fitting and scoring, SPC baseline/control limits and ordered subgroup scoring, Custom mean and threshold semantics, server aggregate adaptation, bounded decode and cooperative fit cancellation. Claim, lease, and shutdown ordering are reviewed here only for their effect on fit output and cancellation; durability and concurrency ownership belong to the data-domain reviewer. Security and query authorization belong to the security-domain reviewer.

## Authority and source coverage

| Authority | Statistical obligation | Source inspected |
|---|---|---|
| Approved spec revision 36, REQ-073, REQ-080/085/110, REQ-146, AC-012; original task scenarios 2–5 | Exact fitted baseline; existing PSI/SPC/Custom method semantics; bounded and cancellable work; fixed window aggregates and inconclusive outcomes | `vala-drift/src/{baseline,psi,spc,custom}`, `wyrd-server/src/verification/{drift,fitter}.rs`, Rust/Python/TypeScript Drift journeys |
| `AGENTS.md` §§4–6, 10–12; `architecture/agent-rules.md`; `architecture/wyrd-design.md` §Drift; `architecture/wyrd-doctrine.mdx`; `architecture/references/domain/drift-monitoring.md`; spec-driven-development reference | Server owns fit and aggregate behavior; methods retain their approved profile and scorer; no client-side fitting or invented Drift method | Cumulative diff, surrounding scorers, server adapter and tests |
| Prior r1–r3 verdicts and findings, r2/r3 remediation tasks | Reassess method-edge evidence, FIND-TASK-005-9 cancellation, and FIND-TASK-005-11 shutdown-drain effect on fit | Prior reports, committed remediation evidence, `93a2e3c9..6d93b758` fitter diff and focused test |

## Assessment

No material statistical finding.

- **PSI:** `fit_psi_baseline_until` preserves numeric `(lower, upper]` edges and categorical proportions; the server SQL counts exact fitted bins and unknown categories over subject, series and `[start, end)`. `score_psi_counts` reuses the existing formula, 100-row minimum, threshold comparison and report construction. Numeric edge, categorical escaping/unknown, short-window, and raw-versus-aggregate parity tests exercise these paths (`psi/mod.rs`, `verification/drift.rs`, SDK journeys).
- **SPC:** The server forms deterministic, ordered chunk aggregates including a trailing partial chunk, and `SpcScorer` applies the existing rule/control-limit fields incrementally. Its history remains capped at maximum WECO lookback; short input is inconclusive. The SQL and scorer tests cover ordering, partial chunks and parity (`spc/mod.rs`, `spc/weco.rs`, `verification/drift.rs`).
- **Custom:** One SQL aggregate computes the row-weighted mean. Empty, partially numeric, or nonfinite windows produce no report; `score_custom_mean` retains strict `>` so equality is no drift. The SQL and journeys cover text metrics, weighted input and both half-open boundaries (`custom/mod.rs`, `verification/drift.rs`).
- **Fit and r3 change:** `decode_bounded` rejects decoded Arrow batches past 256 MiB and checks cancellation; `fit_baseline_until` checks before features and within PSI bin/count work, and its focused test verifies cancelled PSI/SPC fits and uncancelled output parity. The r3 fitter change alters only *when* cancellation is requested: an admitted fit may complete during grace, while timeout or expired grace cancels then awaits the same blocking decode/fit before lease settlement. `FitGate` is test-only and does not enter statistical computation. The Postgres lifecycle test exercises completion during grace, cut-off after grace and no new fit after shutdown (`verification/fitter.rs`, `pg_verification_runtime.rs`).

## Verification limits

The committed r3 evidence reports the focused Postgres test, format/lints, server integration, Wyrd and Drift journey lanes green after the production change. Earlier evidence reports Vala and all Bifrost lanes green after the statistical code changes. I inspected source and tests and ran `git diff --check f8811ac5..6d93b758`; I did not rerun large lanes. The 256 MiB decoded budget is an input bound, not a hard process-memory bound. A single quantile sort and some column-conversion phases do not poll cancellation internally; the existing bounded input limits that latency, and the latest change neither adds nor removes those phases.

**Overall: PASS.**
