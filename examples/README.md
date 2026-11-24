# Examples

Each docs guide owns one directory, named after its page. The directory holds
exactly what a reader builds: its input files and one program per language
(`.py`, `.rs`, and `.ts`), each run from that directory.

| Example | Docs page | Journeys that run it |
| --- | --- | --- |
| `setup-check/` | `get-started/setup` | `examples/tests/setup_check.rs`, `sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_guided_manual_setup`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/declare-and-pull.test.ts` |
| `finance-release/` | `get-started/quickstart` (`service/` on `build-and-verify/declare/author-in-code`) | `examples/tests/finance_release.rs`, `sdks/wyrd-sdk-python/tests/integration/test_docsite_quickstart.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/finance-release.test.ts`, `sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject` (`service/`) |
| `integrate-fastapi/` | `connect/integrate-asgi` | `sdks/wyrd-sdk-python/tests/integration/test_asgi_integration.py::test_fastapi_example` |
| `publish-deferred-evidence/` | `build-and-verify/verify/publish-deferred-evidence` | `crates/wyrd/wyrd-cli/tests/github_journey.rs` (`publish.sh`), `sdks/wyrd-sdk-rust/tests/integration/deferred_evidence.rs` |
| `connect-github/` | `operate/connect-github` | `crates/wyrd/wyrd-cli/tests/github_journey.rs` (`connect.sh`, `status.sh`), `crates/wyrd/wyrd-server/src/config.rs::tests::github_guide_settings_configure_a_production_server` (`server.sh`) |
| `respond-to-a-failed-verification/` (with `scheduled-check/record.*`) | `build-and-verify/verify/respond-to-a-failed-verification` | `sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_failed_verification_pages_on_call_across_restart` |
| `review-a-pull-request/` | `build-and-verify/verify/review-a-pull-request` | `crates/wyrd/wyrd-server/tests/integration/pg_github_delivery.rs::the_review_example_fails_on_a_high_finding_and_comments_its_fix` |

Adding an example: create its directory, add one `[[bin]]` to `Cargo.toml` and
one journey per SDK, and embed the files with `<CodeFromFile>`. The examples
share one Rust crate (`Cargo.toml`, journeys in `tests/`, run by
`mise run test:examples`) and one TypeScript package (`package.json`,
type-checked by `mise run ts:typecheck`).

Older examples keep their earlier layout until their guides are rewritten:

- `python/` and `rust/` hold standalone Python and Rust examples.
- `gateway-setup/` sets up named provider deployments with the installed CLI.
- `support-desk/` is the verified support-desk Service in Rust, Python, and
  TypeScript; `support-desk-app/` is its API application.
