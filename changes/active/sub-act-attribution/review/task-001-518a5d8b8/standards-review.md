# Repository standards review

Overall result: **FAIL** — one consolidated required-rustdoc finding, `REPO-001`.

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Approved authority: `changes/active/sub-act-attribution/spec.md` r4; original task `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md`.
- Reviewed the cumulative range beginning with the initial implementation, not only the final Workflow/timeline edits. The range has 222 changed paths. Used CodeGraph first for request context, Caller, run queue, managed stamping, and tenant telemetry; expanded through the actual diff and surrounding owners, production SQL signatures, contracts, language projections, tests, and verification record.
- This report evaluates repository standards. Task behavior, security correctness, and recovery acceptance remain the independent implementation/domain/system reports' responsibility. No other reviewer conclusions were supplied or used.

## Authority coverage

All rows are also governed by `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the reference router. The applicable sections of the governing architectures cover client/server ownership, runtime identity, delegation, observations, audit, Verifiers, Workflow execution, registry replay, Bifrost ingest, physical schema identity, query authorization, and private peer transport.

| Changed surface | Additional applicable authority | Source and consumer coverage |
|---|---|---|
| Approved packet and appended evidence | `references/languages/spec-driven-development.md`, `implementation-execution.md`, `testing-workflows.md` | Original task, execution evidence/diagnoses, immutable range, prerequisite review whitespace edits |
| Shared Rust runtime, client, queue, telemetry | `references/languages/rust-core.md`, `maintainer-style.md`, `errors.md`; `references/architecture/patterns.md`; `references/doctrine/architecture-constraints.md`; `references/domain/telemetry-observations.md` | `AcceptedAuthority`, `delegated_attribution`, client Observe/state/TableConfig, reserved row preflight, `TenantSpanRoute` and operation subtree rebinding, fixtures |
| Pure public contracts and internal protobuf | `references/languages/errors.md`, `agent-harness.md`; `wyrd-security-posture.md` | `auth/attribution.rs`, verification requests/status/targets, operator template payload, audit/gateway contracts, managed names/error messages, capture-peer protobuf and receiver |
| Server authentication, creator and audit paths | `wyrd-security-posture.md`; `references/architecture/patterns.md`; `references/languages/rust-core.md`, `agent-harness.md` | Tenant and platform audit constructors, JWT bearer/callback/token issuance/exchange, `Caller`, registration/principal/connection/provisioning consumers, gateway capture/ledger/invocation and peer audit |
| Verification execution and background acceptance | `wyrd-security-posture.md`; `references/domain/evaluation.md`, `drift-monitoring.md`, `telemetry-observations.md`, `analytical-operations-reliability.md` | `BackgroundAuthority`, existing issuer recovery, service/direct/eval/drift/runner/results/cache, scheduler, observation outbox, hosted Workflow preparation and gateway caller |
| Persistent SQL and row projections | `wyrd-security-posture.md`; `references/languages/rust-core.md`; `references/architecture/patterns.md` | Defining auth/Card/registration/binding/run/SYSTEM/connection migrations; deleted incremental migration; `verifier_runs`, Card persistence/parse, service-account projection, operator connections, Vala catalog queries and test fixtures |
| Bifrost schema, ingress, sampling and query attribution | `bifrost-design.md`; `references/domain/olap-serving.md`, `arrow-analytical-interop.md`, `telemetry-observations.md`, `analytical-operations-reliability.md` | Gate `AuthContext`/dispatch, `ScribeIngressFrame`, ingress/preprocess/CPU decode, managed arrays/policy/fingerprints, subject sampling, built-in table schemas/projections/layout/Bloom floor, catalog, Oracle `ServedSubject`, outbox frames/peer transport |
| Rust integration/journey tests and helper updates | `references/languages/testing-workflows.md`, `rust-core.md`, `maintainer-style.md` | SQL PG tests; server PG tests; Bifrost fixtures/recovery/peer tests; V1–V8/V11 server/MCP/Rust SDK stories; existing owner tests and exact selector record |
| Python package/tests/example | `references/languages/python-api-and-stubs.md`, `testing-workflows.md`, `maintainer-style.md` | Authored TableConfig docstrings; public `wyrd` imports; managed-row and timeline tests; gateway/realtime query updates; support-desk typed result field/query |
| TypeScript projection/tests/example | `references/languages/typescript-guide.md`, `testing-workflows.md`, `errors.md`, `maintainer-style.md` | Generated Card contract comment aligned with Rust source; managed-row and timeline tests; native-testing mechanical caller update; support-desk query/result projection |
| Architecture and docs site | `references/languages/spec-driven-development.md`, `maintainer-style.md`; relevant product/security/Bifrost authorities | Updated managed names, Card roles, audit queries, MCP target example, verification diagram and all three support-desk examples |

There is no new public PyO3 or N-API operation, class, lifetime handling, manifest dependency, Cargo feature, UI behavior, external egress implementation, planner, storage engine, or compaction algorithm in this range. Their specialized implementation rules do not create an additional work item merely because existing consumers compile against changed contracts.

## Applicable-rule results

`PASS` below means no material standards defect identified from source; it does not substitute for other reviewers' acceptance findings. Verification claims are reported evidence, not rerun results.

| Rule | Result | Evidence |
|---|---|---|
| Server owns durable behavior; shared contract/client boundaries (`AGENTS` §§2–3,9) | PASS | Attribution is a pure `wyrd-spec` value; credential-free accepted authority is runtime state persisted by `wyrd-sql`; server owners accept/execute it; SDK edits project wire names and existing shared-client behavior. No server state or SQL implementation moved into SDKs. |
| Existing concrete owners and synchronous pure work (`AGENTS` §§5–6) | PASS | Extends `Caller`, `VerifierRunQueue`, existing issuer recovery, `ScribeSink`, `GatewayInvocation`, `TenantRoutes`; pure attribution, schema policy, fingerprint and frame construction remain synchronous. New `BackgroundAuthority` retains a real issuer dependency. No new single-implementation trait or runtime is introduced. |
| SQL capability and transaction ownership (agent rules; `rust-core` Postgres section) | PASS | Production changes use `&mut TenantConn<'_>` in run queue, registration, auth account/key and catalog mutation; cross-tenant catalog/operator reads retain `&OperatorPool`. `BackgroundAuthority::resolve` composes on borrowed `TenantConn` without commit. Scheduler/outbox/service/fixture owners acquire and commit their own connections; no added raw `PgPool`, naked connection, or caller-passed SQL transaction. Vala callers retain tier re-exports. |
| RLS remains tenant boundary, PostgreSQL owns coordination clock | PASS | Changed tenant queries continue relying on `wyrd.current_tenant()`/RLS; pair-scoped registration/run keys extend existing constraints; existing statement-clock lease/schedule logic remains. No added manual tenant filter on borrowed tenant paths. |
| Audit remains one nonblocking write path (`AGENTS` §2; agent rules) | PASS, standards architecture only | Lower auth owners stage through existing `AuditStage`; server audit/capture/results/span rows go through the existing `ScribeOutbox`/`ScribeSink`; no second durable staging table, relay, or sink added. Whether every required refusal produces correct content is a behavior/domain question. |
| Managed columns and schema-owned Arrow projections | PASS, standards structure only | Single managed-name source expands the pair; built-in schemas and physical managed stamping align; audit policy explicitly keeps its own identity content; existing sampling owner uses subject; schema-aware projection code checks physical identity before positional assembly. |
| Typed public errors and generated contracts (`AGENTS` §§4,8–9; agent rules) | PASS from source; runtime schema proof limited | Existing derive-backed error codes retained; no handwritten parallel catalog added. TypeScript generated comment matches its Rust source. `codegen:check` is recorded passed. No authored Python native signature changed, so no absent `.pyi` change is inferred. No public native TypeScript declaration change requires `ts:napi:check` here. |
| Language-runtime ownership (`AGENTS` §§7–8,11) | PASS | Rust-only authority/state tests stay Rust; Python/TS managed-row and timeline behavior runs through their public SDKs and real server fixtures; no Python or Node lifetime is emulated inside Rust. Python tests are top-level functions. |
| Test homes, fixtures and gating (`AGENTS` §11; agent rules) | PASS | New heavy scenarios extend existing server/SDK/PG targets and repository-managed fixtures; journey ignores are lane selection exercised by `--run-ignored=all`, not newly disabled required tests. No credentials required for new pure unit tests. |
| Exact commands and test-integrity evidence | PASS with limits below | Task records package/target/exact selector commands and Postgres wrapper. Diagnoses record why V1/V2 setup, V6 expectations, V8 Workflow source, V10 constructor call and V11 span classification changed. No new timeout/sleep or lint-gate bypass was identified in the changed logic. |
| Every materially changed Rust item has substantive rustdoc, including fallibility/panics (`AGENTS` §16, agent rules, `rust-core` Documentation) | **FAIL** | `REPO-001` below. Compiler/lints are not a proof of this rule; `deny(missing_docs)` does not enforce `# Errors` or cover inherited trait methods. |
| Docs/examples/served OpenAPI verification (`AGENTS` §11) | UNVERIFIED | Required affected lanes are absent from recorded commands; see evidence limits. This is missing proof, not a claim that those lanes fail. |

## Material proposed finding

### REPO-001 — Required rustdoc is incomplete on changed persistence operations and new tests

- Classification: **VIOLATION**.
- Governing authority: `AGENTS.md` §16 requires rustdoc on every new or materially modified item, `# Errors` naming fallible conditions, and `# Panics` where a panic remains possible. `architecture/agent-rules.md` repeats this as `BLOCK_BEFORE_MERGE`. The task-review skill expressly keeps missing rustdoc blocking.
- Exact changed locations:

| Item | Candidate location | Missing contract |
|---|---|---|
| `lookup_existing_operation` | `crates/wyrd/wyrd-sql/src/queries/cards/register.rs:144` | Only a one-line lookup description; no `# Errors`, nor the non-expired filter/absent-result/borrowed-transaction behavior. |
| `lookup_expired_operation` | same file, line 168 | No `# Errors`; clarify absent result and stable expired lookup within caller transaction. |
| `lookup_operation_by_id` | same file, line 192 | No `# Errors` for database/unavailable mapping after its row projection changes. |
| `insert_registration_operation` | same file, line 211 | No `# Errors`; no explanation that pair/key conflict yields `false` and caller owns commit/rollback. |
| `insert_card_row` | same file, line 235 | No `# Errors` for missing space, unpinned version, serialization and database failure; no transaction/partial-progress contract for changed creator persistence. |
| `ParsedCardRow::try_from` | `crates/wyrd/wyrd-sql/src/row_types/cards.rs:122` | No rustdoc at all on materially changed conversion; no `# Errors` for invalid stored kind/IDs/version/spec/labels/annotations/status. |
| `accepted_authority_round_trips_without_credential` | `crates/shared/wyrd-runtime/src/request_context.rs:317` | New test uses `expect` and assertions without `# Panics`. |
| `registration_keeps_its_creating_subject_and_actor` | `crates/vala/vala-sql/tests/integration/pg_olap_catalog.rs:84` | New test uses fixture/SQL `expect` and assertions without `# Panics`. |
| `subject_actor_creation_and_replay` | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_register.rs:239` | New test uses fixture/SQL `expect` and assertions without `# Panics`. |

- Source evidence: full candidate bodies and cumulative hunks show changed SQL lookup/write keys and changed row conversion. The new tests are additions in the initial cumulative range. These are not untouched neighboring helpers. In contrast, `capture_peer::ingest_capture` is identical to base and is expressly excluded despite its old documentation gap.
- Consequence: required maintainer-facing documentation omits failure/transaction/replay behavior at the changed durable owner, and the repository's explicit hard completion gate is unsatisfied. This finding does not allege a runtime behavior failure.
- Smallest testable correction: add accurate rustdoc on these changed items in their existing modules, explaining actual invariants, errors, borrowed-transaction cancellation/partial progress where relevant, and test panic conditions. Reuse the nearby `auth_projection::upsert_service_account_from_card` and `VerifierRunQueue` documentation style. Keep implementations and test assertions unchanged; do not introduce a rustdoc scanner or permanent check.
- Closure proof: source-inspect the enumerated rustdoc against complete function bodies; formatting/whitespace check for the documentation-only write set. No new runtime test or broad suite is warranted for documentation alone.

## Verification evidence and limits

The task records V1–V11, SQL tests, server gateway/workflow/audit owners, format/Clippy, workspace check, generated code, dependency/tenancy checks, Python format/lint/typecheck and TypeScript format/lint/typecheck as passing. This reviewer did not run runtime, Cargo, codegen, package, or docs commands; reviewed selectors and source instead. Those results are credible reported evidence within their scopes, not independently reproduced results.

The execution record does not show:

- `mise run docs:check`, required by `AGENTS` §11 for changes under `docs/`;
- the touched support-desk example tasks (`examples:python:support-desk`, `examples:rust:support-desk`, `examples:typescript:support-desk`) or a recorded equivalent covering those example edits;
- `mise run test:principals:integration` served OpenAPI contract evidence after public verification request/status/target fields changed. `codegen:check` does not cover the served document.

These are absent verification, not observed production defects and not an assertion that the commands fail. Preserve these limits when reconciling final acceptance; obtain the narrow required proof or establish an already-run equivalent before claiming complete verification. `ts:napi:check` is conditional on generated native declarations changing; the one mechanical native-testing change does not establish such a change.

## Non-blocking notes

1. Architecture prose still describes the old attribution in a few places: `wyrd-design.md` doctrine generic telemetry text uses `principal_id`, and its principal-model/capture prose plus `wyrd-security-posture.md` capture paragraph still says captured rows carry `GATEWAY_CAPTURE_PRINCIPAL`. The changed pair sections and source now say otherwise. Consolidate this stale wording with bounded documentation corrections; it does not independently establish a runtime defect.
2. `sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs:641` embeds `AC-011` in permanent rustdoc. Agent rules disallow plan/task references in code. Remove that parenthetical when touching the documentation. This has no behavioral consequence and does not independently block acceptance.
3. No recommendation is made to expand APIs, add a migration/backfill, add a second queue/authority resolver, or build a permanent one-time rename check. Those would not follow from this standards finding.

Skipped runtime reruns and build/check execution. The material remaining standards risk is incomplete required rustdoc; docs/examples/OpenAPI proof remains unrecorded.
