# Repository standards review — TASK-003 r2

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved authority: `SPEC-skald-workflow-runtime`, Revision 12, including the human-approved 2026-10-03 addition of `pub model: ModelRef` to `WyrdGatewayCall`
- Task authority: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`, including the approved same-spec native-401 wording correction
- Review mode: source, cumulative diff, repository authority, and recorded implementer evidence only. No build, test, Cargo, mise, package-manager, lint, format, test-listing, or other verification command was run.

The candidate identity remained unchanged throughout this review.

## Authority coverage

| Changed surface | Applicable authority inspected | Coverage result |
|---|---|---|
| Shared Workflow facade, remote handle, gateway caller, HTTP transport, and local dependency composition | `AGENTS.md` §§2–6, 9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client model and Workflow sections; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; error and testing references | Complete; two source-conformance findings below |
| `GlobalConfig.workflow` and external gateway binding selection | Revision 12; TASK-003 shared local execution section; `AGENTS.md` ownership/configuration rules; architecture patterns | Complete; ownership and selected-secret timing conform |
| Shared secret reader and gateway/server consumers | `AGENTS.md` §§3–4, 15–16; security posture secret rules; Rust secret guidance | Complete; redacted public boundary, open-handle checking, size bound, and blocking-pool use conform |
| Skald route planning and `WyrdGatewayCall.model` amendment | Approved human amendment; `AGENTS.md` Skald ownership and struct-centered rules; Rust core guidance | Complete; approved model field and Skald ownership conform, apart from the import-shape finding |
| `wyrd-spec` fallback-header contract | `AGENTS.md` §§2–3, 7–9, 16; doctrine; agent-harness and error references | Complete; pure typed contract remains IO-, async-, and PyO3-free |
| Public gateway ingress, authentication ordering, fallback validation, and served OpenAPI | `AGENTS.md` §§2, 9, 11; security posture; agent-harness; errors and testing references | Complete; server ownership, auth-before-header interpretation, bounds, and typed error projection conform |
| Python shared-Workflow projection and context-retention journey | `AGENTS.md` §§2–3, 7–8, 11, 16; PyO3 and Python API references | Complete; Python retains the shared owner and uses the shared runtime bridge |
| Rust SDK projection | `AGENTS.md` §§2–3; client pattern; Rust core guidance | Complete; thin re-export remains Python-free by source and recorded boundary evidence |
| Tests and recorded evidence | `AGENTS.md` §§11–12, 16; testing-workflows and implementation-execution references | Complete; test placement is earned, but mandatory documentation is incomplete |
| Manifests and lockfile | `AGENTS.md` dependency-cost, feature, and client-tier rules | Complete; no new feature, wildcard, profile block, SQL/cloud/DataFusion client dependency, or unsupported checker/setting was added |

## Applicable rule results

| Rule | Result | Source evidence |
|---|---|---|
| `wyrd-client` is the sole shared SDK-facing implementation; language SDKs project it | PASS | `wyrd-client::Workflow`, `Workflows`, and `PublicWyrdGatewayCaller` own the Rust behavior; Python stores `wyrd_client::Workflow`; Rust SDK only re-exports |
| Skald owns reusable Workflow/provider routing; server owns durable auth, policy, audit, and gateway dispatch | PASS | Route planning remains in `skald-workflow`; public calls enter existing authenticated gateway ingresses and `GatewayInvocation` |
| `wyrd-spec` remains pure, synchronous, and PyO3-free | PASS | Only typed header constants/codec and the approved model field contract were added there |
| Stateful workflows use cohesive dependency-owning structs | PASS | `Workflow`, `Workflows`, `PublicWyrdGatewayCaller`, `SelectedRoutes`, and existing server owners have cohesive responsibilities; no zero-sized utility owner or single-implementation trait was added |
| Async exists at IO/composition boundaries and blocking file IO is moved to the blocking pool | PASS | Network methods await transport; secret file reads use `spawn_blocking`; pure fallback parsing and route planning remain synchronous |
| Secrets use redacted wrappers and do not enter Cards, diagnostics, traces, or portable errors | PASS | Public reader returns `SecretString`; native refusal normalization uses catalog/category text and retains only the permitted field |
| Authentication and tenant authority remain server-owned; header interpretation follows authentication | PASS | Each handler parses fallback only in the authenticated `Ok(caller)` branch; no request field selects tenant or principal |
| Public errors reuse the derive-backed Wyrd catalog and established provider categories | PASS | Recognized native codes reconstruct catalog title/remediation; uncoded statuses use `ProviderError::from_status`; no second catalog was added |
| Imports belong at module top and signatures use imported bare type names | **FAIL** | `STD-R2-001` |
| Every new/materially modified Rust item has complete rustdoc, including panic and async/durable cancellation or partial-progress behavior | **FAIL** | `STD-R2-002`, `STD-R2-003` |
| External tests earn separate binaries; public capability proof includes appropriate HTTP/server/Python journeys | PASS | `workflow_transport.rs` exercises a real HTTP surface across client/Skald contracts; server PG/OpenAPI tests use the real server environment; the Python test is a public SDK-to-server journey |
| Generated artifacts are not hand-edited and OpenAPI is proved through the served document | PASS from source and recorded evidence | No generated stub/schema edit is in the diff; the candidate adds a served OpenAPI contract test and records `codegen:check` plus the principals integration lane |
| No unsupported mechanism, check, file, setting, or option was introduced | PASS | The fallback header and `workflow` config are explicitly approved contracts; the stdlib recording HTTP server and existing test fixtures are ordinary established mechanisms; no new gate/check/allowlist was added |

## Material findings

### STD-R2-001 — Fully qualified types remain in changed signatures and fields

- Classification: **VIOLATION**
- Governing rule: `architecture/agent-rules.md` requires imports at module top and bare type names in struct fields, function parameters, return types, trait bounds, and `where` clauses.
- Locations:
  - `crates/shared/wyrd-client/src/workflow/mod.rs:183` — `wyrd_loader::LoadedTree` in `load_bundle`'s return type.
  - `crates/shared/wyrd-client/tests/workflow_transport.rs:100,387` — `wiremock::Request` in helper return types.
  - `crates/skald/skald-workflow/src/route.rs:286` — `skald_spec::Prompt` in `resolve_route`'s parameter type.
  - `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:5329,5538` — `axum::response::Response` in a method return and `wiremock::Request` in an explicitly typed closure parameter.
  - `sdks/wyrd-sdk-python/src/workflow.rs:213,577` — `wyrd_client::Workflow` in the `PyWorkflow` field and `From` parameter.
- Evidence: all cited spellings are additions in the base-to-candidate diff. They hide dependencies outside the owning module import blocks and directly contradict the mandatory source-shape rule. The remediation fixed the earlier function-local imports but introduced or retained these separate qualified signature/field paths.
- Consequence: the changed modules do not satisfy the repository's required dependency-manifest style, and later ownership/import review must search bodies and signatures rather than reading the top import block.
- Smallest testable correction: import `LoadedTree`, `Request`, `Prompt`, `Response`, and the shared client `Workflow` under unambiguous bare names at the corresponding module tops, then use those names in the cited signatures/field. If the Python module needs both Skald and client Workflows, alias them at import time (for example, `SkaldWorkflow` and `ClientWorkflow`) and use the aliases consistently. Add no checker, allowlist, feature, wrapper, or runtime change.

### STD-R2-002 — New Rust tests and helpers omit required panic documentation

- Classification: **VIOLATION**
- Governing rule: `AGENTS.md` §16 and `architecture/references/languages/rust-core.md` require substantive rustdoc for new test helpers and test functions and a `# Panics` section whenever a panic remains possible.
- Locations and evidence:
  - `crates/shared/wyrd-client/tests/workflow_transport.rs:31-47,51-52,74-80,100-105,109-284,287-295,313-329,333-336,357-373,387-392,413-866` contain `expect`, `assert!`, `assert_eq!`, or explicit `panic!` but their item docs do not state the panic conditions. The same file correctly documents `catalog_problem` and `problem` with `# Panics`, showing the applicable local form.
  - `crates/wyrd-spec/src/gateway/policy.rs:624-657` is a new test with multiple `expect`/assertion panics and no `# Panics` section.
  - `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:5417-5427` is a new helper whose two `expect` calls can panic, but its docs omit `# Panics`; the owning test immediately below correctly documents its panic contract.
- Consequence: the candidate violates the repository's explicit hard-blocker documentation standard and leaves fixture invariants undisclosed at the exact helpers/tests maintainers will modify when wire contracts change.
- Smallest testable correction: add concise `# Panics` sections naming the fixture/serialization/assertion invariants for the cited new helpers and tests. Do not replace the existing tests, add a harness, add a lint/check, or refactor runtime code merely to avoid test-only panics.

### STD-R2-003 — Public durable client operations do not document cancellation and partial progress

- Classification: **VIOLATION**
- Governing rule: `AGENTS.md` §16 and Rust core documentation guidance require async or durable operations to document relevant cancellation, partial-progress, idempotency, and retry behavior.
- Locations:
  - `crates/shared/wyrd-client/src/workflow/remote.rs:41-59` documents create idempotency/retry but not that dropping/cancelling the future after dispatch can leave an accepted server run whose snapshot was not received.
  - `crates/shared/wyrd-client/src/workflow/remote.rs:72-87` does not state that dropping/cancelling the cancellation request after dispatch can leave server-side cancellation applied even though the caller receives no snapshot.
  - `crates/shared/wyrd-client/src/transport/http.rs:346-400` states send-once and 401 renewal, but does not state the partial-progress boundary when its future is dropped by the public caller's timeout/cancellation race after the gateway may have accepted or dispatched the model call.
- Evidence: these are new async network/durable operations. `Workflows::wait` in the same module explicitly documents drop behavior, while the create/cancel methods do not; `post_native` is the one-shot transport whose ambiguous-dispatch behavior motivated the R1 correction.
- Consequence: maintainers and public Rust callers cannot determine from the owning APIs whether cancellation is a local IO stop or a server rollback guarantee. That ambiguity is especially material for run creation, cancellation, and non-idempotent model dispatch.
- Smallest testable correction: extend the existing rustdoc only: state the post-dispatch cancellation/partial-progress behavior for `create`, `cancel`, and `post_native`, preserving the current idempotency, retry, renewal, and send-once behavior. Add no cancellation protocol, provenance marker, option, setting, retry mechanism, or test harness.

## Verification evidence assessment

The implementer recorded focused and broader evidence in the original TASK-003 packet and the R1 remediation: the two exact `wyrd-client` transport tests, selected local dependency test, fallback codec test, gateway secret tests, Python retained-context journey, shared/gateway/server/Python/Rust-SDK/TypeScript families, served OpenAPI integration, code generation, client-tier/SDK-tier/PyO3 boundaries, formatting, lints, and diff whitespace checks. The reports identify passing counts and exact focused commands for the R1 behavioral closures.

This standards review did not rerun any of that evidence. The source findings above are not contradicted by the recorded green commands: ordinary formatting/lint/test lanes do not enforce the repository's bare-signature import rule or the substantive panic/cancellation documentation requirements. No missing required reviewer report was part of this delegated standards role.

## Overall result

**FAIL**

The implementation otherwise follows the applicable ownership, security, contract, error, SDK, dependency, test-tier, and no-extra-mechanism rules, but the three mandatory source/documentation violations above prevent repository-standards acceptance.
