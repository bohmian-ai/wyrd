# TASK-001 repository standards review

**Result: FAIL** — one consolidated rustdoc finding (`REPO-001`).

## Immutable subject and scope

Reviewed `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`, base `ebd75dc224854413d17cb06b70da8181b78cde6b`, candidate `32fbc264adccaf1160894769d5501fbe441259a5`. HEAD still equals the candidate; the working tree has only the authorized untracked review directory. Read the actual complete cumulative diff, including rename detection, all 60 changed paths, generated text and lockfile, and the specification/task/evidence references recorded in `subject.md`. No other discovery report was read.

This is a repository-rule audit, not task acceptance or a preference review. CodeGraph was used before source navigation; subsequent reads expanded the changed owners, callers and declarations. Mechanically renamed items, comment-only replacements and rustfmt-only wrapping were not treated as newly introduced behavioral items.

## Authority coverage

| Changed surface | Governing authority read | Source/consumer coverage |
| --- | --- | --- |
| Root changelog, architecture and change evidence | Root `AGENTS.md`; `architecture/README.md`; `changes/spec-driven-development.md` authority, reuse, TDD, review and verification sections; approved guided-setup revision 5 and original task/evidence | Rename disclosure; amended architecture client adapter and deployment/security command ownership; appended evidence and diagnoses |
| CLI exposure, bootstrap move and saved configuration | Root rules; `agent_docs/index.md`, `rust.md`, `errors.md`, `maintainer-style.md`, `agent-surfaces.md`; deployment-and-release “Local server bundles”; security posture “OAuth authorization server” local credentials paragraph | `cli.rs`, `lib.rs`, `init.rs` renamed owner and materially changed run/readiness paths, `gateway.rs`, server install/mod visibility, `global_config.rs`; installed Python and TS CLI consumers |
| MCP prompt contract and forwarding | Same root/topic guides; Wyrd design “Client model”; agent-first/read/write boundaries | `wyrd-spec/src/mcp.rs`, setup-prompt Markdown, server `mcp/guide.rs` and `mcp/mod.rs`, CLI `mcp/{hosts,mod,proxy}.rs`; authenticated proxy journeys, MCP discovery regression |
| Card Spec schema | Root contract ownership and rustdoc rules; Wyrd design client/wire model; agent-surfaces runtime-schema/generated-artifact rules | `envelope.rs` `JsonSchema for Spec`, typed variant list, MCP `cards.get` consumer and separate existing `ToSchema` sibling projection |
| Rust tests and declarations | Root testing/rustdoc rules; `agent_docs/rust.md`; `TESTING.md` tiers, layout, named selectors and client-facing tests | CLI `tests/cli.rs` new helper and test; `pg_tests.rs` new prompt journey and renamed regressions; MCP discovery and server identity UI comment-only changes; new `wyrd-spec::mcp::tests` declaration/test |
| Python public adapter and typing | Python SDK `AGENTS.md` (entire); root code/testing rules; maintainer types/docs guidance; Wyrd design “Client model”, runtime telemetry lifecycle, “Observation identity”; Bifrost “System boundary” | `integrations` package, ASGI middleware and public `.pyi` re-exports; existing `src/state/mod.rs` start/flush/shutdown and `src/observe/mod.rs` enter/exit owners; no new PyO3/native API |
| Python OTEL correlation | Same Python and architecture authorities; optional execution-local correlation invariant | `otel.py` scope mirror and processor fallback, existing Run enter/exit callers; ASGI and existing observation tests |
| Python tests/dev dependencies | Python rules; root testing; `TESTING.md`; task worker-proof boundary | Full ASGI test and worker-app diffs; local development prompt/manual helpers; readiness/README/quickstart changes; pyproject and complete lockfile delta |
| TypeScript CLI and example | TS SDK `AGENTS.md` (entire); root cross-language rules; maintainer type guidance | `wyrd/src/cli.ts` signal delegation predicate and `examples/support-desk-app/typescript/server.ts` command comment; no N-API/declaration change |
| Documentation content and examples | `docs/AGENTS.md` (entire, including new Integration shape); `docs/CONTRIBUTING.md`; `docs/PRODUCT.md`; root contract/generated rules | All changed Connect/Get-started/Operate pages, FastAPI example, examples index and gateway README; language tabs, CLI rename, install pin, public imports, `CodeFromFile` source and journey |
| Docs build and generated text | Same docs authorities and agent-surfaces generated rules | `highlight.js`, landing `+page.server.ts`, shell-links fixture, command checker, full llms diff, generator functions; read-only regeneration comparison |
| Tooling and lanes | Root command/testing rules; `TESTING.md` scoped verification and named tests; task evidence | Entire mise delta, local-development selectors and Python observation lane, pyproject dev-only worker/instrumentation dependencies and lock closure |

Nested-rule inventory found root, docs, Python SDK, TS SDK, and SQL AGENTS. All touched nested owners (docs/Python/TS) were read. No SQL production file changed, so SQL directory rules do not govern a new write here. Production signatures/fields in changed owners contain no added `PgPool`, `OperatorPool`, `TenantConn`, SQL executor or direct database access; bootstrap remains an installed-server/client composition. This conclusion comes from signatures and fields, not a green tenant-isolation check.

## Rule results

| Applicable rule | Result and source evidence |
| --- | --- |
| Narrowest owner controls contracts; no legacy aliases | PASS. `Command::Init` replaces `ServerCommand::Dev`; no compatibility arm is retained. Deployment/security owner prose and consumers use the new spelling. `[dev]` persisted settings remain the same durable format rather than adding a parallel config contract. |
| Durable server behavior and thin first-class client boundaries | PASS. ASGI calls the existing state's methods, opens the existing Run, and never declares/registers or owns a transport, writer, provider or credential. Existing state methods retain writer and telemetry flushing. |
| Reuse existing workflow owner | PASS within this standards scope. CLI moves the existing bootstrap owner; ASGI adapts the foreign-runtime lifespan around existing lifecycle operations. Shared `setup_prompt` feeds CLI/readiness/MCP and the docs embed the same body; proxy forwards instead of implementing a second prompt service. |
| Spec dependency purity and dependency placement | PASS. New prompt code is string formatting/include only. No Cargo dependency/feature additions or server-tier imports enter spec/client/SDK tiers. Gunicorn and ASGI instrumentation are dev dependencies, not production adapter dependencies. |
| Struct-centered Rust and synchronous pure transformations | PASS. `InitArgs`, existing `DevServer`, `McpHost`, `McpInstallReport` and `McpProxy` retain ownership. Pure prompt formatting/host quoting are synchronous. Server prompt hooks return ready futures for protocol requirements. |
| Fallible APIs and stable errors | PASS for added production surfaces. CLI uses existing coded errors, Python Wyrd failures retain their exception/code, and MCP invalid prompt requests use protocol invalid-parameters responses. No new hand-coded Wyrd error catalog or public transport/database leak was introduced. |
| Portable production behavior and secret handling | PASS by source inspection. New adapter uses asyncio/thread offload and no OS containment or platform-gated behavior. Prompt/handoff carry release text, not saved credentials. Host prompt arguments are single-quoted with apostrophe escaping. Existing protected credential persistence remains at the bootstrap owner. Runtime proof covers Linux x86_64 only. |
| Server auth, tenancy and authorization boundaries | PASS by source inspection. Prompt listing/getting extends the existing authenticated MCP handler and is static, tenant-independent text. No new permission, principal override, tenant input, SQL path or write tool is introduced. Existing dispatch/audit machinery is unchanged. |
| Typed public parameters/returns and authored declaration parity | PASS. Public middleware constructor/call signatures are typed; `Any` is confined to open ASGI message/scope values. Pure Python class is authored inline and re-exported by its stub; `.py`/`.pyi` `__all__` match. `mise run py:lints` independently passes, including stub checker. TS change does not add an API needing generated declarations. |
| No hidden constructor IO; no new global mutable client state | PASS. Middleware constructor retains arguments. `WyrdState.from_path` stays an offline loader. Correlation mirror is execution-local `ContextVar`, not a global request registry. |
| New/materially changed Rust items require rustdoc, including private/test items and panic contracts | FAIL: `REPO-001` below. Other added API methods, fields, enum variant and behavioral tests have item documentation; renamed regression tests already retain their documentation. |
| Tests use appropriate runtime and integration tier | PASS within task-stage scope. ASGI lifetime/concurrency runs in Python and worker proof drives real workers. CLI tests drive the binary/proxy/server; Postgres-backed Rust additions remain in `pg_tests` and the explicit journey lane. No added standalone Rust test binary, weakened assertion or gate suppression. The new ignored lifecycle journey is explicitly selected with `--run-ignored=all`; this is the existing heavy-journey convention, not exclusion from its gate. |
| Tests use public Wyrd surfaces and typed refusals | PASS for material added behavior. ASGI tests use public `wyrd.*`, real state/client/Bifrost and coded Wyrd refusals. Raw HTTP/subprocesses here drive ASGI and installed-host/worker boundaries that those tests specifically prove; they do not implement a parallel Wyrd API. The OTel reset is external test isolation, not a private Wyrd call. |
| Generated artifacts from source, runtime JSON schemas uncommitted | PASS. No committed JSON schema/declaration edit. llms sources render byte-identically to both committed generated files in a read-only comparison. Codegen evidence is recorded in the task; not re-run here because it owns Cargo. |
| Docs use established components, pinned Wyrd version and tested source examples | PASS. Setup uses existing Tabs/Aside/CodeFromFile; integration imports the journey-executed FastAPI app; new install pins Wyrd 0.3.0 and uses uv. No invented product claims or new styling/navigation machinery. `docs:check:commands` independently passes. |
| Documentation page shape and frontmatter | PASS for material usability obligations. New integration follows the explicitly added integration shape and both new pages declare valid section/order/status. “Choose your path” is a navigation page, not an unexecuted deployment procedure. No blocking wording-only issue recorded. |
| Build loader source ownership | PASS. Landing imports existing Quickstart files through Vite raw glob rather than copying code; fixture exposes examples to the existing build. No runtime filesystem dependency is added to published page. |
| Scoped verification and immutable review | PASS. Evidence distinguishes TASK-002/combined checks from task proof and records Linux-only runtime results. This audit did not run Cargo, formatters or source-writing generation, and did not modify the candidate. |

## Material finding

### REPO-001 — New/materially changed Rust items omit required item/panic rustdoc

**Classification:** VIOLATION. **Blocking:** yes, under the explicit rustdoc exception in root `AGENTS.md` and the task-review skill.

**Rule:** Root `AGENTS.md`: “Every new or changed Rust item has rustdoc … `# Panics` when it can panic. Missing rustdoc blocks merge.” `agent_docs/rust.md` explicitly includes private helpers, test helpers and tests regardless of visibility.

**Exact materially changed sites:**

- `crates/wyrd-spec/src/envelope.rs:333`, `Spec::json_schema`: the body changes the public JSON Schema combinator from `oneOf` to `anyOf`, but the method has no rustdoc. The new explanation at lines 324–327 is attached to the `impl`, not the changed method. Describe the method's variant-union/schema-consumer contract on the method itself.
- `crates/wyrd-spec/src/mcp.rs:81`, new `mod tests`: the newly added module declaration has no rustdoc. The new test function beneath it is documented, which does not document the module item.
- `crates/wyrd/wyrd-cli/tests/cli.rs:111`, new `walk` helper: its one-line rustdoc lacks `# Panics`, despite `read_dir`, directory-entry and file-read `expect` calls at lines 113, 114 and 119. It should state that unreadable directories/entries/files panic in the offline-purity fixture. The description also calls paths relative although the implementation preserves full paths from the supplied root; correct that same item's contract while documenting it.

**Consequence:** The candidate fails the repository's explicit changed-item documentation gate. A maintainer cannot read the method/helper's own docs to learn the changed schema semantics or fixture failure contract. This is not a request to add docs to unrelated existing trait implementations or mechanically moved code.

**Smallest testable correction:** Add accurate item rustdoc at these three sites, including `# Panics` on `walk`; retain the existing schema and fixture behavior. No new helper, test framework or source-wide documentation sweep is needed. Closure is a source inspection of these items plus the existing narrow format/lint validation for the edited Rust owners.

## Verification and limits

Independently passed: `mise exec -- git diff --check <base>...<candidate>`, `mise run py:lints` (Ruff and authored-stub validation), `mise run docs:check:commands`, and a read-only call to `render_llms_txt`/`render_llms_full` comparing output with both committed generated artifacts. Initial plain `python` invocation was unavailable; the same read-only comparison passed with `mise exec -- python3`.

Read recorded C1–C8 and existing observation-unit/static check claims as evidence claims, and verified their source selectors and lane wiring. Did not rerun Cargo, live server journeys, worker processes, broad docs/SDK checks or other platform builds. Task evidence itself limits runtime coverage to Linux x86_64 with two Gunicorn workers; macOS arm64/Linux arm64 are unexercised. No task acceptance verdict is implied by this standards report.

**Overall: FAIL (`REPO-001`).**
