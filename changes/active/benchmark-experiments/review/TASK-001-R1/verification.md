# Verification observed during review

Candidate remains aaa152dd4243d57752b73d3a4e9c140bccc7d171. Source was not edited.

## Pre-existing uncommitted repair (outside candidate)

Diff SHA-256: 31791c5295bcee5db36b66cebeefe0c72bfbc95af438e0d10fb818628f73656b

Command: `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=capacity::tests::exported_timing_spans_parse_exactly)'`

Result: PASS, one test selected and passed, 54 skipped; test 0.256 seconds, compile 23.97 seconds. Nextest run f7034593-ae3f-43a3-96b2-fec887e78feb.

This proves the pre-existing repair exports the asserted integer durations through the real telemetry pipeline and Collector. It is not a real release-server request smoke test, does not exercise CPU or throughput evidence, and confers no acceptance credit on the committed unsigned-duration implementation. No RED result was manufactured.

No benchmark run was started: the working tree contains the pre-existing repairs, and the user requires a clean tree without --allow-dirty. Reports cannot stand in for the deleted raw run timings. No deleted output was reconstructed.

## Format check

`mise run fmt:check`: FAIL. Only reported formatting difference is the pre-existing uncommitted regression test at capacity.rs:1251; rustfmt requires the Collector startup expression on one line. No source changed during audit.

## Lints

`mise run lints`: PASS. Both workspace all-feature/all-target Clippy and standalone shipped-server feature check completed with `-D warnings`. This check ran against the preserved worktree including the two pre-existing repairs.

## Shared deterministic runner tests

`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/)'`: PASS, 9 selected tests passed, 46 skipped, 3.177 seconds. Nextest run b0f2582a-1edd-4147-917b-5b25c0176fad. These tests do not prove real release-process evidence collection or full engine qualification.

## Engine declaration and dispatch tests

Each command selected exactly one test and passed (25 skipped per invocation):

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'`: 0.082 s, run fa163f06-3cb6-4a5b-ae99-f40d2b49b637.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_two_preserves_concurrent_work_and_recovery)'`: 0.114 s, run 1b876b9c-6799-41f8-805a-d7b1f6145a1f.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_commands_preserve_records_and_exit_precedence)'`: 0.193 s, run 007511b3-41e8-4fea-a7cc-e8be467de9ff.

Green declaration/controlled-input tests did not exercise the production OTLP duration representation that invalidated the real run. No release-server smoke, L1 or L2 experiment was run during this immutable audit.
