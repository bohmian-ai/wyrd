# TASK-002-r2 independent behavior review

**Result: FAIL.** The loading/registration composition has credible source and focused regression evidence. The required Native SDK execution journeys are incomplete, and the TypeScript input signature differs from the approved contract.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`.
- Candidate: `e7d16b5bd622b9a564a49edb18239df7f209ca92`.
- Reviewed cumulative diff: `/tmp/wyrd-task002-r2.diff`, with current source and consumers expanded independently.
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12.
- Active task: `tasks/TASK-002-cleanup.md`; original `TASK-002-load-and-register-graphs.md` is superseded. Prior `TASK-002-r1` validated findings/remediation were treated as closure hypotheses, with Revision 12 winning over conflicting private mechanics.
- Governing authority consulted: AGENTS.md, agent-rules, Wyrd design/doctrine, reference router, spec-driven-development, maintainer-style, testing-workflows, errors, PyO3 boundaries and TypeScript guide. No CodeGraph directory exists.
- Source remained unchanged; this reviewer wrote only this report. Current peer-review conclusions were not read.

## Caller and owner coverage

1. `Workflow::from_path` → existing `wyrd_loader::load` (pure validation, paths, sandbox, Sibling projection) → `WorkflowBodies::authored` (entry-file root) → canonical `card_body_dependencies` → lazy `Cards::new` only for external refs → existing `CardGraphHydrator`/`GraphTraversal` in Runtime scope → Skald `from_card_bodies` → existing Agent/Prompt construction and resolved validation. This path neither registers nor dispatches.
2. `Cards::workflow().load` → exact Workflow selector checks → existing Cards reads with selector identity assertions → existing graph traversal along UID-bearing Agent/Prompt relationships → active-state checks → same Skald lowering/validation. Runtime scope skips artifact inventories and publication; Bundle scope retains the existing Service/disk consumer.
3. Composite registration → canonical ref inventory and tenant effective bodies → provenance-aware `EffectiveSpecs::load`/`body` → `validate_card_bodies` → existing write transaction/audit → exact expected-UID recheck → existing reference binding and relationship persistence. Validation clears tools only in temporary lowered validation inputs; persisted Agent tool declarations are retained.
4. Rust re-export, Python `PyWorkflow.from_path`/`PyWorkflowCards` with detached shared runtime, and TS native `NativeWorkflow`/Cards view all delegate to shared loading. Runs delegate to Skald; there is no new executor. Existing native builders and explicit Rust dependencies remain available through `as_skald`.
5. Inspected loader bundle tests, client loading test, all three PG Workflow tests, SQL lifecycle regression, three SDK loading journeys, Python save/load consumers, TS ergonomic signatures/native selectors/declarations, the checked-in example and shared loading fixtures, manifests and relevant mise verification declarations. Architecture/skill/task edits accompany approved Revision 12; they do not authorize dropping its execution proof.

The existing graph owner was appropriately extended: the new facade addresses client IO on a foreign Skald type, `WorkflowBodies` holds the necessary authored provenance inputs beside the existing hydration owner, and server bodies remain in the existing EffectiveSpecs owner. New Skald bodies lowering adapts existing resolver seams and canonical slots rather than replacing parser, traversal, binder or executor. No duplicate workflow owner finding is proposed.

## Acceptance matrix

PASS below means the bounded task obligation has source support and the stated evidence; it does not mean this reviewer independently reran every claimed lane. Full remote/CLI execution and shared selected configuration belong to later tasks.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: normal Workflow Card, no alternate YAML/wrapper | Client selects Workflow from loader submissions; existing WorkflowCard envelope decoding | Loader actual seven-Card bundle; client focused test | PASS |
| REQ-002: Agent steps support inline, local paths, exact refs | Canonical visitor, loader Sibling projection, Skald existing lowering with CardBodyResolver | Loader, client and PG local/ref scenarios; SDK mixed fixtures | PASS |
| REQ-003 and INV-014: existing Prompt schema/reference/binder | Three versioned native Prompt Cards; Agent paths; Skald Prompt resolver and unchanged runtime binder | Client and PG fake gateway request assertions contain both reviewer findings | PASS |
| REQ-013: earliest pure validation | Loader validate_card calls WorkflowSpec::validate; Skald lowering validates contract | Loader cycle and PG malformed graph cases | PASS |
| REQ-013A: resolved Prompt binding/dialect/output checks | Both Skald bodies methods call existing Workflow::validate | Client extra-binding and dialect negatives; PG sibling/external invalid matrix | PASS |
| REQ-014: local, registered local, registration resolved validation; registration declaration-only | WorkflowBodies::hydrate; EffectiveSpecs::validate_workflows; validate_card_bodies skips only tool binding | Three independently passing PG tests; client focused test | PASS |
| REQ-024 / AC-001 / AC-003, task Native slice: local and registered local execution in all SDKs | Shared run delegates to Skald; current example/fixtures declare wyrd_gateway | Rust runs fake gateway only; Python loads only; TS asserts binding refusal only | **FAIL: BEH-002-R2-001** |
| REQ-025 / REQ-055: shared loader and automatic ambient external reads | from_path constructs Cards only when discovered external refs are nonempty | Python/TS missing/denied/successful mixed loading; client local loading | PASS for source behavior; per-language proof gap below |
| REQ-028 / AC-002: atomic exact relationships without execution; CLI final proof deferred to TASK-005 | EffectiveSpecs, existing registration writer and exact-UID fence | PG registration asserts seven Active Cards, Agent/Prompt UIDs and both relationship layers; principal count unchanged | PASS for task SDK/HTTP registration slice |
| REQ-029 / INV-005: active exact registered local graph, no floating | Existing exact reads + GraphTraversal identity equality + WorkflowBodies active checks | PG registered journey pending/deleted/foreign/mismatch cases; Rust newer Agent run; Python YAML pins | PASS for task registered-local slice |
| REQ-040: Prompt owns provider/model/native request | Native Prompt schema and existing Prompt::from_native; no Workflow provider request type | Actual example schema/loader test | PASS |
| REQ-052 / INV-008: preserve tools and existing tool loop; registration accepts declarations | hydrate uses existing default ToolResolver; explicit builder/runtime APIs retained; validation-only temporary tool clearing | PG stores bifrost.query/cards.get unchanged; existing Skald/tool tests claimed green | PASS |
| REQ-054: exact Rust/Python loading APIs, runtime-owned tests | shared facade/re-export; Python PathBuf static method and typed Cards view | Rust/PG selected tests green; Python public imports/stubs and journey claimed green | PASS |
| REQ-054: exact TS run input contract | TS `Workflow.run` uses Record<string, unknown> instead of Record<string, JsonValue> | Source and small Node serialization reproduction | **FAIL: BEH-002-R2-002** |
| REQ-056: provenance, exact versions/UIDs, active state, read authority | WorkflowBodies separates siblings/registered; body checks optional asserted UID; EffectiveSpecs separates stores; reads asserts identities | Client cannot satisfy external ref with sibling; PG incompatible collision refusal; missing/denied and inactive cases | PASS |
| REQ-057: reuse owners; no Workflow principal/WyrdState root/new executor/public WorkflowLoader | Existing loader/graph/EffectiveSpecs/Skald seams; WyrdState unchanged; no Workflow principal API | Source/diff; PG principal count | PASS |
| REQ-059: obsolete types/keyed machinery removed without losing protections | No WorkflowLoader/WorkflowGraph loading type or InlineableSlotField machinery in product source; canonical visitor retained | Static source search; loader/Service claimed regression lanes; UID test independently passed | PASS |
| INV-002: spec remains pure/synchronous/PyO3-free | CardRefIdentity is an in-memory identity; client/server/Skald own IO and body lowering | Boundary/codegen lanes claimed green; source dependency direction | PASS |
| INV-003: Cards declarative, no embedded live dependencies | Card envelope/spec fields unchanged; facade/runtime owns handles | Source/diff | PASS |
| INV-007: one WorkflowRun; same shipped local journeys | Native snapshots projected; Python wraps existing run; TS serialization delegates | Rust gateway result works; Python/TS execution parity is absent | **FAIL: BEH-002-R2-001** |
| AC-006 / AC-013: pure/resolved negative behavior before dispatch | Existing validation invoked at loading/registration seams | Client negatives; PG sibling/external binding/output/graph/route refusals with operation/Card counts | PASS for task seam coverage |
| AC-029 / S3–S5: each SDK local/mixed authored load → Native run → registration → pinned load → Native run, outputs/bindings/relationships | Public facades exist; fixtures reuse team Agents and local final Agent | Rust omits authored mixed loading; Python never runs; TS never succeeds or registers v2; no Native complete journey | **FAIL: BEH-002-R2-001** |
| AC-030 / S1–S5: every language lazy credentials/zero local IO/negatives/collision before dispatch | Shared lazy and provenance behavior is credible | Python/TS auth negatives exist; Rust authored external credential/denial/inactive/collision journey is explicitly omitted; collision tests inspect step IDs, not different resolved behavior | **FAIL: BEH-002-R2-001** |
| AC-030: no partial registration, stale UID refusal, no principal | Recheck matches expected UID before operation insert and retains row lock/audit transaction | PG invalid/no-operation and stale replacement test independently passed | PASS |
| AC-031 / S6: loader/Service behavior and declarations preserved | Bundle scope retains inventories/publication; native/SDK source exports align for loading; no lexical-ban checker | Claimed Service/loader, codegen and SDK typecheck lanes; format independently passed | PASS, except TS run contract and deferred shared config |
| Non-goals: no writes during load, secrets/providers during registration, artifact fetch/publication during Workflow load | Runtime graph scope; static Skald validation; no durable path in loading | Source; operation-count registration negative proof | PASS |
| Non-goals: no Workflow authority, duplicate transport, parser dialect, cache, executor or compatibility alias | Cards context shared; Python old static load replaced; native parser documented as lower-tier single-document seam | Source/diff/exports | PASS |
| Preserve existing native builder and explicit dependencies | facade From/as_skald/into_skald delegates to unchanged Skald APIs | Existing Skald lane claimed green; Rust fake gateway injection exercises explicit seam | PASS |
| Shared selected gateway/binding configuration and CLI all-routes proof | Deliberately assigned to TASK-003/TASK-005 | N/A at cleanup gate | PASS: excluded from this task |

## Proposed source-local findings

### BEH-002-R2-001 — MISSING: required Native SDK execution and complete per-language loading proof

**Violated obligations:** TASK-002-cleanup Outcome explicitly says this task proves Native execution with existing provider configuration; S3–S5 require each public SDK Native journey and exact outputs/bindings/pins/negatives. Revision 12 REQ-024/054–056, INV-007 and AC-029/030 require the authored and registered local journeys in every shipped language. This finding does not demand TASK-003 gateway binding/configuration assembly, TASK-005 CLI proof, a new provider API, or a new executor.

**Exact changed locations:** `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:17-20,109-126,242-327`; `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py:485-566`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:45-113`. The producer is the acceptance fixture/proof selection: `examples/workflows/code-review/workflow.yaml:9-10` and all mixed journey Workflow fixtures select `wyrd_gateway`, while the tests supply no Native happy path.

**Evidence and reachable result:** Rust loads the actual local bundle but never runs that loaded value; it does not call public from_path on the mixed file and explicitly assigns ambient-ref proof to the other languages. Its registered run supplies a fake WyrdGatewayCaller through as_skald/run_with_options, bypassing public default run and the Native provider path. Python local/mixed/collision/registered values are inspected for IDs/YAML only and never invoked. TS registered values call run only to assert `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE`; it never creates v2 or asserts output/binding equality. Shared/client and server fake-gateway tests prove the common binder but cannot establish Python/Node lifetime execution or per-language complete journeys. Independently green named tests have exactly these weaker assertions. The task evidence's declared limits acknowledge the omissions; a passing command cannot supersede the approved acceptance outcome.

**Observable consequence:** this candidate's mandatory task proof can remain green while Native dispatch, SDK input/result conversion, mixed authored execution, per-language pinning or sibling/external body selection fails. Python/TS users have no successful shipped execution journey in this task, and Rust's automatic external-resolution path is unproved at its required language boundary.

**Smallest testable correction:** extend the existing three SDK journeys using existing provider configuration and deterministic local upstream fixtures, preserving the actual native Prompt bodies/DAG and declared route semantics. An explicitly authored Native variant/copy can use the same bundle; do not reinterpret the gateway route at runtime or add TASK-003 configuration to close this proof. Exercise public local and mixed loading plus successful Native runs before and after registration in each owning language; assert named outputs and final bound reviewer values, exact Agent/Prompt pins, and no dispatch on loading/registration refusal. Complete Rust ambient-ref/negative proof with process-isolated environment setup rather than weakening the requirement because multithreaded set_var is unsafe. Reuse existing repository test ownership; no new framework, duplicated validator or provider/runtime implementation.

**Focused closure proof:** the existing three named SDK journey commands must select tests that now assert successful Native terminal runs and output/binding equality. Rust must exercise mixed from_path and its credential/read/collision negatives; TS must introduce a newer dependency and establish pinning by executing the original graph. Retain the passing PG/client provenance and UID fence commands and the task's broader SDK/native/typecheck lanes. CLI apply and selected gateway bindings remain later-task obligations.

### BEH-002-R2-002 — INCORRECT: TS public run accepts non-JSON inputs contrary to the exact contract

**Violated obligations:** Revision 12 public TypeScript contract (`spec.md:65-71`) and REQ-054; task Public contracts require `run(input?: Record<string, JsonValue>): Promise<WorkflowRun>`. The TypeScript guide requires typed native projections that do not erase invalid inputs.

**Exact changed location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1321-1324`.

**Evidence:** the new method exposes `Record<string, unknown>` and passes it immediately to JSON.stringify. Neither a JsonValue type nor input narrowing exists at that boundary. A typed caller can therefore provide `{unexpected: undefined}` or `{code: 1n}`. The former becomes `{}` before `NativeWorkflow::run` parses/validates keys, and the latter throws a JavaScript TypeError before the catalog error boundary. A small read-only reproduction through `mise exec -- node -e` printed `{}` for the first and threw `TypeError: Do not know how to serialize a BigInt` for the second. The native Rust Map<String,Value> cannot recover fields already erased in JavaScript.

**Observable consequence:** the public declaration permits values outside the approved JSON domain. A caller that type-checks can silently lose an input key or receive a raw serialization failure, instead of reaching the native declared-input validation with the value it supplied.

**Smallest testable correction:** project the approved recursive JSON value type and use `Record<string, JsonValue>` in the public signature. Reuse an existing JSON-domain type if present; otherwise its ordinary recursive TypeScript alias is sufficient. Keep the existing Rust input validation owner and native snapshot conversion; no TypeScript workflow validator or runtime is needed. Do not broaden this correction into a new generic arbitrary-object serialization API.

**Focused closure proof:** the SDK typecheck must accept all valid JSON inputs while rejecting undefined, bigint, functions and other non-JSON values in typed calls; existing integration run/error proof remains green. Check the published ergonomic declaration from source plus owning TypeScript build/typecheck lane.

## Prior finding closure

| Prior validated finding | Current behavior assessment |
|---|---|
| FIND-TASK-002-1: erased Sibling/external provenance | Source correction is present at both existing owners. Client/PG incompatible external collision negative is retained. Per-language positive distinct-body execution proof is still part of BEH-002-R2-001. |
| FIND-TASK-002-2: missing versioned Prompt Cards | Closed behaviorally: actual example has three versioned Prompt files; PG asserts all seven outcomes and exact Agent-to-Prompt relationships. |
| FIND-TASK-002-3: duplicate keyed normalization | Superseded mechanics correctly removed under Revision 12; existing untagged/reference visitor used. No replacement parser demanded. |
| FIND-TASK-002-4: preflight/write replacement UID | Closed behaviorally: writer preserves pairs; SQL checks expected UID + Active + exact identity under lock; independently passing stale plan regression rejects replacement before operation persistence. |
| FIND-TASK-002-5: missing changed-item rustdoc | Removed old loading/parser items no longer apply; new loading/body paths document roles/errors/read-only cancellation. Detailed repository-wide documentation audit belongs to the fresh standards reviewer. No behavior reopening proposed. |
| FIND-TASK-002-6: function-scoped imports/qualified signatures | Historical locations removed or corrected in inspected new body/loader test modules; no behavior reopening proposed. |

## Verification limits and diagnosis audit

Root independently reran client `workflow::tests::from_path_uses_existing_loader` (1/1), all three named PG Workflow tests (3/3), Rust SDK journey (1/1), read-only formatting and diff checks; see `verification.md`. The broader implementation lane claims and Python/TS runs were not independently rerun by this reviewer. No expensive test was started by this reviewer. The only additional execution was the read-only Node serialization reproduction above; its intentional TypeError is evidence, not a repository gate failure.

The Vitest selector correction is supported by the enclosing describe name and changes no assertion. No test timeout/retry/skip change to disguise a diagnosed behavior failure is claimed here. The retained Rust #[ignore] is the approved integration gating pattern, with independently selected execution. Successful tests establish their asserted behaviors but do not close the missing Native outcomes.

**Overall FAIL**, with proposed findings `BEH-002-R2-001` and `BEH-002-R2-002`, for independent validation. No implementation changes or specification revision are prescribed.
