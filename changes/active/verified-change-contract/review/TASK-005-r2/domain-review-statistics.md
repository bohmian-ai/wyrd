# Statistical Drift domain review — TASK-005 round 2

**Subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f` → `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d` (HEAD confirmed at candidate).  
**Boundary:** baseline fitting; fixed-window PSI, SPC, and Custom aggregation and scoring; inconclusive outcomes; bounded SPC history; and method journey proof.  
**Result: PASS.** No material statistical finding.

## Authority and source coverage

| Authority | Applied obligation | Source inspected |
|---|---|---|
| `AGENTS.md` §§2, 5, 10–12; `architecture/agent-rules.md` | Preserve existing Drift algorithms and contract; server-owned aggregation; complete user journeys | `crates/vala/vala-drift/src/{baseline,psi,spc,custom}/*`, `crates/wyrd/wyrd-server/src/verification/{drift,fitter}.rs`, SDK journeys |
| `changes/active/verified-change-contract/spec.md` rev. 36, REQ-073/080/110 and AC-012 | Pinned fitted PSI/SPC baseline; authored Custom baseline; fixed half-open ingest-time window; existing PSI/SPC scorer semantics; insufficient input inconclusive | `crates/wyrd/wyrd-server/src/verification/{drift,fitter}.rs`, `crates/vala/vala-drift/src/{psi,spc,custom,report}.rs` |
| `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`, scenarios 1–5 and Remediation r1 Evidence | Exact method behavior and prior statistical findings F1, F2, F5, F9 | Same implementation; `sdks/wyrd-sdk-{rust,python,ts}` Drift journeys; named tests in task evidence |
| `changes/active/verified-change-contract/architecture/logic/drift.md` | Raw observation selection, SPC ordering, score-only aggregates, and no client aggregate | `ObservationWindow` SQL and folds; Vala aggregate-input entry points |

## Boundary findings

| Obligation | Evidence | Result |
|---|---|---|
| PSI uses fitted numeric `(lower, upper]` bins and categorical labels, counting unknown labels in the denominator; below-minimum sample is inconclusive | `ObservationWindow::{psi_numeric,psi_categorical}` and `fold_psi`; `score_psi_counts`; SQL and Vala tests; Rust/Python/TS method journeys | PASS |
| SPC retains authored rule, alert threshold, row-count adaptive chunk size, observation order, trailing short chunk, and insufficient-input verdict | `ObservationWindow::spc` orders by `created_at, record_id`; `SpcScorer::{new,push,finish}` reuses `WecoScan`; scorer parity and journey tests | PASS |
| SPC memory is bounded by rule lookback | `WecoScan::push` caps `VecDeque` at `WecoChecks::lookback`; `scan_history_is_capped_at_lookback`, `retained_history_is_bounded_over_many_chunks`, and reference parity test | PASS |
| Custom scores raw-value window mean, with threshold equality no drift; empty, text, or incomplete numeric input is pre-scoring inconclusive | `ObservationWindow::custom`, `fold_custom`, `score_custom_mean`, Rust/Python/TS method journeys | PASS |
| The window filters exact subject, series, and `[start,end)` by managed `wyrd_event_time` | `ObservationWindow::rows`; SQL execution tests and method journeys exercise exclusions on both boundaries | PASS |
| Fitted baseline is persisted before PSI/SPC scoring; invalid artifact and oversized decode fail | `BaselineFitter::{fit_next,fit,resolve}`, `decode_bounded`, `DriftEngine::fitted`; baseline and journey tests | PASS |
| Never-written observation table produces no report or feature rows; malformed aggregate remains an error | `Reader::caller` returns `None` if no registered table, `DriftEngine::try_verify` returns `None` for empty Custom input, folds reject malformed aggregates; SDK method journeys | PASS |

## Verification limits

I inspected source and the focused test coverage, plus the task's recorded successful commands; I did not rerun the broad lanes. The three SDK journeys directly exercise the changed method semantics, while scheduler, retry, delivery, and restart behavior is covered by shared runtime tests and lies outside this statistical boundary. The current source uses fixed SQL rather than the original task's typed-plan wording, as authorized by spec revision 36.
