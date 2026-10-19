# Independent structured Ponytail validation

Recommended verdict: **FIX_REQUIRED** for the bounded implementation findings below. **PASS is additionally blocked by two evidence-only obligations**: supported-host execution and compatible published-artifact onboarding. These are not requests to add platform code or publish a release. If implementation findings close while those prerequisites remain unavailable, record **BLOCKED**, not PASS.

## Subject, inputs, and method

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`; immutable cumulative base `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`, candidate `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`. HEAD matched the candidate. TASK-001–003 only; TASK-004/live acceptance and the final integrated gate are excluded. No prior agent-readiness remediation ledger exists.

Read subject/map, approved revision-2 spec, original three task packets, all seven discovery reports, verification.md and claims-comparison.md. No follow-up report exists or is required by the recorded comparison. Validation uses the cumulative diff and owning source, including surrounding callers and sibling consumers; the reports' agreement is not treated as proof. Generated docs/declarations are consumers of their owning source, not independent runtime implementations. No source, test, configuration, commit, or other report was changed.

Authorities used: AGENTS.md (especially §§2–9,11–12,16), agent-rules, current design client/registry/authoring/observation boundaries and doctrine, spec-driven-development, maintainer-style, reference routing, and agent-harness/error guidance. The approved spec explicitly authorizes unshipped in-place replacement and the three-target correction. No compatibility reader, upgrade migration, platform-specific isolation, new SDK/runtime engine, or release operation is justified by these findings. No `.codegraph/` exists.

I independently traced the proposed correction owners: process subscriber → shared CLI dispatcher/error printer; CLI pull → Cards → graph puller → BundleWriter → download callback/display/storage; published deploy functions → shared rollback-capable publication; Service YAML → Observe binding/table lookup and three language snippets; bundle reader → derive-backed errors → SDK/CLI/catalog; and new/materially changed Rust functions versus their base bodies. Full affected function bodies and the pertinent sibling owners were inspected before selecting corrections.

## Every discovery proposal and unique claim

| Source ID or acceptance claim | Disposition | Source-based resolution |
|---|---|---|
| BHV-001 | CONFIRMED | main applies ambient EnvFilter before an unconditional stderr sink. Shared boundary formatting cannot undo previously emitted dependency logs. FIND-TASK-002-1. |
| INV-001 | REVISED | One reported invariant contains two distinct producers. Parent tracing is FIND-TASK-002-1; graph-pull terminal drawing is FIND-TASK-002-2. Do not create command-local error scrubbers or a second startup owner. |
| MAINT-001 | CONFIRMED | Same parent subscriber defect; merged into FIND-TASK-002-1. |
| SYS-001 | CONFIRMED | Installer/foreground setup share main's sink, so dependency failure reproduces the same defect. Merged into FIND-TASK-002-1, with existing child diagnostics/lifecycle preserved. |
| BHV-002 | REVISED | Complete BundleWriter explicitly creates a stdout display. Terminal-dependent drawing is reachable; pipe-only proof cannot close it. Minimum correction removes/suppresses terminal painting on this pull path without changing transfer integrity or adding a public transport API. FIND-TASK-002-2. |
| SYS-002 | CONFIRMED | Same terminal producer; FIND-TASK-002-2. No claimed artifact corruption or shared-server outage is supported. |
| BHV-003 | REVISED | Observe download, table, binding, subject/input-slot mismatches and invisible Rust/TS telemetry setup are confirmed. Fix the public recipe using existing owners; do not require a new Rust WyrdState telemetry method or make SupportDesk own global telemetry. FIND-TASK-003-1. |
| INV-003 | REVISED | Split by owning source: continuation/visible telemetry is FIND-TASK-003-1; derive-owned recovery metadata is FIND-TASK-001-1. Generated copies must not be patched independently. |
| BHV-004 | CONFIRMED | Both derive attributes prescribe get after get became envelope-only. FIND-TASK-001-1. |
| REPO-002 | CONFIRMED | Same derive-backed recovery source; FIND-TASK-001-1. |
| INV-002 | CONFIRMED | Deletion is inherited, but these exact functions are changed, embedded and claimed as TASK-003's supported deployment recipe. It defeats the explicitly preserved local-data invariant on ordinary redeploy. This is in-scope inherited behavior, not a newly introduced regression or unrelated debt. FIND-TASK-003-2. |
| REPO-001 | REVISED | Confirm cited new functions and materially changed output-directory refusal. Exclude mechanical renames/unchanged assertion bodies and unrelated private progress tests. Batch the bounded rustdoc fixes with implementation remediation. FIND-TASK-002-3. |
| MAINT-002 | CONFIRMED | Materially changed main has module prose and an inline comment, but no item contract. Included in FIND-TASK-002-3. |
| BHV matrix AC-010; invariant/standards target-proof limits | REVISED | Source target conformance passes; required target execution does not. AC-010 and TASK-002 acceptance explicitly require supported-host evidence. Treat as evidence-only FIND-TASK-002-4, not unsupported-host code failure. |
| BHV matrix published acquisition/honesty; TASK-003 unavailable-artifact limit | REVISED | Honest caveats pass REQ-009's truthfulness obligation. They do not close REQ-001's installed acquisition path or TASK-003's explicit unavailable-real-artifact stop condition. Evidence-only FIND-TASK-003-3. No unauthorized release is a remediation step. |
| domain-review-security.md: no proposed finding | VALIDATED EMPTY | Neither merged Card selectors nor static onboarding creates a trusted tenant/principal source. Existing audited route owners remain consumers; no separate permission, replay or credential defect is established by the proposal union. |
| domain-review-data.md: no proposed finding | VALIDATED EMPTY | Shared staged publication and reader integrity remain intact. This does not contradict INV-002: deletion occurs in a published caller before that owner can preserve the prior destination. No new SQL/lifecycle/storage machinery is justified. |

No rejected proposal is carried forward as optional advice. The union is **not empty**. All retained concerns below have independent source/authority support; repetition creates no additional IDs.

## Deduplicated stable ledger

### FIND-TASK-002-1 — JSON process tracing bypasses the machine-stream policy

Status **CONFIRMED**; classification **INCORRECT**. Sources BHV-001, INV-001(part), MAINT-001, SYS-001. Authority REQ-006, AC-007, TASK-002 scenario 1.

**Location:** `crates/wyrd/wyrd-cli/src/main.rs:9–22`. Consumer: `lib.rs::run_cli_code` and `error.rs::CliBoundaryError::print`.

**Reachable cause/consequence:** valid RUST_LOG wins over the JSON default `off`; the subscriber always writes stderr. A real client/installer connection attempt emits dependency diagnostics before the one error document. Complete stderr is no longer JSON. All native commands share the process subscriber. Embedded Python/TS use the shared dispatcher but do not own this executable's subscriber and need no second transport or global subscriber as a correction.

**Independent small reproduction:** `mise exec -- python3` invoked the existing candidate-era `target/debug/wyrd get --kind Prompt --uid 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00 --server http://127.0.0.1:1 --format json` with isolated temporary WYRD_CONFIG_HOME, synthetic WYRD_ACCESS_TOKEN and RUST_LOG=debug. Exit 1; stdout 0 bytes; stderr 11 lines; whole-stderr JSON parsing failed. This binary was not rebuilt, so the source sink/filter trace is the primary proof; no specific error-code identity is inferred from that probe.

**Minimum correction:** select the diagnostic destination at the existing CLI process/presentation owner before installing tracing. In JSON mode preserve requested diagnostics in a file outside both machine streams, reusing/extending the current append-file diagnostic mechanism. Keep human filtering/output and the existing DevServer child-log destination, secret redaction, readiness timing, startup refusal and foreground wait/cleanup. Do not simply suppress ambient logging, clear RUST_LOG in proof, parse the last line, or duplicate per-command boundaries. Moving the small destination policy out of private dev scope is sufficient if reuse needs it; no logging framework or public configuration surface is required.

**Closure:** extend the existing compiled-CLI machine-stream regression with RUST_LOG=debug and an attempted unreachable-loopback operation (early usage failure alone is insufficient); assert empty stdout and one whole-stderr error JSON, and verify requested diagnostics are in the designated log. Include a successful JSON command with empty stderr. Retain the existing dev readiness/interrupt and secret-sentinel assertions if destination ownership changes. The machine result/error schema and exit semantics remain unchanged.

### FIND-TASK-002-2 — Graph pull implicitly paints terminal progress on JSON stdout

Status **REVISED**; classification **INCORRECT**. Sources BHV-002, INV-001(part), SYS-002. Authority REQ-006/AC-007.

**Locations:** `crates/wyrd/wyrd-cli/src/card.rs:498–506`; `crates/shared/wyrd-client/src/cards/pull/bundle.rs:68–75`; producer `cards/download_progress.rs:22–25`.

**Reachable cause/consequence:** dispatch_pull applies format only after pull_bundle awaits Cards::pull. Complete BundleWriter constructs DownloadProgressDisplay::stdout; materialize_artifacts → download_artifact → start/set_position draws through indicatif's terminal stdout target before the final JSON. Pipes hide the problem; clearing terminal bars cannot retract already emitted bytes. Failure can leave stdout progress without a result.

**Sibling boundary:** ArtifactMaterializer also uses the same display for internal per-Card materialization, while declaration has caller-owned progress. Those paths explain reuse choices; they do not require changing their transfer semantics. Python/TS pull use the same graph writer. Storage verification, graph traversal and workspace publication already solve their respective concerns and must stay intact.

**Minimum correction:** eliminate automatic terminal painting from graph pull, using the existing display's hidden-target mechanism or removing that presentation when it earns no machine-facing role. Human final summaries and all existing diagnostic/error information remain. Do not introduce a new public SDK options/transport/traversal layer merely to pass CLI format through. If human bars are retained, select a non-machine destination before transfer starts through the existing presentation boundary. This is a presentation correction; it must preserve authenticated init, expected digest/size validation, staged writes, failure cleanup and rollback. No consumer guard is needed in JSON serializers.

**Closure:** a focused compiled-CLI terminal-stdout capture of a complete artifact-bearing JSON pull with transfer long enough to draw must contain exactly the result document; a failed transfer must contain no stdout progress/result and one stderr error document while retaining the previous destination. Reuse existing artifact fixture/process facilities. Static proof of a hidden graph-writer target plus the existing pipe capture is also relevant, but pipe-only execution cannot falsify the original terminal defect. Do not rerun all SDK journeys.

### FIND-TASK-001-1 — Bundle recovery metadata prescribes the envelope-only command

Status **CONFIRMED**; classification **INCORRECT / public-contract regression**. Sources BHV-004, INV-003(part), REPO-002. Authority REQ-004/009, AC-009, TASK-001 error consumer closure; derive-owned error rules.

**Locations:** `crates/wyrd-spec/src/error.rs:1144,1188`, SdkUnhydratedArtifact and SdkInvalidStateBundle remediation attributes.

**Trace/consequence:** metadata-only or malformed/retired/inconsistent bundles → BundleReader/WyrdState → these stable errors → SDK/CLI metadata and generated error catalog/full reference. Both tell the caller to regenerate with get. Candidate get returns one envelope and accepts neither output-dir nor metadata mode, so following the instruction cannot restore a runnable bundle. Existing independent runtime hydration concepts remain valid and are not rename targets.

**Minimum correction:** edit these two derive attributes to instruct a complete pull of the intended Service/root into the explicit destination, without metadata-only, then load it. Reuse current error generation; regenerate errors.md/llms projections. Preserve codes/status, rejection conditions and all bundle validation. No alias, new variant, mapper or hardcoded generated copy is needed.

**Closure:** assert the generated remediation of both existing errors identifies pull and destination/completeness; preserve the existing reader negative assertions. A narrow shared-state/error test plus generated-output checking is sufficient; the already established complete pull/load proof covers the underlying operation. No broad journey rerun is required for two metadata strings.

### FIND-TASK-003-1 — Canonical Observe continuation and telemetry startup remain incomplete

Status **REVISED**; classification **MISSING/INCORRECT**. Sources BHV-003, INV-003(part). Authority REQ-001/004/008/009, AC-001/009, TASK-003 consistent runnable recipe and visible initialization.

**Locations:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:17–142,236–267`; sibling `understand/runs-observations-and-evidence.md:24`; quickstart startup at `get-started/quickstart.md:147–157`. Existing TS startup: `examples/support-desk-app/typescript/server.ts:131–151`; Rust public owner `sdks/wyrd-sdk-rust/src/otel.rs::start_telemetry/Telemetry::shutdown`.

**Producer/consumer:** canonical quickstart and Declare links lead to the runnable Observe page. Its support-desk prerequisite resolves to the actual Service YAML, which declares tickets, Service binding no-refund-promise, agent bindings ticket-complete/follows-policy, and their answer/ticket/letter slots. Observe executes get with output-dir, then records/queries answers and invokes my-check/my-review/document without declaring or identifying replacements. Clap rejects the first command. Even after manually changing the verb, table resolution and subject-bound Verifier lookup refuse the later values. All three language tabs repeat the mismatch; generated full reference republishes the source faithfully.

**Telemetry visibility:** Python quickstart embeds initialization. TS startTelemetry/shutdown exist in the app entrypoint outside every embedded startup region. Rust SupportDesk::deploy starts Bifrost but leaves telemetry to its caller, exposing state() for that purpose. The existing public Rust `wyrd_ai::otel::start_telemetry(&state)` returns its separately owned Telemetry handle; its shutdown flushes that provider, and state.shutdown drains the writer. Adding a fictitious Rust state.start_telemetry or silently relying on Python lifecycle semantics is incorrect. No SDK API addition is needed.

**Minimum correction:** use pull plus destination; align the runnable continuation and its readback with the existing tickets/binding/slot/agent-view recipe. Prefer the existing example source to another independent recipe; no new table/Verifier just to preserve stale placeholders. Show existing language-specific startup/framework hooks and matching flush/shutdown visibly in the onboarding path, including the actual Rust OTEL owner and retained handle. The Rust app need not acquire ownership of global telemetry merely to satisfy documentation. Correct the sibling download description and regenerate machine references from source. Preserve buffered-admission versus visibility, bounded execution waits, failed-verdict versus execution-error, Run attribution, async/window distinctions and abrupt-loss caveats.

**Closure:** focused content/source assertions pin accepted pull syntax, declared table/binding/slot/subject correspondence, visible TS initialization, and Rust public OTEL startup/drain. Compile/typecheck affected real source snippets through existing Rust example lib and TS example workspace; use the existing Python quickstart proof only if its executable flow changes. No generic snippet interpreter, new app runtime or full three-language journey suite is required.

### FIND-TASK-003-2 — Published redeploy deletes the bundle before safe pull replacement

Status **CONFIRMED**; classification **VIOLATION** of preserved-data behavior. Source INV-002. Authority INV-005, REQ-004/AC-005 and TASK-003 ownership of the executable published recipe.

**Locations:** `examples/support-desk-app/python/deploy.py:25`; `examples/support-desk-app/typescript/deploy.ts:20`.

**Reachability/scope:** deploy first declares, then rmtree/rmSync erases the current bundle, then invokes pull. A routine second deployment whose declaration succeeds but pull fails on read/download leaves no runnable prior bundle. These deletion lines predate this range; the finding is not a claim they were introduced here. TASK-001 intentionally limited example work to mechanical closure, but cumulative TASK-003 explicitly promotes and verifies these exact callers as the safe supported recipe. INV-005 forbids implicit local-data reset. Therefore they are in scope; unrelated deployment debt elsewhere is not.

**Existing owner:** Cards.pull delegates to BundleWorkspace: graph resolution first, complete writing into sibling staging, prior destination moved aside only for promotion, restoration attempted and failures reported. Rust support-desk-app and canonical language siblings call pull directly. Nothing requires eager deletion or bundle conversion.

**Minimum correction:** delete both eager removals and now-unused shutil/rmSync imports. Rely on the existing publication owner. Preserve declaration idempotency, explicit destination, gateway policy application, receipt/output and the guarantee that a failed later gateway-policy operation is not rolled back by inventing another transaction. Add no backup/restore wrapper, new switch or migration.

**Closure:** focused runtime-owned tests of each changed deploy caller: start with an existing usable bundle; declaration succeeds; force pull to fail; original bytes remain and still load. Successful repeat deploy still publishes replacement. Existing fixtures/fakes can isolate caller ordering; the shared workspace tests already prove promotion rollback, so no new server or filesystem fault harness is needed.

### FIND-TASK-002-3 — Bounded new/materially changed Rust items miss required documentation

Status **REVISED**; classification **VIOLATION** of explicit rustdoc gate. Sources REPO-001, MAINT-002. Authority AGENTS §16 and agent-rules; skill explicitly retains missing changed-item rustdoc as blocking. No runtime fault is inferred from prose alone.

**Exact closure set:**

- `crates/wyrd/wyrd-cli/src/main.rs:4`: materially adds JSON-sensitive logging; missing item rustdoc. Document TLS/subscriber initialization, machine versus human destination policy and dispatch/exit.
- `crates/wyrd/wyrd-cli/src/lib.rs:157`: new json_mode_is_read_from_raw_arguments test; document assertion panic conditions.
- `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:588,617,651`: new removed_card_verbs_are_usage_errors and get_help_describes_one_envelope_without_bundle_options; materially changed pull_requires_an_output_directory_before_client_construction now asserts Clap refusal rather than prior domain validation. Missing panic contracts; the latter has no item rustdoc.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/discovery.rs:221`: new onboarding helper returns transport/shape errors but assert! panics for absent taught terms. Document those distinct Errors/Panics conditions accurately.

**Minimum correction/proof:** substantive local item documentation only, batched with the behavior fixes. Preserve every assertion and error return. Exclude unchanged tests made visible by a mechanical rename, unrelated old helpers, all-file documentation sweeps and a new checker. Source inspection plus owning formatting/lint proof closes this finding; no new runtime tests for comments.

### FIND-TASK-002-4 — Supported-host execution acceptance remains unproved

Status **REVISED**; classification **MISSING EVIDENCE**, evidence-only obligation. Sources BHV AC-010 matrix and invariant/standards disclosed limits. Authority spec AC-010 and TASK-002 acceptance table.

**Location/evidence:** TASK-002 Implementation Evidence target row and commands; TASK-003 limits. Both report Linux x86_64 execution only. AGENTS target text and installer target inventory conform to exactly Linux x86_64/Linux arm64/macOS arm64. No unsupported mechanism or other-host source defect is established.

**Consequence:** AC-010 explicitly asks relevant CLI/SDK journeys to supply evidence for each supported target. Source inspection and release inventory cannot satisfy execution. An implementation table labelled PASS with a Linux-only parenthetical cannot close the runtime part.

**Closure:** obtain and attach focused existing CLI machine-output, foreground readiness/recovery and relevant SDK pull/load evidence from Linux arm64 and macOS arm64, recording candidate identity, toolchain/target, exact commands and results; use the corresponding narrow existing task selectors. Retain Linux x86_64 results. Do not rerun full journey suites, infer execution from cross-compilation, add platform-specific code or change approved target scope. This requires capable hosts, not a code patch. If hosts are unavailable, keep this acceptance obligation BLOCKED; it cannot be waived by remediation completion.

### FIND-TASK-003-3 — Installed-package onboarding has a recorded external artifact blocker

Status **REVISED**; classification **MISSING EVIDENCE / external prerequisite**, evidence-only obligation. Source: behavior published-evidence acceptance discussion, TASK-003 Material Stop Conditions and Implementation Evidence limits. Authority REQ-001/009, AC-001/009 and explicit instruction to report unavailable real package/artifact as an evidence blocker.

**Locations:** `TASK-003-self-contained-agent-onboarding.md` Material Stop Conditions and limits at 271–279; setup.md:15–23; quickstart.md:28–35. The candidate records published artifacts at 0.1.1 while the documented contracts are 0.2.0; its installed CLI/server version check refuses mismatched minors. The acquisition examples therefore do not yet supply the documented runtime. Local provider/locally built artifacts prove the repository recipe, not installation from compatible released packages. I did not query current external registries; this is the candidate's recorded blocker, not a claim about today's independent registry state.

**Disposition:** truthful version caveats are correct and must stay. They pass honesty; they do not turn unavailable acquisition into a completed installed-user workflow. Do not call this a new API defect, require removal of caveats, invent a checkout requirement for installed-server setup, or demand publication within this implementation remediation.

**Closure:** when compatible packages/server artifacts are available through a separately authorized release process, record their identities and a narrow acquisition/setup/quickstart execution using them. Until then mark this proof BLOCKED and retain the partial/unavailable-artifact qualification. No release, version bump, tag, registry write, image publication or credential/provider run is authorized by this finding. TASK-004's real-provider/live-agent proof remains separately excluded.

## Focused verification selection and limits

Inspected mise.toml, owning Cargo/example manifests and TS example package before choosing proof. Remediation should run the smallest affected exact selectors and checks; **no `mise run gate`, full CLI journey aggregate, or full SDK journey suites** are requested here. Existing relevant commands (not executed by this validator) include:

- CLI stream closure: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable)'`, extended to exercise configured tracing/terminal transfer as above. Pure subprocess additions should use their newly collected exact selectors through mise, not broaden this suite.
- If foreground diagnostic ownership changes: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'`.
- Bundle metadata negative proof: `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::rejects_metadata_only_bundle_as_unhydrated) or test(=state::tests::rejects_retired_bundle_api_version)'`. Confirm list/feature requirements when selecting the new metadata assertion; record its exact selector after authoring.
- Docs iteration: `mise exec -- pnpm --dir docs exec vitest run src/lib/content.test.ts src/lib/mdsvex/highlight.test.ts src/lib/mdsvex/page-text.test.ts src/lib/indexes.test.ts`; final touched docs: `mise run docs:check`, which regenerates source-owned references and checks drift/build/links/a11y. `mise run codegen:check` only if affected generated contract outputs require it; it does not prove MCP runtime or served OpenAPI.
- Touched Rust snippet source: `mise exec -- cargo check --locked -p wyrd-rust-examples --lib`. Touched TS example: `mise exec -- pnpm --dir sdks/wyrd-sdk-ts --filter @wyrd/example-support-desk-app exec tsc --noEmit`. Neither claims telemetry execution merely from compilation.
- Changed Python caller: repository Python format/lint/type lanes as applicable, and its new focused runtime test via mise. Changed Rust: `mise run fmt` and required owning lint proof; the existing `mise run lints` is workspace-wide, not a reason to invoke the gate. No SQL schema/security/tenant-owner change is prescribed, so do not add unrelated database/boundary suites.

The orchestrator independently ran diff-check and 42 docs tests successfully. Discovery includes compiled-binary tracing reproductions. This validator independently reproduced malformed whole-stderr JSON as recorded above and inspected source; no Cargo compile/test, database suite, live provider, PTY artifact transfer, other-host execution or external package-availability verification was run. Reported implementation results are prior evidence, not independently reexecuted results. No optional refactor/style preference or inherited unrelated debt is retained.

Skipped broad/live suites and release operations. Remaining risks are the validated stream/recipe/redeploy defects and the two explicitly blocked proof obligations; static review cannot establish other-host or published-artifact execution.
