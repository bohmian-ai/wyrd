# Independently executed verification

Immutable subject: `443a79337...1b72ec177`, candidate full identity
`1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`. New full review requested by the
maintainer. These checks were executed after committing the working changes.

## Runtime journeys

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_passes_and_fails) | test(=finance_release_holds_uncertain_releases) | test(=finance_release_policy_versions_remain_attributable)'"
```

PASS: three selected tests, none skipped, 5.220 seconds. Both migration setup
tests passed. Linker emitted a compact-unwind size warning; execution succeeded.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats tests/integration/test_docsite_quickstart.py::test_yaml_release_check_judges_like_the_code tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_insufficient_authority tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_changed_content_and_broken_references tests/integration/test_docsite_quickstart.py::test_finance_api_releases_only_passed_reports"
```

PASS: five tests, 17.05 seconds. Migration setup passed. Tests use the locally
installed SDK and local provider fixture; no live-model quality claim is made.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/finance-release.test.ts -t '^(runs the finance release Quickstart from a reader.s directory|judges each finance requirement on its own)$'"
```

PASS: two tests, none skipped, 9.44 seconds including fixture setup. Native
in-process server ran against repository test Postgres and local provider.

## Docs and static checks

| Exact command | Observed result |
|---|---|
| `mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts src/lib/lang.test.ts` | PASS, 4 files / 21 tests |
| `mise run docs:test` | PASS, 13 files / 102 tests |
| `mise run docs:check:commands` | PASS |
| `mise run docs:linkcheck` | PASS |
| `mise run docs:svelte:check` | PASS, zero errors/warnings |
| `mise run docs:build` | PASS, static site and Pagefind index built; 32 pages indexed |
| `mise run docs:a11y` | PASS, 32 pages, light/dark AA contrast |
| `mise run fmt:check` | PASS |
| `mise run py:lints` | PASS, Ruff and authored stub checks |
| `mise run ts:lints` | PASS, 66 files |
| `mise run ts:typecheck` | PASS, SDK/support-desk/examples and native build prerequisites |
| `mise run py:typecheck` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::model_slot_carries_the_derived_schema) \| test(=authoring::tests::fields_of_declares_one_slot_per_field)'` | PASS, 2 selected tests, 286 excluded |
| `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::verdict_predicates_match_the_verdict)'` | PASS, 1 selected test, 765 excluded |
| `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/authoring.test.ts tests/unit/observe.test.ts` | PASS, 48 tests |
| `mise exec -- git diff --check 443a79337...1b72ec177` | Exit 2: Markdown two-space endings in positioning.md; finance_service.ts extra blank EOF. Reported to standards reviewer for rule/materiality assessment |

An initial typecheck invocation was interrupted to avoid concurrent Cargo
work; the complete command was rerun successfully after the Rust journeys.
Native builds print platform-package/deprecation/linker warnings without
failing the checks. Node prints module.register deprecation warnings.

## Limits

No broad gate, live provider, release/tag publication or performance benchmark
was run. No browser was used in this review; source, SSR tests and the static
accessibility checker do not prove browser animation behavior. Historical
browser observations remain available in the task, not treated as fresh
independent observations. Docs generation has not been rerun against source
because this review keeps its source immutable; generated content parity is
inspected by reviewers and existing tests.

Shared memory search for positioning/TASK-001/revision 5 returned repository
preferences and no additional task-specific approval or verification evidence.
