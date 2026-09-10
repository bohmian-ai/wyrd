# TASK-001 independent acceptance review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/rbac-alignment`
- Approved specification: `changes/active/object-scoped-rbac/spec.md`, revision 3
- Original task: `changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md`
- Base: `1c4b518dbb69dd10b9107c06e8ccb410c67c5449`
- Candidate: `3de6aed2f86b4bb039e85c7f62c0a48e12602282`
- Reviewed range: `1c4b518db..3de6aed2f`
- Candidate remained at the recorded commit throughout review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: exact required-scope JSON and typed schema/table identities | `crates/wyrd-spec/src/auth/permission_scope.rs`; `crates/shared/wyrd-runtime/src/permission.rs` | Reported `wyrd-spec` and `wyrd-runtime` permission tests; `codegen:check` | PASS |
| REQ-001: malformed identity, missing scope, and invalid resource/scope combinations fail closed | `PermissionWire` and `Permission::validate` reject missing scope, malformed identity, and unrelated resources, but do not validate the action | Existing tests omit non-read, wildcard, and `AnyOf` action cases | FAIL (`FIND-TASK-001-3`) |
| INV-004 / AC-001: wildcard resource/action authority is object-wide only with `All` | Resource wildcard with Bifrost scope is rejected; `Action::Wildcard` with Bifrost scope is accepted and can cover `Read` | `coverage_is_three_axis` exercises only a valid all-scoped wildcard | FAIL (`FIND-TASK-001-3`) |
| REQ-001 / AC-002: tenant role JSON resolves through the existing JSONB, resolver, set, epoch, and cache path | `SqlPermissionResolver` still decodes `Vec<Permission>` from the existing column; no migration or cache was added | Reported resolver and `wyrd-auth-verify` focused tests | PASS |
| INV-001: tenancy comes only from verified principal/catalog authority | `AuthorizedQueryContext::try_new`, tenant-bound catalog preparation, and forwarding validation preserve tenant equality | Reported server/oracle journeys and peer-authority test | PASS |
| REQ-002 / INV-003: every resolved table is authorized before source IO | `OraclePlanner::pin_cut` calls `authorize_resolved_tables` only after `protect_and_materialize`; `materialize_reader_cut` opens Iceberg manifests and hot-cut catalog state | Journey proves pre-stream denial, not absence of pre-denial source IO | FAIL (`FIND-TASK-001-1`) |
| REQ-002: logical catalog/schema plus stable table UID drive authorization | `resolved_table_scope` projects the catalog-resolved binding and existing `TableUid` | Reported scope containment and journey matrix tests | PASS |
| REQ-002 / AC-005: sensitive payload access requires the same resolved table scope and preserves categories/error | `authorize_payload_projection`, `payload_permission`, and `PermissionSet::contains` | Reported three exact `vala-bifrost-redux` tests | PASS |
| INV-005: scoped decisions participate in audit | Accepted decisions use `scoped_permission_digest`; scoped object denials only log a warning and return `QueryForbidden`, bypassing the route's durable denial audit | Journey asserts the error and zero rows but no denial audit row | FAIL (`FIND-TASK-001-2`) |
| INV-005 / AC-005: forwarding, stage tickets, and workers cannot widen coordinator authority | Typed context is signed; scoped table set enters the permission digest used by audit and stage authority | Reported digest test and exact peer-authority tamper test | PASS |
| No scoped decision is reduced to the operation-only string | `AuthorizedQueryContext.permission` is typed; `scoped_permission_digest` serializes it and the table scopes | Reported digest and codegen checks | PASS |
| REQ-003 / AC-003 / AC-004: one real-server journey proves both role matrices and zero-row denials | `tenant_scoped_roles_reach_only_their_granted_bifrost_tables` uses public SDK/server paths and all six cases | Reported server journey binary 5/5; accepted cases contain no fixture rows | PASS, with stated row-content limit |
| Roles remain test-only and public adapters retain one server/Oracle path | Role names occur only in resolver fixtures and the server journey; HTTP/gRPC reach `stream_query`/Gate/Oracle | Reported server and oracle journey binaries | PASS |
| Permission foundation docs and active/public authorities agree with required scope | Most pages were updated; `permission-model.md` still permits a narrow wildcard in prose and shows two scope-less Rust examples | `docs:check` checks site mechanics, not semantic agreement | FAIL (`FIND-TASK-001-4`) |
| No new store, policy lookup, cache, checker, string scope language, identity, registry, dependency, or builtin role | Diff contains none of the excluded additions | Manifest/name/diff inspection | PASS |
| Generated artifacts changed through their owner and are current | Scope enters `BifrostPermissionDescriptor`; both schema projections changed identically | Reported `codegen:check` | PASS |
| Required struct-centered Rust style | New async admission and denial-audit workflows are free functions threading `AppState` and `Caller` despite the module's existing `ControlAudit` owner | Source inspection | FAIL (`FIND-TASK-001-5`) |
| Every new/materially modified Rust item, including tests, has required rustdoc | New production items are documented; seventeen new focused test functions are not | Source inspection; lints do not enforce this repository rule | FAIL (`FIND-TASK-001-6`) |
| Task-required exact focused commands select and prove every named test | Vala, peer, auth, and verifier expressions are exact; `wyrd-spec`/`wyrd-runtime` use regex module filters and the new journey was run only through a whole-binary mise task | Implementation report lines 281-298 | FAIL (`FIND-TASK-001-7`) |
| Required format, lint, codegen, boundary, docs, and diff checks | Reported passing except `check:design-sync` and `check:unwrap-audit`, both reproduced unchanged at base and outside this write set | Supplied implementation evidence | PASS; pre-existing failures are not candidate findings |
| Constraint forbidding `gate`, `test:bifrost`, and complete platform/family suites | Only focused commands and journey leaves were reported | Implementation report | PASS |

## Material findings

### FIND-TASK-001-1 — INCORRECT: object authorization follows source materialization

- Violated obligation: REQ-002, INV-003, and the task requirement that the complete object decision precede source IO.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/planner.rs:183-226,383-395`; `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:461-499`.
- Evidence: `attempt_protected_cut` resolves all `PreparedReaderIdentity` values, then acquires a reader guard, revalidates, and calls `materialize_reader_cut` for every table. Materialization opens the gated Iceberg table, manifests, and hot-cut catalog state. Only after that returns does `pin_cut` call `authorize_resolved_tables`.
- Observable consequence: a scoped principal can drive manifest/hot-cut IO and observe storage/catalog timing or failures for an unauthorized table before receiving `WYRD_VALA_403_QUERY_FORBIDDEN`.
- Required correction: take one whole-query authorization decision from the complete prepared catalog identities and stable UIDs before guard acquisition, revalidation, or materialization; prove an uncovered table causes no materialization/source-IO observation.

### FIND-TASK-001-2 — MISSING: scoped object denials are not durably audited

- Violated obligation: INV-005, the security posture's authorization-denial audit requirement, and preservation of the existing audited-denial boundary.
- Location: `crates/wyrd/wyrd-server/src/query/service.rs:58-112`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4909-4928`.
- Evidence: a principal with any scoped query capability passes route admission. An uncovered table then produces only `tracing::warn!` and `QueryForbidden`; the durable `record_denial` path runs only for coarse route denial.
- Observable consequence: repeated attempts to read out-of-scope tables have no tenant audit-chain record even though the tenant and principal are verified.
- Required correction: reuse the existing tenant-bound query denial-audit mechanism for authoritative object denials, fail closed if that append fails, preserve the stable public query-forbidden error, and add focused proof of exactly one durable denial event and zero accepted-read events.

### FIND-TASK-001-3 — INCORRECT: Bifrost object scope accepts non-read actions

- Violated obligation: REQ-001 and INV-004 require Bifrost object scope only on Bifrost query/sensitive-payload read permissions and require wildcard action authority to be object-wide only with `All`.
- Location: `crates/shared/wyrd-runtime/src/permission.rs:208-215,244-265`.
- Evidence: `Permission::validate` checks the resource but never checks `action`. JSON carrying Bifrost scope with `write`, `wildcard`, or `any_of` therefore deserializes; `Action::Wildcard` subsequently covers `Read`.
- Observable consequence: malformed persisted role JSON becomes effective read authority instead of being rejected as corrupt.
- Required correction: reject every Bifrost-scoped permission whose action is not exactly `Read`, including wildcard and `AnyOf`, at the single `Permission` decode boundary and prove the resolver surfaces the corrupt role.

### FIND-TASK-001-4 — VIOLATION: the active permission authority contradicts the required contract

- Violated obligation: the task requires `permission-model.md` and all active authority to agree that scope is required and wildcard resource/action grants use `All`.
- Location: `architecture/v1/00-foundations/permission-model.md:115-116,185-199`.
- Evidence: the document says a wildcard paired with narrow scope "stays narrow" even though wildcard resources must reject Bifrost scope, and its delegation examples omit the required `scope` field.
- Observable consequence: a literal client or maintainer following the active foundation document constructs inputs the runtime rejects, or assumes an invalid narrow-wildcard contract exists.
- Required correction: make the wildcard statement match decode rules and add explicit `All` scope to both delegation examples.

### FIND-TASK-001-5 — VIOLATION: new query authorization workflows have no concrete owner

- Violated obligation: `AGENTS.md` section 5 and `architecture/agent-rules.md` require dependency-backed workflows and IO orchestration to be inherent methods on one cohesive concrete owner.
- Location: `crates/wyrd/wyrd-server/src/query/service.rs:58-112`.
- Evidence: `admit_query_capability` and `record_denial` are new free async workflow functions that repeatedly thread `AppState` and `Caller`; the same module already uses `ControlAudit` as a state-owning authorization/audit owner for query lifecycle operations.
- Observable consequence: query authorization and audit policy are split across free functions and the owner, making the required fail-closed behavior discoverable only by tracing a functional call chain.
- Required correction: consolidate route admission, authorization denial recording, and lifecycle authorization/audit on one cohesive query authorization/audit owner, reusing the existing `ControlAudit` owner/mechanism rather than adding a parallel abstraction.

### FIND-TASK-001-6 — VIOLATION: new focused tests omit mandatory rustdoc

- Violated obligation: `AGENTS.md` section 16 and `architecture/agent-rules.md` require rustdoc on every new Rust item, explicitly including test functions.
- Location: `crates/wyrd-spec/src/auth/permission_scope.rs:224-314`; `crates/shared/wyrd-runtime/src/permission.rs:883-1011`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:5506-5602`; `crates/wyrd/wyrd-auth/src/permission_resolver.rs:133-184`.
- Evidence: seventeen new `#[test]` functions have no preceding rustdoc.
- Observable consequence: the candidate violates a repository hard completion gate even though Clippy passes.
- Required correction: add concise intent/invariant rustdoc to every new test function; do not document unrelated existing items.

### FIND-TASK-001-7 — MISSING: exact focused verification evidence is incomplete

- Violated obligation: task verification lines 219-230 require an exact fully qualified nextest expression for every new focused test and forbid proof that can silently select the wrong set.
- Location: `changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md:281-298`.
- Evidence: the report uses `test(/auth::permission_scope/)` and `test(/permission/)` regex selectors and invokes the server journey through the full journey binary rather than recording/running its exact test expression.
- Observable consequence: the durable report does not prove that each named scoped test was selected, despite reporting green aggregate counts.
- Required correction: run and record exact expressions for every new/changed focused test, including the real-server matrix, plus the corrected real peer-authority test name.

## Verification limits

- This review relied on the recorded green commands and independently inspected the cumulative source/diff; it did not rerun the full reported suite.
- The task's original peer expression names a helper and selects zero tests. The candidate correctly ran the real test `oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence`; remediation must retain that correction.
- Accepted journey cases stream zero rows. They prove successful authorize/admit/audit/execute completion, not returned row content. This does not invalidate the stated access matrix, but it cannot prove projection contents.
- `check:design-sync` and `check:unwrap-audit` failures were reported as identical at the base commit and outside the write set; they are not findings in this review.
- A separate security/tenancy specialist reviewed the changed boundary. Its source-materialization, denial-audit, and action-validation findings are incorporated above. No separate critical issue remained.

## Prior-finding closure

No prior task-review verdict exists for this candidate.

## Verdict

`FIX_REQUIRED`
