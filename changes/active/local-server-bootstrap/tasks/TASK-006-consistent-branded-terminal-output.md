---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-local-server-bootstrap@7
maps: [REQ-020, REQ-021, REQ-022, INV-001, INV-003, INV-005, INV-006, AC-011, AC-012]
depends_on: []
---

## Outcome and Value

A developer sees consistent Evidence Thread status, selection, and progress
across Wyrd CLI commands, installed Python/TypeScript CLI entry points, existing
client artifact displays, and human-readable server terminal output. Scripts
and MCP hosts receive the same machine data and stable failures as before.

This task is independent of local-server orchestration in TASK-004 and native
SDK model work in TASK-005. If `server dev` exists on the implementation
candidate, its output is a consumer; otherwise TASK-004 must use the delivered
terminal policy when it adds that output. Do not implement server setup here.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Canonical brand projection | `brand/palette.json`; `gen-theme.mjs::{loadPalette,renderTargets}` | `gen-theme.test.mjs`; `mise.toml` `codegen:check`; `brand/DESIGN.md` | Evidence Thread projects CSS and marks, but no terminal roles | Extend the existing generator and drift workflow with a terminal projection | One generated projection closes an existing consumer gap; no second palette or runtime asset loader |
| CLI human output | `Cli`, `run_cli_code`, `eval::output::{print_cli_error,print_local_summary}`, `mcp::hosts::prompt_selection`, `InstallArgs::run`, `ServerCommand::dispatch`, `RegistrationProgressRenderer` | `tests/cli.rs`, `card_lifecycle.rs`, `mcp_journey.rs`, `server_install.rs`; registration renderer tests | Plain human messages, Clap defaults, and cyan/green/yellow progress lack shared semantics and policy | Apply one lightweight shared Rust presentation policy to the existing renderers | No suitable shared terminal owner exists in the inspected shared crates; share only retained stream/capability policy, not command execution or domain formatting |
| SDK-owned artifact display | `wyrd_client::cards::download_progress::DownloadProgressDisplay`; `Cards::load`; `CardGraphHydrator` | `cards/handle.rs`; `cards/hydrate/bundle.rs`; download-display tests; CLI load/get real-server journeys | Existing stdout progress is unstyled and separate from registration presentation | Reuse the shared terminal policy inside the existing transfer owner | No new transfer orchestration or transport; no public SDK printing API |
| Human-readable diagnostics | CLI `main` subscriber; `wyrd_telemetry::init`; server `main::{migrate,setup,recover_root}` | `wyrd-telemetry` capture regression; server operator output; server startup lanes | Subscriber defaults and operator prints have no common stream/color policy | Adapt existing terminal formatting and operator messages; retain OTLP layers and structured fields | No parallel logging system, global subscriber in an SDK, or telemetry dependency in terminal formatting |
| Installed language projections | Python `cli::run_wyrd_cli`; TS native `run_wyrd_cli`; TS `wyrd/src/cli.ts` | Python `tests/unit/cli/test_cli_surface.py`; TS `tests/unit/cli.test.ts`; each runtime's local-development integration target | Native dispatch is shared, but terminal eligibility and presentation are not proven through package entry points | Verify the installed entry points and existing shared-client transfer output through each owning runtime | Binding/package adjustments only when the journey proves they are needed; no independent Python or TS renderer |
| Machine-output boundaries | `query::{execute,write_jsonl,write_arrow}`; `eval::output::print_cli_error`; `mcp::proxy::run` | `query_server_journey.rs`; `mcp_proxy_discovers_and_reads_with_shared_auth`; CLI load/get receipts | Broad styling could contaminate JSON errors, query bytes, or MCP stdio | Keep styling at human presentation boundaries and prove the data remains parseable | Existing encoders and MCP transports remain the owners |

Keep the shared policy in the narrowest existing shared-runtime boundary that
can serve CLI, client, and telemetry consumers without costly dependency
propagation. A small shared crate is permitted only if existing ownership and
dependency evidence makes extending an existing crate inappropriate. Do not
make clients depend on `wyrd-cli` or `wyrd-telemetry` to obtain colors.
Private file structure, signatures, and helpers remain implementation-owned.

Use portable ANSI primary blue, success green, warning yellow, and failure
red. Active bars and working spinners use primary; remaining bar portions are
neutral. Success green denotes completed outcomes. Lime is not a generic
success or working accent. Preserve default text/background and pair meanings
with words or glyphs. Do not add truecolor, terminal theme detection, user
configuration, new switches, banners, a TUI, or new dependencies merely to
color a few labels. SDK values, `Display`/serialization contracts, and returned
errors remain unstyled. Existing transfer displays are in scope.

Human log level labels map INFO to primary, WARN to caution, ERROR to failure;
DEBUG/TRACE and ordinary log fields retain neutral text. INFO does not imply
successful verification. Preserve existing filter and exporter behavior.

## Approach

1. Inventory every Wyrd-owned human output and machine-output boundary in the
   implementation candidate, including outputs added after this task was planned.
2. Prove missing terminal semantics and stream policy in the existing CLI
   process-test target before changing behavior.
3. Extend the canonical brand projection and share only the lightweight
   terminal policy required by existing output owners.
4. Apply the policy to CLI messages, help, prompts, progress, client transfers,
   operator messages, and terminal log formatting without changing execution.
5. Prove real-server transfer, negative, query, and MCP behavior, then installed
   package entry points through their owning language runtimes.
6. Document the terminal contract and record the output inventory, generated
   projection, evidence, and any deliberately neutral surfaces.

## Ordered Implementation Scenarios

Planned test names below are new assertions in confirmed existing targets,
not claims that those tests currently exist. Fixtures and private test
organization remain implementation-owned. Confirm each selector selects a
test when it is added; a zero-selection run is not evidence.

### Scenario 1 — Consistent human presentation

**Behavior.** In an eligible terminal, help and argument errors, MCP selection,
installation/completion messages, status labels, and working progress use the
approved roles and remain understandable without color. Colors reset at the
presentation boundary. Human message text, stable codes, and exit codes are
preserved. (REQ-020, INV-006, AC-011)

**RED.** Add `terminal_output_uses_brand_roles` to the existing `wyrd-cli`
`cli` target. Exercise real command output and terminal prompts/progress;
the expected failure is inconsistent or missing role styling, not a missing
database. Run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli \
  -E 'test(=terminal_output_uses_brand_roles)'
```

**GREEN.** Connect existing human output owners to the shared policy and
canonical terminal projection, then rerun the exact test. Include help/parser
presentation; styling only successful command messages is incomplete.

**REFACTOR.** Consolidate only duplicated terminal policy. Keep domain table
formatting, command orchestration, transfer lifetimes, and logging in their
existing owners. Do not create a utility struct with no meaningful state.

### Scenario 2 — Plain and machine-safe output

**Behavior.** Nonempty `NO_COLOR`, incapable terminals, and redirected streams
emit no presentation escapes. Stdout and stderr are evaluated independently.
JSON/JSONL, Arrow, saved results, structured error records, and MCP stdio stay
unstyled even when their stream is a terminal. Empty `NO_COLOR` does not
disable otherwise eligible color. (REQ-021, INV-006, AC-011–AC-012)

**RED.** Add `terminal_output_respects_stream_policy` to the same confirmed CLI
target. Assert actual bytes and parse machine records; expect failure where
existing independent formatters ignore the shared policy. Run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli \
  -E 'test(=terminal_output_respects_stream_policy)'
```

**GREEN.** Complete stream-specific eligibility and machine boundaries; rerun
both scenario tests. Preserve the first structured stderr problem record
from `print_cli_error`; style only its human-readable companion.

**REFACTOR.** Use existing encoders and output sinks. Do not strip ANSI after
serializing data or add a second error renderer. Pure policy edge cases may
have unit support, but process behavior must remain proven at the boundary.

### Scenario 3 — Real client transfer and diagnostics

**Behavior.** A real client registers and downloads an artifact from a real
server. Human progress follows the palette, shutdown/cleanup clears it, and
the returned receipt and downloaded bytes remain correct. A refused operation
keeps its structured error and exit code. Human logs follow the same policy
while structured fields and OTLP records remain intact. (REQ-020–REQ-022,
INV-001, INV-003, INV-006, AC-011–AC-012)

**RED.** Extend the existing load/get transfer journeys with terminal and
plain-output assertions, and add `terminal_logs_preserve_structured_fields`
to the existing telemetry `tests` module. Expect missing branded display or
inconsistent log eligibility. Run the focused commands below:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::cli_load_renders_download_progress) | test(=card_lifecycle::pg_tests::cli_get_renders_download_progress)'"
mise exec -- cargo nextest run --locked -p wyrd-telemetry --lib \
  -E 'test(=tests::terminal_logs_preserve_structured_fields)'
```

**GREEN.** Extend the existing download and diagnostic owners and rerun these
focused tests plus earlier scenarios. Exercise server operator messages
through the compiled server command, not by reimplementing their text in a test.
Required structured regression commands:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=query_server_journey::query_command_reads_seeded_table) | test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision) | test(=mcp_proxy_discovers_and_reads_with_shared_auth)'"
mise exec -- cargo nextest run --locked -p wyrd-telemetry --lib \
  -E 'test(=tests::capture_pipeline_preserves_span_semantics)'
```

**REFACTOR.** Keep subscriber/exporter ownership, credential resolution,
artifact transfer, and process cleanup unchanged. Extend existing journeys
instead of duplicating server provisioning or transfer fixtures.

### Scenario 4 — Installed language entry points and SDK displays

**Behavior.** Installed Python and TypeScript `wyrd` entry points show the same
terminal semantics and plain-output rules as the Rust CLI. Rust, Python, and
TypeScript SDK artifact journeys use the existing shared display without
styling their return values or exceptions. (REQ-020–REQ-022, INV-005–INV-006,
AC-011–AC-012)

**RED.** Add one focused `branded_terminal_output` case to each existing
local-development integration owner, including package entry-point coverage
in the Python/TypeScript cases. Expect the observed transfer/CLI presentation
to differ from the shared policy. Build through `mise run py:setup`,
`mise run ts:build`, and `mise run ts:build:testing`, then run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(=local_development::branded_terminal_output)'"
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- bash -lc "cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_local_development.py -k branded_terminal_output"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- bash -lc "cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/local-development.test.ts -t branded_terminal_output"'
```

**GREEN.** Close only the binding or packaging seams exposed by the tests;
rerun these exact cases and prior scenarios. Actual installed console scripts
must be exercised; typed in-process testing commands alone do not prove the
installed terminal entry point.

**REFACTOR.** Remove duplicate language styling if any appears. Keep runtime
lifetime tests in their owning Python/Node runtimes, never inside Rust tests.

## Acceptance Criteria

- Every output category in REQ-020 has an inspected owner and a shared-policy
  implementation; ordinary neutral text need not acquire color.
- Working progress uses primary, completed outcomes success, cautions warning,
  and failures danger. Lime appears only with observation/evidence meaning.
- Terminal selection remains multi-select and preserves the existing host-edit
  behavior; color adds no host, endpoint, credential, or configuration choice.
- Stream-specific TTY, incapable-terminal, and `NO_COLOR` behavior passes;
  machine outputs parse without stripping presentation bytes.
- Real-server CLI transfers, negative/query flows, SDK displays, and MCP calls
  retain their state, data, permissions, and stable failures.
- Installed package entry points inherit the same presentation with no separate
  Python/TypeScript renderer or checkout dependency.
- Generated terminal assets remain synchronized with Evidence Thread; dependency
  checks show no UI/server/OTLP cost introduced into clients for styling.

## Expected Write Set and Consumer Closure

Likely: existing brand generator/tests/projections and their regeneration
registration; the chosen narrow shared terminal owner; `wyrd-cli` renderers
and entrypoint; `wyrd-client` download display; `wyrd-telemetry` human formatter;
server operator prints; existing CLI, telemetry, and runtime-owned SDK test
targets; CLI terminal documentation. Manifests change only where the approved
shared ownership requires wiring. Paths guide ownership, not private structure.

The palette/library replacement requested alongside this task is already
present in this checkout. The old UI and docs application styles still consume
retired tokens such as `--brand-strong`, `--lime`, and `--ok-text`; that is a
separate frontend consumer migration, not part of terminal implementation.
Do not restore those names as aliases or expand this task into a UI redesign.
The existing generated CSS/mark projections must remain reproducible.

## Verification and Evidence

Execute the exact scenario commands. These are focused task proofs; complete
language suites and broad aggregates run on the integrated candidate at change
review, not as this task's iteration recipe.

Generator/static proof does not need a manufactured behavioral RED: extend
the existing generator regression assertions and run
`mise exec -- node --test brand/gen-theme.test.mjs` and
`mise exec -- node brand/gen-theme.mjs --check` from the UI directory, followed
by `mise run codegen:check`. Compiler checks cannot protect palette-to-generated
artifact drift, so extend that existing drift lane instead of adding a new
standalone policy check. Do not edit generated assets by hand.

Run `mise run fmt`, `mise run lints`, `mise run check:deps`, and
`git diff --check`. For changed Python tests run `mise run py:format` and
`mise run py:lints`; for changed TS tests run `mise run ts:format`,
`mise run ts:lints`, and `mise run ts:typecheck`. Public SDK typing is not an
intended change; if legitimately touched, add the owning declaration/stub
check. Documentation changes under `docs/` run `mise run docs:check`.

Record a terminal inspection in light and dark terminal themes and a plain
redirected run. Record every inventoried output owner and which boundary
test proves it. Never claim RGB fidelity from terminal-native ANSI names;
those shades belong to the user's terminal palette.

## Material Stop Conditions

Stop for specification revision if satisfying this task requires changing a
wire/SDK value, error record, credential disclosure, command spelling, exit
code, auth/audit behavior, log exporter/filter, or adding exact-RGB/theme
configuration. A shared implementation that introduces a costly client
dependency must be redesigned within the approved lightweight boundary.
If an owner cannot use the shared policy without a new material dependency
or public API, report that precise seam rather than duplicating the renderer.

## Authority Links

`changes/active/local-server-bootstrap/spec.md` revision 7;
`AGENTS.md` §§3, 5, 9, 11–12;
`architecture/agent-rules.md`; `architecture/wyrd-design.md`;
`architecture/wyrd-doctrine.mdx`;
`architecture/references/languages/spec-driven-development.md`;
`architecture/references/languages/implementation-execution.md`;
`architecture/references/languages/testing-workflows.md`;
`crates/wyrd/wyrd-server/wyrd-ui/brand/{DESIGN.md,palette.json}`.
