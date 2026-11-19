# Repository standards review — TASK-003

Result: **FAIL**. One mandatory changed-item rustdoc finding, `REPO-001`.

## Immutable subject and limits

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; complete first-review range `5775c3795ebd6557d080b34d55dd2a67b8dbad63...634e19cbc517293ed2458f30ce73401714f3447d` (47 paths). Approved positioning specification revision 5 and original task working content hash `6846032793910170e76c0cc0ec9de2f50cb6ad6f`. HEAD and task content hash were independently rechecked and match the recorded subject. No prior acceptance was carried forward.

Read-only source review; only this report was written. Source was inspected from the recorded Git range, with applicable current authorities and the pinned original task. No compilation, format mutation, code generation, database journey, or broad suite was run. `verification.md` supplies independently executed docs tests/link/command checks and diff whitespace checks. The original task supplies reported three-language journeys, format/lints/typechecks, build/accessibility checks and example compilation; these remain supplied evidence, not runs independently repeated by this reviewer. This report reviews repository standards, not the task acceptance verdict or the domain review's evidence conclusions.

## Authority coverage

| Changed surface (all changed paths grouped) | Applicable authority | Source and compliance evidence |
|---|---|---|
| `examples/finance-release/api.{py,rs,ts}`, request JSON and captured API output | Root AGENTS Code, Testing, client/server and errors; `agent_docs/{rust,errors,maintainer-style}.md`; `architecture/wyrd-design.md` Client model and Verifier; docs AGENTS Examples | All three use public WyrdState verification and preserve catalog error codes. Python uses the existing typed `Release`; Rust and TS use their supported JSON/input boundary. Output is checked by actual-program finance journeys. No Wyrd handler, schema or transport implementation changes. |
| `examples/refund-agent/refund_agent.{py,rs,ts}`, `investigate.{py,rs,ts}`, two captured outputs | Root AGENTS Code, client/server and Bifrost clients; Rust guide; errors and maintainer guides; wyrd-design Client model/Verifier; bifrost-design Durability and visibility; security-posture Permission model and Tenant/data isolation | Public telemetry/state owners generate spans; public Bifrost and Execution perform inspection. Inspection SQL is parameterized on resolved Verifier UID. Three-language real-server journeys execute the actual files. Rust constant docs fail below. |
| `examples/scheduled-check/checkout/{checkout,every-minute,latency-budget}.yaml`, `record.{py,rs,ts}`, `results.{py,rs,ts}`, captured output | Root AGENTS Cards, Time, platforms and Bifrost clients; Rust guide; wyrd-design Verifier/Trigger/Runtime identity; bifrost-design Durability and visibility; security-posture Permission model | YAML uses `wyrd/v1` Service/Verifier/Trigger with a binding; existing server schedule owns execution. Programs record using the workload account and read public results using Bifrost. Sample-window test timing does not implement coordination deadlines or SQL. Rust constant docs fail below. |
| `examples/tests/{finance_release,refund_agent,scheduled_check}.rs` | Root AGENTS Code/Testing; Rust guide including private and test rustdoc; maintainer guide; TESTING testing tiers and fixtures | Real WyrdTestServer, actual compiled binaries and public SDK readback. Separate binaries earn their integration placement by driving real servers/programs. `mise.toml:1136` registers the example journey lane with `--run-ignored=all`; newly ignored database journeys are included, not disabled proof. Changed helper panic docs fail below. |
| Python integration changes in `test_docsite_quickstart.py`, `test_scheduled_drift_alerts_operator.py`, `test_verifier_building_blocks.py` | Root AGENTS Testing; `sdks/wyrd-sdk-python/AGENTS.md` Errors/Tests; TESTING fixtures; maintainer guide | Top-level test functions, integration marking, public wyrd modules and exact error-code assertions. Tests run actual tutorial source via runpy against WyrdTestServer, with output assertions; Python telemetry behavior is exercised in Python. No native wrapper/export/stub changes. |
| TS integration changes in `finance-release.test.ts`, `refund-agent.test.ts`, `scheduled-check.test.ts` | Root AGENTS Testing; `sdks/wyrd-sdk-ts/AGENTS.md` Types/Style/Errors/Async/Tests; TESTING fixtures | Public SDK and existing serverTest/runtimeAccount support; actual Node example programs; typed WyrdError unknown-execution assertion and output comparisons. Work is bounded, not an unbounded Promise fanout. No N-API or generated declaration changes. |
| `examples/{Cargo.toml,package.json}`, root Cargo.lock, TS pnpm-lock.yaml | Root AGENTS Dependencies/Code; wyrd-design Client model; SDK boundary rules | New binary registrations, existing workspace tracing dependencies and SDK telemetry peer packages. No server dependency introduced into a client, no PyO3 feature expansion, wildcard version or per-crate profile. Lockfile changes match importer/dependency additions. |
| Four verify tutorial pages; response-draft link repair; verifier-card reference addition | `docs/{AGENTS,CONTRIBUTING,PRODUCT}.md`; root vocabulary/ownership; architecture Verifier/Trigger and bifrost visibility | Four task-oriented guides with prerequisites, numbered steps, source file tabs, visible outcomes, failure tables and Next; supported public components and retained canonical paths. Reference adds drift method/baseline facts. Existing response page stays draft with corrected destinations. No new UI component or styling. |
| `docs/public/{llms.txt,llms-full.txt}`, `docs/src/lib/indexes.test.ts` | docs CONTRIBUTING Generated pages/frontmatter; docs AGENTS; `agent_docs/agent-surfaces.md` Generated artifacts | Four published tutorial destinations added to compact index; full text incorporates source-backed content. Index assertion changes its expected noun while preserving route/content/component-stripping assertions. Supplied docs generation/build evidence and independent docs tests/link checks cover consistency; no wire schema or runtime generated contract changed. |
| `changes/active/docsite-incremental-reset/dispositions.md` | `changes/spec-driven-development.md` authority/handoff; approved spec/task scope | Changes only tutorial disposition ownership and canonical destinations. It does not change approved runtime behavior or add an implementation contract. |

Authorities inspected: root and all three applicable nested AGENTS; `agent_docs/index.md`, Rust, maintainer-style, errors and agent-surfaces; `architecture/README.md`; owning sections of wyrd-design (Client model, Runtime identity, Verifier, Trigger), bifrost-design (Durability and visibility) and security-posture (Permission model, Tenant and data isolation); TESTING; spec-driven development; docs PRODUCT/CONTRIBUTING. No `.codegraph/` exists. No operations, SQL-directory or UI-directory rules govern changed implementation: the diff changes no deployed server, SQL capability, migration, host UI or process topology.

## Rule results

| Applicable rule | Result | Evidence / qualification |
|---|---|---|
| Wyrd nouns, existing Card envelope/kinds and exact-version contracts | PASS | New checkout YAML uses only Service, Verifier and Trigger; investigation prints the stored judged/subject versions. No compatibility alias added. |
| Server owns durable behavior; clients reuse public state, telemetry, verification and Bifrost owners | PASS | Example imports are SDK-facing; no server runtime or native binding changed. Scheduling remains declared Trigger/binding behavior. |
| Tenant authority and public grants stay at existing owners | PASS | Public Cards/Bifrost/Execution clients; checkout runtime account uses workload restriction. Python investigation journey checks a foreign tenant's exact not-found code. No SQL pools, tenant-table queries or authorization implementation added. |
| No platform-specific execution, library runtime, secret exposure or schema migration | PASS | No platform conditional/sandbox command, library Tokio runtime, literal real credential or migration in range. Top-level example Tokio runtimes are binaries. Test credentials are passed to readers rather than printed. |
| Structured existing errors and negative public flows | PASS | API examples branch on Wyrd error code; viewer invocation proves permission refusal; unknown execution tests assert the catalog code. No invented public error catalog or message parsing. |
| Every changed Rust item and helper has meaningful rustdoc; fallibility/panics documented | **FAIL** | `REPO-001` covers three undocumented new constants and scheduled helper panic omissions. Other added functions, structs, fields and tests have intent docs; fallible binary mains have Errors sections. |
| Caller-owned pure helpers, concrete dependency owners and synchronous pure work | PASS | New Reader test fixture owns its subprocess directory/connection environment; pure response normalization stays synchronous. No new trait or abstraction layer. Detailed abstraction comparison belongs to selected maintainer review. |
| No IO/input unwrap or weakened existing gate | PASS | Added Rust external failures use propagation or named test expectations. New ignored database tests are exercised by the existing ignored-journey lane. No allow or weakened assertion introduced. |
| Real-server journey proof at all presented languages, no unit-only substitute | PASS (source and supplied runs) | Rust binary tests, Python runpy tests and TS Node tests all reach WyrdTestServer. Source/output fixtures are the actual guide files. No independent database rerun here. |
| Python/TS errors, public imports, boundary projection and lifetime placement | PASS | Changed SDK files are tests only; native APIs, exports and generated types are unchanged. Python trace provider and Node execution remain in their own language journeys. |
| Docs publication, source embeddings, native reader tools and no invented guarantees | PASS (within standards scope) | Four pages embed actual example files and show captured outcomes; no adoption/performance/model-quality promise. Public steps use uv/cargo/Node/pnpm and existing docs components. Approved revision 5 excludes live-provider/package-availability publication gates. |
| Generated content consistency and unchanged public-contract generation | PASS (supplied and independent docs evidence) | No schema/type/error generator input changed. `verification.md` reports docs tests 102/102, linkcheck and command check passing. Task records regeneration plus docs build/accessibility. No claim of independent codegen execution. |
| Applicable narrow checks and no extra runtime/hot-path work | PASS (supplied evidence) | Task records focused three-language actual-program journeys and format/lints/typechecks; diff contains no server hot path, so no benchmark obligation activated. |

Non-blocking convention observations: the new finance helper imports BufRead/BufReader locally despite the Rust guide's module-import preference. The docsite program journeys also retain the existing example-test execution pattern (subprocess/runpy, output parsing, bounded visibility polling and per-program credentials), which differs from TESTING's direct client-facing SDK story checklist. These do not establish a product failure or a mandatory merge gate comparable to the explicit rustdoc requirement; this review does not prescribe a new harness, source relocation, or rewrite of the proof.

## Material finding

### REPO-001 — Missing required rustdoc on newly added Rust items

Classification: **VIOLATION**.

Authority: root AGENTS Code, “Every new or changed Rust item has rustdoc” and “Missing rustdoc blocks merge”; `agent_docs/rust.md:235` explicitly includes private helpers and tests, and requires Panics sections for possible panics.

Locations at candidate:

- `examples/refund-agent/refund_agent.rs:21`: `ORDER_1042` has no rustdoc, although it is the shared synthetic order used by both complete and skipped branches.
- `examples/refund-agent/investigate.rs:14`: `FAILED` has no rustdoc for the retained-failure query used by the investigation.
- `examples/scheduled-check/results.rs:11`: `RECENT` has no rustdoc for the subject/Verifier-filtered recent-window query.
- `examples/tests/scheduled_check.rs:96`: `now()` has an `expect` on the clock/epoch invariant but no Panics section; `clear_of_boundary()` at line 105 calls it and likewise does not document that panic invariant.

Source evidence: these items are wholly added in the immutable diff. The three constants are preceded by imports or a completed preceding item, with no `///` or `#[doc]` attribute. `now()`'s documentation is only “Seconds since the Unix epoch,” while its body at lines 97–100 calls `duration_since(UNIX_EPOCH).expect(...)`. The later boundary helper invokes this function before and after its wait.

Consequence and materiality: the candidate fails an explicit repository merge gate covering private/example/test code. This is not a claim that the synthetic fixture or clock case breaks the tutorial. Green compiler/lint runs do not waive the broader changed-item rustdoc rule.

Smallest testable correction: add concise intent/workflow rustdoc to the three existing constants and document the clock invariant's possible panic on the two existing helpers. Preserve the functions, query text, fixture values and behavior. Complete this as one bounded documentation correction with any other retained task corrections; no abstraction, dependency, source relocation, new test, broad suite or separate style round is needed. Closure is inspection of these changed-item docs and the narrow existing formatting check for changed Rust source.

## Overall result

**FAIL**, solely for `REPO-001`. Coverage of the assigned standards surfaces is complete to the static and supplied-verification limits above. No source or test changes were made.
