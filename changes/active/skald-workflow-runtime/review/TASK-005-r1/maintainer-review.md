# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence inspection only; no build or test was run

Revision 14 supersedes the task's Revision 12 wording where they conflict. The
approved Scenario 5 division of proof, TypeScript's Skald packaging cone, the
dev-only wiremock allowlist entries, the deferred TASK-004-r6 findings, and all
Revision 13/14 provider decisions were treated as fixed inputs rather than
review questions.

## Changed-surface coverage

| Changed surface | Owning symbols and surrounding source inspected | Callers, consumers, tests, and declaration parity inspected | Result |
|---|---|---|---|
| CLI command and dependency surface | `wyrd-cli/Cargo.toml`; `WorkflowCommand`, `WorkflowVerb`, `Execution`, `RunArgs`, `RunIdArgs`, `Source`, `Registered`, and `Report` in `src/workflow.rs` | `Cli::dispatch`, `Command::Workflow`, `run_cli_code`, `code_to_u8`, `wyrd_client::{Workflow, Workflows}`, `Cards::workflow`, and the `cli` integration target | PASS |
| CLI/Card seam | `card::{OutputFormat, parse_id, invalid_argument, exact_card_ref}` and their existing Card callers | New Workflow callers, error projection, exact-identity construction, and existing card command rendering | PASS |
| Compiled CLI journey | All helpers, `Upstream`, `Journey`, and the three Workflow journey tests in `tests/workflow_journey.rs` | Real compiled binary, shared test server, registry/apply, public gateway, external binding, server-run lifecycle, signal handling, and recorded focused selectors | FAIL (`MAIN-001`) |
| Server test-support correction | Modified `components/workflow/tools.rs::tests::state` | `RunTools` deadline test and the established `crate::test_support::{test_server_postgres,test_storage,test_catalog,test_app_state}` composition | PASS |
| Rust SDK journey | Modified `load_in_child`, route fixtures, gateway deployment helpers, `assert_example_routes`, and `workflow_loading_journey` | Public `wyrd_sdk`/`wyrd_client` handles, child-process ambient configuration, exact Cards loading, local upstream dispatch counts, and recorded focused selector | FAIL (`MAIN-001`) |
| Python SDK journey and fixture move | Shared `gateway_server`; `_apply`, route/config helpers, and `test_workflow_loading_journey`; removal of gateway-local `conftest.py` | Installed `wyrd apply`, public Python `Cards`/`Workflow`, public HTTP envelope reads, gateway support module, integration-test selection, and the added package marker | PASS |
| TypeScript SDK journey | Recording upstream, route/config helpers, gateway deployment, and the extended `workflow loading journey` | Public `Cards`, `Workflow`, and `Gateway` APIs, native test-server surface, exact selector typing, ambient configuration, and corrected recorded Vitest selector | PASS |
| CI and mock scope | Changed CI-selection expectation and four documented mock-scope entries | Recorded dependency-cone diagnosis, existing selector behavior, dev-dependency/test-only locations, and the explicit human approvals | PASS |
| Architecture, operations, security, and user docs | Workflow additions to design, doctrine, security posture, deployment/recovery, Prompt reference, workflow how-to, and generated `llms-full.txt` | Revision 14 request/destination vocabulary, CLI lifecycle, route ownership, affinity/restart loss, bounds, source schemas, and recorded `docs:check`/gate evidence | PASS |
| Task evidence | TASK-005 implementation evidence, command records, diagnoses, final verification, and limits | Named source locations and test names were checked against the candidate; no implementation-summary claim was used without source inspection | PASS |

The candidate introduces no public Python or TypeScript declaration change, so
there is no new generated `.pyi` or N-API declaration to reconcile. The Prompt
reference and `llms-full.txt` changes project the already-approved Revision 14
schema rather than defining another contract.

## Material finding

### MAIN-001 — Function-scoped imports hide two test modules' platform dependency

- Changed locations:
  - `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:150`
  - `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:467`
- Governing rule: `architecture/agent-rules.md` requires every `use` statement
  to live at the top of its module, with only the named test-module and narrow
  generic-function trait-import exceptions. These imports are inside ordinary
  helper functions and do not meet either exception. The maintainer guide's
  layout rule likewise treats the top-of-file import block as the readable
  dependency boundary.
- Evidence: both new secret-file setup helpers place
  `use std::os::unix::fs::PermissionsExt as _;` inside a `#[cfg(unix)]` block in
  the function body. Nearby Wyrd journey code uses the established top-level
  form, for example
  `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:12` and
  `crates/wyrd/wyrd-testing/src/server.rs:8`.
- Concrete maintenance cost: a maintainer reading either module's import block
  cannot see that its fixture construction has a Unix extension-trait
  dependency. Repeating the hidden import in two parallel journey harnesses
  also makes later platform changes easy to apply to one surface but miss on
  the other.
- Smallest correction: in each affected module, move the same import to the
  top-level import block and guard it with `#[cfg(unix)]`; leave the permission
  logic, fixture behavior, and all accepted proof boundaries unchanged. Do not
  introduce a helper, abstraction, check, file, option, or compatibility path.
- Focused closure proof: source inspection shows no function-scoped `use` in
  either corrected file; then the existing recorded CLI Workflow focused
  journeys and Rust SDK `workflow_loading_journey` command, plus the repository
  format/lint lane selected by the task, remain green.

## Maintainer assessment

Apart from `MAIN-001`, the implementation is discoverable and follows its
owners. The new CLI does not add a loader, graph, transport, executor, or
configuration parser: it delegates to the existing shared-client handles.
`RunArgs` and `RunIdArgs` own their respective command workflows, while
`Report` owns the two rendering choices without a trait or speculative
extension point. The large journey files use state-bearing fixtures for real
multi-surface behavior and keep scenario outcomes visible; their size alone is
not a finding. The Python shared fixture move removes duplication, and the
server test correction reuses repository test-support owners rather than
preserving an ad hoc pool/storage construction path.

No preference-only or uncertain maintainer concerns are recorded.

## Overall result

**FAIL**

`MAIN-001` is a bounded violation of an explicit repository maintainer rule in
new code. The rest of the reviewed changed surface passes the maintainer lens.
