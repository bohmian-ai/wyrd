# Independent behavior review

Result: **FAIL**. Proposed findings: **BHV-001**, **BHV-002**, **BHV-003**, **BHV-004**.

Subject: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6...98d8ef46e61ea6aa5dae90d59572467a256a3ef3`, cumulatively TASK-001–003 under approved `changes/active/agent-readiness/spec.md` revision 2. HEAD remained the candidate during this review. TASK-004, live acceptance and final integrated gate are excluded. Only this report was written.

## Scope and authority

I used the subject map as navigation, then expanded the reachable paths through CLI dispatch, local loader validation, shared Cards reads/declaration/pull, bundle reader/writer, Python generic/per-kind registry projections, TypeScript native/public projections, server Card/MCP routes, SQL declaration/replay sources, public error metadata, docs generation and the published example. No other discovery report was read. Authority includes AGENTS.md, agent-rules, the spec/task originals, spec-driven-development, maintainer-style, agent-harness/errors guidance, relevant design registry/authoring/client contracts and doctrine. No `.codegraph` exists.

The important caller paths are:

* `run_cli_code` → `dispatch_declare` → local loader/build input → shared saga → POST `/v1/cards` → existing permission/audit/service → tenant declaration transaction/replay seed → upload/completion → receipt.
* CLI `get` → selector validation → `Cards::get`; Python generic get → `PyCardEnvelope`, per-kind get → unloaded holder; TS get → shared native Cards read. Artifact transfer is separate.
* CLI/three SDK `pull` → existing graph resolver → staged bundle writer → rollback-capable publication → `WyrdState` validation → language runtime loading. The new header/mode are written and read together; metadata-only is refused for runtime use.
* Native CLI `main` installs the tracing subscriber before dispatch. All command error documents share `CliBoundaryError::print`; pull additionally reaches the shared library's terminal progress display.
* MCP initialize → `wyrd.guide start` → setup/quickstart/Declare/Observe/Verify docs; quickstart → actual support-desk-app sources → public SDK/server evidence. `llms-full.txt` repeats those owning page bodies, including their mistakes.

## Acceptance matrix

PASS below means the inspected implementation and relevant supplied proof support that obligation; it does not imply independent reexecution of every reported test.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002/AC-002 local check is local, validates transitive Cards/artifact files, reports diagnostics, assigns no versions | `card.rs::check_tree`, `dispatch_declare` check branch; `wyrd-loader::load/build_declaration_input`, `PathSandbox::resolve_regular_file` | `declare_check_validates_disk_references_without_contacting_a_configured_server`; loader and CLI invalid-reference/artifact cases | PASS |
| REQ-003/AC-003 declare rename reaches Rust, Python generic/per-kind registries, TS, testing/embedding APIs | `Cards::declare*`; Python state registry and exports/stubs; TS native/public Cards/testing; CLI commands | Existing three-language declare-and-pull and export-surface tests; supplied type/codegen evidence | PASS |
| REQ-003 persisted SQL/replay/audit vocabulary changes in place while independent outcomes remain distinct | declaration migration, query module, server mapping/service; spec submission/enums/audit detail | SQL declaration shape/replay and authenticated server declaration/replay tests; generated/served contract evidence | PASS |
| REQ-004/AC-004 get reads one envelope without artifacts; exact and UID selectors remain | `card.rs::read_envelope`; shared Cards read; Python removed eager-load branches; TS native get | CLI lifecycle/get-help; three SDK envelope tests and Python per-kind CRUD | PASS |
| REQ-004 merged MCP get rejects partial/ambiguous forms and preserves existing authorization | `mcp/cards.rs::GetCardArgs::read`, shared `get_card*_for` owners, one descriptor/dispatch entry | MCP discovery/Card journey covers selector variants, refusals, absence of old tool | PASS |
| REQ-004/AC-005 complete pull, new bundle header/types/mode, atomic publication and integrity checks | shared `cards/pull`, `BundleManifest`, state `BundleReader` | CLI complete/metadata/missing-related/integrity journeys; three SDK pull/load stories and retired-header refusal | PASS |
| REQ-004 metadata-only stays non-runnable; removed CLI load/old public APIs have no alias | state manifest header validation; CLI parser/exports; native language projections | removed-verb tests, metadata-only rejection and old-bundle refusal | PASS |
| REQ-006/AC-007 one JSON result/error without progress or logs in machine streams, including configured diagnostics and terminals | `main.rs` honors RUST_LOG with stderr writer; `BundleWriter` unconditionally creates stdout terminal progress | Independent logged CLI reproduction fails stderr whole-document parsing; pipe-only existing tests miss TTY path | **FAIL: BHV-001/002** |
| REQ-006 usage errors are structured and preserve stable codes/fields | `json_requested`, parser-error projection, `CliBoundaryError` | JSON lifecycle/usage tests; independent old get/output-dir refusal | PASS |
| REQ-006 unattended setup/MCP inputs never prompt; selected-host install remains explicit/repeatable/secret-free | dev format/noninteractive policy; MCP install format guard, report projection, existing host owner | installed-process/setup/selected-host/proxy tests supplied | PASS |
| REQ-006 foreground readiness precedes exit and existing cleanup/recovery remain | dev readiness result at proven-ready boundary; existing retained child/wait/stop owner | first-run readiness, failure/recovery and saved-client/proxy journeys supplied | PASS |
| REQ-007 start topic/instructions teach roles and retain authenticated read-only guidance | `GuideTopic::Start`, guide descriptor/response, initialize instructions, no new write tool | discovery/guide/proxy instructions tests; catalog source | PASS |
| REQ-001/008/AC-001/008 final docs continuation uses same Service tables/bindings and teaches visible initialization | quickstart links to Observe, but Observe still prescribes graph-download get, undeclared answers/my-check/my-review, and no telemetry initialization | Independent exact documented CLI command returns usage error; actual Service YAML disproves continuation names; snippet tests do not exercise this page | **FAIL: BHV-003** |
| REQ-008 actual newer Python quickstart code performs custom rows/telemetry, sync and queued checks, shutdown/readback | support-desk-app Python app/desk/deploy/evidence; new quickstart journey | supplied real-server local-provider quickstart test covers checks, unknown binding/input, denied declaration and correlated readback | PASS, within fixture-only limit |
| REQ-008 all language operations stay existing public SDK projections | support-desk reference examples, Rust SDK OTEL owner, TS app and native Cards | supplied retained three-language evidence journeys and static example/type checks | PASS for runtime APIs; documentation discovery remains BHV-003 |
| REQ-009/AC-009 llms full contains owning page bodies/actual source embeds, base-aware links, no second recipe | generator calls `pageText`, which reuses `embedCodeFromFile` renderer | orchestrator independently reran 42 content/embed/index tests successfully | PASS |
| REQ-009 error remediation agrees with final CLI semantics | derive-owned SDK bundle errors still prescribe get | source-derived error catalog and llms full reproduce stale instructions; get no longer writes a bundle | **FAIL: BHV-004** |
| REQ-009 published evidence remains honest about unexecuted real-provider/published-artifact behavior | setup/quickstart version caveats and quickstart partial status; captured fixture output is labelled | supplied quickstart fixture test; no 0.2.0 published-install or real-provider proof | PASS for honesty; external publication remains an explicitly disclosed limit |
| REQ-010 authority exactly names three targets and preserves portability prohibitions | AGENTS target correction; installer supported-target inventory unchanged | source/package/installer inspection | PASS |
| AC-010 relevant CLI/SDK execution evidence exists for every supported target | no source defect established; implementation evidence records Linux x86_64 only | Linux arm64/macOS arm64 execution unavailable, source inspection is not execution | **FAIL: evidence incomplete** |
| INV-001–005 existing Rust/server ownership, no parallel loader/transport/verification/audit, no compatibility or data conversion | shared owners extended/renamed; pure contracts stay pure; tenant/replay operations preserved | owning regression/contract/tenancy evidence; source tracing | PASS |
| INV-006/007 and non-goals: no detached process manager, new stop, MCP check/upload/declaration, live lane/CI automation, sibling-repo work | CLI retains foreground process; MCP only read/verify; task diff contains no live harness | source/diff and catalog tests | PASS |
| Limited UI wording correction only, not a new UI feature | existing Card inventory/Service wording and mocks | supplied UI check/build; static source | PASS |

## Proposed findings

### BHV-001 — JSON stderr is corrupt when tracing is configured

Classification: **INCORRECT**, blocking public machine-contract consequence. Violates REQ-006 and TASK-002 scenario 1.

Location: `crates/wyrd/wyrd-cli/src/main.rs:9-22`, particularly environment filter selection and `.with_writer(std::io::stderr)`.

The JSON branch changes only the default filter to `off`. Any existing `RUST_LOG` wins and routes subscriber output to stderr. The normal client transport emits dependency tracing during an operation; the boundary then appends its error JSON to the same stream. This affects every native CLI command using that subscriber, including get/pull/declare/server install/dev/MCP install, rather than one error formatter.

Independent reproduction on the existing candidate binary (through `mise exec -- python3`, `subprocess.run(..., capture_output=True)`): environment `RUST_LOG=debug`, an isolated nonexistent `WYRD_CONFIG_HOME`, placeholder `WYRD_ACCESS_TOKEN`; command `target/debug/wyrd get --kind Prompt --uid 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00 --server http://127.0.0.1:1 --format json`. It exited 1 with zero stdout bytes and **11 stderr lines**, including rustls/reqwest DEBUG lines followed by `wyrd_cli_error`. `json.loads(stderr)` failed. This uses no real credential or external network service.

Producer/sibling tracing: main owns the one subscriber; client construction/reqwest produces logs; `run_cli_code` and `CliBoundaryError::print(true)` correctly produce one error document but cannot remove already emitted logs. DevServer's new `Diagnostics` already demonstrates a file destination for child/progress output; it does not govern this parent subscriber. Existing lifecycle test helper explicitly removes RUST_LOG (`tests/card_lifecycle.rs:36-40`), so its passing JSON assertions cannot prove this case. Python/TS in-process embedding does not own the native executable's subscriber and should not acquire a second transport or error contract to fix it.

Required testable correction: apply the CLI JSON diagnostic destination policy at the parent subscriber owner as well as command presentation. Preserve explicitly requested diagnostics by routing them outside stdout/stderr in JSON mode, reusing the existing diagnostic-file approach; do not merely rely on a default filter or append/parse around logs. Add the smallest compiled-CLI regression with an enabled logging filter and a reachable failure, asserting empty stdout, whole-stderr JSON, and diagnostic logs outside both streams. No new error catalog or per-command logging framework is necessary.

### BHV-002 — Complete pull still writes terminal progress into JSON stdout

Classification: **INCORRECT**, blocking machine-contract consequence. Violates REQ-006/TASK-002 and the final pull projection.

Locations: `crates/wyrd/wyrd-cli/src/card.rs::dispatch_pull/pull_bundle`; `crates/shared/wyrd-client/src/cards/pull/bundle.rs:68-75`; `cards/download_progress.rs:22-25`.

The CLI does not pass its output policy into the download workflow. Every complete bundle writer creates `DownloadProgressDisplay::stdout()`, which uses `ProgressDrawTarget::stdout`. Indicatif suppresses this on a pipe, but when stdout is a terminal a sufficiently long artifact transfer draws terminal control sequences/progress before the eventual JSON summary. Clearing the display is another terminal write; it does not make the historical byte stream a JSON document. `--format json` must work independently of terminal detection.

Producer/sibling tracing: `dispatch_pull` → `pull_bundle` → `Cards::pull` → `CardGraphPuller::pull` → `BundleWriter::write/materialize_artifacts` → shared download callback → display. `Cards::download_artifacts_to` also constructs the same implicit stdout display through its internal materializer (`handle.rs:514`); Python/TS pulls project the shared pull owner. Both complete and failing transfers clear the display, but neither knows CLI JSON mode. The existing progress/JSON CLI tests use `Command::output` (pipes) and therefore cannot falsify the terminal case. This is source-proved; I did not create a separate artifact server/PTY fixture during review.

Required testable correction: eliminate implicit machine-stream presentation from the shared pull operation, or route/suppress its presentation under the existing CLI output policy before transfer starts. Follow the smallest solution: the shared SDK transfer need not own terminal UI, and no new transport or traversal is required. Preserve artifact verification, staging and publication. A focused actual-CLI check with terminal stdout, JSON selected and a delayed artifact must leave stdout as one parseable JSON result (and a transfer failure must leave no stdout result). Reuse the current transfer fixture and process/terminal facilities; do not add a generic harness.

### BHV-003 — The linked Observe continuation cannot run against the declared Service

Classification: **MISSING/INCORRECT**, blocking documented user workflow. Violates REQ-001, REQ-004, REQ-008, AC-001/009 and TASK-003's consistent-recipe obligation.

Primary location: `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:17-33,43-68,85-142,236-267`. Related consumer: `understand/runs-observations-and-evidence.md:24`; generated `llms-full.txt` carries the same source text.

The canonical Declare page links directly to this Observe guide, and quickstart's own-app adaptation links to it too. Its prerequisite explicitly identifies the newly aligned support-desk Service, but step 1 still says get downloads its graph and executes `wyrd get ... --output-dir ./support-desk`. The candidate rejects that exact command with `WYRD_CLI_400_USAGE`, `unexpected argument '--output-dir' found` (independently executed). After changing only that verb, all three language continuations record/query `vala.datasets.answers` and call `my-check`/`my-review`; `examples/support-desk-app/service/support-desk.yaml:10-29` instead declares `tickets`, binds `no-refund-promise` on the Service, and binds `ticket-complete`/`follows-policy` on component alias `agent`. No instructions explain creating the alternate table/Verifiers or replacing those names. The shared Observe verifier lookup (`crates/shared/wyrd-client/src/observe/mod.rs:292-318`) refuses an unbound name before server execution.

The startup snippets only start Bifrost; none initializes telemetry, despite later querying spans. Quickstart now shows Python initialization, but its TypeScript initialization is outside all embedded regions (`examples/support-desk-app/typescript/server.ts:131-151`) and its Rust reference does not initialize telemetry at all. The existing Rust mechanism is `wyrd_ai::otel::start_telemetry`, used by `examples/support-desk/rust/support_desk.rs:242`, with its owned telemetry shutdown. The shared WyrdState has no Rust `start_telemetry` inherent method. Thus an unfamiliar Rust caller cannot obtain the required trace recipe by translating the Python call literally.

Producer/sibling tracing: the actual Service YAML is the recipe authority; quickstart and Declare/Understand now embed it; Observe retained a different, incomplete recipe, which the full-text generator faithfully exports. Python/Rust/TS versions repeat the same mismatch, so this is one incomplete documentation integration, not three SDK defects. The focused new quickstart journey tests the Python app, and example/type checks prove compilation; neither exercises the Observe continuation or validates visible multi-language startup instructions.

Required testable correction: reconcile this continuation with the owning support-desk recipe, using pull, the declared table and exact existing binding names/subjects/input slots, or explicitly introduce and explain every replacement before its use. The smallest correction reuses the already tested example's names and source snippets; do not create an answers table or new Verifiers merely to preserve old prose. Show the existing language-specific telemetry startup and flush/shutdown in the onboarding path (including Rust's actual OTEL owner), and correct the sibling Understand download description. Regenerate machine references from sources. Focused content/source assertions plus executing or compiling the selected public snippets against the existing Service are sufficient; no generic docs interpreter or new SDK lifecycle is needed.

### BHV-004 — Bundle errors instruct callers to use an operation that no longer creates bundles

Classification: **INCORRECT**, blocking public error-remediation consequence. Violates REQ-004/009, AC-009 and the task's graph-download error vocabulary closure.

Location: `crates/wyrd-spec/src/error.rs:1144,1188` (`SdkUnhydratedArtifact`, `SdkInvalidStateBundle`).

The first remediation says `Run wyrd get without --metadata-only and load the complete bundle`; the second says `Regenerate the bundle with wyrd get and load the complete output`. Candidate get prints one Card and accepts neither an output directory nor metadata mode. A caller following either source-derived recovery instruction never obtains a runnable replacement bundle.

Producer/sibling tracing: `WyrdState::from_path`/`BundleReader` produces these errors for metadata-only, old-format, missing/corrupt or inconsistent bundles; Python and TS loaders delegate to it. Derive-backed metadata is the shared producer consumed by SDK exceptions/CLI boundary/catalog. `docs/src/content/docs/api/errors.md:307-309` and llms full reproduce it. Fixing only those generated text copies would leave every runtime caller wrong. The existing state error/refusal tests prove the errors occur but do not prove their remediation remains executable after the get/pull split.

Required testable correction: change the two owning derive attributes to prescribe complete pull with an output directory and the selected root identity; regenerate the owning catalog/reference output. Preserve codes, error conditions and metadata-only refusal. Extend the nearest existing bundle-error assertion to verify the shared recovery metadata names pull, and use the existing pull/load focused journey as execution proof. No new mapper, compatibility command or error variant is warranted.

## Verification and limits

The independent checks here were the two actual CLI reproductions described above, source/caller inspection, the removed-name search, and HEAD/status checks. A first attempt to launch the reproduction through `mise exec -- python` could not resolve that shim; rerunning through `mise exec -- python3` completed the reproduction. No broad test suite or Cargo lane was run. The orchestrator's `verification.md` records independent diff-check and 42 docs test passes. Those successful checks do not cover the failures above.

The supplied task evidence reports focused SQL/server/CLI/three-SDK/MCP/quickstart/codegen/docs/lint/type checks. I treated those as reported results and checked their scenarios against source, rather than treating green checks or implementation summaries as acceptance proof. Review execution was Linux x86_64 only; the native binary was the existing `target/debug/wyrd`, whose runtime behavior agrees with candidate source. This review did not rebuild it or execute the terminal-transfer case independently.

AC-010 remains unproven for Linux arm64 and macOS arm64. Its exact text requires relevant CLI/SDK journeys and target-specific execution evidence for each supported target; TASK-002 likewise names runtime evidence on the supported hosts. The original evidence explicitly says those two hosts received source/release-matrix inspection only. That is an evidence gap, not proof of a platform code defect and not justification to add a platform-specific mechanism or a new permanent gate. Required closure is actual relevant focused execution on those supported hosts, or explicit owner-approved scope/evidence revision. Final integrated gate and TASK-004 live acceptance remain deferred under this review's established scope.

No optional refactor, new dependency, stronger durability guarantee, compatibility path, product decision, or unrelated defect is requested. Existing source owners are sufficient for all four bounded corrections.
