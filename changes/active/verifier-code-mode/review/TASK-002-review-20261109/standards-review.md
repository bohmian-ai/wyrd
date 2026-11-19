# Repository standards review

Overall: **PASS**. No material repository-rule finding.

Subject: base `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e` to candidate
`ff78b51b61fea8f215a2139b17840044f65b17e4`, using `git diff base...candidate`.
This is the first standards review of the complete range. No prior findings or
coverage were carried forward. The approved specification is revision 2 at the
candidate; the original task is the base version of
`tasks/TASK-002-public-code-mode-workflow.md`. The unrelated working revision
of the specification was excluded. Other reviewer reports and implementation
completion summaries were not read.

## Authority coverage

The root `AGENTS.md`, `agent_docs/index.md`,
`agent_docs/maintainer-style.md`, `architecture/README.md`,
`changes/spec-driven-development.md`, and `TESTING.md` govern every row.
The root and nested rules and architecture references have no working-tree
differences from the candidate. There is no `.codegraph/` directory.

| Changed surface, all paths included | Additional governing authority inspected | Source and consumer coverage |
| --- | --- | --- |
| Python `python/wyrd/verify/__init__.pyi` and `tests/typecheck/verify_contracts.py` | Python SDK `AGENTS.md`, agent-surfaces guide; Wyrd design Verifier result contract and Agents/tools/workspaces | Authored `RequirementResult`, `ToolCallRecord`, `ToolRecipient`; public `verify/__init__.py`; unchanged native requirements/steps/typed getters in `src/verify.rs`; Rust source fields/defaults in `wyrd-spec/src/verification/execution.rs` |
| Python `tests/integration/conftest.py` and `test_verifier_building_blocks.py` | Python SDK rules; testing client-facing checklist; security posture accepted Verifier authority and authorization | New provider branches, shared program loading, typed result assertions, existing public setup, changed `requirement` and `retained` helpers and their callers, immediate/queued/status/retained observations |
| TypeScript `tests/integration/verifier-building-blocks.test.ts` and `tests/types/verification-data.test-d.ts` | TypeScript SDK `AGENTS.md`; testing client-facing checklist; agent-surfaces guide | Provider branches, shared program loading, public client setup, helpers and assertions; unchanged generated `src/card-types.ts` and generic `src/index.ts` result transformation |
| Rust SDK `tests/integration/verifier_building_blocks.rs` | Rust guide; testing fixture and journey ownership | Both changed program constants, their rustdoc, include paths, surrounding provider/journey callers, shared checked-in Card owner |
| MCP `tests/bifrost/mcp/verification.rs` | Rust and agent-surfaces guides; Wyrd design MCP/Verifier/tool rules; security posture accepted authority; Bifrost design verification tables | Entire added test/helper block, existing `pg_tests` imports and execute/enqueue/settle/readback helpers, real client/server setup, scoped principals, output schema and record assertions |
| Server `tests/integration/pg_openapi_contract.rs` | Rust and agent-surfaces guides; Wyrd design result contract | Added served-document test, shared schema resolution/document reader, optional field/default agreement with Rust source |
| `fixtures/cards/verifier_building_blocks/programs/{nested-investigation,reduce-notes}.py` | Root code/fixture rules; Wyrd design Agents/tools/workspaces; security accepted-authority rules; docs embedding rules | Entire programs and Rust/Python/TypeScript/MCP consumers; reference page embeds the executed nested program |
| Docs `reference/{agent-tools-and-authorization,verification-runtime-limits}.md` | `docs/AGENTS.md`, `CONTRIBUTING.md`, `PRODUCT.md`; Wyrd design Verifier/tool/worker contract; security posture accepted authority | Complete source page changes, live field/default owners, catalog codes, shared embedded source |
| Docs `operate/configure-verification-runtime.md` and `build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md` | Same docs authorities; operations deployment-and-release Rollout and version skew | Existing guide context and changed operator guidance/evidence table, startup limits, upgrade guidance, verification failure descriptions |
| Generated `docs/public/llms-full.txt` | Root generated-output rule; docs contribution generated-pages contract; agent-surfaces guide | Entire generated diff matches corresponding source-page changes and expands the checked-in program; inspected `docs/scripts/generate_llms_txt.py` and shared mdsvex projection/embedding owner |
| Task Implementation Evidence | Spec-driven development and original task verification discipline | Evidence section only as supplied verification; original task obligations read separately; no source authority inferred from completion claims |

No SQL implementation, capability-bearing production field/signature, new
handler, manifest, dependency, feature, native binding, error implementation,
or generated SDK declaration is changed. `TenantConn`/`OperatorPool`
placement, handler instrumentation, PyO3 lifetime work, migrations and
coordination-clock changes are therefore not newly applicable surfaces.
The added queries use public Bifrost/MCP clients over canonical verification
tables; they do not introduce a raw `PgPool` capability.

## Applicable rule results

These results concern repository standards, not task acceptance or an
alternative implementation design.

| Rule | Result | Evidence |
| --- | --- | --- |
| Root authority order, narrowest owning contract, Wyrd-only nouns and paths | PASS | Added fields/types use the existing Verifier result vocabulary; documentation agrees with the architecture's flat model/code record contract. No alias or parallel contract is introduced. |
| Durable behavior remains in server owners; SDKs project contracts | PASS | Production diff is authored Python declarations only. Python generic Rust serialization and TypeScript spread retain their existing ownership. No client runtime or permission engine appears. |
| Dependencies, supported platforms, feature and Python-boundary restrictions | PASS | No manifest/feature/dependency/platform or PyO3 implementation changes. Shared Python programs use the existing mediated tool bridge. |
| Rust owning structs versus caller-owned free work; sync unless awaiting IO | PASS | Added helpers are caller-owned test preparation/assertion work. `script_code` mounts provider mocks with awaited IO; journey and schema tests await the real server/client. No retained runtime owner or ad hoc runtime is introduced. |
| Rust error handling and invariants; no input/IO unwrap | PASS | Added fallible MCP helpers return `McpJourneyError`; parsing/transport propagate errors. The server test's `expect` messages identify test/schema invariants. No new public error mapping is hand-written. |
| Every changed Rust item has meaningful rustdoc, including helpers/tests; fallible functions document errors and assertions document panics | PASS | MCP constants and `completion`, `script_code`, `notes`, `by_id`, `records`, `assert_reduced`, journey test all have workflow-specific docs. Fallible helpers/test have `# Errors`; assertion-bearing helpers/test have `# Panics`. Server schema test documents historical compatibility and panics. Both changed Rust SDK constants retain detailed docs. |
| Rust imports/signatures follow module-level imports and bare imported type names | FAIL, non-blocking | MCP `verification.rs:2959` adds a function-local matcher import, and `completion`/`script_code` use qualified `wiremock` type names. This is a local placement deviation from `agent_docs/rust.md` Imports and signatures. It creates no material ambiguity, behavior failure, or mandatory-gate failure; the skill's placement/materiality threshold excludes it from remediation findings. |
| No plan/task/review identifiers in permanent source; no attribution footers | PASS | Changed fixtures/tests/docs describe permanent behavior. No task IDs or review references occur in their added code. Task packet evidence remains in the task packet. |
| Tests earn their integration location; Postgres-backed Rust tests use existing `pg_tests`/`pg_*` ownership | PASS | Added MCP journey is in `pg_tests`, served schema test is in `pg_openapi_contract.rs`; both boot actual server surfaces. Rust SDK change remains in the existing real-server story. |
| Journeys use real SDK/MCP → WyrdTestServer → SDK/MCP; negative flows and retained assertions, no unit substitute | PASS | Python/TypeScript tests call public observe/Execution APIs, MCP calls real tools; each observes records/results and denied nested reads. No replacement in-process runtime fake is introduced. |
| Test lifetime boundaries, deterministic mock providers, shared checked-in Cards/programs | PASS | Python-specific typed results are exercised in Python; TypeScript parser transformations in TypeScript. Providers are local/scripted; declarations remain the existing fixture Cards. Both program files are consumed by every story surface. |
| Python authored stubs, precise types, public imports/export parity | PASS | `ToolRecipient` is the closed literal union, new record/requirement fields mirror Rust output, custom-data generic remains. Native runtime exports and `__all__` are unchanged; added shapes are typing declarations like the existing TypedDict/alias family, not missing native classes. |
| Python tests are top-level functions and use public modules | PASS | New test is top-level under existing `pytestmark = integration`; imports use `wyrd.*`. `Hits` is a custom-data model, not a test class. |
| TypeScript closed unions, readonly wire types, native/generated source ownership | PASS | Compile-time fixture checks the generated literal union and optional record list; tests consume `CardTypes.ToolCallRecord`; no hand-maintained duplicate wire declaration or native declaration edit. |
| Stable public error codes; no invented client codes or message-based recovery | PASS | Shared program parses structured problem JSON and branches via `code`; docs/test denial code matches the existing catalog. No error producer changes. Record-string assertions observe the existing record error representation. |
| Authorization, tenancy, write scopes and safe record guidance stay server-owned | PASS | Added journeys obtain real scoped principals through existing fixture controls; Python programs cannot choose authority. Docs expressly require both invocation and capability grants and describe denied calls without protected output. No public tenant/credential parameter is added. |
| Canonical verification summary/output storage separation | PASS | Retained checks query `bifrost.verification.execution_outputs` for requirement/step detail; the nested query uses payload-free `bifrost.verification.results`. No new table, stored payload or raw intermediate record is added. |
| Generated artifacts derive from source; no committed JSON Schema | PASS | Only generated docs change, matching source pages/program. No schema artifact is added; served OpenAPI test examines the actual document. Supplied `codegen:check` and `docs:check` passed. |
| Docs correct page types, vocabulary, tested CodeFromFile source, neutral error/limit references, honest performance claims | PASS | Reference embeds the exact nested program executed by the journeys; source/LLM projection agree. Limits retain binary units/startup ownership. Operator guide states worker occupancy and upgrade requirement. No universal token/latency promise appears. |
| No bypassed gates or weakened existing assertions | PASS | Existing Python retained helper callers are updated to preserve their requirement equality assertions. New MCP ignore uses the established Postgres-backed journey lane and the supplied exact command runs ignored tests; it does not suppress required journey proof. Negative compile-time ignored diagnostic is the existing type-fixture convention, not a disabled behavioral test. |
| Narrow focused verification and language format/lint/type/contract/docs gates | PASS on supplied evidence | Candidate evidence records focused Rust SDK/MCP/OpenAPI/Python/TS journeys and format/lint/type/generated/docs checks passing. Orchestrator separately reports independent Python/TS typechecks and range diff whitespace check passing. No build was run by this specialist. |

## Findings and verification limits

Material finding ledger: **empty**. The local import placement deviation above
is explicitly non-blocking under the review skill; it is not a request for
optional cleanup or a new remediation round.

Verification results in the task are supplied execution evidence, not checks
independently repeated here. The orchestrator separately reported successful
`mise run py:typecheck`, `mise run ts:typecheck` (including its binding and
consumer dependencies), and range `git diff --check`. This specialist inspected
source, diff, declarations, tests, fixture consumers, generated projection,
and governing authorities. It did not run Cargo/build/journey/docs checks or
audit unchanged runtime fault matrices. Task acceptance and deeper security
behavior remain assigned to the separately selected reviewers.
