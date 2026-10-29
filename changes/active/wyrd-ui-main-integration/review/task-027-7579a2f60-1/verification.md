# Verification evidence and limits

The review is read-only with respect to implementation and tests. No integrated check was repeated merely to duplicate the candidate-integrated results supplied by the caller.

| Evidence | Provenance / result |
|---|---|
| UI package + app check | Caller supplied, candidate 7579a2f60: zero errors |
| UI test suite | Caller supplied, candidate 7579a2f60: 40/40 |
| UI build | Caller supplied, candidate 7579a2f60: PASS |
| Real Postgres UI integration | Caller supplied, candidate 7579a2f60: 2/2 after db:migrate:inner |
| Cumulative whitespace check | Orchestrator freshly ran `git diff --check 5e57416d5...7579a2f60`: PASS |
| Rebase preservation | Orchestrator freshly ran `git range-diff a1e9b48d2..wyrd-ui-pre-205 5e57416d5..7579a2f60`: all prior implementation mappings equal; packet links only in unequal mapped commit; see subject.md |
| Browser geometry, chart fill, light/dark desktop/mobile, focus/capture and human gallery acceptance | Historical original/R1/R2 evidence in prior packets and references/verification.md, not fresh browser execution in this round |
| R3 focused regression RED/GREEN | Supplied implementation evidence in existing R3 task; independently source-reviewed, not treated as proof of all cumulative behavior |

Exact caller-supplied commands:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts"
```

Any reviewer-specific source probes are recorded in that report. No broad gate, new browser session, Linux build or release/deployment verification is implied. Another concurrent process created `review/task-003-7579a2f60-1/` after initial clean status; this review neither owns nor modifies that packet.

## Fresh discovery and follow-up probes

- Behavior used an installed-runtime scheduling model; it is explicitly not actual mounted-component proof.
- Invariant, interaction and system reviewers independently compiled and mounted the unmodified candidate in memory using the installed Svelte runtime and jsdom. Narrow valid-document ID replacement throws while the wide control succeeds; details and exact bounded reproduction are recorded in the reports.
- Focused follow-up independently reproduced the candidate and tested a source variant only in memory, in fresh processes. Its table records replacement/removal/recovery, loading/error retention, same-ID reload, layout-only no-reload, aborted signals, stale-settle exclusion and wide behavior. No patched file or harness artifact was created.
- These probes validate reactive scheduling and simulated-width branch selection. They do not supply new physical browser geometry or visual qualification.
