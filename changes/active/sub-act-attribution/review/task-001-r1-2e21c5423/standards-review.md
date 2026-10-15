# Repository standards review

Overall result: **FAIL**. `REPO-R1-001` is a continuation of the required-rustdoc closure boundary `FIND-TASK-001-8`; the previously enumerated items were corrected, but the cumulative changed surface still has required documentation gaps. This is not a runtime defect claim.

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `2e21c5423036460d5da2ca22a51efde63635e352`.
- Authority: approved `changes/active/sub-act-attribution/spec.md` r6; original task and R1 are subordinate to repository authority and that revision.
- Reviewed the cumulative base-to-candidate surface, including the initial implementation; R1 was a locator and prior findings were closure hypotheses. CodeGraph was used first for authority, caller, ingress, and fixture navigation, then expanded through the actual diffs, owning modules, declarations, SQL signatures, schema projections, language tests, docs, and verification records.
- At the concluding source check, HEAD still equaled the candidate, the only working-tree changes were the new review directory, and cumulative `git diff --check` passed. No runtime, Cargo, package, or codegen commands were run by this reviewer.

## Authority coverage

Every row is governed by `AGENTS.md`, `architecture/agent-rules.md`, the reference router, `architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx`. References below are repository-relative under `architecture/references/`.

| Changed surface | Additional applicable authorities | Reviewed owners and consumers |
|---|---|---|
| Specification/task/review packet and verification evidence | `languages/spec-driven-development.md`, `implementation-execution.md`, `testing-workflows.md` | Approved r6, original task, prior standards finding, R1 corrections/diagnoses/exact selectors; no implementation summary treated as acceptance authority |
| Pure wire contracts and internal protobuf | `languages/rust-core.md`, `errors.md`, `agent-harness.md`; `doctrine/positioning-and-vocabulary.md`, `architecture-constraints.md`; security posture | `Attribution`, reserved constants, `AuditEvent`/`AuditDetail`, verification targets/status/input, operator template context, gateway accounting/payload, `IngestCaptureRequest`; source-driven TS projection |
| Shared runtime, client, queue and telemetry | `languages/rust-core.md`, `maintainer-style.md`; `architecture/patterns.md`; `domain/telemetry-observations.md` | `AcceptedAuthority`, delegation projection, client Observe/state/TableConfig, queue reserved-name validation, registered Workflow hydration/loading/local route, `TenantSpanRoute` rebinding and tests |
| Authentication, audit, creation and gateway owners | Security posture; `architecture/patterns.md`; `languages/rust-core.md`, `errors.md`, `agent-harness.md` | Auth grant/issuance/callback/revocation audit construction, `Caller`, permission audit paths, Card/principal/connection creation, tenant provisioning/built-ins/reconcile, gateway invocation/ledger/capture/workflow and private capture receiver |
| Verifier acceptance, execution and input reads | Security posture; `domain/evaluation.md`, `drift-monitoring.md`, `telemetry-observations.md`, `analytical-operations-reliability.md`; Rust/owner/testing references | `BackgroundAuthority`, existing issuer recovery, `SystemReadAuthority::serving`, direct/service/eval/drift/results/cache/runner/scheduler, observation outbox, Workflow host |
| SQL and migrations | Security posture; `architecture/patterns.md`; `languages/rust-core.md`, `testing-workflows.md`; protocol coordination-clock rules | Defining auth/Card/operation/binding/run/SYSTEM/connection migrations; deleted incremental migration; `verifier_runs`, Card registration/read/delete/projection, service-account/key/connection and OLAP catalog SQL; fixtures and PG tests |
| Bifrost schemas, ingest, internal writes and reads | `architecture/bifrost-design.md`; `domain/olap-serving.md`, `arrow-analytical-interop.md`, `telemetry-observations.md`, `analytical-operations-reliability.md` | Gate auth/dispatch, ingress frame and preprocessing, CPU decode/stamping/fingerprints, managed policies, built-in table definitions and layout/Bloom changes, subject sampling, Oracle audit context, outbox encoding and peer forwarding |
| Rust tests and materially changed test helpers | `languages/testing-workflows.md`, `rust-core.md`, `maintainer-style.md`; relevant domain authorities above | Existing SQL/server/Bifrost/MCP/SDK targets, fixture construction, fresh-DB creator test, provisioning/reconciliation test, authority persistence and delegated reads, real SDK and MCP journeys, exact selector evidence |
| Python package, tests and example | `languages/python-api-and-stubs.md`, `testing-workflows.md`, `maintainer-style.md`; common client/contract authority | TableConfig reserved-name/Bloom docstrings; public `wyrd` integration imports; model rows, reserved-field and span-pair assertions; gateway identity fixture changes; support-desk query/result field |
| TypeScript projection, tests and example | `languages/typescript-guide.md`, `testing-workflows.md`, `errors.md`, `maintainer-style.md` | Generated Card type comment and owning Rust source; Zod managed-row schema, reserved-field/pair assertions, gateway/realtime/timeline stories, mechanical native-testing caller update, support-desk query/result field |
| Architecture, docs site and examples | Spec-driven authority; product/security/Bifrost authorities; `languages/maintainer-style.md`, `testing-workflows.md` | Renamed result and audit SQL fields, verification diagram joins, MCP request example, public attribution explanation, three support-desk implementations and reported example journeys |

No Cargo manifest, feature, dependency, new PyO3 or N-API signature, Python native export/stub, Svelte UI behavior, provider wire implementation, external-URL fetching implementation, DataFusion plan algorithm, Iceberg publication algorithm, or compaction algorithm is introduced. Those specialized implementation authorities do not independently require edits merely because an existing consumer is rebuilt. Arrow schema and lifecycle invariants do apply to the changed ingest/table paths and are covered above.

## Applicable-rule results

`PASS` means no material standards violation identified from the reviewed source; behavioral acceptance and domain verdicts remain separate. Reported verification is distinguished from reviewer-executed proof.

| Rule | Result | Source evidence |
|---|---|---|
| Correct owner and pure foundational contract (`AGENTS` §§2–3,9; architecture patterns) | PASS | `Attribution` is IO-free in `wyrd-spec`; accepted authority is runtime state; SQL/server retain durable acceptance and execution. Shared client Workflow hydration extends existing owners; language SDKs do not acquire durable behavior. |
| Cohesive concrete owner and synchronous pure work (`AGENTS` §§5–6; Rust core) | PASS | `BackgroundAuthority` owns the issuer; existing queue/Caller/telemetry/outbox owners carry state. Attribution projection, schema fingerprinting, policy checks, and Arrow preparation stay synchronous. New async paths compose existing IO. |
| Actual SQL capability signatures/fields, including paths outside a scanner's proof (agent rules) | PASS for materially changed production paths | `VerifierRunQueue`, Card writes and lookups, service-account/key functions, and OLAP catalog writes accept `&mut TenantConn<'_>`; queue cross-tenant inventory and built-in reconciliation use `OperatorPool`. `BackgroundAuthority::resolve` borrows a `TenantConn` and does not commit it. No new production raw-pool signature/field was found. Existing fixture assertion/migrator pools and test-only pool construction are not new serving capabilities. |
| RLS and transaction ownership; PostgreSQL coordination clock (`AGENTS` §15; agent rules) | PASS | Tenant writes/read joins remain RLS-bound, with no newly added hand-written tenant filter. Scheduler/observation/service owners acquire and finish their own transactions; borrowed SQL helpers do not end them. Queue due/claim/retry logic retains statement-clock predicates. Creation-pair values do not become clocks. |
| Audit is permission content, one nonblocking process outbox (`AGENTS` §2; agent rules) | PASS for architecture/structure | Auth uses `AuditStage`; server decision, capture, result, and span writes use existing `ScribeOutbox`/`ScribeSink`. Audit content identity is retained by `CorrelationPolicy::None` rather than replaced by transport identity. No second durable audit store, sink, or synchronous audit-result requirement is added. |
| Managed schema identity, name mapping, policy preservation (agent rules; Arrow reference) | PASS for structure | One managed-column declaration expands the pair; source fingerprint, user field and array projection use `CorrelationPolicy`; builtin physical definitions and schema assertions change together. Existing name-based schema checks precede canonical assembly. Sampling changes identity to subject without adding a sampling algorithm. |
| Typed public errors and declarations (`AGENTS` §§4,8–9; errors/agent-harness references) | PASS with reported codegen proof | Derive-backed public codes remain the source. Changed reserved-column errors retain their stable codes. Generated TS wording matches `BifrostTableDescription` rustdoc. No changed native Python signature creates an unimplemented `.pyi` obligation; no changed production napi signature creates a missing declaration change. |
| Runtime ownership and test homes (`AGENTS` §11; agent rules) | PASS | Rust authority logic stays Rust-tested; Python/TS stories use their real public clients. New Python tests are top-level functions; `AttributedRow` is a data model, not a test class. Heavy stories extend existing PG/journey targets; ignore attributes select existing gated journey lanes and are exercised explicitly. |
| Test integrity and exact command policy | PASS within reported evidence | Original V1–V11 and R1 P1–P15 carry package/target/selectors and PG setup. Test repairs have recorded diagnoses. The new expiry test polls the server's observable refusal with a bounded deadline; it does not claim that a fixed sleep proves expiry. No gate suppression, disabled required test, dependency or feature expansion was identified. |
| Every new/materially changed Rust item has substantive rustdoc, error/panic sections (`AGENTS` §16; agent rules; Rust core Documentation) | **FAIL** | `REPO-R1-001` below. Prior enumerated persistence items are corrected; new/remaining cumulative gaps still fail the same explicit rule. |
| Verification appropriate to the changed surface (`AGENTS` §§11–12) | PASS as reported evidence, not rerun | R1 now records docs gate, served verification OpenAPI selector, and three support-desk journey equivalents in addition to format/lints, dependency/tenancy and codegen. `mise.toml` confirms codegen does not prove served OpenAPI; separate P12 does. Broad integrated capability suites remain change-review work as explicitly required by the task. |

## Material proposed finding

### REPO-R1-001 — Required changed-item rustdoc is not fully closed

- Classification: **VIOLATION**.
- Stable closure boundary: continue `FIND-TASK-001-8`; do not create separate style-only remediation rounds or assign a new stable finding number solely for these additional locations.
- Rule: `AGENTS.md` §16 requires rustdoc for every new or materially modified Rust item, including private items, constants and tests, and `# Panics` whenever a panic remains possible. Agent rules explicitly repeat this as a hard gate; trait implementation inheritance does not exempt changed methods.

| Exact candidate location | Material cumulative change and complete-body evidence | Missing contract |
|---|---|---|
| `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:57`, `AuditLogTable::arrow_fields` | The body replaces the old single audit principal/kind with the subject/kind/actor content fields; it returns the entire audit content schema, including the Variant detail and decision timestamp. | No item rustdoc. Explain that these are decision identities retained as content under `CorrelationPolicy::None`, with the managed physical envelope added separately. The struct-level documentation is useful but does not satisfy the explicit per-item requirement. |
| Same file, line 75, `AuditLogTable::physical_layout` | The body changes the audit Bloom identity to `sub_principal_id`; it returns daily decision-time layout sorted by `created_at`. | No item rustdoc explaining that decision-time layout and subject/resource/operation pruning responsibility. |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:3255`, `reloaded_authority_cannot_widen_gateway_access` | Entirely new R1 test; its full body builds the real server/provider, accepts delegated work, edits a grant, waits for observed expiry, claims stored authority and checks the gateway/audit. The model construction inside `call` uses `.expect("the model reference is valid")` at line 3362. | Has `# Errors`, but lacks `# Panics` for its retained model-reference invariant. |
| Same file, line 3256, inner `MODEL_ROLE` constant | Entirely new R1 constant identifying the model role granted after acceptance and held by the positive control. | No rustdoc, despite the explicit constant/private-item rule. |
| `crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs:5240`, `new_and_existing_tenants_receive_every_builtin` | Materially extended in R1 with a platform-principal lookup and creator-pair assertions across provisioning/restart/reconciliation. Its full body uses fixture/request/ID `.expect` calls and asserts the durable builtin inventory and immutable pairs. | No `# Panics` section for fixture/parse failures or unmet durable expectations. The new provenance explanation does not describe its panic conditions. |

Consequence: the required maintainer documentation at the changed schema owner and new/extended proof paths remains incomplete, so the repository's explicit completion gate is unsatisfied. No production failure is alleged by this finding.

Smallest correction: document these existing items accurately in place. Preserve production behavior, schema fields/layout, test assertions, expiry configuration and runtime coverage. Reuse nearby documented `DomainTable` methods in `tables/verification/results.rs` and the already corrected tests' `# Panics` style. Do not introduce a scanner, new test, wrapper, or fixture to close a documentation defect.

Focused closure: compare each added rustdoc with its complete body and run formatting/whitespace validation for that documentation write set. Runtime reruns do not establish rustdoc completeness and are unnecessary for documentation-only corrections.

## Prior documentation closure and remaining notes

The original `FIND-TASK-001-8` locations are corrected in current source: five Card registration lookup/write functions now document errors and transactional/absent/conflict semantics (`register.rs:157,192,226,260,298`); `ParsedCardRow::try_from` (`row_types/cards.rs:133`) documents decoding failures; the runtime round-trip, OLAP creator, and registration replay tests (`request_context.rs:322`, `pg_olap_catalog.rs:88`, `pg_cards_register.rs:244`) now document panics. `components/cards/service.rs::replay` also gained its fallibility contract. Those specific gaps stay closed; the finding above concerns additional genuinely changed items.

Nonblocking documentation consistency notes to batch with any required correction:

- `architecture/wyrd-design.md` doctrine still describes generic telemetry using managed `principal_id`, although the dedicated observation section defines the pair.
- `architecture/wyrd-security-posture.md` still says capture rows carry `GATEWAY_CAPTURE_PRINCIPAL` and retains the internal result-token paragraph under Access and refresh tokens. These descriptions contradict the new tokenless/pair paragraphs and source; current accepted-authority code does not mint those tokens. This report does not infer a runtime/security defect from that prose. The approved spec itself calls for replacing obsolete result-token prose.
- New permanent rustdoc includes change-local identifiers (`FIND-TASK-001-6` in `verification/authority.rs`, and `AC-013/017, FIND-TASK-001-7` in the new authority-reload test; `AC-019` in the provisioning test). Agent rules prohibit plan/task/agent references in permanent code; remove these parentheticals when batching documentation. This has no independent behavioral consequence.
- Several new field/signature sites use fully qualified type paths rather than module imports (for example `ServedSubject` fields). This is the agent-rule import style issue, with no demonstrated security or behavioral consequence; it does not independently block acceptance.

## Verification limits

The initial and R1 execution records report the focused Rust/SQL/SDK/MCP journeys, pure authority tests, workspace format/Clippy, codegen, dependency/tenancy, Python/TS format/lint/typechecking and R1 docs/OpenAPI/example proof as green. The R1 record explicitly includes the current served verification-contract selector and the three public SDK support-desk equivalents, closing the earlier missing-evidence categories. Source confirms those equivalents exercise the changed example functions.

This reviewer performed static source and diff checks only. No absent evidence is asserted to be a runtime failure, and no reported green command is represented as independently rerun. No broad runtime suite, cloud test or performance qualification was attempted; those are not required to fix the documentation finding.

Skipped runtime/build reruns. The remaining standards risk is the explicit rustdoc gate above; current security prose also needs consistent wording.
