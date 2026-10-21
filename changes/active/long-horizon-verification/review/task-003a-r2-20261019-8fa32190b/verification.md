# Verification at 8fa32190b

Root ran these commands on the immutable candidate, with PostgreSQL lanes sequential. No source edits or artificial rebuild touches were made. Full output is retained in local /tmp logs; no diff/source archive is stored here.

## Focused runtime proof

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)" && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff) | test(=pg_verification_groups::incomplete_snapshots_retry_then_error)"'
```

Exit 0. Log: /tmp/wyrd-r2-8fa32190b-focused.log.

- Managed Wyrd and Vala migration tests each selected one and passed.
- P2 selected 1, passed 1, skipped 103; 0.727s; run 530ffc53-80f6-46b6-8720-69b0d5d8cdfd.
- P1 selected 2, passed 2, skipped 275; incomplete retry/error 2.137s, pending handoff 7.094s; run e11c648e-5383-435e-9149-a57978a47d96.

## Static checks

```bash
mise run fmt
mise run lints
mise run check:tenant-isolation
git diff --check
```

All exit 0. Log: /tmp/wyrd-r2-8fa32190b-static.log. Format task caused no tracked changes. No full SDK suites, broad gate or benchmarking rerun; integrated change review owns those lanes.

## Focused DST fixture boundary investigation

At data reviewer's request, after the focused PG lane completed, root used another isolated managed PG session through mise exec and scripts/postgres/with-test-postgres.sh. Read-only SELECT probes constructed the test's exact session POSIX zone from explicit timestamps and compared calendar-day addition with 23 elapsed hours. Dates 2026-12-31 23:30, leap-year 2024-12-31 23:30, 2026-01-01 00:30 and 2026-10-19 12:30 all returned true. Transactions rolled back. No source change or new test harness.

Command: mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'psql "$DATABASE_URL" -X -v ON_ERROR_STOP=1 -f /tmp/wyrd-r2-dst-boundaries.sql'. Exit 0. SQL and log retained only at /tmp/wyrd-r2-dst-boundaries.{sql,log}. This supplements source review of the fixture; actual production-query arithmetic is proved by P2.

## Limits

Implementation RED reports in the untracked R2 task evidence are claims inspected against source; root did not revert candidate source to repeat RED. No fresh full Rust/Python/TS/MCP journeys or benchmark qualification are claimed. No runtime failure occurred in these focused reruns.

## Read-only generated error-reference comparison

Root reproduced standards review's source rendering without writing files:

```bash
mise exec -- python3 - <<'PY'
import runpy
from pathlib import Path
module=runpy.run_path('docs/scripts/generate_api_docs.py')
actual=Path('docs/src/content/docs/api/errors.md').read_text()
expected=module['render_errors']()
print('catalog_matches:',actual==expected)
print('\n'.join(line for line in expected.splitlines() if line not in actual.splitlines()))
PY
```

The diagnostic command exited 0 but reported `catalog_matches: False`: the sole missing rendered row is WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE (409), its title and remediation. This is a detected mismatch, not a passing gate. No generator was run in write mode and no docs source was changed. The full docs:check task mutates generated outputs before comparing; it was not run during immutable review.
