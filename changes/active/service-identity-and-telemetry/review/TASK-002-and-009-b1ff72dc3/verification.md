# Review-time verification

Candidate b1ff72dc34c344b596c85cf408e6a9217685be35. These checks supplement, rather than replace, implementation evidence appended to both tasks. No broad aggregate or generated-output checks rerun.

| Command | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::configured_service_context_is_immutable) | test(=auth::tests::renewal_and_caches_keep_workload_context_separate)'` | 2 selected, 2 passed; 264 skipped |
| `mise exec -- bash -lc 'cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/unit/client/test_http_auth.py::test_state_auth_snapshots_context_before_token_refresh'` | 1 selected, 1 passed |
| `mise exec -- bash -lc 'cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/wyrd-client.test.ts -t "^state httpAuth carries its own service, run, and environment$"'` | 1 selected, 1 passed; 2 skipped |
| `mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib -E 'test(=tests::explicit_card_access_claims_are_fail_closed)'` | 1 selected, 1 passed; 36 skipped |
| `git diff --check` | exit 0 |

Runtime journeys, Postgres, identity-provider, Vault and UI-dependent suites have not been independently rerun in this review. Their existing evidence and limitations remain subject to source review.
