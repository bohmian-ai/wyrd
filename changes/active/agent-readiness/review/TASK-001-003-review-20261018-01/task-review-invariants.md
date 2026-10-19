# Independent invariant review — TASK-001–003

Result: **FAIL**. Proposed IDs: **INV-001**, **INV-002**, **INV-003**.

## Subject and method

Base `1b962bb00`; candidate/unchanged HEAD `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`. Authority is approved agent-readiness spec revision 2, original TASK-001/002/003, AGENTS.md, agent rules, spec-driven-development, maintainer-style, current design/doctrine and routed agent-harness/errors references. TASK-004 is excluded. No implementation summary or other reviewer report was used. No source edits or broad test suites were made.

Expanded the navigation map through loader parse/path/composition and artifact validation; declaration hashing, replay and completion; selectors and audited read delegation; bundle staging/publication and state validation; Python per-kind get and local materialization; TS native/public projections; CLI tracing, error, setup and foreground owners; MCP guide/catalog; source-to-reference generation; published quickstart and sibling deployment callers. Reviewed the cumulative range rather than a latest-fix range. Reported task verification is supporting evidence, not independently rerun proof.

## Acceptance matrix

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002/AC-002: local check, transitive files, diagnostics, no server contact or identity assignment | `card.rs::dispatch_declare` branches to `dispatch_check/check_tree` before client creation; loader `load/build_declaration_input`, `PathSandbox::resolve_regular_file` opens artifacts; shared graph ordering/composition | CLI offline cases and retained loader file/artifact/cycle tests; TASK-001 focused evidence | PASS |
| REQ-003/AC-003: hard declaration names through SDK, wire, SQL/replay/audit; independent lifecycle outcomes retained | DeclarationInput; Cards/saga; DeclareCardRequest/Response; DeclarationReceipt/ReplaySeed/OperationId; declaration operation migrations/queries/reconciliation; CardDeclaration audit; native bindings/stubs/generated projections | Reported SQL/server/SDK declare-and-pull, serialization/export/codegen/typecheck and served OpenAPI proof | PASS for executable contracts; residual prose noted below |
| REQ-004/AC-004: one artifact-free envelope, exact/UID selectors and refusals | Cards::get; CLI read_envelope; Python generic/per-kind get without eager download; TS get; MCP GetCardArgs::read delegates exactly one selector to existing audited reads | Changed CLI/SDK get and MCP selector/catalog journeys reported | PASS |
| REQ-004/AC-005: complete integrity-verified bundle, metadata-only refusal, new format only | Existing CardGraphPuller/BundleWriter/BundleWorkspace retained; BundleManifest `wyrd/bundle/v1`/`mode`; state header/count/root/edge/inventory/digest validation | Three-SDK download/load journeys, state retired-header/metadata-only and publication-fault tests | PASS at shared owner; FAIL at published callers — INV-002 |
| REQ-004/INV-005: removed public CLI load/hydrate names, local runtime materialization retained | CLI enum/commands/testing projections remove load; no hydrate alias; Data/Model local load takes path supplied by state hydrator | Removed-name refusal/export/type/state journeys reported | PASS |
| REQ-006/AC-007: one JSON result or error, no diagnostic/progress stream corruption | Boundary formatter and raw mode detection correct; main still sends RUST_LOG-enabled tracing to stderr; complete pull still constructs stdout progress | Existing tests clear RUST_LOG and use pipes; independent candidate-binary reproduction fails whole-stderr JSON parsing | FAIL — INV-001 |
| REQ-006/INV-006: foreground readiness before exit, cleanup and saved state retained | DevServer keeps existing start/prove/save/ready/wait/stop; Diagnostics redirects child output; readiness redacts storage URL | Reported real readiness/fresh-client/input/listener/credential/host journeys | PASS for lifecycle; parent diagnostics affected by INV-001 |
| REQ-006/INV-003: unattended input/host refusal, protected credentials, repeatable host config | JSON disables interaction; existing installer/HostInstaller/proxy/credential owners reused | Installer/mock-release, host/proxy and protected-persistence proof reported | PASS |
| REQ-007/AC-001/007: authenticated static start guidance, existing investigation/verification owners | GuideTopic::Start; guide response and instructions; merged get; no check/declaration/upload/ingest tool | Discovery/examples/catalog and selected-host proxy proof reported | PASS |
| REQ-001/008/009/AC-001/008/009: consistent runnable continuation and recovery | Quickstart embeds actual app; next Observe guide still executes get with output-dir and uses undeclared table/verifier names; catalog still recommends get for bundles | Published quickstart journey does not exercise next-page continuation; source proves rejected command | FAIL — INV-003 |
| REQ-008/INV-004: exact attribution, telemetry/custom rows, sync and async judgments, shutdown/readback | App uses declared table and bindings, component Run views, explicit telemetry/shutdown and existing SDK owners | Published Python app and canonical three-SDK support-desk journeys reported; permission/binding/input/failed-verdict and post-shutdown readback | PASS for exercised path; guide continuation fails above |
| REQ-009/AC-009: full reference bodies and embeds, source generation and base links | Generator calls pageText; pageText reuses embedCodeFromFile resolution and preserves fenced content | Page-text/highlight/index tests and docs:check reported | PASS mechanics; stale source faithfully republishes INV-003 |
| REQ-010/AC-010: supported target authority and portable lifetime | AGENTS three-target correction; no Intel installer expansion or new platform mechanism | Static inventory; reported execution only Linux x86_64 | PASS source; cross-target execution is an evidence limit |
| INV-001/002: existing owners, pure contracts and thin language projections | Existing loader/graph/saga/read/state/bootstrap owners extended; no parallel durable implementation; pageText is presentation over existing embeds | Source comparison, reported deps/codegen/types | PASS |
| INV-003/004: tenant/auth/audit/replay/durability preserved | Existing TenantConn, replay attribution/hash, server permission owners and Scribe outbox retained | Reported SQL/server/MCP/SDK negative/replay and boundary checks | PASS within changed paths |
| INV-005: no implicit local reset | No converter/aliases; but published Python/TS deploy deletes old bundle before pull | Shared publication rollback test cannot cover caller deletion | FAIL — INV-002 |
| INV-006/007/non-goals: no detached/stop API, new engine, MCP upload/check/declaration, sibling edits or automated live lane | Diff retains existing foreground/engine owners; TASK-004 machinery absent; sibling projects described separately | Source and task non-goal evidence | PASS |
| AC-011: deterministic task contribution; final change gate still required | Task evidence lists focused native/SDK/docs/contract/boundary checks | No broad suite rerun or final gate claimed by this reviewer | PASS as task contribution, not final change completion |

## Findings

### INV-001 — INCORRECT: diagnostic routing still breaks JSON streams

**Obligation:** REQ-006/AC-007 require complete stderr to be one error document and progress/logs to use a diagnostic destination, without exceptions for RUST_LOG or terminal output.

**Locations:** `crates/wyrd/wyrd-cli/src/main.rs:9–21`; `crates/shared/wyrd-client/src/cards/pull/bundle.rs:68–70`; `cards/download_progress.rs::stdout`.

**Trace:** Raw JSON selection reaches main and dispatch. Main only changes the default filter; `EnvFilter::try_from_default_env` accepts RUST_LOG and its subscriber always writes stderr. TLS/HTTP/client events therefore precede `CliBoundaryError::print(true)` on the same stream. Complete pull also always constructs an indicatif stdout display. Pipes hide that display, but a terminal target can emit progress bytes; clearing bars cannot retract bytes already emitted.

**Independent reproduction:** Current built candidate binary, no real credential/server:

```sh
WYRD_API_KEY=wyrd_test_sentinel RUST_LOG=debug target/debug/wyrd get \
  --kind Prompt --name does-not-exist --space default --version 1.0.0 \
  --server http://127.0.0.1:9 --format json
```

Six rustls/reqwest/hyper DEBUG lines preceded the `WYRD_CLIENT_503_TRANSPORT_DOWN` JSON document. The command failed nonzero and whole stderr is not JSON.

**Sibling/existing-owner comparison:** All installed setup/lifecycle commands share main's subscriber. Embedded Python/TS run_cli_code shares the formatter but inherits host tracing. Pull and artifact downloads share DownloadProgressDisplay. DevServer::Diagnostics already routes child output/progress to its log but does not route CLI-parent tracing. LocalDev/installer/lifecycle process tests remove RUST_LOG; pipe capture misses terminal drawing.

**Consequence:** Required unattended error parsing fails under a normal diagnostic environment; terminal JSON pull may emit non-JSON progress.

**Smallest correction and proof:** Make JSON parent tracing and download presentation obey the diagnostic destination through current CLI/output owners; preserve human progress and diagnostic availability. Do not clear RUST_LOG in tests, discard diagnostics, or weaken parsing. Keep durable downloads and direct SDK behavior independent of CLI presentation. Add process proof with enabled RUST_LOG asserting empty success stderr or exactly one error document, and artifact-bearing terminal JSON pull proof without drawing bytes. Preserve DevServer child-log redirection.

### INV-002 — VIOLATION: published deployment erases the prior bundle before safe replacement

**Obligation:** INV-005 prohibits implicit local reset; REQ-004/AC-005 and TASK-001 scenario 4 require safe failed/incomplete pull. TASK-003 owns the published recipe and executable proof.

**Locations:** `examples/support-desk-app/python/deploy.py:25–26`; `examples/support-desk-app/typescript/deploy.ts:20–21`; both embedded in quickstart/llms-full.

**Trace:** On redeploy, declaration succeeds idempotently, then `shutil.rmtree(BUNDLE, ignore_errors=True)` or `rmSync(bundle, {recursive:true,force:true})` destroys the last usable bundle. If pull then fails on network, authorization or transfer, no prior bundle survives. The shared pull owner cannot restore data deleted by its caller.

**Sibling/existing-owner comparison:** BundleWorkspace::publish already moves an existing destination aside, promotes only complete staging, and restores on failed promotion. All SDK pull projections use that owner. The Rust support-desk-app deployment and all canonical support-desk language siblings use pull directly without deletion. No conversion/reset is required by the coordinated replacement contract.

**Provenance and reachability:** The deletion statements are inherited, not introduced by this range. This is not claimed as a new regression or unrelated repository debt: these exact pull callers are changed and newly embedded/verified as the supported TASK-003 recipe, and its preserved-data invariant fails on a routine second deployment. Existing proof uses a fresh destination only.

**Consequence:** A failed documented redeploy destroys the previous runnable deployment, potentially preventing restart.

**Smallest correction and proof:** Delete the eager deletions and unused imports in both examples, relying on Cards.pull's existing publication/replacement. Exercise an existing valid bundle, successful declaration followed by failed pull, and assert original bytes/state remain loadable; also prove successful redeploy replaces normally. Add no backup owner, restore guards, migration or SDK option.

### INV-003 — MISSING: final get/pull semantics did not reach continuation and recovery sources

**Obligation:** REQ-001 requires consistent final runnable guides and declared aliases/tables/Verifier/slot names. REQ-004 makes get envelope-only. REQ-009 requires accepted arguments and actionable source-derived remediation to agree. TASK-001 owns graph-download error closure; TASK-003 owns onboarding continuation.

**Locations:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:29–33`; `docs/src/content/docs/understand/runs-observations-and-evidence.md:24`; `crates/wyrd-spec/src/error.rs:1144,1188`; generated `docs/src/content/docs/api/errors.md:307,309` and llms-full.

**Trace:** The declaration guide's Next link leads to Record a run and observations. Its first command remains `wyrd get ... --output-dir ./support-desk`. GetArgs no longer accepts output-dir, so this fails with usage error before step 2 can load state. Removing the option produces only an envelope, not the required bundle. The overview repeats the obsolete download semantics. Missing/metadata-only/corrupt bundles flow through shared state errors to SdkUnhydratedArtifact/SdkInvalidStateBundle; both derive-owned remediations still tell callers to regenerate using get. Every language/catalog faithfully projects this unusable recovery.

The same runnable Observe page starts from the actual support-desk Service but records `vala.datasets.answers` and calls `my-check`/`my-review` with `document`, without replacement instructions or declarations. The actual published Service declares `tickets`, Service-bound `no-refund-promise`, agent-bound `ticket-complete`/`follows-policy`, and slot `letter`. These continuations cannot run against that declaration after the download command is manually repaired.

**Sibling/existing-owner comparison:** Quickstart and declare-and-inspect use pull correctly. GetArgs/PullArgs, Cards and state owners already implement the intended split; no compatibility alias is needed. Error metadata belongs in wyrd-spec::error; docs bodies/embeds flow through the existing generator into llms-full, so editing generated copies independently would duplicate authority. Actual quickstart source already supplies matching table/binding/slot/component recipes.

**Consequence:** A reader following the canonical next link stops before state loading; recovery from invalid/incomplete bundles cannot succeed as instructed; the subsequent unqualified runnable example targets undeclared resources.

**Smallest correction and proof:** Complete source-owned get→pull instructions with explicit destination, update both derive-owned bundle remediations, and regenerate dependent references. Align runnable Observe values/component views with the declared example, or explicitly identify replacements and where to declare them as REQ-001 requires. Reuse existing example/SDK operations; do not add get aliases or a second recipe engine. Prove corrected download creates state-loadable output, error metadata names the working operation, and continuation record/verify calls match the published declaration. Use existing journey/docs owners, not a generic docs-command interpreter.

## Non-blocking notes and limits

- Residual Registering/Registration prose exists in current docs/type descriptions. Without an executable consequence it is not a separate blocking issue under the review threshold.
- Rust/TS quickstart copies are honestly labeled compiled/typechecked references; standalone sibling projects are outside scope. Published 0.1.1 versus source 0.2.0 and local-provider output limitations are disclosed.
- No broad suite, cross-target execution, live-agent lane, cloud call or process-failure injection was performed. Independent binary reproduction supports INV-001; other findings are source-traced. No claim of final integrated gate completion is made.
