# Repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Reviewed range: `git diff 526355fb8..f1e1be815`
- Approved specification: `changes/active/bifrost-errors/spec.md`

The candidate remained at `f1e1be815069b23fa9920a61c1198524ecaad514`
through this review. Review artifacts created beside this report are outside the
immutable candidate.

## Authority coverage

| Changed surface | Applicable authority | Coverage and result |
|---|---|---|
| Oracle logical- and physical-planning error classification in `crates/vala/vala-bifrost-redux/src/oracle/mod.rs` | `AGENTS.md` §§2–6, 9–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md` Query/Errors; `references/languages/errors.md`; `references/domain/{vala-architecture,olap-serving,datafusion}.md` | **PASS.** The change remains in Vala's Oracle owner, maps dependency failures into the existing derive-backed `BifrostError::QueryInvalidSql`, exposes planner detail only from the authorized logical-planning session, retains a fixed detail for physical planning, and leaves non-caller and resource failures on the existing 5xx/resource mapping (`oracle/mod.rs:2744-2817`, `3000-3034`, `3735-3814`). No parallel public error catalog or language-specific mapping was added. |
| Live-tail filter pushdown in `crates/vala/vala-bifrost-redux/src/oracle/live.rs` | `AGENTS.md` §§3–6, 9–12, 16; `architecture/bifrost-design.md` System boundary and Query: Oracle; `references/domain/datafusion.md` §§DataFusion is execution, Oracle execution paths, Rejected shapes; `references/domain/analytical-operations-reliability.md` | **PASS.** `LiveScribeExec` already owns the live-provider predicate and truthful pushdown decision. The added synchronous expression walk keeps a `ScalarSubqueryExpr` beside the leader-owned `ScalarSubqueryExec`, while `all_unsupported` preserves the leader `FilterExec` (`oracle/live.rs:383-421`, `509-523`). The pinned DataFusion fork documents the result container as runtime state supplied by the surrounding `ScalarSubqueryExec`; a detached live-fragment predicate cannot reconstruct it. This is therefore a Wyrd live-fragment boundary rule, not a replacement DataFusion planner or custom SQL implementation. Other static filters retain the existing remote pruning path. |
| Rust unit coverage in Oracle | `AGENTS.md` §§11–12, 16; `agent-rules.md` test placement and rustdoc rules; `references/languages/{rust-core,testing-workflows}.md` | **PASS.** The pure mapper coverage stays inline with its owner and covers context, diagnostic, collection, shared, internal, and resource variants (`oracle/mod.rs:4466-4569`). New and materially modified Rust items have intent rustdoc; fallible items document errors and test functions document panics where applicable. |
| Shared Bifrost test harness and support workload under `crates/wyrd/wyrd-testing` | `AGENTS.md` §§3–6, 10–12, 16; `agent-rules.md` test/runtime/dependency rules; `references/languages/testing-workflows.md`; `references/domain/datafusion.md` | **PASS.** The correlation addition reuses the existing `BifrostWriter` and typed `RunId`; all callers of the changed `enqueue` signature are updated (`bifrost/write.rs:83-118`, `154-159`; `query_fixture.rs:136-235`). The deterministic workload is test-harness logic, has rustdoc on every added item, uses the workspace-pinned DataFusion dependency already in the harness cone, and compares identical Arrow rows without introducing production behavior (`support_workload.rs`). The dependency remains in non-shipping `wyrd-testing`; no production or client-tier dependency boundary changed. |
| Rust SDK journeys | `AGENTS.md` §§2–3, 8–12, 16; `references/doctrine/architecture-constraints.md` Client-Tier Constraints and Verification Contract; `references/architecture/patterns.md` Client and Verification patterns; `references/languages/testing-workflows.md` | **PASS.** Journeys use the public `wyrd_ai::Bifrost` facade through a real `WyrdTestServer`, cover live scalar/`IN`/`EXISTS` and managed-`run_id` correlation plus planner-detail and unauthorized-table negative paths, and remain in the ignored Postgres-owned journey lane (`query_bifrost.rs:230-454`). The wider analytical correctness journey covers published, live, and mixed states through the same public SDK (`analytical_correctness.rs`). No SDK transport or durable semantics were duplicated. |
| Python SDK journey | `AGENTS.md` §§8, 11–12; `references/languages/{python-api-and-stubs,testing-workflows,errors}.md` | **PASS.** The change is test-only, imports public `wyrd` modules, uses the real Python SDK and `WyrdTestServer`, and covers live subqueries, planner details, and authorization non-disclosure (`test_query_bifrost.py:1-191`). It adds no PyO3 code, export, stub, or generated artifact. |
| TypeScript SDK journey | `AGENTS.md` §§2–3, 11–12; `references/languages/{typescript-guide,testing-workflows,errors}.md` | **PASS.** The journey uses public `wyrd-ai`, the existing server fixture, typed Zod results, stable structured error fields, and bounded fixed-size test cases (`query-bifrost.test.ts:1-205`). It adds no N-API or declaration change and no language-specific error semantics. |
| MCP journey | `AGENTS.md` §§2, 9, 11–12, 16; `references/languages/{agent-harness,testing-workflows,errors}.md`; `references/architecture/patterns.md` Agent Surface Pattern | **PASS.** The MCP test drives the mounted tool through its real client/server path, asserts the stable structured error code/detail, and proves live subquery behavior without changing the catalog or bypassing the shared Bifrost facade (`wyrd-mcp/tests/bifrost/mcp/query.rs:314-503`). |
| Manifests, generated artifacts, and verification wiring | `AGENTS.md` §§1, 4, 8, 11–12; `agent-rules.md`; `mise.toml`; workspace `Cargo.toml` and affected crate manifests | **PASS with verification limits below.** No feature, lockfile, profile, generated schema, stub, or declaration changed. The only manifest move makes the already-pinned DataFusion dependency available to the `wyrd-testing` library support workload. Existing `test:bifrost` and capability journey tasks already select every changed test target; no new task was required. |

## Rule results

| Rule | Evidence | Result |
|---|---|---|
| Vala owns Bifrost query-engine behavior; server remains the only serving surface. | Production edits are confined to `vala-bifrost-redux::oracle`; HTTP/SDK/MCP files are tests only. | PASS |
| Use the existing DataFusion plan once; do not add a classifier, second build, fallback planner, or custom execution. | `build_retained_plan` still calls the one pinned `create_physical_plan` and retains that root (`oracle/mod.rs:2749-2817`). The live change only governs which existing parent predicates the custom live fragment may evaluate. | PASS |
| Provider pushdown claims and fragment predicates must be truthful. | The live leaf continues to return `all_unsupported`, leaving the authoritative filter on the leader, and excludes the runtime-bound scalar expression only from the fragment predicate (`oracle/live.rs:396-421`). | PASS |
| Public errors use the stable derive-backed Wyrd catalog and do not expose internal failures. | Existing `QueryInvalidSql` is reused. Logical SQL/plan/schema/unsupported leaves expose `strip_backtrace`; physical leaves get a fixed repair action; resources and internal/execution leaves retain existing mappings (`oracle/mod.rs:3735-3814`). Authorization precedes provider registration and logical planning. | PASS |
| Preserve tenant isolation and authorization non-disclosure. | No SQL, pool, tenant identity, or permission path changed. The unauthorized-table journeys on Rust, Python, and TypeScript assert `WYRD_VALA_403_QUERY_FORBIDDEN` without column disclosure. `mise run check:tenant-isolation` passed. | PASS |
| Keep synchronous computation synchronous and use cohesive existing owners. | Both added production helpers are synchronous pure classification functions; stateful behavior remains on `Oracle` and `LiveScribeExec`. No new trait, feature, owner, or runtime was introduced. | PASS |
| All new/materially modified Rust items, including tests, have complete rustdoc. | Inspected every changed Rust item in the cumulative diff, including private helpers, constants, harness values, test fixtures, and tests. Fallible functions include `# Errors`; tests that can panic include `# Panics`. | PASS |
| Every shipped surface has a real user/agent journey; runtime-specific behavior is tested in its runtime. | Rust, Python, TypeScript, MCP, and peer/live journeys are present in their established targets and use their actual runtimes. | PASS |
| Generated artifacts are source-derived and never hand-edited. | No `.pyi`, schema JSON, generated TypeScript declaration, OpenAPI snapshot, or error-code artifact changed. | PASS |
| Client tiers do not acquire analytical-engine dependencies or duplicate behavior. | DataFusion was added only to `wyrd-testing`; production SDK manifests and implementations are unchanged. | PASS |
| No gate was weakened or bypassed. | Added `#[ignore]` attributes are the repository's sanctioned marker for Postgres-backed journey tests and the owning `mise` tasks run them with `--run-ignored=all`; no lint suppression, deleted assertion, relaxed check, or boundary allowlist was added. | PASS |

## Material repository-rule findings

None.

## Verification evidence and limits

Observed during this review:

- `git diff --check 526355fb8..f1e1be815` passed.
- `cargo fmt --all -- --check` passed.
- `mise run check:tenant-isolation` passed.
- `mise run py:format:check` passed.
- `mise run py:lints` passed.
- `mise run ts:format:check` passed.
- `mise run ts:lints` passed.

No completed result was available here for workspace Clippy, `check:deps`,
`codegen:check`, TypeScript type-checking, or the Bifrost unit/integration and
Rust/Python/TypeScript/MCP journey lanes. The candidate changes no generated
contract, but `AGENTS.md` still requires the owning aggregate
`mise run verify:bifrost` (which composes formatting/checks, dependency and
tenant checks, and every Bifrost tier/language surface) to be green before the
change itself is complete. This source standards audit does not substitute for
those runtime results.

## Overall result

**PASS**

The cumulative candidate conforms to the applicable repository authorities.
No material repository-rule defect was found. The remaining risk is execution
proof: the unfinished or unavailable lanes above must still pass at change
closeout.
