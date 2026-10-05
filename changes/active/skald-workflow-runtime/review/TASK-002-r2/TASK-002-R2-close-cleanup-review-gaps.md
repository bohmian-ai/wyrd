---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-002-cleanup
remediates: [FIND-TASK-002-5, FIND-TASK-002-6, FIND-TASK-002-7, FIND-TASK-002-8, FIND-TASK-002-9, FIND-TASK-002-10]
base: 0569b79702218600c4f9790f45cc03100d5c6f1c
reviewed_candidate: e7d16b5bd622b9a564a49edb18239df7f209ca92
---

# Close validated cleanup acceptance gaps

Implementation skill: `$wyrd-implement`. This is one bounded remediation of the cumulative TASK-002 outcome, not another design plan. Read the approved authority and validated diagnosis before editing. Keep the active packet and prior review history; work forward without resetting the candidate.

## Authority and immutable inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Active replacement task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Original superseded task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`
- Original cumulative base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Reviewed candidate: `e7d16b5bd622b9a564a49edb18239df7f209ca92`
- Current verdict/validated ledger: `changes/active/skald-workflow-runtime/review/TASK-002-r2/{verdict.md,findings-validation.md}`
- Prior verdict/ledger/remediation: `changes/active/skald-workflow-runtime/review/TASK-002-r1/{verdict.md,findings-validation.md,TASK-002-R1-close-validated-graph-gaps.md}`
- Repository authority: AGENTS.md; architecture/agent-rules.md; architecture/wyrd-design.md; architecture/wyrd-doctrine.mdx; applicable references from architecture/references/README.md.

Revision 12 replaces old WorkflowLoader/keyed-reference instructions. Preserve corrected provenance and UID fencing. FIND-1/2/3/4 are source-closed; FIND-5/6 retain their original stable IDs because the same obligations remain at new locations. New IDs are FIND-7/8/9/10.

## Diagnosis and selected correction

### FIND-TASK-002-10 — Restore legal authored root version intents

The public registration contract accepts omitted, scoped and pinned root versions; the server allocates a durable pin under its existing version-line lock. New Workflow preflight at `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:430` creates a typed WorkflowCard from original authored metadata. `wyrd-spec/src/card/workflow.rs:1001–1010` requires resolved_pin and therefore rejects omitted or scoped roots before server version resolution. Pure planning already uses `graph_ready_submissions` to provide graph-only metadata, but that projection is not passed into preflight. An otherwise valid singleton Workflow with inline Agent/Prompt bodies reaches the refusal; no ambiguous dependency target is needed. Current pinned-only fixtures miss the regression.

Required outcome: fresh omitted/scoped Workflow roots reach the existing server resolver and return exact registered versions, while invalid resolved graph semantics still refuse before durable writes.

Selected correction: use the existing graph-only metadata projection for the transient Workflow root holder used by EffectiveSpecs declarative preflight. Preserve the original request/submissions for hashing, replay, sibling source selection and persistence. The preflight envelope conversion is the correction boundary; the similarly named persistence conversion is not. Keep non-pin submissions absent from the exact sibling-target cache. Keep durable allocation in the existing locked write transaction and retain exact dependency/registered-load requirements. Do not relax the general WorkflowCard::from_envelope contract or replace authored metadata with a guessed durable pin. A temporary validation pin selects no dependency and is discarded with the validation holder.

### FIND-TASK-002-9 — Put materially changed orchestration on meaningful owners

AGENTS §5 and agent-rules require new/materially changed dependency-backed workflows to be inherent owner operations. Client `cards/hydrate/graph.rs:322,350` exposes changed resolve_graph/new resolve_refs as free IO orchestration, threading RegistryEngine from three CardGraphHydrator callers. Server `components/cards/resolve.rs:49` materially extends free preflight orchestration, and `components/cards/service.rs:1059` materially changes free audited write orchestration with the UID fence. These are not pure deterministic helpers. Green behavior checks cannot waive the structural acceptance rule.

Required outcome: changed graph, preflight and write workflows have clear state/dependency owners without a replacement graph, registration system or broad service refactor.

Selected boundaries:

- Reuse existing CardGraphHydrator for graph entry and dependency-backed root/selector preparation. Keep GraphTraversal as the existing traversal state owner. Existing disk hydration, external composition and registered Workflow loading use that same handle; pure conversions/alias checks may stay free.
- Reuse EffectiveSpecs for reference-resolution/preflight orchestration, including its existing binding, baseline and Workflow validation phases. It owns provenance-separated bodies and resolved UID pairs and borrows the caller's TenantConn. Keep tenant acquisition, commit, audit and durable writes outside this owner; do not convert non-pin roots into exact sibling targets.
- The current write module has no cohesive write owner: RegistrationPlan is a pure domain value and BindingProjector owns frozen binding projection only. Use the minimum private dependency-owning registration writer for the changed atomic write operation, retaining the same call boundary and actual AppState/Caller dependencies. Do not attach IO to RegistrationPlan, create a zero-state utility, expand AppState into a broad operation owner, or relocate unrelated read/list/upload/lifecycle operations.

Preserve root-response reuse, Runtime/Bundle graph scopes, identity/UID/Active/alias/cycle checks, Service inventory/disk publication, caller-owned tenant transaction composition, audit append ordering, request hash/replay, exact UID locking, idempotency race rollback/replay indication, version-line lock, binding/node/relationship writes, atomic commit and existing post-commit upload initialization. This fixes shape at the current owners; it does not change deployed behavior or strengthen replacement machinery.

### FIND-TASK-002-7 — Complete Native and per-language journey proof

Cleanup Outcome and Scenarios 3–5 explicitly require Native execution using existing provider configuration. Current proof at Rust `tests/workflow_loading.rs:17–20,109–126,242–327`, Python `tests/integration/cards/test_cards_crud.py:485–566`, and TS `wyrd/tests/integration/workflow-loading.test.ts:45–113` is weaker. The gateway-routed example loads, but Rust executes registered values only through a fake gateway via as_skald, Python never runs, and TS only expects unavailable gateway. Rust never calls public from_path for the mixed authored case; TS never introduces v2. Collision step IDs cannot prove which distinct body was executed. All current tests can pass with broken public Native dispatch or Python/Node input/result conversion.

Required outcome: each owning-language journey proves public local and mixed authored loading, successful Native run, registration, exact/UID reload and successful pinned run, with outputs/bound values and relevant negative flows.

Selected proof: extend the existing three SDK journeys rather than add another harness. Retain the canonical checked-in gateway example and its declarative loading/registration assertions. Use an explicitly authored Native test variant of the same DAG, Agent/Prompt structure and native Chat payloads. Existing default ProviderRegistry::from_env already includes MockProvider::echo for Custom("mock"), and ProviderRequest::OpenAiChatCompatible supports explicit provider dispatch around the same native request. Existing Python `test_workflow_parameter_injection.py` supplies the source precedent. This seam exercises public default run without a gateway, credentials, new provider/configuration API or TLS harness. Preserve native payload/model/variables and binder semantics; the variant explicitly declares its route and dispatch, with no runtime reinterpretation of the gateway artifact.

Use observable echo/bound content to distinguish sibling and external bodies at the same identity and to distinguish old/new dependencies. Assert successful terminal results, named/intermediate outputs, final bound reviewer values, locked Agent/Prompt identities and retained old output after a newer version is registered. Exercise public mixed automatic refs and missing/denied/inactive/mismatched loading refusals in Rust too, using existing subprocess/environment-isolation patterns rather than unsafe mutation in a multithreaded runtime. Private fixture/environment mechanics remain implementation-owned. Keep loading/registration/refusal free of execution and partial writes. Do not prescribe HTTP localhost OPENAI_BASE_URL: the existing environment parser requires HTTPS. The selected default mock seam avoids that unsupported configuration entirely.

TASK-003 selected gateway/configuration composition and TASK-005 CLI/all-routes proof remain excluded. This finding is missing mandatory proof, not a claim that the existing executor is broken. Distinct-body execution closes the remaining positive provenance proof without reopening its repaired source.

### FIND-TASK-002-8 — Project the TypeScript JSON and canonical run contract

`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1255–1273,1321–1324` accepts Record<string, unknown> rather than the approved Record<string, JsonValue>. JSON.stringify silently drops undefined/function properties or throws for bigint before native validation sees the input. The same public projection marks steps/outputs unknown and exposes only code/message on errors. NativeLifecycleResult serializes the canonical Rust WorkflowRun unchanged; its authoritative fields are in `wyrd-spec/src/card/workflow.rs:723–733,775–790,811–831`. Typed consumers cannot access step status/attempts/text or error details/remediation without unchecked parallel casts.

Required outcome: the public TS signature rejects non-JSON typed inputs and exposes the full canonical snapshot, with correct wire names/nullability.

Selected correction: fix the existing public TS projection. Reuse a JSON-domain type where available; otherwise the ordinary recursive JsonValue alias is sufficient. Project canonical WorkflowStepStatus, WorkflowStepResult and WorkflowRunError, including details/remediation, and use JSON types for open input/output/detail values. Preserve native serialization/execution, lifecycle/catalog error mapping and selector behavior. Do not add a TS input validator, workflow runtime, consumer casts or a second snapshot. Publish declarations through their current owners/generators; do not hand-edit N-API declarations.

### FIND-TASK-002-5 — Complete loading boundary contracts

New `sdks/wyrd-sdk-python/src/workflow.rs:490` returns WyrdPyResult but documents Python Raises without mandatory Rust # Errors. New native async loads at `sdks/wyrd-sdk-ts/native/src/workflow.rs:105` and `native/src/cards.rs:157` omit catalog outcome and interrupted/read-only partial-loading semantics. Maintainers cannot determine the result channel and side effects from these boundary docs. These locations keep the earlier documentation finding open.

Required outcome and correction: reuse the shared Workflow loading contract in item-local rustdoc. Add Python # Errors alongside aligned Args/Returns/Raises. Document that native functions return catalog errors in NativeWorkflowLoad, while public TS converts that envelope to WyrdError. Explain that interruption may follow completed reads, publishes no partial Workflow and writes no durable state. Keep Python registered-load documentation aligned without inventing a synchronous cancellation API or changing the shared runtime bridge. Regenerate affected declarations from source. This is documentation-only; no new behavioral test is needed.

### FIND-TASK-002-6 — Restore the module import convention

New `sdks/wyrd-sdk-ts/native/src/workflow.rs:37,75` uses qualified std::result::Result in live loading signatures, contrary to the mandated module-top import/bare-name rule. napi::Result is independently needed for run. Existing native/lib.rs already imports std Result as StdResult.

Required outcome and correction: reuse that existing alias convention at module scope and in these two signatures, leaving napi Result and all behavior intact. No abstraction, dependency or behavioral test is required. This retains the earlier stable import-policy ID.

## Constraints and non-goals

- Preserve approved Revision 12 public loading APIs and untagged reference forms; no compatibility alias or old normalization.
- Reuse loader/reference visitor, existing Cards graph traversal, EffectiveSpecs, Skald resolver/binder/executor and canonical registration/audit/version owners.
- Preserve tenant/RLS authorization, transactional audit, exact dependency identities, Ref/Sibling provenance and expected-UID Active locking.
- Preserve original authored version intent through hash/replay/persistence; exact registered selectors remain exact.
- Keep Workflow declarative and non-principal; WyrdState stays Service-rooted. No Agent privilege transfer.
- No secret/provider resolution during load or registration; registration binds no execution tools. Execution uses the existing selected/default runtime.
- Keep foundational contracts pure, synchronous, IO-free and PyO3-free; keep foreign-runtime behavior/tests in owning SDKs.
- No new transport, cache, graph/traversal/parser/executor, generic runtime configuration or test framework/dependency.
- No broad conversion of untouched functional code, new lexical/structural scanner, gate suppression, weakened assertion or unrelated cleanup.
- Do not implement TASK-003/004/005, CLI apply or gateway execution preparation to close these findings.

## Ordered proof and implementation discipline

1. Extend the existing registration scenario for legal omitted/scoped root intents and invalid resolved bodies. Run its exact selector: RED must demonstrate the preflight pin refusal on a valid request, then GREEN at the existing preflight projection boundary. Retain provenance/UID negatives.
2. Add the small TS compile-time JSON/snapshot assertions and close the public projection mismatch. Keep native execution unchanged.
3. Extend existing Rust/Python/TS journeys for the selected Native proof and missing language assertions. Work one scenario at a time with expected RED, minimum GREEN and retained earlier proof. Successful Native execution is mandatory; unavailable-gateway assertions remain separate negative evidence.
4. Apply the owner-shape corrections with retained behavioral proof. This structural obligation does not require manufacturing runtime RED or adding a scanner.
5. Complete docs/import obligations and regenerate affected declarations. Use source/declaration parity plus checks, not artificial tests.

Record selected counts, actual expected RED failures, GREEN results and final verification. If a failure requires changing an assertion/timing/skip, follow repository traced diagnosis rules rather than altering it solely to clear a gate.

## Acceptance mapped to findings

| Finding | Required observable/static closure |
|---|---|
| FIND-TASK-002-10 | Fresh omitted/scoped roots register to server-resolved exact versions and reload; invalid resolved bindings under both intents refuse without operation/Card/relationship writes; exact dependencies and authored intent preserved. |
| FIND-TASK-002-9 | Changed IO workflows are inherent operations of meaningful owners at the selected boundaries; no duplicate orchestration or unrelated relocation; existing transaction, audit, locking, replay and Service hydration behavior stays green. |
| FIND-TASK-002-7 | All three public SDK journeys successfully execute local/mixed and registered Native variants, prove output/binder/body-source equivalence, exact/UID identity and pinning after newer bodies; Rust automatic ambient refs and required refusals are covered without unsafe environment mutation. |
| FIND-TASK-002-8 | Public TS nested JSON inputs typecheck, non-JSON inputs fail typechecking; canonical step fields and full error details/remediation are accessible without casts; Native run returns that snapshot. |
| FIND-TASK-002-5 | Every affected loading boundary states true error channel/conditions and no-partial/read-only behavior; Python Rust/Python doc sections align; generated declarations match source. |
| FIND-TASK-002-6 | New native signatures use the existing imported StdResult convention; napi Result and public behavior unchanged. |

## Exact focused closure commands

Use the repository-managed environment and pinned toolchain. Preserve existing test names; extend their assertions. Verify selectors and selected counts. Execute each named test, not only an aggregate.

```bash
mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=tests::load_explicit_workflow_bundle)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=workflow::tests::from_path_uses_existing_loader)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=registers_only_valid_explicit_workflow_graphs)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=refuses_stale_preflight_after_dependency_replacement)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_workflow_registration -E "test(=fetches_and_executes_locked_workflow_graph)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_cards_register -E "test(=relationship_recheck_blocks_target_lifecycle_race)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_cards_crud.py -k test_workflow_loading_journey'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"'
```

The corrected TS selector includes the describe block. The Rust ignored journey is not selected by test:wyrd-sdk and must be run explicitly. Private test fixture/setup mechanics may change without adding a new framework or weakening these observable obligations.

## Broader verification

Run the active cleanup task's complete relevant lanes after focused closure:
`test:shared`, `test:skald`, `test:cards:integration`, `test:wyrd-sdk`,
`py:test:unit`, `py:test:cards:integration`, `py:typecheck`, `ts:test:unit`,
`ts:test:integration`, `ts:typecheck`, `ts:napi:check`, `codegen:check`,
`check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`,
`check:registry-tx-coupling`, `fmt`, `lints`, `py:format`, `py:lints`,
and `git diff --check`, through mise where applicable. Preserve Service and loader regression coverage in test:shared. Run docs:check for touched docs-site content and check:skills-sync if skills are touched; both currently pass independently and need no unrelated edit. Use narrower applicable lanes for iteration; full capability/release aggregate remains TASK-005.

No historical green result substitutes for the new Native or version-intent assertions. No raw live-provider credentials are required. Completion evidence must state exactly which language/result paths ran and what remains outside this task.

## Handoff

Route directly to `$wyrd-implement`. After implementation, review the complete original-base-to-new-candidate range against Revision 12, original/replacement task, both review attempts and this remediation. Do not review only the latest fix diff. Any genuinely new public/authority/security/concurrency/persistence decision returns to specification approval; the selected corrections above need none.

## Implementation Evidence

Commits on `wyrd/skald-workflow-runtime/TASK-002` after reviewed candidate
`e7d16b5bd`: d6e397dfd (FIND-10), b6d184c4c (FIND-8), 7d0a2078d and 05c7d4fab
(FIND-7), a following docs commit (FIND-5/6), 32aa5cfe6 (FIND-9).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-10 | `wyrd-server/src/components/cards/resolve.rs::EffectiveSpecs::validate_workflows` decodes the Workflow from `graph_ready_submissions` holders; authored submissions still drive hashing, replay, sibling cache, persistence | `pg_workflow_registration::registers_only_valid_explicit_workflow_graphs` extended: omitted → `0.1.0`, scoped `"2"` → `2.0.0`, each reloaded exactly; invalid binding under both intents refused at `steps[0].inputs.extra` with no operation/Card writes. RED before fix: `Workflow Card envelope missing resolved version pin` | PASS |
| FIND-TASK-002-9 | `CardGraphHydrator::{resolve_graph,resolve_refs,load_root,resolve_root_selector}` (`wyrd-client/src/cards/hydrate/graph.rs`); `EffectiveSpecs::resolve` replaces free `resolve_card_references`; private `RegistrationWriter { state, caller }::write` replaces free `write_registration` (`service.rs`). Structural only | the three `pg_workflow_registration` tests, `pg_cards_register::relationship_recheck_blocks_target_lifecycle_race`, `test:cards:integration`, `test:shared`, `check:registry-tx-coupling` | PASS |
| FIND-TASK-002-7 | `tests/fixtures/workflow-loading` now the explicit Native variant: `llm_route: native`, Prompts `provider: {custom: mock}` (default `ProviderRegistry` echo mock) with body-naming user messages; `shadowed/local-workflow.yaml` added; Rust fake gateway and its five dev-deps removed; Rust ambient loads run in a child test process (`load_in_child` / `authored_load_child`, wyrd-tls precedent) | All three journeys: local run, mixed authored run, shadowed run (local vs registered body at same identity), registered exact + UID runs after `team-v2` registers, exact outputs and `final_review` text; 401/403/404 authored refusals in Rust, Python, TS; gateway example `run` → `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` in Python/TS | PASS |
| FIND-TASK-002-8 | `wyrd/src/index.ts`: `JsonValue`, `WorkflowStepStatus`, `WorkflowStepResult`, `WorkflowRunError`; `WorkflowRun.outputs/steps/error` typed; `run(input: Record<string, JsonValue>)` | `tests/unit/workflow-types.test.ts` (`@ts-expect-error` for undefined/function/bigint; full snapshot without casts). RED against the old build: 9 tsc errors (missing `JsonValue`, unused expect-error ×3, missing `details`/`status`/`attempts`/`error`/`remediation`). Native run returns the snapshot in the TS journey | PASS |
| FIND-TASK-002-5 | rustdoc on `load_workflow_from_path`, `NativeCards::load_workflow` (result channel, TS throw, abandoned-promise read-only/no-partial); Python `PyWorkflow::from_path` gains `# Errors`, `PyWorkflowCards::load` codes; `stubs/agent.pyi`/`cards.pyi` aligned; declarations regenerated | `ts:napi:check`, `codegen:check`, `py:typecheck` | PASS |
| FIND-TASK-002-6 | `native/src/workflow.rs` imports `std::result::Result as StdResult` (crate convention) for `from_outcome` and `parse_workflow_selector`; `napi::Result` kept for `run` | `lints`, `ts:napi:check` | PASS |

### Observed during remediation

- Registered Workflow loading refused a malformed reference with a code from
  another family in every language: Python `WYRD_DATA_400_VALIDATION` (the
  DataCard helper `WyrdPyError::validation`), TS `WYRD_SPEC_400_VALIDATION`,
  Rust `WYRD_REGISTRY_400_VERSION_REQUIRED` /
  `WYRD_REGISTRY_400_INVALID_CARD_SPEC`. Added
  `WyrdError::WorkflowInvalidCardRef` (`WYRD_WORKFLOW_400_INVALID_CARD_REF`,
  `details.field` names the field), returned by `wyrd-client`
  `WorkflowCards::load` (versionless, wrong kind), Python
  `PyWorkflowCards::load` and TS `parse_workflow_selector` (mixed,
  incomplete, malformed field). RED: Python journey got
  `WYRD_DATA_400_VALIDATION`, TS journey got `WYRD_SPEC_400_VALIDATION`.
  GREEN: Rust, Python, TS journeys 1 passed each; `fmt`, `lints`,
  `codegen:check`, `ts:napi:check`, `ts:typecheck`, `py:typecheck`,
  `py:format`, `py:lints`, `test:shared`, `test:wyrd-sdk`,
  `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope` PASS.
  Not changed: other Python Cards selectors still raise the DataCard helper
  code for non-Data input through `parse_space`/`parse_name`/`parse_version`
  (same defect class, outside this task); `from_path` keeps
  `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for a malformed file.
- `check:workspace-hack` (not in this task's lanes) fails on an `opentelemetry`
  `spec_unstable_logs_enabled` feature drift that is present with or without
  this change's dev-dependency removal. Not fixed here; route to the owner of
  the workspace-hack.

### Material limits

- Native execution uses the default registry's echo mock; no live provider.
  TASK-003 gateway composition and TASK-005 CLI/all-routes proof stay excluded.
- `authored_load_child` is `#[ignore]` and fails fast if started without
  `load_in_child`'s environment.

### Verification commands

All exact focused closure commands in this task passed on the final tree
(loader, wyrd-client unit, three `pg_workflow_registration` tests,
`pg_cards_register` recheck test, Rust/Python/TS journeys with the corrected
vitest filter). Lanes: `test:shared`, `test:skald`, `test:cards:integration`,
`test:wyrd-sdk`, `py:test:unit`, `py:test:cards:integration`, `py:typecheck`,
`ts:test:unit`, `ts:test:integration`, `ts:typecheck`, `ts:napi:check`,
`codegen:check`, `check:client-tier`, `check:sdk-client-tier`,
`check:pyo3-scope`, `check:registry-tx-coupling`, `fmt`, `lints`,
`py:format`, `py:lints`, `git diff --check` — all PASS. No docs-site or skill
files touched.

Status: IMPLEMENTED.
