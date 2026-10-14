# Bifrost / distributed-query domain review

## Subject

- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Authority: `changes/active/example-journey-fixes/spec.md` revision 4, `owner-decisions.md` OD-6 and OD-7, `TASK-002-bifrost-errors-and-subqueries.md`, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`, and `architecture/wyrd-security-posture.md`
- Candidate identity remained `c4ba2b0502f8968a78e5e960bcc9eba43e960643` throughout this pass. The reviewed Bifrost files remained at that commit; an unrelated generated Python file was modified in the shared review worktree by another concurrent review.

## Boundary and source coverage

| Boundary | Source traced | Assessment |
|---|---|---|
| Public authorization before planning | `oracle/planner.rs:99-153`, `oracle/mod.rs:3405-3445`, `wyrd-server/src/query/service.rs:185-221` | The complete catalog-resolved table set is authorized before provider registration and logical planning. An ungranted table returns `QueryForbidden`, so its schema is never installed in the DataFusion session. |
| SQL/logical planning error classification | `oracle/mod.rs:3000-3034`, `oracle/mod.rs:3735-3813` | SQL and logical planning unwrap `Context`, `Diagnostic`, `Shared`, and the first `Collection` member; SQL/plan/schema/not-implemented caller faults become `QueryInvalidSql` with `strip_backtrace()` detail. Resource and unclassified failures retain opaque stable mappings. |
| Physical planning and internal opacity | `oracle/mod.rs:3784-3798`, `oracle/mod.rs:3800-3813` | Physical-plan caller refusals retain a fixed public repair message; resource exhaustion and internal failures remain stable 5xx errors without dependency text. |
| HTTP, SDK, and MCP projection | `wyrd-server/src/query/routes.rs:241-290`, `wyrd-server/src/query/service.rs:200-221`, `wyrd-server/src/query/collect.rs:361-405`, `wyrd-client/src/bifrost/query.rs:205-235` | The same typed pre-stream `WyrdError` crosses HTTP and all SDKs; MCP uses the server's bounded collector and preserves the canonical problem. No second error classifier was introduced. |
| Live filter pushdown and fragment serialization | `oracle/live.rs:390-423`, `oracle/live.rs:509-523`, `oracle/live.rs:278-336` | `ScalarSubqueryExpr`-reading predicates are retained by the leader `FilterExec` and omitted from the signed Scribe fragment. Static non-subquery filters still ship for pruning, and every filter remains unsupported at the leaf so the leader evaluates it authoritatively. |
| Local/remote Scribe execution and lifecycle | `oracle/live.rs:462-505`, `oracle/live.rs:700-959`, `architecture/bifrost-design.md:330-356`, `architecture/bifrost-design.md:525-537` | The change does not alter dispatch identity, snapshot ownership, backpressure, cancellation, footer validation, or degraded/failure rules. The same fragment construction serves local and remote Scribe routes. |
| Representative analytical workload | `wyrd-testing/src/bifrost/support_workload.rs:169-309`, `:389-502`, `:587-685` | Deterministic three-table data contains nulls, duplicate run keys, tied values, absent children, and orphan rows. Ten join/subquery/window/aggregate queries compare to an independent in-memory DataFusion session; two answers are also iterator-counted and literal-pinned. |
| Published/live/mixed state proof | `wyrd-sdk-rust/tests/integration/analytical_correctness.rs:17-132` | All ten queries run through the public Rust SDK in published, live, and split mixed states and compare complete rendered results. |
| Remote-Scribe proof | `wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:13-106` | A two-Oracle/one-Scribe process topology writes every row only to the remote Scribe, compares all ten results, and asserts that Scribe fragment execution increased. |
| Tenant isolation and sensitive-column rule | `oracle/planner.rs:117-139`, `oracle/mod.rs:3412-3445`, `oracle/mod.rs:3448-3518`, `architecture/bifrost-design.md:1390-1420` | Table authorization precedes schema installation and data IO. Gateway payload values retain their separate post-plan permission gate; their column metadata is intentionally descriptive under Bifrost authority, so planner suggestions do not expand value access. |

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002: actionable 4xx planning errors across single-table, join, and subquery forms | `oracle/mod.rs:3000-3034`, `oracle/mod.rs:3739-3798` | Rust/Python/TypeScript unknown-column journeys; MCP unknown table/field/function journey; focused mapper unit | **FAIL**: the specified type-mismatch and ambiguous-reference paths have no real SQL journey on any surface (BIFROST-1). |
| REQ-002: internal failures stay opaque | `oracle/mod.rs:3784-3813` | `oracle::tests::datafusion_query_rejections_are_safe_and_actionable` covers internal, collected execution, and exhaustion errors | PASS |
| REQ-002 / INV-001 / AC-002: an unauthorized table reveals no columns | `oracle/planner.rs:117-139`, `oracle/mod.rs:3412-3445` | Rust `query_bifrost.rs:334-387`; Python `test_query_bifrost.py:134-153`; TypeScript `query-bifrost.test.ts:154-173` | PASS |
| REQ-006: scalar, `IN`, and correlated `EXISTS` live subqueries match joins | `oracle/live.rs:390-423`, `oracle/live.rs:509-523` | Rust, Python, TypeScript, and MCP live-table journeys | PASS for ordinary user columns |
| REQ-006: correlated predicates on managed `run_id` | The filter fix is type/name agnostic, but no managed-`run_id` source path is exercised | Existing small journeys correlate `id`; the analytical workload deliberately substitutes `run_key` (`support_workload.rs:22-23`) | **FAIL** (BIFROST-2) |
| OD-6: representative null/tie/duplicate/orphan workload, plain DataFusion oracle, some hand answers | `support_workload.rs:169-309`, `:389-502`, `:587-685` | `oracle_agrees_with_hand_counted_answers`; `every_query_answers_with_rows` | PASS |
| OD-6: published, live, mixed, and remote-Scribe states | `analytical_correctness.rs:17-132`; `peer_network/support_workload.rs:13-106` | Recorded TASK-002 lanes cover all four states; live and mixed fail on the base, all states pass on the candidate | PASS |
| OD-7: root-cause correction without an added query-shape mechanism | `oracle/live.rs:390-423`, `oracle/mod.rs:3735-3798` | Diff adds one expression-property check and reuses the existing leader filter and stable error catalog | PASS |
| Durability/concurrency/distributed lifecycle remains unchanged | Only filter membership in the Scribe fragment and pre-stream error mapping changed | Existing analytical and peer-network lanes recorded green; no admission, roster, grant, exchange, terminal, or publication owner changed | PASS |

## Material findings

### BIFROST-1 — MISSING: required type-mismatch and ambiguous-reference SQL journeys

- **Violated obligation:** Spec REQ-002 requires caller planning failures for type mismatches and ambiguous references to return actionable 4xx errors consistently over HTTP, SDK, and MCP; AC-001 requires user-journey proof for the shipped surfaces.
- **Exact evidence:** The Rust SDK journey enumerates only misspelled-column single/join/subquery SQL at `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:303-330`. Python does the same at `sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py:115-131`, and TypeScript at `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts:139-152`. MCP adds unknown table and unknown function, but no type mismatch or ambiguous reference, at `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:299-376`. The mapper unit synthesizes variants rather than planning the missing SQL shapes at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4480-4569`.
- **Observable consequence:** DataFusion chooses the wrapper and leaf error variants. The candidate can pass every recorded check while an actual ambiguous reference or type mismatch falls outside `caller_planning_fault`, becomes a generic 5xx, or loses its actionable detail—the precise wrapper regression this task exists to prevent.
- **Required testable correction:** Extend the existing query journeys, without a new harness, with real ambiguous-reference and planning-time type-mismatch SQL. Assert the canonical 4xx code and actionable planner detail through Rust, Python, TypeScript, and MCP/HTTP. If either real query reveals a mapper gap, correct the shared `caller_planning_fault` source once and keep physical/internal errors opaque.

### BIFROST-2 — MISSING: REQ-006 never exercises the managed `run_id` correlation path

- **Violated obligation:** Spec REQ-006 explicitly requires live-table subqueries to work “including correlated predicates on `run_id`,” with tenant isolation preserved; AC-001 requires a user journey.
- **Exact evidence:** The Rust live journeys correlate only fixture `id` at `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:242-292`; Python uses `id` at `sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py:82-112`; TypeScript uses `id` at `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts:114-136`; MCP uses `id` at `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:383-456`. The expanded workload expressly renames its key to `run_key` because `run_id` is server-managed (`crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:22-23`) and all correlated queries use that substitute (`:404-499`). A repository-wide search found no subquery predicate on `run_id`.
- **Observable consequence:** The journeys bypass managed-column stamping, schema projection, and live-fragment handling for the exact correlation key users employ across Wyrd evidence tables. The generic expression fix is plausible but the mandated managed-column behavior remains unproved and can regress while all current lanes stay green.
- **Required testable correction:** Reuse the existing live query fixtures and public run/observation attribution mechanisms to create rows sharing and differing in real managed `run_id` values. Add a correlated scalar/`EXISTS` or `IN` subquery on `run_id`, compare it with the equivalent join, and carry that case through the Rust, Python, TypeScript, and MCP journeys. Do not add a second query engine or test-only SQL path.

## Verification performed and limits

- `git diff --check` over the Bifrost implementation and journey write set: PASS.
- `MISE_STATE_DIR=/tmp/mise-domain-bifrost mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'`: PASS (1 test).
- `MISE_STATE_DIR=/tmp/mise-domain-bifrost mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^bifrost::support_workload::tests::/)'`: PASS (2 tests).
- I did not rerun Postgres-backed published/live/mixed/remote or language journeys. TASK-002 records those focused commands green, including `mise run test:bifrost`; this review inspected their complete source and treats the missing cases above as gaps in that evidence, not as failures of the recorded cases.
- TASK-002 records that remote-live already passed on the base because its physical plan did not push a scalar-subquery expression into the Scribe fragment. That limits its value as a regression reproduction, but it still satisfies OD-6's remote-Scribe state coverage when combined with the base-failing local live and mixed journeys.

## Overall result

**FAIL**

The shared error and live-fragment corrections are narrow and consistent with Bifrost's distributed lifecycle, and OD-6 is otherwise met. Acceptance remains incomplete because two explicit user-visible cases—real type-mismatch/ambiguous planning errors and managed-`run_id` correlated live subqueries—are absent from the required journey proof.
