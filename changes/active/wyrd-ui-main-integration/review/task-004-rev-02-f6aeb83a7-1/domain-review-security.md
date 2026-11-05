# Security boundary review

Result: FAIL (one explicit permission-boundary violation; no demonstrated privilege escalation).

## Subject and scope

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `4a76331113864f9bddc43d3cc8df01080a696937`; candidate `f6aeb83a77179ac4450110671df6f564adc1914a`. First review of TASK-004-REV-02; complete cumulative range available and examined for the predicate boundary. Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md`; approved spec revision 18. Shared starting map: `subject.md`. No prior acceptance carried forward. HEAD remained the recorded candidate. Source unchanged; only this report written.

This specialist covers predicate grammar/normalization, authorization before column disclosure, tenant-bound catalog resolution, gateway payload permission, SDK projections, and BFF/MCP consumers. Visual fidelity, full lifecycle, general repository-rule coverage and metric arithmetic are assigned elsewhere.

## Authority and source coverage

| Boundary | Authority | Inspected source |
|---|---|---|
| Grammar and wire contract | spec REQ-040 / AC-023; task Safe predicates | `wyrd-spec/src/vala/api.rs::ValidatePredicateRequest/Response/MAX_PREDICATE_BYTES`; `wyrd-server/src/query/floor.rs::normalize_predicate/check_predicate/check_column/is_literal`; HTTP routes and MCP tool descriptor/dispatch |
| Scoped permissions and disclosure | security posture Permission model / Permission check, lines 510–592; Bifrost Query permissions, lines 1858–1880 | `query/service.rs::validate_predicate`, full `QueryAuthority` implementation; runtime `permission_check.rs`; Oracle `authorize_resolved_tables/resolved_table_scope`; catalog `table_uid/registration/describe_table`, `TableRef::permission_scope` |
| Authenticated tenant and browser credential | security posture Rust-owned browser sessions, lines 310–349; root AGENTS | UI `hooks.server.ts`, tenant `ui/load/+server.ts`, Observe and Query server actions; `node.ts`, `mcp.ts`, `observe.ts::predicateFor/Binds`, metric planner predicate construction |
| Language projections | root and Python/TS SDK AGENTS; task first-class surface requirement | Rust `bifrost/query.rs`, facade and blocking projections; Python native Bifrost method and public sync/async methods; TS native method/public validatePredicate; first-class predicate integration test source |
| Proof | task V3 and V10–V13; repository named-test rule | floor parser unit cases; Rust SDK and MCP predicate journey assertions; Python and TypeScript predicate journey source |

## Acceptance matrix for this boundary

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Installed parser, one expression, EOF, bounded UTF-8 input, no comments/statements/placeholders/functions/subqueries | `floor.rs:252–303` byte check and tokenizer/parser; `307–425` closed AST checks | Two exact parser tests rerun: 2 passed | PASS |
| Selected table columns checked before normalized output | `service.rs:358–374`; unquoted names fold for comparison, quoted names exact in `floor.rs:408–419` | Parser tests; Rust/MCP source assertions for unknown column | PASS |
| Query capability then exact table grant before schema description | `service.rs:332–361` | Rust journey source covers datasets-scoped allow and verification-table deny without column text; MCP analogous logs-only scope | PASS for effective RBAC behavior; mandatory chokepoint violation below |
| Tenant cannot resolve another tenant table | `table_uid` calls registration with authenticated `caller.data_tenant_id`; cache and lookup tenant-qualified | Rust/MCP/Python/TS journey source asserts absent-table response in other tenant | PASS by source; integration results not rerun |
| Payload columns require gateway payload grant | `service.rs:362–366`; CallsTable namespace is `gateway`, matching constructed FQN | MCP source explicitly tests metadata-only refusal, allowed run_id, payload-granted success | PASS for current effective permission semantics; mandatory chokepoint violation below |
| No client validation substitutes for server authorization | all projections call shared Rust Bifrost HTTP endpoint or MCP service; normalized predicate remains plain SQL, no authority token; queries still pass stream_query and Oracle | Source tracing through BFF loadPanel/node executor and `predicateFor` refusal before reads | PASS |
| Shared predicate cannot override range/tenant/read bounds | Binds adds parenthesized predicate as another AND clause; charts receive normalized predicate; query execution is authorized independently | Shared loader test source covers refused validation with no dependent query; no weaker client execution path found | PASS |
| Required handler permission chokepoint | `service.rs:346–347` duplicates contains rather than injected PermissionCheck | Static source proof; existing checker is currently equivalent RbacCheck | FAIL — SECURITY-01 |

## SECURITY-01 — Handler duplicates exact permission decisions outside PermissionCheck

- Classification: VIOLATION.
- Violated obligation: `architecture/wyrd-security-posture.md`, Permission check, lines 576–592: PermissionCheck is the one runtime authorization chokepoint; handlers call the configured check and add no shortcut helpers. Root authority rules make this owning architecture mandatory.
- Exact location: `crates/wyrd/wyrd-server/src/query/service.rs:346–347` (`granted` closure), used at lines 348–355 for the resolved table and 362–366 for gateway payload admission.
- Reachable path: public HTTP `/v1/query/predicate`, MCP `bifrost.validate_predicate`, and every SDK validation call enter this service after coarse capability admission. Exact table identity is resolved from the caller tenant, then the closure reads the principal permission set directly. The payload decision uses the same closure. These are required production paths, not dormant code.
- Source proof and sibling comparison: `QueryAuthority::authorize` at service.rs:109–119 already uses `state.auth.permission_check.check`; `crates/shared/wyrd-runtime/src/permission_check.rs` owns the PermissionVerdict projection and sole RbacCheck implementation. Oracle has an explicitly documented resolved-table rule in Bifrost architecture; that internal planner decision does not authorize adding an alternative handler checker. `admit_capability` is expressly coarse route admission and must remain scope-agnostic.
- Observable consequence/materiality: this new public validation boundary bypasses the configured handler authorization chokepoint and owns a second implementation of permission decisions. This is an explicit architecture violation at a sensitive boundary. Current RbacCheck itself calls exactly the same `PermissionSet::contains`, so this review does **not** claim a demonstrated privilege escalation, different current allow/deny result, or tenant leak. Retain as a mandatory boundary correction, not an invented exploit.
- Smallest testable correction: use the existing configured `state.auth.permission_check.check` with the already-built exact table-scoped Permission and the existing gateway payload Permission. Preserve table UID resolution, authorization-before-description, coarse capability admission, current stable table refusal and payload refusal behavior, and nonblocking audit owner. Do not invoke existing `QueryAuthority::authorize` unchanged: its global permission would incorrectly reject schema/table-scoped readers. No new permission model, checker, policy service, cache, or public contract is needed. Whether the small call resides inline or beside the owner remains implementation-owned.
- Correction burden: two decisions at one service boundary; reuse existing PermissionVerdict and error/audit plumbing. Deletion would remove required authorization; replacing scope with global permission is incorrect; the existing checker satisfies the requirement with no new machinery.
- Focused closure proof: inspect that both exact decisions call configured PermissionCheck; rerun the existing Rust and MCP predicate tests preserving scoped success, ungranted-table denial before column disclosure, and payload grant distinction. Keep ordinary query authorization untouched. A broad journey aggregate is unnecessary for this correction.

## Verification performed and limits

Ran through mise:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::floor::tests::predicate_accepts_and_normalizes_permitted_constructs) | test(=query::floor::tests::predicate_refuses_everything_else)'
```

Result: 2 tests passed, 573 unrelated tests skipped by the exact filter. Nextest run ID `1094adec-184f-42ae-af86-181280f026b1`. This proves only the parser cases; it is not substituted for HTTP/MCP integration.

Source-inspected integration selectors, not rerun in this specialist pass:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=query_bifrost::predicates_are_typed_bounded_and_tenant_scoped)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::predicates_are_typed_bounded_and_tenant_scoped)"'
```

No additional security defect was source-proven. No speculative grammar hardening, alternate transport, new validator or blanket denial is recommended. UI evidence remains outside this report; this security pass cannot establish whole-task acceptance.
