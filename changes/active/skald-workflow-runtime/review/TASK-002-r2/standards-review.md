# Repository standards review

Result: **FAIL**

Subject: base `0569b79702218600c4f9790f45cc03100d5c6f1c` → candidate `e7d16b5bd622b9a564a49edb18239df7f209ca92`. HEAD remains the candidate; tracked source is unchanged. This is a repository-rule audit, not task acceptance or an optional Ponytail audit. No current discovery reviewer reports were read.

## Authority coverage

Read the complete governing repository rules and routed references below, plus the owning architecture, manifests, changed source, surrounding consumers and tests. The reference router is `architecture/references/README.md`. No `.codegraph/` exists, so repository navigation used `rg` and Git.

| Changed surface | Governing authority read | Source and consumer coverage |
|---|---|---|
| Shared client Workflow facade, Cards exports and graph hydration (`workflow.rs`, `cards/{mod,hydrate/{mod,graph,workflow}}.rs`, `lib.rs`) | AGENTS §§2–6,9,11–12,15–16; agent-rules; wyrd-design (Client model, Workflow, reference forms, registry lifecycle); wyrd-doctrine; references doctrine/architecture-constraints, doctrine/positioning-and-vocabulary, architecture/patterns, languages/rust-core, errors, maintainer-style, testing-workflows | Workflow::from_path → existing loader → WorkflowBodies → CardGraphHydrator → GraphTraversal → Skald; registered typed view and disk/Service hydration sibling caller; RegistryContext/RegistryEngine and Cards::handle composition |
| Loader validation and bundle test (`wyrd-loader/src/{lib,validate}.rs`) | Same foundational/reference authority; AGENTS §§4–6,11,16; rust-core/testing-workflows | Existing parse/resolve contract and canonical ReferenceSlotVisitor; new pure WorkflowSpec::validate call; complete bundle test and existing registration-input consumer |
| Skald body adapter (`bodies.rs`, lib exports, workflow_surface docs) | AGENTS §§2–6,10–12,15–16; wyrd-design Workflow; architecture/patterns; rust-core/errors/maintainer-style | CardBodyResolver Agent/Prompt seams, from_card_bodies/validate_card_bodies, existing lowering/validate/run_with_options and native load seam; provider/tool manifests and callers |
| Pure contract helper (`wyrd-spec/src/reference.rs`) | AGENTS §§2–4,7,9; wyrd-design reference forms; doctrine references, architecture/patterns/rust-core | InlineableRef::to_durable and canonical slot visitor consumers in client/server/Skald; no wire variant or IO/PyO3 dependency added |
| Server preflight/registration and SQL lock recheck (`cards/{resolve,service}.rs`, SQL relationships.rs) | AGENTS §§2–6,9,11–12,15–16; agent-rules SQL/RLS/audit rules; complete wyrd-security-posture; wyrd-design registry lifecycle; architecture/patterns; rust-core/errors/agent-harness | Caller → resolve_external tenant transaction → EffectiveSpecs → write_registration transaction → recheck_active_card_refs; reference lookup, provenance stores, permission/audit context and expected-UID row locks; SQL caller and regression test |
| Python wrappers/registration/export/source stubs/generated public stubs/examples/tests | AGENTS §§2–8,11–12,16; pyo3-boundaries, python-api-and-stubs, errors, maintainer-style/testing-workflows | PyWorkflow::from_path and From conversion; PyCards.workflow/PyWorkflowCards.load; register_cards; public wyrd.agent/wyrd.cards exports; source stubs and generated projections; save/load unit test and full Cards journey |
| TypeScript napi/source/declarations/journey | AGENTS §§2–6,9,11–12,16; typescript-guide, errors, rust-core, maintainer-style/testing-workflows | NativeWorkflow, NativeWorkflowLoad, selector parser, NativeCards load; public Workflow/WorkflowCards/WorkflowSelector/WorkflowRun; index.d.ts/index.d.cts parity; full Node journey |
| Rust SDK manifest/re-export/journey | AGENTS §§2–6,11–12,16; rust-core/testing-workflows | wyrd_sdk re-export, production dependency remains wyrd-client only; full workflow_loading integration binary and exact ignored selector |
| Cargo.lock and client/server/Rust SDK manifests | AGENTS §§1–4,7,11–12,15; architecture-constraints/patterns | Workspace edition/MSRV and dependency pins; changed dependency edges; SDK Python feature aggregation and napi native manifest; no new feature/version wildcard |
| YAML/JSON example and shared fixtures, README and docs-site edit | AGENTS §§2,9,11–12,16; wyrd-design authoring/reference contract; doctrine/positioning-and-vocabulary; maintainer-style | Every changed example/fixture and its declared references, exact identities, Prompt variables and Workflow bindings; updated public from_path usage and example path |
| Architecture and language reference edits; canonical skills and Claude mirrors; active spec/task/review packet additions | AGENTS §14; complete spec-driven-development and implementation-execution; applicable wyrd-* skill contracts; wyrd-design/doctrine | Approved Revision 12 replaces prior private mechanics and updates authority together; mirrored skill diffs; packet remains tracked in active change namespace. Prior review artifacts are historical evidence, not a competing authority or a basis for this verdict |

Bifrost/Arrow/OLAP implementation, UI, migrations, deployment listeners, provider wire implementations, MCP tool catalog and OpenAPI registrations do not change in this range. Their specialized authority/gates are not substituted for applicable SDK/server rules. No new SSRF fetch path is introduced.

## Applicable-rule results

PASS below means source conformance with the stated proof limits; it does not independently attest every historical command claim.

| Rule | Result | Exact evidence |
|---|---|---|
| AGENTS §§2–3,9: shared Rust client owns SDK composition; server owns durable state; Skald independent of server/Vala | PASS | All new SDK loading paths delegate to wyrd_client::Workflow or Cards::workflow; EffectiveSpecs uses tenant SQL and synchronous Skald validation; bodies.rs imports no server/SQL/client owner |
| AGENTS §2 and design reference forms: canonical Ref/InlineableRef slots; no duplicate wire dialect, new Card kind or credential on Workflow | PASS | reference.rs::to_durable preserves Ref/Sibling provenance; WorkflowBodies and EffectiveSpecs keep sources apart; fixtures use untagged mappings/path strings/native inline bodies; no Workflow principal mutation added |
| AGENTS §4: typed durable IDs and stable derive-backed public errors | PASS | CardRefIdentity, CardUid, CardSelector, SpaceName/CardName at Rust boundaries; WyrdError variants propagated; napi uses NativeWyrdError::from_wyrd, Python uses WyrdPyError; no new parallel catalog |
| AGENTS §§4–5: meaningful ownership, borrowing and explicit dependencies | PASS in compliant owners; FAIL for touched free orchestration | Workflow owns native runtime; WorkflowCards borrows Cards; WorkflowBodies owns provenance-separated bodies; GraphTraversal owns engine/state. Exceptions are detailed in REPO-R2-1 |
| AGENTS §5 and agent-rules: every materially modified Rust workflow must be an inherent operation on its dependency/invariant owner | FAIL | graph.rs:322,350; resolve.rs:49; service.rs:1059 remain free dependency-backed orchestration. REPO-R2-1 |
| AGENTS §6: synchronous pure validation, async only where IO is awaited, shared runtime bridge | PASS | New body checks synchronous; graph loading awaits registry; Python py.detach + wyrd_runtime::runtime().block_on; no runtime creation or Python Bound value retained across await |
| AGENTS §§7–8: PyO3 confined, GIL released for IO, native registration/public exports/stub projection | PASS | New wrappers under SDK src; optional python feature unchanged; PyWorkflowCards::load and PyWorkflow::from_path detach; register_cards and public cards export; source stubs project public signatures |
| AGENTS §§4,9: structured trace/error/secret treatment and typed HTTP contracts | PASS | write_registration retains tracing::instrument with scrubbed args and AuditEvent context; no new secret-bearing value or public untyped handler; reused CreateCardRequest/Response |
| agent-rules and security: TenantConn/RLS, no raw pools in production signatures, callee does not end caller transaction | PASS | resolve.rs and relationships.rs accept TenantConn; no manual tenant WHERE predicate; relationships recheck never commits; state-owning service opens/commits transaction |
| AGENTS §2 and security: audit permission decisions through canonical transactional append; integrity/identity checked at use | PASS | write_registration appends allowed events before recheck/writes; unchanged outer failure auditing remains; SQL locks exact expected UID and active state; replacement journey proves refusal without registration-operation/card writes |
| AGENTS §§4,11–12: no unchecked production unwrap, unsafe bypass, lint suppression or weakened test | PASS | Changed production code propagates catalog errors; new tests assert exact failures and durable outcomes; new #[ignore] is explicitly gated SDK journey, separately executed. Save/load helper now uses unversioned inline Agents because the changed public loader intentionally reads versioned external refs |
| AGENTS §16 and agent-rules: substantive rustdoc and # Errors for every changed fallible Rust operation | FAIL | PyWorkflow::from_path:490 returns WyrdPyResult but has only Python Raises prose; new napi loading entries omit failure/cancellation contract. REPO-R2-2 |
| agent-rules: import types with use and use bare names in signatures | FAIL | native/workflow.rs:37,75 spells std::result::Result in signatures despite required import/alias pattern. REPO-R2-3 |
| AGENTS §11: language lifetime tests run under owning interpreter/runtime; unit tests do not require live credentials | PASS | Python fixture journey owns Python lifetime, Vitest owns Node lifetime; Rust new body behavior is Rust-only; deterministic gateway fixtures do not call live models |
| AGENTS §11: journey/integration/unit tiers and exact focused selection | PASS for existence/routing; breadth not an acceptance verdict here | Rust SDK WyrdTestServer journey and server HTTP/SDK journeys; Python integration marker; TS integration directory; exact parent checks selected 1 client + 3 server + 1 SDK tests. Detailed behavior sufficiency belongs to implementation reviewers |
| AGENTS §§11–12: applicable format/lint/test/boundary/codegen proof | PASS with evidence limits | TASK-002-cleanup records all listed shared/Skald/SDK/Python/TS/codegen/boundary/lint lanes PASS. Current verification.md independently records focused Rust selectors and fmt:check PASS. Historical broad/Python/TS results have no supplied raw logs |
| AGENTS §11: docs-site proof | PASS | Parent independently ran mise run docs:check; log /tmp/wyrd-task002-r2-docs.log, including generated drift/commands/links/build/a11y |
| AGENTS §14: canonical skill mirrors synchronized | PASS | Parent independently ran mise run check:skills-sync; /tmp/wyrd-task002-r2-skills.log |
| AGENTS §§11–12: actual changed example has runnable proof | PASS | Client exact bundle load/run test (1/1), loader bundle test source, server registered bundle run journey (3/3 target) exercise canonical example. Python from_yaml.py is inspection-only and points at that same bundle |
| AGENTS §14, spec-driven development: approved authority precedes material surface/ownership changes; active packet remains available | PASS | spec.md revision 12 approved opening records explicit user approval; design/doctrine updated in same cumulative range; no forced staging, compatibility shim or lifecycle controller added |
| AGENTS §12: review source immutable, no gate weakening | PASS | HEAD candidate and tracked source unchanged; only assigned review directory is untracked; no source mutations by this reviewer |

## Material source-local findings

### REPO-R2-1 — Touched IO orchestration remains outside its concrete owner

Classification: VIOLATION. Governing rule: AGENTS §5, Required Struct-Centered Rust Style: “Public operations and internal orchestration that use an owner's state or dependencies MUST be inherent methods on that owner”; materially changed functional drift is not precedent. The same obligation is explicit in agent-rules and rust-core.

Locations and reachable paths:

- `crates/shared/wyrd-client/src/cards/hydrate/graph.rs:322` (`resolve_graph`) and `:350` (`resolve_refs`). Their full bodies perform registry reads, seed traversal, run it, and return the graph while accepting the RegistryEngine parameter. Every caller is an inherent CardGraphHydrator operation: hydrate at mod.rs:84, resolve_external at workflow.rs:179, and load_workflow at workflow.rs:200. The existing GraphTraversal already owns that exact engine and all traversal state; these are not deterministic conversions or ownerless algorithms.
- `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:49` (`resolve_card_references`) is materially changed to run Workflow preflight and still orchestrates sibling inspection, external SQL resolution and three effective validation phases around EffectiveSpecs. Its caller is service.rs::resolve_external at :1002. EffectiveSpecs already owns submitted bodies, external bodies and resolved references; the workflow is split outside that owner.
- `crates/wyrd/wyrd-server/src/components/cards/service.rs:1059` (`write_registration`) is materially changed to carry the expected-UID write fence but remains a free multi-step audited registration workflow threading AppState, Caller, RegistrationPlan and audit context. Its registration caller at :583 supplies those dependencies; the function opens the transaction, audits, fences, reserves idempotency, inserts nodes/relationships and commits.

Consequence: the candidate violates a hard structural completion criterion, and maintainers must discover and modify the same stateful workflow across owner methods and separately threaded free entry points. Correct leaf-call reuse does not satisfy this rule. This is confined to materially modified symbols; it does not request a crate-wide rewrite of untouched legacy functions.

Required testable correction: put the changed graph resolution workflow on its existing dependency-owning graph/hydration owner, preflight orchestration on its effective-body owner, and changed atomic write orchestration on a cohesive registration owner carrying its real dependencies/invariants. Preserve selectors, provenance separation, traversal scopes, caller-owned TenantConn composition, audit ordering, UID lock fence and commit/rollback behavior. Do not introduce zero-sized utilities or a parallel graph store. Closure proof: source inspection shows inherent owning operations; rerun the exact client load test and three pg_workflow_registration tests plus the SQL lifecycle-lock test and applicable format/lint checks.

### REPO-R2-2 — New loading boundaries omit mandatory Rust error/cancellation documentation

Classification: VIOLATION. Governing rule: AGENTS §16 requires `# Errors` on every fallible changed Rust operation and cancellation/partial-progress documentation when relevant; agent-rules calls incomplete rustdoc a hard blocker.

Locations:

- `sdks/wyrd-sdk-python/src/workflow.rs:490`, new `PyWorkflow::from_path`: returns WyrdPyResult and performs detached filesystem/registry loading. Its rustdoc has Python `Raises:` prose but no mandatory Rust `# Errors` section. The error conditions are known and already documented on the shared client; no new behavior must be invented.
- `sdks/wyrd-sdk-ts/native/src/workflow.rs:105`, new `load_workflow_from_path`, and `sdks/wyrd-sdk-ts/native/src/cards.rs:157`, new `NativeCards::load_workflow`: return the load failure inside NativeWorkflowLoad but document neither how that error result is returned nor cancellation/partial-loading behavior. Their exported generated declarations carry the same incomplete prose. A caller cannot tell from those boundary docs whether a refused load throws a napi error or returns the catalog error field, or whether interrupted loading writes anything.

Required correction: document error conditions and the actual outcome channel on these touched operations, with the mandatory `# Errors` section for the Result-returning Rust method, and relevant no-partial-Workflow/no-durable-write cancellation contract for async loading. Keep Python Args/Returns/Raises aligned; regenerate declarations from source rather than editing generated output. Closure proof: rustdoc/source inspection plus codegen:check and ts:napi:check; no new behavioral test is needed for this documentation correction.

### REPO-R2-3 — New native signatures bypass the mandated import shape

Classification: VIOLATION. Governing rule: agent-rules requires imported bare type names in function parameters/returns, including trait bounds and fields; fully qualified paths belong in the module use block.

Locations: `sdks/wyrd-sdk-ts/native/src/workflow.rs:37` (`NativeWorkflowLoad::from_outcome` parameter) and `:75` (`parse_workflow_selector` return) use `std::result::Result<...>` while the same module imports napi::Result. This is a new module and new symbols, not unrelated old style debt.

Consequence: the dependency/type distinction is spread across signatures instead of the module's dependency manifest, contrary to the explicit repository convention. The existing native lib.rs uses `use std::result::Result as StdResult`, which already resolves precisely this name collision without another abstraction.

Required correction: use the existing imported alias convention in this module and leave napi's boundary Result intact. Closure proof: source inspection and the native compile/lint lane; behavior and generated public declarations must remain unchanged.

## Proof limits and result

The orchestrator supplied independent executable evidence for client loading (1 selected/1 passed), the three server registration/loading/replacement selectors (3/3), and the ignored Rust SDK journey (1/1), plus fmt:check, diff --check, docs:check and check:skills-sync. Broader lanes, Python and TypeScript remain recorded implementation claims, not independently rerun checks. This reviewer ran no expensive build or environment lane and did not mutate source.

All changed language/layer families have an authority route and source coverage. The failed hard repository rules above prevent standards PASS even with the focused behavioral checks green. No task-acceptance conclusion or optional improvement is included.
