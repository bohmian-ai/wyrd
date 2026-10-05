# Repository Standards Review — TASK-005 r1

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence review only; no build or test command was run

## Overall Result

**FAIL**

The candidate generally follows the repository's owner, dependency, test-tier,
documentation, secret-handling, and generated-artifact rules. Two material CLI
standards violations remain: the new command rejects the established endpoint
override for authored-file runs, and it treats signal-listener initialization
failure as a user interrupt instead of preserving the error.

The fixed human decisions supplied for this review were treated as authority:
Rust and TypeScript register through `Cards::register_from_path`, Python runs the
installed `wyrd apply`, TypeScript packaging of Skald is required, the dev-only
wiremock allowlist entries are approved, the five TASK-004 r6 findings are
deferred, and the Revision 13/14 provider decisions stand. None is reported as
a finding.

## Authority Coverage

| Changed surface | Applicable authority read and applied | Coverage and result |
|---|---|---|
| `wyrd-cli` Workflow command, manifest, shared card helpers, dispatch, and exit-code projection | `AGENTS.md` §§3–6, 9, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/{positioning-and-vocabulary,architecture-constraints}.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors,agent-harness}.md`; approved spec Revision 14 | Shared `Workflow`, `Workflows`, `Cards`, client, typed DTO, and error owners are reused. Struct-centered organization and rustdoc are present. **FAIL** for `STD-001` and `STD-002`. |
| Compiled CLI Workflow journey and `cli` test-target registration | `AGENTS.md` §11; `architecture/agent-rules.md` external-test rule; `architecture/references/languages/testing-workflows.md`; spec REQ-024, REQ-026–028 and AC-001–004, AC-013–019, AC-023–024 | Correctly lives in the existing earned external target because it spawns the compiled binary and a real server. Journey breadth and deterministic synchronization are credible. **FAIL** because the journey asserts the contract drift in `STD-001`. |
| `wyrd-server` Workflow tool test-state repair | `AGENTS.md` §§3–6, 11; `architecture/agent-rules.md` SQL/pool and test rules; `architecture/references/{architecture/patterns,languages/rust-core,languages/testing-workflows}.md` | Reuses `test_server_postgres`, `test_storage`, and `test_catalog`; removes raw lazy-pool construction rather than widening an allowlist. PASS. |
| Rust SDK journey and dev dependency | `AGENTS.md` §§3, 4, 11; client-tier and testing rules; `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/rust-core,languages/testing-workflows}.md`; spec REQ-054–059 and AC-029–031 | Public SDK-to-shared-client path is retained. `wiremock` is dev-only and supports a credential-free journey. The approved Cards registration deviation is respected. PASS. |
| Python SDK integration fixture relocation and journey | `AGENTS.md` §§3, 8, 11; `architecture/references/{architecture/patterns,languages/testing-workflows}.md`; spec REQ-054–059 and AC-029–031 | Existing gateway fixture is moved to the nearest shared integration scope; Python uses the installed CLI as explicitly approved. No Python public API or generated stub changed. PASS. |
| TypeScript SDK integration journey | `AGENTS.md` §§3, 11; `architecture/references/{architecture/patterns,doctrine/architecture-constraints,languages/typescript-guide,languages/testing-workflows}.md`; spec REQ-054–059 and AC-029–031 | Journey stays on the public `@wyrd/sdk` and `@wyrd/testing` surfaces, uses typed inputs, and adds no parallel transport/runtime. Approved Cards registration and Skald packaging decisions are respected. PASS. |
| Workflow design, doctrine, security, deployment, and recovery updates | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/operations/{deployment-and-release,reliability-and-recovery}.md`; `AGENTS.md` §§1–3, 9–10; applicable doctrine/architecture references | Records the scoped surfaces, route ownership, secret timing, endpoint security, process-local lifecycle, affinity, restart loss, capacity, and retention boundaries without legacy vocabulary. PASS. |
| Generated Prompt reference and docs corpus; Workflow how-to | `AGENTS.md` §§8, 11–12; `architecture/agent-rules.md` generated-artifact rule; `architecture/references/languages/agent-harness.md`; `architecture/wyrd-design.md`; Revision 14 provider contract | Changes match the Revision 14 provider destination/wire-schema model, and recorded `docs:check`/gate evidence is present. No hand-authored parallel error or schema catalog was added. PASS. |
| CI selection expectation and mock-scope allowlist | `AGENTS.md` §§11–12 and “Adding And Retiring Checks”; `architecture/agent-rules.md`; `architecture/references/languages/{implementation-execution,testing-workflows}.md` | The selector implementation is unchanged; the expectation follows the required TypeScript dependency closure. The four allowlist additions are the explicitly approved dev-only wiremock seams. No new mechanism or option outside established repository/common practice was added. PASS. |
| Task evidence and recorded verification | `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md`; Revision 14 authority order | Evidence records focused selectors, corrected TypeScript selector, the aggregate gate, the three out-of-gate ignored CLI journeys, docs check, and diff check. The original task's Revision 12 header is explicitly superseded by approved Revision 14 in its evidence. PASS as recorded evidence; commands were not rerun by this read-only review. |

## Applicable Rule Results

| Rule or invariant | Evidence | Result |
|---|---|---|
| Approved specification outranks task mechanics and tests | Revision 14 is named in task evidence; provider changes and human decisions are preserved | PASS |
| CLI/SDK surfaces project shared contracts and owners | `workflow.rs` delegates to `wyrd_client::{Workflow, Workflows}` and `Cards`; SDK tests use public SDKs | PASS |
| Existing `--server` remains a connection-endpoint override | `RunArgs.server` conflicts with `file`; the compiled CLI test requires that combination to fail | **FAIL — STD-001** |
| Stateful Rust workflows have cohesive owners | `WorkflowCommand`, `RunArgs`, `RunIdArgs`, and `Report` own their operations; free helpers are pure render/parse helpers | PASS |
| Async is limited to IO/composition; blocking work is not introduced into server request paths | New production async methods await loader/client/network/signal operations; no new server blocking path | PASS |
| Public identifiers and request/response values remain typed | `WorkflowRunId`, `CardSelector`, `CardRef`, `WorkflowRun`, and `CreateWorkflowRunRequest` are used | PASS |
| Public failures retain structured Wyrd errors and error results are not swallowed | Parse/client/server/output failures are mapped, but the `ctrl_c()` `io::Result` is discarded | **FAIL — STD-002** |
| Every new/materially modified Rust item has substantive rustdoc | Production items and new Rust journey helpers include workflow-role documentation and applicable error/panic notes | PASS |
| External tests earn separate-target placement | Existing `cli` target runs compiled CLI and real server; SDK files are real language/runtime integration journeys | PASS |
| User-facing surfaces have journey evidence | CLI, Rust, Python, and TypeScript journeys are present; HTTP/server behavior is exercised by the Rust/CLI server path | PASS (recorded evidence only) |
| Secrets do not enter Cards, argv, snapshots, or logs | Production CLI accepts no secret argument; tests use secret references and assert non-leakage/header separation | PASS |
| Client-tier/dependency boundaries remain intact | No SQL/cloud/DataFusion/Iceberg dependency is added to a client tier; new wiremock dependency is dev-only | PASS |
| Generated artifacts are changed through their owner and checked | Prompt reference and `llms-full.txt` align with source schema/provider changes; recorded docs/codegen aggregate is green | PASS (recorded evidence only) |
| No unrelated custom mechanism/check/setting/option is introduced | Existing checks are only updated at approved, documented seams; no new checker or compatibility setting appears | PASS |

## Material Findings

### STD-001 — `--server` is incorrectly forbidden for authored-file local runs

- **Violated authority:** approved spec Revision 14 REQ-026
  (`spec.md:1928-1943`), `architecture/references/doctrine/architecture-constraints.md`
  Surface Alignment, and the CLI client owner contract in
  `crates/wyrd/wyrd-cli/src/client.rs:27-43`.
- **Location:** `crates/wyrd/wyrd-cli/src/workflow.rs:105-107` and
  `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:242`.
- **Evidence:** the new `server` argument declares `conflicts_with = "file"`,
  and the compiled CLI journey puts `workflow run --file ... --server ...` in
  the expected usage-failure table. REQ-026 instead fixes `--server <url>` as
  the Wyrd endpoint selector and says it must not become an execution-mode
  flag. File sources are required to execute locally; that does not make the
  endpoint override invalid. Authored bundles can still need the endpoint for
  registered Agent/Prompt refs or a local `wyrd_gateway` route.
- **Observable consequence:** a user cannot explicitly point an authored-file
  local run at a Wyrd server. The invocation fails in Clap with exit 64 before
  the shared Workflow loader or gateway client can use that endpoint, leaving
  only ambient configuration as a workaround and making the CLI surface differ
  from its approved contract.
- **Required correction:** keep `--execution server --file` invalid, but allow
  `--server` with a file source and ensure that endpoint override is consumed
  by the existing shared client/Workflow configuration owner for both external
  dependency loading and local `wyrd_gateway` execution. Do not add a CLI-owned
  transport, parser, global-environment mutation, compatibility option, or new
  configuration mechanism. Replace the contrary usage assertion with compiled
  CLI proof that a file run using an explicit endpoint reaches the selected
  server while still running locally; preserve the existing negative test for
  `--execution server --file`.

### STD-002 — signal-listener failure is misreported as an interrupt

- **Violated authority:** `architecture/references/languages/implementation-execution.md`
  “Implementation and test integrity” (do not swallow errors),
  `architecture/references/languages/rust-core.md` Error Handling, and
  `architecture/references/languages/errors.md` CLI Errors.
- **Location:** `crates/wyrd/wyrd-cli/src/workflow.rs:262-271`.
- **Evidence:** `tokio::signal::ctrl_c()` returns `std::io::Result<()>`, but the
  select arm binds its output to `_`. Both `Ok(())` after SIGINT and `Err(_)`
  while installing/receiving the OS signal listener therefore print
  `interrupted` and return 130. The Tokio API documents that listener
  registration itself is fallible.
- **Observable consequence:** when signal registration fails, the CLI stops
  polling an accepted run and tells the caller they interrupted it even though
  no interrupt occurred. The underlying cause and stable CLI failure are lost,
  and automation sees a false signal exit rather than an actionable error.
- **Required correction:** inspect the signal future's result in the existing
  `tokio::select!`; only `Ok(())` may take the interrupt/130 path. Convert an
  `Err` through an appropriate existing typed CLI error when its semantics fit,
  or through the smallest derive-backed CLI error needed to describe signal
  listener failure. Do not add a signal abstraction, injection framework,
  checker, setting, retry, or compatibility option. Preserve the existing
  successful SIGINT journey as closure proof and verify from source/compile
  that the error result is no longer discarded.

## Verification Notes

Reviewed recorded evidence in the task:

- `mise run gate` — recorded exit 0 at `b88102317`.
- The three exact ignored compiled-CLI Workflow journeys — each recorded PASS
  after the gate.
- Exact Rust, Python, and corrected TypeScript SDK journey commands — recorded
  PASS.
- Python gateway integration regression — recorded 9 passed.
- `mise run docs:check` and `git diff --check` — recorded PASS/clean.

Per the caller's strict read-only direction, this reviewer ran no build, test,
formatter, linter, code generator, or executable verification command. Source
inspection covered every file in the base-to-candidate diff. Candidate HEAD was
`f2f4b87dc783df790b831b6c818c6273106d661b` immediately before this report was
written.
