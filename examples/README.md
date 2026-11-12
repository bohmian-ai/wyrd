# Examples

Each docs guide owns one directory, named after its page. The directory holds
exactly what a reader builds: the Card YAML files and one program per language,
`<name>.py`, `<name>.rs`, and `<name>.ts`, each run from that directory.

| Example | Docs page | Journeys that run it |
| --- | --- | --- |
| `quickstart/` | `get-started/quickstart` | `sdks/wyrd-sdk-python/tests/integration/test_docsite_quickstart.py`, `examples/tests/quickstart.rs`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/declare-and-pull.test.ts` |
| `finance-release/` | — (Card definitions through `verify` in Rust and Python; `service/` composes the Service from Python component modules) | `examples/tests/finance_release.rs`, `sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py::test_finance_release_example`, `::test_multi_component_service_verifies_each_subject` (`service/`) |
| `integrate-fastapi/` | `connect/integrate-asgi` | `sdks/wyrd-sdk-python/tests/integration/test_asgi_integration.py::test_fastapi_example` |
| `connect-github/` | `operate/connect-github` | `crates/wyrd/wyrd-cli/tests/github_journey.rs` (`connect.sh`, `status.sh`), `crates/wyrd/wyrd-server/src/config.rs::tests::github_guide_settings_configure_a_production_server` (`server.sh`) |

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
