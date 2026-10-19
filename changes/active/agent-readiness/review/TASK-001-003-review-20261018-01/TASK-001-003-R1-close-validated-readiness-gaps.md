---
id: TASK-001-003-R1
kind: remediation
status: ready
spec: SPEC-agent-readiness
spec_revision: 2
parent_task: [TASK-001, TASK-002, TASK-003]
remediates: [FIND-TASK-001-1, FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-003-1, FIND-TASK-003-2]
evidence_dependencies: [FIND-TASK-002-4, FIND-TASK-003-3]
---

# Close validated machine-output and onboarding gaps

Execute directly with `$wyrd-implement`. This task packages independently validated findings, not a new design cycle. Do not implement from discovery reports alone.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Approved spec: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: `changes/active/agent-readiness/tasks/TASK-001-declare-get-pull.md`, `TASK-002-machine-readable-local-tooling.md`, `TASK-003-self-contained-agent-onboarding.md`.
- Original cumulative base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Reviewed candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Prior verdict and source-proven ledger: `verdict.md` and `findings-validation.md` beside this task.
- Governing obligations: REQ-001/004/006/008/009, INV-001–005, AC-001/005/007/009, and AGENTS.md §16.

Preserve concurrent work; do not revert another contributor's changes. Review on the next round is cumulative against the original base and all three original tasks, with this task and prior findings supplied.

## Diagnosis and selected correction boundaries

### FIND-TASK-002-1: parent tracing corrupts machine errors

`crates/wyrd/wyrd-cli/src/main.rs:9–22` only defaults the filter to off in JSON mode. A valid RUST_LOG wins and its subscriber still targets stderr. HTTP/TLS diagnostics arrive before `CliBoundaryError::print(true)`, making the entire error stream unparseable. The installed get/installer path reproduces this using an unreachable endpoint and synthetic credentials. Shared native commands all enter through this producer; per-command formatters cannot retract its earlier bytes. Existing JSON tests remove RUST_LOG and therefore omit the failure condition.

Select the diagnostic destination at the existing CLI process/presentation boundary before installing tracing. Reuse/extend the append-file diagnostic mechanism already used by DevServer so JSON mode preserves requested diagnostics outside stdout/stderr. Retain human-mode filtering/output and existing dev child-log routing, secrets policy, readiness timing and foreground cleanup. Do not discard requested logs, clear RUST_LOG to make proof pass, scrub consumer strings, add command-specific subscribers, or introduce public logging options/frameworks. A small ownership adjustment to reuse the existing diagnostic destination is sufficient; private layout stays implementation-owned.

### FIND-TASK-002-2: graph pull renders progress into terminal stdout

CLI `card.rs:498–506` applies output format after awaiting the transfer. `wyrd-client/src/cards/pull/bundle.rs:68–75` always creates `DownloadProgressDisplay::stdout`, whose callback draws during an artifact transfer. Indicatif hides painting on pipes, so current captured JSON tests miss terminal-backed agent sessions. Both success and failure can contain progress/control bytes on stdout.

Eliminate automatic terminal painting from graph pull using the existing hidden display target or removing that presentation. This is the smallest sufficient correction; final human summaries remain useful without new SDK configuration. If retaining bars, their destination must be selected before transfer and stay outside machine streams. Do not add a public pull-options/transport/traversal layer just to pass CLI format. Preserve authenticated downloads, expected size/digest verification, staging, cleanup, publication and rollback. Sibling per-Card materialization is not an excuse to change its unrelated transfer semantics.

### FIND-TASK-001-1: generated recovery points to envelope-only get

`crates/wyrd-spec/src/error.rs:1144,1188` owns SdkUnhydratedArtifact and SdkInvalidStateBundle remediation. Metadata-only, retired-format, missing/corrupt/inconsistent bundles reach these errors through BundleReader/WyrdState, then all SDK/CLI/catalog projections. Both attributes still recommend regenerating with get. Candidate get cannot create the required directory/bundle, so following recovery repeats failure.

Change the two derive attributes to prescribe complete pull of the intended root into an explicit output directory, then loading it. Keep metadata-only refusal and all validation, codes/status and runtime hydration concepts. Regenerate error/reference projections through their existing owners; editing generated copies, adding aliases, error variants or a mapper would duplicate authority.

### FIND-TASK-003-1: linked continuation mismatches the declared recipe and hides initialization

`docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:17–142,236–267` is linked from Declare and quickstart. It starts with rejected `wyrd get ... --output-dir`, then uses answers/my-check/my-review/document against a Service declaring tickets and the no-refund-promise/ticket-complete/follows-policy bindings with their actual subject/input slots. The sibling `understand/runs-observations-and-evidence.md:24` also calls get a downloader. Fixing only the first verb leaves table and exact binding/subject lookup failures.

Use the existing tested support-desk Service and public SDK operations throughout the continuation, including correct tickets schema/readback and Service versus agent bindings/answer/ticket/letter inputs. Reuse source snippets where practical; do not create tables/Verifiers merely to preserve stale placeholders. Show pull with a destination. Correct the sibling description and regenerate full references from owning pages/snippets.

Make language initialization and flush/shutdown visible in the onboarding path: Python already embeds it; TS `server.ts:131–151` has telemetry initialization outside embedded regions; Rust uses `wyrd_ai::otel::start_telemetry(&state)` and retains the returned Telemetry handle, flushes it with its shutdown, then drains state. Do not invent Rust `state.start_telemetry`, give SupportDesk global telemetry ownership, or rely on inferred Python semantics. Existing framework hooks and package requirements must be visible where needed. Preserve buffered admission versus visibility, sync versus queued/window semantics, exact attribution, bounded waits, terminal errors and abrupt-loss caveats.

### FIND-TASK-003-2: published deploy callers erase their safe-replacement input

`examples/support-desk-app/python/deploy.py:25` and `typescript/deploy.ts:20` delete the current bundle after declaration but before pull. On an ordinary repeat deployment, a later read/download refusal destroys the previous usable bundle. Although the deletion predates the range, these exact changed callers are newly embedded and verified as TASK-003's supported recipe, and INV-005 preserves local data. The shared pull owner cannot restore data its caller erased.

Delete both eager removals and unused shutil/rmSync imports. Rely on the existing Cards.pull → BundleWorkspace staged replacement/restore owner, as Rust and the canonical examples already do. Preserve idempotent declaration, explicit destination, gateway policy application and output. Do not add a backup wrapper, new reset switch, migration, or rollback spanning gateway operations.

### FIND-TASK-002-3: bounded changed-item rustdoc gate

Batch these documentation corrections with the behavior work; do not create a style-only round or documentation sweep:

- CLI `src/main.rs:4`: document actual TLS/subscriber setup, machine/human destination policy, shared dispatch and exit behavior.
- CLI `src/lib.rs:157`: new raw-argument JSON test needs its actual assertion panic contract.
- CLI `tests/card_lifecycle.rs:588,617,651`: new removed-verb/get-help tests and materially changed pull-output-dir refusal need substantive item documentation and actual panic conditions.
- MCP `tests/bifrost/mcp/discovery.rs:221`: onboarding helper returns transport/shape failures but panics on missing taught terms; distinguish Errors from Panics.

These are new or materially changed items, independently compared with base. Preserve assertions/returns. Unchanged mechanically renamed tests and unrelated old helpers are excluded.

## Evidence dependencies, outside the code correction

**FIND-TASK-002-4:** AC-010/TASK-002 require relevant execution on Linux arm64 and macOS arm64, but the packet records Linux x86_64 only. Retain source target conformance separately from execution. On available supported hosts, attach candidate/target/toolchain identities and exact focused CLI machine-output/readiness/recovery plus relevant SDK pull/load commands/results. Do not infer execution from source/cross-compilation, add a platform gate or platform-specific code, or weaken the supported target set. If hosts are unavailable, report this obligation BLOCKED.

**FIND-TASK-003-3:** the candidate records published 0.1.1 artifacts versus documented 0.2.0 contracts and incompatible CLI/server minors. Truthful caveats satisfy honesty but not installed-package acquisition proof. Once compatible artifacts become available through a separately authorized release process, record artifact identities and narrow acquisition/setup/quickstart proof. Until then retain BLOCKED/partial qualifications. This task authorizes no release, version change, tag, registry/image publication or provider run. Local source-built fixture proof must not be labelled published-install proof.

No external availability claim should be refreshed from memory. Record actual observed identities if that proof is attempted; unavailable prerequisites are not skipped successes. Code completion alone cannot close these evidence obligations.

## Constraints and non-goals

Keep the approved declare/get/pull split, no compatibility aliases, unchanged error codes and permission/audit/tenant boundaries, protected credentials, verified bundle bytes and runtime refusal rules. Existing server, shared client, loader, graph, storage, OTEL and process owners remain authoritative. No new public API, durable behavior, process manager, detached lifetime, stop command, MCP declaration/check/upload, generic snippet interpreter, new dependency, full journey suite, TASK-004 live lane, sibling-repository work or release operation is requested.

## Acceptance mapped to findings

| Finding | Required observable closure |
|---|---|
| FIND-TASK-002-1 | Configured tracing never reaches JSON stdout/stderr; requested logs remain available outside both streams; result/error/readiness/exit semantics preserved |
| FIND-TASK-002-2 | Artifact-bearing terminal JSON pull yields only the result, and transfer refusal yields no stdout progress/result; safe prior bundle remains |
| FIND-TASK-001-1 | Both derive-generated recovery instructions prescribe complete pull/destination and preserve existing rejection codes/conditions |
| FIND-TASK-003-1 | Accepted pull command and all continuation tables/bindings/subjects/slots agree with the actual Service; visible Rust/TS/Python startup and shutdown use real public owners |
| FIND-TASK-003-2 | Failed repeat deploy preserves usable prior bytes for each changed language caller; successful repeat deploy replaces through the existing owner |
| FIND-TASK-002-3 | Exact changed-item documentation set describes workflow and actual Errors/Panics without weakening code |
| FIND-TASK-002-4 | Actual missing supported-host proof attached, otherwise explicitly BLOCKED |
| FIND-TASK-003-3 | Compatible released-artifact acquisition proof attached when independently available, otherwise explicitly BLOCKED |

## Focused execution and verification

Follow Red–Green–Refactor for executable changes. New regression tests must fail for the diagnosed cause before the minimum correction, not for environment/compilation. Diagnose unexpected failures with tracing and keep machine-stream logs in their diagnostic destination. Confirm collected selectors and record every new named test's exact focused mise command; do not count empty selections. Do not add a generic PTY/snippet/server harness when existing process/artifact/runtime facilities suffice.

1. Extend existing compiled-CLI proof with RUST_LOG enabled, successful JSON output and an attempted unreachable-loopback operation, checking whole streams and log retention. Add terminal artifact-transfer success/refusal coverage for the original drawing condition. Run the existing focused machine-stream owner:

   ```sh
   mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable)'
   ```

   If diagnostic ownership changes, retain focused foreground readiness/log/interrupt proof:

   ```sh
   mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'
   ```

2. Assert both derive-owned remediation values in the nearest shared bundle/error test. Preserve the existing negative proof:

   ```sh
   mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::rejects_metadata_only_bundle_as_unhydrated) or test(=state::tests::rejects_retired_bundle_api_version)'
   ```

   Confirm feature/list requirements from the current manifest and record the new assertion's exact selector. Do not rerun all graph journeys for two metadata strings.

3. Add focused content/source checks for pull syntax, Service correspondence and visible language startup/drain. Existing docs iteration command:

   ```sh
   mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/mdsvex/highlight.test.ts src/lib/mdsvex/page-text.test.ts src/lib/indexes.test.ts
   ```

   Check affected real Rust/TS source through existing example homes:

   ```sh
   mise exec -- cargo check --locked -p wyrd-rust-examples --lib
   mise exec -- pnpm --dir sdks/wyrd-sdk-ts --filter @wyrd/example-support-desk-app exec tsc --noEmit
   ```

   Compilation is not telemetry runtime proof. Reuse the existing actual lifecycle snippets/owners and source assertions; execute the existing Python quickstart only if its executable flow changes:

   ```sh
   mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_published_quickstart_declares_observes_and_verifies
   ```

4. Add focused Python-owned and TS-owned deploy caller tests with an existing usable bundle, successful declaration and refused pull, then successful repeat replacement. Reuse existing fixtures/fakes to isolate caller behavior; the shared workspace already has rollback proof. Collect and run each exact pytest/vitest selector through mise and record it. No new real-server/fault harness or full SDK suite is needed for deleting eager removals.

5. Final write-set checks: `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run codegen:check`; changed Python uses `mise run py:format`, `mise run py:lints` and its relevant typing proof; changed TS uses owning format/lint/type tasks inspected from mise. Source-generated references must regenerate cleanly. Rustdoc-only closure is source inspection, not a new test/checker. Run `git diff --check` and inspect tracked/untracked changes.

No `mise run gate`, full CLI/SDK journey aggregates or unrelated SQL/security/boundary suites are required by this bounded remediation. Final integrated change review owns broad proof once. Report actual focused results, preserved behavior and evidence-only blockers; do not mark original tasks accepted merely because code corrections pass.

## Implementation Evidence

Candidate: `98d8ef46e..303e096ed` on `change/local-dev-setup` (`9d0395db2` streams, `9713e74f2` recovery, `20d050adf` onboarding and deploy callers, `303e096ed` MCP rustdoc). Host: Linux x86_64, rustc 1.99.0.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-1 | `wyrd_cli::install_tracing` (`crates/wyrd/wyrd-cli/src/lib.rs`), called from `main.rs`: JSON mode with a valid `RUST_LOG` appends uncolored tracing to `<config>/logs/cli.log` through the shared `open_diagnostic_log`, which `server dev`'s `Diagnostics::open` now uses too. Human mode is unchanged. JSON mode without `RUST_LOG` logs nothing, as before. A log that cannot be opened prints one error document and runs nothing | `json_results_and_errors_are_machine_readable` now runs `get` with `RUST_LOG=debug` against the live server (stderr empty, stdout one envelope) and against `127.0.0.1:1` (stdout empty, stderr one error document); `cli.log` holds the attempt and no credential. Red before the fix: rustls debug lines reached stderr. `server_dev_first_run_is_client_ready` still passes | PASS |
| FIND-TASK-002-2 | `BundleWriter::write` uses `DownloadProgressDisplay::hidden()`; `stdout()` remains only for the `internal` per-Card materializer | `terminal_json_pull_prints_only_its_result` (renamed from `cli_pull_renders_download_progress`) runs `pull --format json` with stdout on a pseudo-terminal (rustix `pty`, unix dev-dependency): the success output is exactly one JSON document. Stored bytes are then tampered: exit 74, the terminal stays empty, stderr carries `WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED`, and the prior bundle's artifact is unchanged. Red before the fix: bar frames and `\x1b[2K` preceded the JSON | PASS |
| FIND-TASK-001-1 | Both `#[wyrd_error]` remediations in `crates/wyrd-spec/src/error.rs` now name `wyrd pull ... --output-dir <dir>` and loading that directory. Codes, statuses, and refusal paths are unchanged. `docs/.../api/errors.md` is regenerated by `docs:generate` | The new `assert_pull_recovery` inside `rejects_metadata_only_bundle_as_unhydrated` and `rejects_retired_bundle_api_version`. Red on the old strings (metadata-only failed on `wyrd get`), green after. `codegen:check` passes | PASS |
| FIND-TASK-003-1 | `record-a-run-and-observations.md` now uses `wyrd pull ... --output-dir ./.wyrd-bundle` and embeds the tested support-desk regions: init, answer (tickets), check (Service `no-refund-promise` with `answer`, agent `ticket-complete` with `ticket`), submit (`follows-policy` with `letter`), status, shutdown, and the evidence join. The Rust telemetry sentence is corrected. `runs-observations-and-evidence.md` no longer calls `get` a downloader. The quickstart's step 5 has LangTabs for startup and shutdown. The Rust app gains `SupportDesk::serve`, which owns the `Telemetry` from `wyrd_ai::otel::start_telemetry`, then runs `telemetry.shutdown()` and `state.shutdown()`. It replaces the unused `state()` accessor; SupportDesk holds no telemetry. TS `server.ts` gains `startup` and `shutdown` regions | `page-text.test.ts`: `shows every language connecting at startup and draining at exit` and `continues the record-a-run path with the declared support-desk Service` (table, three bound Verifiers and their slots derived from `service/*.yaml`, no stale placeholders, no `get --output-dir`). Both red on the HEAD pages, green after. `cargo check -p wyrd-rust-examples --lib`, `ts:typecheck`, `check:examples` | PASS |
| FIND-TASK-003-2 | `shutil.rmtree` and `rmSync` are removed from `examples/support-desk-app/python/deploy.py` and `typescript/deploy.ts`, along with their imports. Replacement is left to `Cards.pull` | `test_quickstart_deploy_keeps_the_prior_bundle_until_pull_replaces_it` (pytest) and `tests/unit/support-desk-deploy.test.ts` (vitest): a refused pull keeps `prior`, then a successful pull writes `replaced`. Both red before the fix (the bundle was gone). The published quickstart journey passes on the rebuilt extension | PASS |
| FIND-TASK-002-3 | Rustdoc added: `main` (TLS, subscriber destinations, dispatch, exit), `json_mode_is_read_from_raw_arguments` (Panics), `removed_card_verbs_are_usage_errors`, `pull_requires_an_output_directory_before_client_construction`, and `get_help_describes_one_envelope_without_bundle_options` (workflow and Panics), plus MCP `onboarding` (separate Errors and Panics). New and changed items (`install_tracing`, `open_diagnostic_log`, `hidden`, `serve`, `cli_command`, `run_cli_with_token`, `run_cli_logged`, `run_cli_on_terminal`, `assert_pull_recovery`) are documented | Source inspection; `lints`, `fmt` | PASS |
| FIND-TASK-002-4 | No code change | Only Linux x86_64 was available (`uname -sm`). No Linux arm64 or macOS arm64 execution was attempted or inferred | BLOCKED |
| FIND-TASK-003-3 | No release, version, or provider action | Observed 2026-10-18: PyPI `wyrd-ai` 0.1.1, npm `wyrd-ai` 0.1.1. Compatible 0.2.0 artifacts are unavailable; the quickstart stays `status: partial`, and local fixture proof is not published-install proof | BLOCKED |

Commands, all passing on the committed candidate:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable) or test(=card_lifecycle::pg_tests::terminal_json_pull_prints_only_its_result) or test(=pg_tests::server_dev_first_run_is_client_ready) or test(=card_lifecycle::removed_card_verbs_are_usage_errors) or test(=card_lifecycle::pull_requires_an_output_directory_before_client_construction) or test(=card_lifecycle::get_help_describes_one_envelope_without_bundle_options)'   # 6 passed
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::rejects_metadata_only_bundle_as_unhydrated) or test(=state::tests::rejects_retired_bundle_api_version)'   # 2 passed
mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/mdsvex/highlight.test.ts src/lib/mdsvex/page-text.test.ts src/lib/indexes.test.ts   # 44 passed
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_quickstart_deploy_keeps_the_prior_bundle_until_pull_replaces_it   # 1 passed
(cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/unit/support-desk-deploy.test.ts)   # 1 passed
mise run py:setup && mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_published_quickstart_declares_observes_and_verifies   # 1 passed
mise exec -- cargo check --locked -p wyrd-rust-examples --lib
mise run fmt; mise run lints; mise run codegen:check; mise run check:examples; mise run check:deps; mise run ts:typecheck; mise run py:format; mise run py:lints; mise run py:typecheck; mise run docs:check   # 95 docs tests; generated docs clean
(cd sdks/wyrd-sdk-ts && pnpm exec biome format && pnpm exec biome lint --error-on-warnings); git diff --check
```

Notes:
- `cargo hakari generate` updated `workspace-hack` for rustix `pty`, which implies `fs`.
- The unreachable-server `get` maps to `WYRD_SPEC_500_INTERNAL`, exit 1. That mapping predates this range and is out of scope; the test asserts stream shape only.
- `docs:check` drift only passes against committed generated files.
- Non-goals stayed excluded: no public API, alias, error code, release, platform gate, PTY harness, or full journey suite was added. The only untracked file outside this packet is `changes/active/local-dev-setup/review/TASK-001-review-20261017-03/`, which was left alone.

Status: IMPLEMENTED for the six code findings. FIND-TASK-002-4 and FIND-TASK-003-3 remain BLOCKED as evidence-only.
