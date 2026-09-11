# Mock data — development only

Everything under `src/lib/server/mock/` is development fixture data and the
projections over it. Nothing here is production behavior, and none of it may
be imported outside `WyrdClient` (`src/lib/server/wyrd.ts`) and tests.

- `WYRD_UI_MOCK_DATA=true` — `WyrdClient` serves these fixtures.
- `WYRD_UI_MOCK_DATA=false` — `WyrdClient` calls wyrd-server `/v1` routes;
  surfaces without a live route return the honest `upstream` error.

Deleting this directory must never break a production code path. The wire
contracts the live path must satisfy are recorded in
`changes/active/wyrd-ui-foundation/evidence/TASK-005-observe-search-contract.md`.
