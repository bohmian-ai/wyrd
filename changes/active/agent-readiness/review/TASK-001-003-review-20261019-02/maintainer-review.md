# Independent maintainer review

Result: **FAIL**. Proposed source IDs: **MAINT-R2-001**, **MAINT-R2-002**.

Subject: base `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`, candidate
`d6b7c75b88db5eb144e6c282263f51692371bff1`, repository
`/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
This is the cumulative TASK-001–003 review against approved specification
revision 2, including R1. The previous candidate and latest-fix range were
used to locate remediation, not as a replacement for the cumulative audit.
HEAD still matched the candidate at the final source check. No current-round
peer report was read. Only this report was written.

## Authority and coverage

Applied AGENTS.md, agent-rules, maintainer-style, spec-driven-development,
the reference router and relevant Rust/client/contract rules, current design
doctrine/client/registry/authoring boundaries, wyrd-doctrine, the approved
specification, the three original task packets, and the prior verdict,
validated findings and R1 correction/evidence. No `.codegraph/` exists.
Mechanical name changes were compared with their base implementation; they
were not treated as newly invented implementations or as a reason for an
unrelated rustdoc sweep.

| Changed surface | Owning symbols, callers and tests inspected | Maintainer assessment |
|---|---|---|
| Local declaration/check | CLI `card.rs` check/declaration/get/pull adapters, `cli.rs`, commands exports, loader input/build/load boundary, local-file/no-contact and removed-command tests | The adapters retain the existing loader and shared Cards workflows. Pure checks stay synchronous; local rejection precedes credential construction. No second validator or graph loader was introduced. |
| Declaration saga and public handle | `Cards::{declare_from_path,declare,declare_with_progress,get,pull}`, engine/context, saga stages and progress types; CLI terminal renderer and SDK callers | The existing saga is renamed rather than replaced. Transport, hashing, upload and completion remain with their existing owners; the CLI owns presentation. Removing public one-Card load also removes its obsolete return/projection machinery. |
| Bundle publication and load | Former hydrate/current pull module, `CardGraphPuller`, graph traversal, `BundleWriter`, `BundleWorkspace`, manifest DTOs, `BundleReader`, state negative/recovery tests, CLI and three SDK graph consumers | Whole traversal/staging/publication workflows remain the existing owners, rather than new orchestration built out of reused leaves. New manifest header/mode and reader agree. Hidden progress changes only the presentation sink. |
| Persistent/wire/audit vocabulary | Request/response/receipt/replay/outcome DTOs; Card routes/service/mapping/resolve and declaration writer; SQL declaration/reconciliation/version query and migration diffs; audit detail projections and route/SQL consumers | The in-place rename keeps transaction, idempotency, artifact completion and independent lifecycle/audit outcomes. No parallel persistence or replay owner was added. Generated contracts were assessed against their source owners. |
| CLI diagnostic and foreground ownership | `main`, `install_tracing`, `open_diagnostic_log`, `json_requested`, `run_cli_code`, boundary errors; `Diagnostics`, `DevReady`, DevServer setup/run/stop; installer/MCP result rendering | The binary owns its process subscriber; embedders retain theirs. The existing foreground owner still owns the child. Sharing the file-opening operation does not duplicate startup or credential orchestration. Ambient tracing is now directed outside machine streams. |
| Compiled CLI proof | Local check/lifecycle helpers, machine-results test, logged success/refusal, terminal pull helper/test, installed readiness/recovery and installer tests | PTY proof targets the actual condition that triggered drawing and retains verified bytes after refusal. The newly inserted logged helper has displaced another helper's rustdoc; MAINT-R2-001. |
| MCP | `GetCardArgs::read`, descriptors and dispatch, existing HTTP audited reads, guide/instructions, hosted tool references, discovery/onboarding/verification/proxy callers | The single selector adapter retains both existing read workflows and rejects incomplete/ambiguous input. The static start topic extends the existing guide. No MCP declaration, upload or ingestion owner was added. |
| Python surface/parity | Generic/per-kind registries, `RegistryGetQuery`, `PythonCardRegistry`, registry envelope projection, local state/materialization owners; package exports/authored stubs; CLI testing native/wrappers and CRUD/state/declaration consumers | Native/public signatures agree on the removed eager-get arguments and final declaration/pull names. Interface/object work remains at the Python boundary and shared network work remains on Cards. CLI get returns an envelope; pull returns its summary. |
| TypeScript/Rust surface/parity | TS native Cards and native testing, public Cards/testing wrappers, generated N-API/wire declarations, Rust facade/reexports and journey callers | Final methods and result fields remain aligned across their native/source declarations. No independent SDK transport or bundle publisher was introduced. |
| Agent reference presentation | llms generator and entry point, page-text projection, existing `embedCodeFromFile`, content/index/page-text tests and generated body projection | Source expansion reuses the site's complete embed resolver through a renderer argument. Plain-text component projection has its own required output; no second snippet parser/interpreter or manually maintained recipe was added. |
| Onboarding and examples | Setup/quickstart/Observe continuation, actual support-desk Service/Verifier slots and example regions, Python/TS deploy callers and runtime-owned regression tests, Rust `SupportDesk::deploy/serve/router`, public OTEL owner and existing language shutdown owners | Resource/slot names and visible initialization now agree. Deploy callers preserve the existing safe publication input. New Rust serving glue owns an application lifetime, not an SDK runtime, but its early-return cleanup sequence creates MAINT-R2-002. |
| Ancillary closure | Platform/maintainer/Rust authority changes, fixtures/examples, UI declaration wording, mise/build manifests and rustix dev-feature closure | These close approved vocabulary, target and build consumers. No additional UI feature, compatibility layer or process manager was introduced. |

## Proposed material findings

### MAINT-R2-001 — the new logged CLI helper steals the JSON helper's rustdoc

- Stable prior ID: **FIND-TASK-002-3**, still open for the changed helper
  documentation. This is the existing bounded rustdoc obligation, not a new
  style initiative.
- Classification: **VIOLATION**.
- Changed location: `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:882–908`.
- Governing rule: AGENTS.md §16 and agent-rules require substantive, accurate
  rustdoc on changed Rust items, including private/test helpers; the review
  skill explicitly keeps missing rustdoc blocking.
- Evidence: R1 inserts `run_cli_logged` between the original
  `run_cli_json` documentation and its declaration. Consequently
  `run_cli_logged -> Output` now claims it decodes successful JSON and returns
  string errors, neither of which it does. `run_cli_json -> Result<Value,
  String>` at line 908 has no item rustdoc at all. The old comment describes
  that second function's actual body: subprocess failure and JSON decoding
  become `Err(String)`.
- Concrete maintenance cost: a maintainer following these helpers' docs sees
  validation that the logging helper never performs, and loses the result and
  error contract of the helper that actually performs it. This is introduced
  by the new helper insertion, not missing documentation on an untouched test.
- Smallest correction: put the existing decode/result/Errors documentation
  back on `run_cli_json` and give `run_cli_logged` only its actual environment,
  raw-output and panic contract. Preserve bodies/assertions. Inspect the
  resulting item attachment and run the applicable format/lint lane; no new
  behavior test or documentation checker is needed.

### MAINT-R2-002 — the visible Rust shutdown recipe abandons the writer after span-flush failure

- Prior relationship: residual lifecycle defect in the correction for
  **FIND-TASK-003-1**. Preserve that stable ID when consolidating this with the
  original requirement to expose the real language telemetry/drain lifecycle;
  the validator can determine whether the changed producer warrants a new ID.
- Classification: **INCORRECT**, with observable buffered-data and published
  recipe consequences.
- Changed location: `examples/support-desk-app/rust/lib.rs:100–134`, especially
  `telemetry.shutdown()?` at line 128 followed by `state.shutdown().await?`.
  Consumers: the `serve` region embedded in quickstart step 5 and Observe step
  4, and their generated full-reference bodies.
- Governing obligations: REQ-008's complete, accurate Observe/shutdown recipes;
  INV-005's data-preservation boundary; maintainer-style's requirement that
  documented public behavior and failure/partial-progress handling agree.
- Producer-to-consumer evidence: `SupportDesk::deploy` starts Bifrost at line
  95; requests record ticket rows through that retained state. `serve` keeps
  a state clone while the router owns the application, awaits graceful
  serving, then invokes the public `Telemetry::shutdown`. That method in
  `sdks/wyrd-sdk-rust/src/otel.rs:61` calls the stock provider's `force_flush`
  and returns a catalog error on failure. The installed SDK batch processor
  forwards its ForceFlush export result, so an export failure/timeout is a
  real failure path. The first `?` returns before the state's drain is even
  attempted, leaving independently admitted ticket rows unflushed as the
  application exits. The new rustdoc describes loss when a drain fails, but
  this path skips the drain altogether. The page-text test at
  `docs/src/lib/mdsvex/page-text.test.ts:30` currently asserts the exact
  short-circuiting pair rather than its completion behavior.
- Existing owner comparison: Rust `WyrdState::shutdown` already owns native
  writer shutdown; no new drain workflow is necessary. Python's native state
  shutdown (`src/state/mod.rs:316–319`) and TypeScript's public shutdown
  (`wyrd/src/index.ts:2967`) retain the telemetry outcome, attempt the native
  drain, and only then settle errors. Shared `TelemetryFlushFailure::settle`
  documents that policy. The older Rust support-desk request-loop example
  explicitly documents its error short-circuit; it does not establish a safe
  graceful-lifetime recipe for the new HTTP app. Calling existing leaf APIs
  does not by itself preserve this shutdown workflow.
- Smallest correction: keep telemetry ownership in this application-level
  call and the actual drain on the existing state owner, but retain both
  stage outcomes so native shutdown is attempted before propagating a
  telemetry-flush error. Preserve existing stable errors and serving-result
  handling; do not add a framework, SDK method, new telemetry owner or durable
  contract. Update the embedded source expectation and the new method's
  failure/cancellation documentation to describe actual sequencing and loss.
- Focused proof: exercise a telemetry flush refusal with an independently
  drainable pending ticket and prove the state drain still runs while the
  call reports the telemetry failure. Use the existing Rust example/SDK test
  facilities, not a new general harness or full journey suite. Retain the
  existing example compile and source-expanded docs checks. The current
  compile/content checks cannot prove this failure branch.

## Prior-finding closure from source

| Stable ID | Current assessment |
|---|---|
| FIND-TASK-001-1 | Closed in source: both derive attributes prescribe complete `wyrd pull` with explicit destination and loading it; nearest state tests assert recovery and preserve stable negative codes. |
| FIND-TASK-002-1 | Closed in source: main delegates tracing policy before dispatch; JSON's valid ambient filter writes to `logs/cli.log`; logged live/refused get proof checks complete streams and credential absence. |
| FIND-TASK-002-2 | Closed in source: graph `BundleWriter` uses hidden drawing; existing internal materialization keeps its separate presentation. PTY success/refusal proof covers terminal output and prior artifact preservation. |
| FIND-TASK-002-3 | Original named items now have substantive docs, including main and MCP onboarding Errors/Panics. R1 creates the attachment defect above; the bounded gate is not fully closed. |
| FIND-TASK-003-1 | Original get/output-dir, table/binding/slot and hidden-startup problems are corrected. Rust now exposes the real OTEL handle, but the published failure/drain lifecycle remains incomplete as above. |
| FIND-TASK-003-2 | Closed in source: both eager removals/imports are deleted; repeat deploy stays on Cards.pull/BundleWorkspace. Runtime-owned caller tests refuse then replace and inspect retained/replaced bytes. |
| FIND-TASK-002-4 | Remains evidence-only BLOCKED: attached execution is Linux x86_64 only. Source target conformance is distinct from absent supported-host execution. |
| FIND-TASK-003-3 | Remains evidence-only BLOCKED: packet retains incompatible published-artifact qualifications and partial quickstart status. Local/source-built proof is not installed-release proof. No release action is authorized. |

## Verification limits

No Cargo execution, compilation, broad tests, live provider call, publication
lookup or supported-host run was performed by this reviewer. Exact commands
and passing results in the task/R1 packets were treated as reported evidence,
not independently rerun results. Static source, baseline/cumulative/latest-fix
diffs and relevant callers/tests establish the proposals above. The OTEL
force-flush behavior was checked in the installed dependency source, without
changing it. Source and other contributors' reports were preserved.

No optional naming, layout, module-size or abstraction preference is proposed.
The two material proposals prevent maintainer PASS; the independent evidence
blockers remain regardless of their eventual disposition.
