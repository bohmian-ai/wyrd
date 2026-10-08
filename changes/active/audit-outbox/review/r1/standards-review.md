# Repository Standards Review

## Immutable Subject

- Repository root: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Reviewed range: `base..candidate` only
- Candidate check: `HEAD` was `f451d52be01acba66bae003b8509c95086fc8558` at the start of this review. The report write does not modify the reviewed commit.
- Scope: repository standards only. Task acceptance and Ponytail validation are intentionally excluded.

The repository has no `.codegraph/` directory, so the CodeGraph path was correctly skipped.

## Authority Coverage

| Changed surface | Applicable authority read completely | Coverage result |
|---|---|---|
| Repository-wide Rust, SQL, contracts, tests, docs, and verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/maintainer-style.md` | Complete |
| Wyrd authorization/audit contract and public surfaces | `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Complete; conflicting authority text is a finding |
| Bifrost Gate, Oracle, catalog, publication, shutdown, and analytical reliability | `architecture/bifrost-design.md`; `architecture/operations/README.md`; `architecture/operations/deployment-and-release.md`; `architecture/operations/reliability-and-recovery.md`; `architecture/operations/runbooks.md`; `architecture/references/domain/vala-architecture.md`; `architecture/references/domain/olap-serving.md`; `architecture/references/domain/analytical-operations-reliability.md` | Complete; conflicting operations/reference text is a finding |
| Rust ownership, async, SQL tenancy, documentation, and signatures | `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md` | Complete |
| Rust/Python/TypeScript SDK projections and generated declarations | `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Change evidence and canonical commands | `changes/active/audit-outbox/tasks/01-publication-progress.md`; `02-one-outbox.md`; `03-remove-audit-unavailable.md`; relevant `mise.toml` task definitions | Complete |

## Rule Results

| Repository rule | Evidence | Result |
|---|---|---|
| One audit write path and publisher; every audited surface uses the process outbox and no audit-only failure remains (`AGENTS.md` §2; `architecture/agent-rules.md`) | `vala-sql/src/audit_outbox.rs` owns the production call to crate-private `append_audit_events`; direct append exports are `test-support`-only; server/auth/Gate/Oracle composition stages through `AuditOutbox`; audit-unavailable variants and projections are removed. | PASS |
| Permissions block while audits do not; hard-kill/uncommitted loss is accepted and counted | `AuditOutbox::stage` is synchronous and bounded by `pending`; failed enqueue/commit records `audit_outbox_commit_failures_total`; `AuditOutbox::shutdown` drains to a deadline. The approved eventual-consistency/loss window was not treated as a defect. | PASS |
| Audit records authorization decisions, not engine mechanics | Changed Gate, Oracle, auth, server, gateway, verification, and platform paths stage permission verdicts; Scribe/Forge mechanics remain lineage. | PASS |
| Publication progress is tenant-scoped, separate from append locking, monotonic, and replay-stable | Migration creates RLS-protected `vala.audit_publication`; freeze/settle use the progress row; staged retirement remains coupled to watermark advancement. Recorded SQL and server journey evidence covers contention, replay, and retirement. | PASS |
| Tenant SQL uses `TenantConn` and RLS without hand-written tenant predicates (`AGENTS.md` §5/§15; `architecture/agent-rules.md`) | The materially changed canonical append continues to bind `data_tenant_id` and explicitly filter the chain-head row, and its new rustdoc endorses a bypass-RLS use that the `TenantConn` signature cannot represent. | **FAIL — STD-003** |
| Cross-tenant SQL uses only `OperatorPool`; no raw pool in production library signatures | New production code composes `ValaPostgres`/`TenantConn`; raw `PgPool` occurs only in external Postgres tests. | PASS |
| Stateful workflows have a cohesive concrete owner; async is limited to IO | `AuditOutbox` owns queue, lifecycle, writer, and shutdown; `AuditOutboxWriter` owns receiver/per-tenant state/in-flight commits; async methods await database/task/timer IO. | PASS |
| All imports are top-level and signature types are imported/bare (`architecture/agent-rules.md`) | Candidate-introduced signatures contain fully qualified `tokio`, `vala_bifrost_redux`, `serde_json`, and `crate` paths. | **FAIL — STD-004** |
| Rustdoc on every new/materially modified item accurately explains workflow, errors, cancellation, and side effects (`AGENTS.md` §16) | New outbox items are substantially documented, but several materially changed/public audit items still assert transactional/fail-closed audit behavior that the candidate removes. | **FAIL — STD-002** |
| Public contracts, generated schemas, SDKs, docs, and architecture remain aligned | The candidate updates top-level authorities and error projections, but checked-in schema text, normative security/operations material, and routed references still describe the deleted transactional/WAL/relay model. | **FAIL — STD-001, STD-002** |
| Generated artifacts are regenerated, not hand-maintained | Recorded `codegen:regen`, `codegen:check`, and `check:proto-drift` are green; the binary descriptor and generated schema/error projections changed with their owners. | PASS, subject to the stale source documentation in STD-002 |
| Stable public errors use the derive-backed catalog and no compatibility alias is added | Removed audit-unavailable errors disappear from Rust/HTTP/proto/SDK projections; the retained historical `AuditErrorCode::AuditUnavailable` is explicitly decode-only, not a public refusal. | PASS |
| Python/PyO3 and TypeScript changes remain in their SDK boundaries | Python and napi edits are confined to the first-class SDK/testing packages; no PyO3 entered `wyrd-spec`; no durable logic moved into a language binding. | PASS |
| Every changed language/layer receives its canonical verification; broad multi-boundary changes use `mise run gate` (`AGENTS.md` §11-§12; testing workflow) | Evidence includes Rust families, SQL/Redux/server journeys, Python unit/type/format/lint, codegen, proto, docs, fmt, and lints. It does not include `mise run gate`; therefore the changed TypeScript native testing binding/declaration/integration test lacks `ts:napi:check`, `ts:typecheck`, `ts:test:unit`, and `ts:test:integration` proof. | **FAIL — STD-005** |
| No gate was weakened or skipped to manufacture success | Removed tests/hooks asserted a now-forbidden public refusal and were replaced by non-blocking audit failure tests; no `#[allow]`/`#[ignore]` workaround was introduced in the reviewed paths. | PASS |

## Material Findings

### STD-001 — Applicable architecture and operations authorities still require the deleted Oracle WAL/relay and transactional exceptions

- **Violated rule:** `AGENTS.md` §1-§2 requires all applicable architecture authority and public/internal behavior to align; `architecture/references/README.md` requires every selected reference to conform to its governing authority; `architecture/agent-rules.md` permits exactly one outbox/write path and no other audit WAL, relay, or sink.
- **Locations:**
  - `architecture/wyrd-security-posture.md:369-380` requires an Oracle fsynced local acceptance record/relay and keeps gateway administration transactional.
  - `architecture/operations/README.md:25,50` makes the removed Oracle acceptance WAL and relay readiness dependencies.
  - `architecture/operations/reliability-and-recovery.md:148-150,220-222` requires that WAL/relay and says audit dependency loss stops requests.
  - `architecture/operations/runbooks.md:141-145` keeps Oracle readiness gated on the acceptance WAL and relay.
  - `architecture/references/architecture/patterns.md:253-267`, `architecture/references/doctrine/architecture-constraints.md:112-122`, `architecture/references/languages/rust-core.md:604-612`, and `architecture/references/domain/vala-architecture.md:77-80` retain transactional surfaces or a special Oracle local-WAL exception.
- **Evidence:** The candidate's governing rules instead state that every surface stages on one process outbox, no audit-only error exists, and no other audit WAL or relay exists. The implementation deletes `oracle/query_audit.rs` and routes Oracle/gateway administration through `AuditOutbox`.
- **Consequence:** Repository authority gives operators and future implementers mutually exclusive requirements. Following the operations/security text would mark valid replicas unready or reintroduce infrastructure explicitly forbidden by the current governing rules; following the implementation makes the documented recovery/go-no-go procedures impossible to execute.
- **Testable correction:** Update every listed normative operations document and routed reference to the one-outbox model: no Oracle acceptance WAL/relay, no transactional surface exception, request success independent of audit commit, bounded/countable audit gaps, and graceful outbox drain. A focused repository search for `acceptance WAL`, `audit-acceptance WAL`, audit `relay`, transactional audit exceptions, and gateway-administration audit atomicity must leave no live statement that contradicts the governing model.

### STD-002 — Public AuditEvent/schema and touched Rust documentation still promise atomic audit or audit-write failures

- **Violated rule:** `AGENTS.md` §9 and §16 require public contracts and rustdoc to match behavior; generated schemas must project their source; `architecture/references/languages/agent-harness.md` requires deterministic agent-facing contracts rather than prose that contradicts runtime semantics.
- **Locations:**
  - `crates/wyrd-spec/src/vala/api.rs:2422,2450-2456` calls the event transactional and promises same-transaction append before the operation.
  - That source text is projected into `crates/wyrd-spec/schemas/bifrost_audit_event.json:4` and `crates/wyrd-spec/tests/schemas/bifrost_audit_event.json:4` even after regeneration.
  - `crates/wyrd-spec/src/vala/audit_detail.rs:1` still calls the detail transactional in a materially changed module.
  - `crates/wyrd/wyrd-auth/src/callback.rs:165-173` says a canonical audit append can fail the OIDC session and that everything commits together, while the candidate now stages after the grant transaction.
  - `crates/wyrd/wyrd-server/src/auth/callback.rs:41-47` still exposes a failed audit write as an error condition even though no such request error exists.
  - `crates/wyrd/wyrd-auth/src/issue_api_key.rs:42` still describes the materially changed issue result as transactional-audit state.
- **Evidence:** The new outbox documentation and governing rules explicitly accept a committed effect with no row after enqueue/commit/process loss; the changed auth paths call `AuditOutbox::stage`, which returns `()` and cannot produce the documented request errors.
- **Consequence:** Agent/schema consumers and maintainers receive a stronger atomicity/error contract than the server provides. They may infer that a successful operation has a durable audit row or implement handling for a failure that can no longer occur.
- **Testable correction:** Correct the owning Rust docs and all materially changed item docs to describe a permission verdict handed to the non-blocking process outbox, possible counted loss before commit, and no audit-write request error. Regenerate schemas from the corrected source and run `mise run codegen:check`; search changed source/schema docs for `same transaction`, `transactional audit`, and audit-write failure language that refers to authorization decisions.

### STD-003 — The materially changed canonical TenantConn append preserves a forbidden manual tenant filter and documents an impossible bypass-RLS caller

- **Violated rule:** `architecture/agent-rules.md`: “TenantConn (Postgres RLS) is the load-bearing tenant boundary. Do not add manual per-query tenant filters on a TenantConn path”; `AGENTS.md` §15 applies the same boundary to materially modified Rust.
- **Location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:60-75,88-97,165-170`.
- **Evidence:** `append_audit_events` accepts only `&mut TenantConn<'_>`, derives its tenant id, then explicitly filters `vala.audit_chain_head` by `data_tenant_id`. Its new rustdoc justifies the predicate by a boundary “where row-level security is bypassed,” but a bypass-RLS caller must use `OperatorPool`; it cannot be represented by this signature. The production outbox obtains the connection with `ValaPostgres::tenant_conn`.
- **Consequence:** The canonical append carries two tenant authorities and preserves exactly the drift the repository rule forbids. Its documentation invites a future caller to treat `TenantConn` as a bypass-RLS abstraction.
- **Testable correction:** Keep the production append tenant-scoped through `TenantConn` and rely on `wyrd.current_tenant()`/RLS for row selection and mutation; remove the bypass-RLS claim and hand-written tenant predicates from the materially changed path. Preserve platform events by acquiring a `TenantConn` for `DataTenantId::SYSTEM_OWNER`, not by widening the query. Re-run the SQL integration lane and tenant-isolation boundary check.

### STD-004 — Candidate-introduced Rust signatures use fully qualified paths instead of top-level imports and bare names

- **Violated rule:** `architecture/agent-rules.md`: “Bring types in with `use` and use bare names in signatures”; all `use` statements belong at module top.
- **Locations:**
  - `crates/vala/vala-sql/src/audit_outbox.rs:276` — `tokio::task::JoinError`.
  - `crates/wyrd/wyrd-server/src/state.rs:622` — `vala_bifrost_redux::oracle::OracleRuntimeInspection`.
  - `crates/wyrd/wyrd-server/src/auth/callback.rs:761` — `serde_json::Value`.
  - `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:3719-3723` — fully qualified peer/context types in a changed test implementation.
- **Consequence:** The module dependency surface is hidden inside signatures, contrary to the repository's mandatory import convention; the production `state.rs` case also makes the new public test-support accessor noisier and harder to relocate.
- **Testable correction:** Add the needed top-level imports in each module/test module and use bare type names in the signatures. `mise run fmt` and `mise run lints` must remain green.

### STD-005 — Required broad/TypeScript verification is missing

- **Violated rule:** `AGENTS.md` §11-§12 and `architecture/references/languages/testing-workflows.md`: use `mise run gate` for an intentionally broad change crossing several ownership boundaries without one complete capability gate; TypeScript edits require the matching build, declaration, typecheck, unit, and journey coverage.
- **Locations:** verification evidence in `changes/active/audit-outbox/tasks/01-publication-progress.md`, `02-one-outbox.md`, and `03-remove-audit-unavailable.md`; changed TS surfaces under `sdks/wyrd-sdk-ts/native-testing`, `sdks/wyrd-sdk-ts/testing`, and `sdks/wyrd-sdk-ts/wyrd`.
- **Evidence:** Recorded commands cover many focused Rust, SQL, server, Python, codegen, documentation, format, and lint lanes, but no `mise run gate`. `mise.toml` shows that `codegen:check` snapshots `wyrd/src/error-codes.ts` and Python stubs only; it does not build/typecheck the changed private TypeScript testing binding/declaration. The aggregate gate includes `ts:napi:check`, `ts:typecheck`, `ts:test:unit`, and `ts:test:integration`.
- **Consequence:** The reviewed evidence does not prove that the changed napi test binding, `@wyrd/testing` declaration, and surviving TypeScript integration surface compile and agree at runtime. Repository completion standards are therefore unmet even if the source is otherwise correct.
- **Testable correction:** After the source/documentation findings are corrected, run the one canonical broad aggregate, `mise run gate`, and record its green result. Do not duplicate the gate's component lanes as separate final verification unless `mise.toml` proves a required specialized lane lies outside it.

## Verification Notes

Recorded green evidence reviewed:

- `mise run fmt`, `mise run lints`, `git diff --check`
- SQL integration and migration tests, including publication-progress and audit-outbox concurrency/drain cases
- `mise run test:wyrd`, `mise run test:principals:integration`
- Bifrost server, MCP, SDK, Redux, and SQL lanes listed in the task evidence
- `mise run codegen:regen`, `mise run codegen:check`, `mise run check:proto-drift`
- Python unit, typecheck, format, and lint lanes
- `mise run docs:check`

Missing or insufficient for repository completion:

- `mise run gate` for this 139-file, multi-owner, contract/SDK/test-harness/architecture change.
- Consequently, no recorded TypeScript build/declaration/typecheck/unit/integration proof for the changed TypeScript testing projection.
- Green codegen proves the checked-in schema matches its Rust source; it does not validate the truth of the stale same-transaction description in that source.

## Overall Result

**FAIL**

The implementation observes several central repository boundaries, but five material repository-rule failures remain: contradictory governing/operations references, false public and Rust documentation, a forbidden TenantConn tenant predicate, fully qualified types in newly added signatures, and missing required aggregate/TypeScript verification.
