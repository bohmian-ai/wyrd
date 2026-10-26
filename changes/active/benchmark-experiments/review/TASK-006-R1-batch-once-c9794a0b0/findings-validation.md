# Findings validation

| ID | Kind | Location | Finding | Status |
|---|---|---|---|---|
| FIND-006R1B-1 | MISSING | task evidence, Steps 1–4 rows | Per-step profile, allocation, copied-byte, rows-per-frame and local-vs-peer retained-memory evidence required by the task is not recorded; tests alone are cited. | Open: profiled verification runs stop at the same `driver.saturated` prepass |
| FIND-006R1B-2 | DRIFT | `crates/wyrd/wyrd-server/src/scribe_outbox.rs` `Part::dense` | Mixed-frame fallback rebuilt one-row batches and concatenated them, the path Step 2 removes; unreachable because staged-row and batch destinations never overlap. | Fixed: a `Row` part in a mixed frame returns `InvalidArgumentError`; doc corrected |
| FIND-006R1B-3 | VIOLATION | `tables/staged.rs` `StagedRow::column`; `verification/results.rs` test `project` | Panicking items lacked `# Panics`. | Fixed |
| FIND-006R1B-4 | INCORRECT | task evidence | Replacement map omitted `text_columns_hold_about_their_values`; "exactly sized" overstated a < 256 B assertion; cited `e39c9d4e5` instead of `b47082554`; P3 selector path matched 0 tests. | Fixed |

All other observations were rejected by the validator as non-blocking.
