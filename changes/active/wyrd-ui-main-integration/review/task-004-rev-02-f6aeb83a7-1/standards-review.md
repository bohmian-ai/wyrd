# Repository standards review

Overall result: **FAIL** — three bounded mandatory-gate omissions, below. This report reviews repository rules, not visual acceptance or task feature completeness.

## Subject and method

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `4a76331113864f9bddc43d3cc8df01080a696937`; candidate `f6aeb83a77179ac4450110671df6f564adc1914a`. Complete first-review range `git diff 4a7633111...f6aeb83a7`; 48 changed files. No prior acceptance carries forward. Source was not changed. CodeGraph was consulted before source navigation (`mise exec -- codegraph explore "ValidatePredicateRequest validate_predicate"`), then actual diff and neighboring owners/callers were inspected.

Inputs: root and applicable nested AGENTS; `changes/spec-driven-development.md`; `agent_docs/{index,rust,errors,agent-surfaces,maintainer-style}.md`; `TESTING.md`; approved revision 18 and original TASK-004-REV-02; architecture sections identified below. The task's execution table is a verification claim, not independently reproduced test output.

## Authority and changed-surface coverage

Paths below are repository-relative. UI shorthand means `crates/wyrd/wyrd-server/wyrd-ui/`.

| Changed surface (all changed files grouped) | Applicable authority | Source reviewed and rule result |
|---|---|---|
| `crates/wyrd-spec/src/vala/api.rs`; `examples/gen_schemas.rs` | Root AGENTS contract/dependency/generated-artifact rules; agent-surfaces; architecture/wyrd-design.md Client model; architecture/bifrost-design.md Public surface | Pure typed request/response and maximum byte constant, closed request deserialization; transient schema generator registration. PASS for tier purity, typed contract and runtime schemas. No new dependencies, async or IO in spec. |
| `crates/shared/wyrd-client/src/bifrost/{blocking,facade,query}.rs` | Root AGENTS; rust/errors; Client model and Bifrost public surface | Bifrost facade, its blocking projection, existing QueryClient transport, stable errors. PASS: no second SDK transport/authority, proper shared client use, public rustdoc and # Errors present. |
| `crates/wyrd/wyrd-server/src/query/{floor,routes,service}.rs` | Root AGENTS handler/docs/tenancy/audit rules; rust/errors/agent-surfaces; architecture/bifrost-design.md Query: Oracle, Read audit and terminal contract; security posture Permission check/Tenant and data isolation/Audit integrity | Reuses installed SQL parser and query authority/catalog. Caller derives tenant; no new PgPool, SQL migration, transaction, or persistent store. Typed HTTP contract and canonical error wrapper. FAIL: RS-02 and RS-03. Exact grant evaluation is also within independent security review scope; this report does not assert its runtime equivalence. |
| `crates/wyrd/wyrd-server/src/mcp/{bifrost,guide,mod}.rs`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/{connectivity,discovery,query}.rs` | Root AGENTS; rust/errors/agent-surfaces; Bifrost Public surface; security trust boundary | Read tool remains discoverable, closed args, delegates same server service, structured canonical errors; exact discovery counts updated. Server-backed test lives in existing pg_tests, covers grammar/denial/payload/tenant separation. PASS for projection/discovery/test-tier rules; no native runtime emulation. |
| `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; `python/wyrd/bifrost/_client.py`; `tests/integration/test_query_bifrost.py` | Root and Python AGENTS; Rust docs; Client model | Native method uses existing handle/block_on/error conversion; sync and async Python projections, typed inline public docs and public integration test. Existing `__init__.pyi` re-exports `_client` classes, so no duplicated public class stub is needed. FAIL: RS-01, missing native authored method declaration. |
| `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs` | Root AGENTS; rust; TESTING journey owner | Existing real deployment integration target; new test covers allowed/refused/scoped/other-tenant cases; new test rustdoc includes panic behavior. PASS source-level test-tier and error-code assertions. No claim of a new runtime rerun here. |
| `sdks/wyrd-sdk-ts/native/src/lib.rs`; `wyrd/{index.d.ts,index.d.cts,src/index.ts,tests/integration/query-bifrost.test.ts}` | Root and TS AGENTS; Client model; generated-artifact rules | Thin NativeBifrost call through shared client and existing result conversion; paired native declarations match added signature; wrapper preserves names/errors and public journey. PASS source parity. Generation provenance not independently rerun (would rebuild/write reviewed declarations); task records build/codegen claims. |
| UI `packages/ui/src/lib/{contract,node,mcp,schemas,observe}.ts`, `catalog.json`, `components/{RecordTable,StatStrip}.svelte` | Root AGENTS; wyrd-ui skill and SvelteKit architecture reference; spec REQ-030 shared host boundary; agent-surfaces | Predicate projected through shared typed executor Node/MCP; signal SQL remains in shared package; catalog source is schemas.ts (catalog diff inspected for corresponding projections). No backend credentials added to browser objects; existing components reused. PASS for placement/projection rules. Metric correctness belongs to behavior review; this row does not accept its semantics. |
| UI `src/lib/{observe-host,ui-pages.test,ui-pages.integration.test,ui-signals.integration,ui-library.integration.test}.ts`; tenant `+layout.svelte`; Observe and Query `+page.{svelte,server.ts}` | Root AGENTS; UI skill/references; browser-session security authority; TESTING | Existing authenticated BFF/remoteLoader/Node executor and actions used; state is URL-based; no browser-direct backend credential/transport. Server-side signal population and approved composition assessed by maintainer/behavior reviewers, not accepted merely because BFF transport exists. Signal fixture extraction keeps real OTLP assertions; page tests drive real production BFF/Chromium. PASS for no secret exposure, production/test separation, test location. |
| UI `package.json`, `pnpm-lock.yaml` | Root AGENTS dependency/platform rules; original task test-only Playwright authorization | Playwright is dev-only, not a shipped client dependency. No Cargo feature or tier dependency introduced. Lockfile includes broader re-resolution; no source-proven mandatory-rule defect identified from that alone. PASS scoped dependency rules. |
| `mise.toml`, `scripts/dev/wyrd-example-py.patch` | Root AGENTS commands/platforms; operations README readiness/drain boundaries; original task canonical patch delivery | Existing process owner changed to stop app before backend; patch is reviewed canonical counterpart delivery rather than generated artifact. No new lifecycle service, cgroup/proc/OS-gated runtime, durable migration or server behavior moved to SDK. System reviewer owns failure/drain correctness. PASS placement/platform/boundary rules at source level. |
| `.gitattributes` | Root testing no-gate-circumvention; Git patch format | `*.patch -whitespace` treats patch context whitespace as format. It does not disable language/type/unit/browser checks. No source-backed weakened semantic check found. PASS. |
| `docs/src/content/docs/reference/mcp-tools.md` | docs/AGENTS, CONTRIBUTING/PRODUCT; agent-surfaces; public Bifrost contract | Existing reference table gains exact tool name, scope, input/output and bound. PASS scoped reference conventions; no full docs build claimed. |
| Original task Execution Evidence section | changes/spec-driven-development.md; immutable-review rule | Records checks and limits; independent/human acceptance explicitly pending. PASS as recorded claims only; neither claims nor a numeric table override missing visual/behavior obligations. |

No changed production signature or field introduces `PgPool` or `OperatorPool`; catalog calls bind `caller.data_tenant_id`. No tenant SQL or migration write set requires the SQL directory AGENTS. The applicable nested rules are Python, TypeScript, and docs; no additional nested AGENTS were found in changed code owners.

## Proposed findings

### RS-01 — VIOLATION: new native Python method bypasses the authored stub contract

- Location: `sdks/wyrd-sdk-python/src/bifrost/mod.rs:979`; missing counterpart in `sdks/wyrd-sdk-python/python/wyrd/_wyrd/bifrost.pyi:64–107`.
- Rule: Python AGENTS “Public API: six layers” and “Stubs”: a Python-visible native change must update the authored native API declaration in the same change. Pure-Python definitions should be declared inline and re-exported rather than duplicated.
- Evidence: new native `validate_predicate(&self, py, namespace: &str, name: &str, expression: &str) -> WyrdPyResult<String>` is called by both public Python facades (`_client.py:1110`, `:1437`). Native `Bifrost` stub declares constructor/connect/alter_table, then `__getattr__ -> Any`, with no `validate_predicate` declaration. The diff contains no `.pyi` update.
- Consequence/materiality: the new boundary signature is unchecked and invisible in the authored declaration. Catch-all Any masks argument/return mismatch; passing `ty` and stub checker does not establish native parity. This is an explicit mandatory boundary gate, not a request to clean up older undeclared methods.
- Smallest correction: add only the exact new method declaration and its documented parameters/return to existing native Bifrost stub. Keep public classes defined once in `_client.py`; existing package re-exports need no new symbols. Do not broaden or remove the unrelated existing fallback as part of this fix.
- Focused closure: inspect native/stub signature parity, then `mise run py:lints` and `mise run py:typecheck`. Existing public Python predicate journey already covers runtime routing; no new harness or broad suite required.

### RS-02 — VIOLATION: new fallible parser helpers omit mandatory error rustdoc

- Locations: `crates/wyrd/wyrd-server/src/query/floor.rs:307` (`check_predicate`) and `:408` (`check_column`).
- Rule: root AGENTS requires `# Errors` on fallible changed Rust items; `agent_docs/rust.md` explicitly includes private helpers and states missing rustdoc blocks merge.
- Evidence: each new helper returns `Result<(), WyrdError>` and has only a single summary comment. `check_predicate` recursively rejects operators/constructs/lists/unknown columns; `check_column` rejects a name the table does not describe. Neither documents those fallible outcomes under `# Errors`.
- Consequence/materiality: mandatory merge gate is unmet in the new validation implementation. This finding is limited to these actual changed fallible items; it does not demand unrelated documentation/style changes or invent missing sections for infallible helpers.
- Smallest correction: complete these two existing doc blocks with their validation role, the rejection conditions and stable invalid-SQL error. Preserve parser behavior. Documentation-only correction requires static inspection and format check, not a newly invented test.
- Focused closure: inspect both doc blocks and `mise run fmt:check`. Do not create a style-only remediation round; batch with behavioral corrections.

### RS-03 — VIOLATION: new public handler lacks required scrubbed instrumentation

- Location: `crates/wyrd/wyrd-server/src/query/routes.rs:191` (`validate_predicate`).
- Rule: root AGENTS server handlers require `#[tracing::instrument]` with scrubbed args.
- Evidence: the mounted new POST handler has `utoipa::path` metadata and rustdoc but no tracing instrumentation. It accepts AppState, Caller and predicate-bearing JSON. Neighboring Bifrost route `crates/wyrd/wyrd-server/src/bifrost/routes.rs:219` demonstrates the established scrubbed instrumentation mechanism.
- Consequence/materiality: the new public query validation operation lacks the explicit handler span required for debugging/attribution. Blindly instrumenting default argument Debug would expose SQL/filter data and caller state, so the fix must also honor privacy.
- Smallest correction: use existing tracing instrumentation on this handler, skipping state, caller and body/predicate arguments; add only existing safe correlation fields if appropriate to the local pattern. No new tracing wrapper, service, telemetry schema or raw predicate logging.
- Focused closure: source-inspect skip list and compile/lint through the existing affected Rust lane when behavioral server changes already require it. No new test harness; preserve existing predicate journey assertions.

## Checks independently executed

| Exact command | Exit | Result / limitation |
|---|---|---|
| `mise run py:typecheck` | 0 | `All checks passed!`; native catch-all stub explains why this does not prove RS-01 closure. |
| `mise run py:lints` | 0 | Ruff and `scripts/check_stubs.py` passed; checker does not require an explicit declaration for this native method. |
| `git diff --check 4a7633111...f6aeb83a7` | 0 | No patch-format whitespace errors under current attributes. |

No Rust/Cargo build was launched, to avoid overlapping the orchestrator's verification processes. No full SDK journeys, UI browser run, image comparison, code regeneration, production replay or platform matrix was rerun by this specialist. Existing task verification is recorded evidence only. Source and mandatory omissions above are directly inspectable without manufacturing failing tests.
