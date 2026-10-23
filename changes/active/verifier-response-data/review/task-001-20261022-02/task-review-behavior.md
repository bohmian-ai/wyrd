# TASK-001 independent behavior review

Result: **FAIL**.

## Immutable subject and method

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `4d0f7005888a899910008399d1b5e15e2a3207f9`; candidate `9e56fc340cb5fd769d3b34f47433e2719339fdfb`. Reviewed the cumulative range, approved revision-1 `changes/active/verifier-response-data/spec.md`, original task and its recorded verification, root/nested SDK/docs rules, workflow, maintainer/error/agent-surface/Rust guides, testing map, and owning Prompt/Verifier, Variant, and authorization architecture. HEAD remained the candidate. Existing unrelated local edits were not treated as implementation.

CodeGraph was the first code-navigation tool. Traced named-format authoring through `AuthoredPrompt::compile_with`, `compile_prompt`, inline Agent decoding, `Verdict::response_format`, check declarations and `VerifierValidator`; Agent/code executors through `verdict_from`, citation filtering, `Judged` and result serialization; Rust/Python/TS typed siblings through ordinary verification/polling and local decoding. Read SDK journeys, Agent negative/boundary coverage, declaration/readback assertions and documentation changes. No other discovery reports were read.

Verification is source review plus small independent semantic reproductions, not reruns of the submitted Cargo/DB suites. A Node check reproduced null/undefined exception inspection failing with TypeError. Installed Python `jsonschema` reproduced the exact schema relocation counterexample below, via `mise exec -- uv run --project sdks/wyrd-sdk-python python -c ...`. This is supporting JSON Schema evidence, not represented as a production Rust execution. No Cargo commands or broad aggregates ran. This restarted attempt additionally ran the frozen existing Python HttpAuth unit module and the focused verification-contract typecheck, documented below.

## Restart and cumulative candidate coverage

Attempt 01 stopped because its candidate changed during discovery. It supplied unvalidated hypotheses, not a prior acceptance verdict or FIND ledger. This fresh report retains the same independent behavior role and reviews the complete original-base-to-`9e56fc340cb5fd769d3b34f47433e2719339fdfb` candidate. No other reviewer report was consulted.

Read the complete incremental commit `a71156e...9e56fc3` and rechecked the cumulative changed-path list against prior coverage. The incremental commit updates only the ty lock entry, generator annotations in existing context-manager test/support functions, one `.get("data")` type assertion, the task limitation note, and `HttpAuth.auth_flow = sync_auth_flow`. The context-manager changes affect annotations, not execution. HttpAuth continues using its existing per-request token acquisition/header replacement owner and its existing async worker-thread path; no new transport, credential cache, retry, or durability owner was added. This small auth adjustment is included in acceptance coverage as a regression boundary of the user-directed checker upgrade.

Re-read the three finding locations directly from committed `git show 9e56fc3:<path>`; no source relevant to BEH-001, BEH-002, or BEH-003 changed. Their original producers, consumers, and proof remain applicable. No hypothesis is closed by this candidate. The candidate fixes the separately reported Python typing limitation: the new generic TypedDict `.get("data")` assertion passes with installed locked ty. That limitation is not retained as a finding.

Frozen checks completed:

- `mise exec -- uv run --frozen --no-sync --project sdks/wyrd-sdk-python python -m pytest -q -p no:cacheprovider sdks/wyrd-sdk-python/tests/unit/client/test_http_auth.py`: **3 passed** (fresh-token sync/async and provider-header replacement regressions).
- From `sdks/wyrd-sdk-python`: `mise exec -- uv run --frozen --no-sync ty check tests/typecheck/verify_contracts.py`: **All checks passed**, including the new `.get("data") -> Judge | None` assertion.
- `git diff --check 4d0f700...9e56fc3`: **PASS**.

These checks perform no dependency sync/install and do not substitute for full candidate server/SDK acceptance journeys. HEAD and working-tree source status were checked before writing; only the pre-existing three workflow edits and review artifacts were present.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: scalar and expanded plain format, canonical closed schema, portable/native and inline/referenced support | `authoring.rs` named formats/compiler/native lowering; PromptSpec and AgentSpec compile through Wyrd resolver | `named_verdict_formats_preserve_authored_forms`, `named_verdict_formats_validate`; SDK scored-review fixture contains both plain forms | PASS |
| Preserve authored format/hash source and native unrelated settings; refuse native duplicate response settings | `Prompt::authored`, retained authored/named format; `named_output` checks all four native protocol fields | compiler native round-trip/conflict assertions | PASS |
| REQ-002: object data schema becomes required non-null data; code omission keeps plain contract; preparation code actions remain arbitrary JSON | `CheckCode::response_schema`, separate `CodeCall`, generated Verdict composition | named-format tests; scored-review whole/step code and Agent fixtures | PASS for direct declarations; FAIL for handwritten composition, BEH-003 |
| REQ-002 / REQ-007: supported schema semantics and exact declared contract for both kinds | `Verdict::response_format` embeds custom schema and moves definitions; both executors use resulting schema | semantic counterexample BEH-001; existing tests do not cover root-local pointer relocation | FAIL |
| AC-003: invalid names/schema/native conflicts and incompatible declarations refused before persistence, structured refusal | named resolver, Prompt decode and `VerifierValidator`; HTTP declaration journey checks rejection followed by absent identity | `verdict_declarations_validate_before_persistence`; pure declaration tests | FAIL: optional data hidden under supported composition is admitted, BEH-003 |
| REQ-007 / AC-004: malformed/missing/wrong/null/undeclared data and unknown envelope members become inconclusive, rejected data omitted | shared `verdict_from` schema + closed Verdict decode; BlockError terminal -> Judged without data | Agent integration negative cases; both-kind SDK BAD/HUGE cases | PASS except exact-schema defect BEH-001 |
| AC-005: 65,536 compact UTF-8 bytes and 16 containers inclusive, no truncation/payload diagnostics | `Verdict::check_data_bounds`, serde compact encoding and object/array depth | pure exact/over Unicode/escape/wide-array cases; Agent execution exact/over cases; both-kind oversized journeys | PASS implementation; hosted-code journey does not independently exercise every exact/depth/Unicode/escape case requested by scenario 2 |
| Citation/pass policy, aggregate verdict, status, actual and drift detail unchanged | executors retain citation paths; `Judged::checked` only adds data; generic result preserves adjacent fields | citation regression, plain and mixed-data journey assertions | PASS |
| REQ-011 / AC-002: valid data survives direct, queued wait/status, whole/step results and publication | Judged -> RequirementResult -> existing outcome Variant; unchanged result lifecycle/storage | Rust/Python/TS readback; HTTP/MCP; Bifrost published outcome comparison | PASS |
| Old/plain results omit data, no aggregate custom data or new Bifrost column | optional skip-serialized data on requirement, generic result/outcome only | pure omission and SDK plain-format assertions | PASS |
| AC-007: tenant/scoped readers gain no access and typed reads preserve surrounding facts | existing tenant-bound result/query paths retained; local decoder conversion only | HTTP foreign tenant 404, scoped query refusal, stored-outcome equality | PASS under existing architecture authorization; specification wording caveat below |
| REQ-012: default generic data remains ordinary JSON; actual runtime types in all SDKs and every typed operation | Rust DeserializeOwned conversion; Python model_validate conversion; TS parse conversion; typed siblings delegate ordinary workflows | typed success/mismatch journeys; Rust deadline wait; Python/TS compile-time fixtures | PASS normal model/parser paths |
| REQ-012 / AC-006: absent data not decoded, failed/mixed data decoded, no partial return, no mutation/rerun | conversion loops conditional on present data; whole-return failure; stored/raw result retained | absent/failed/union success and mismatch preservation/call-count assertions | PASS |
| REQ-012 / AC-006: every decoder mismatch yields exact normal SDK error and safe path | Rust/Python conversion errors wrapped; TS catch attempts parser-specific error inspection | Zod mismatch is tested, arbitrary throwing parser is not; BEH-002 | FAIL |
| Python positional-only verifier/type; input named data_type remains usable; group/input forms retained | `PyObserveHandle::verify_typed` signature and delegation | Python journey forwards data_type as input and receives input-validation error, not argument collision | PASS |
| AC-008: public supported model/schema authoring, imports/stubs/generated types and declarations | Python named response coercion reuses schema coercion; TS verdictFormat reuses jsonSchemaText; public generic stubs/types | Python public model authoring test; TS compile-time fixture; recorded generation/typechecks | PASS implementation; TS helper has compile-time-only authoring proof |
| Updated checker/auth regression boundary | locked ty 0.0.91, generator annotations, existing HttpAuth shared sync path | frozen focused verify-contract typecheck and existing HttpAuth unit module, 3 passed | PASS |
| Documentation/architecture/examples match and all three SDKs explained | owning architecture and limits updated; both policy-judge YAML copies removed; custom recipe only demonstrates Python | recorded docs/codegen/example checks; Rust support-desk journey | FAIL literal three-SDK documentation coverage; non-blocking note below |
| Non-goals: no new check kinds, arbitrary envelope fields, configurable limits, new store/API/lifecycle/transport, mandatory model library, inferred format or action judgments | complete range has additions within existing owners and optional local decoding only | diff/manifests and caller inspection | PASS |
| Required test-first evidence | task records scenarios 1–3 production landed before tests | recorded limitation; no historical RED supplied | FAIL process evidence; cannot reconstruct past RED through a later rerun |

## Existing owner / alternative analysis

Named resolution extends the existing authored compiler without reversing Skald dependency direction. The optional named state prevents unresolved Prompts rendering; native lowering stays in the existing compiler. No competing compiler/transport/store was found. `verdict_from` is a shared pure validation transformation used by the two existing executor owners; it does not replace their runtimes or citation policies. Typed SDK methods correctly reuse ordinary execution/status/wait and existing schema conversion, rather than duplicate transport/polling. Generic contract projections are needed to express actual caller data types. `CheckCode` keeps check-only fields away from preparation actions.

Two local mechanisms need correction: schema composition must preserve authored reference meaning at the existing canonical resolver; handwritten data-contract analysis should extend the existing bounded schema walker instead of introducing a shallow parallel interpretation.

## Proposed findings

### BEH-001 — INCORRECT: composing custom schema changes its local-reference meaning

- Obligation: REQ-002 supported object schema and REQ-007 validation against the **exact declared schema**; AC-004.
- Location: `crates/wyrd-spec/src/verification/execution.rs:595-620`, particularly assigning the unchanged custom schema to `/properties/data` after moving only definitions.
- Producer and consumers: Prompt and code-check named declarations both call `Verdict::response_format`; the resulting schema is sent to providers and passed to the shared `verdict_from` by `AgentExecutor::execute` and `CodeExecutor::execute`. Whole and step code requirements share it. This affects every SDK/HTTP/MCP consumer of resulting judgments.
- Reachable counterexample: declare data schema `{"type":"object","properties":{"reason":{"type":"integer"},"copy":{"$ref":"#/properties/reason"}},"required":["reason","copy"],"additionalProperties":false}`. In the authored custom schema, `copy` must be an integer. After embedding, that reference resolves to the **Verdict envelope's string reason**. A return with data `{"reason":1,"copy":2}` is incorrectly inconclusive, while `{"reason":1,"copy":"wrong"}` satisfies the composed schema and is retained despite violating the declared schema. This requires no external reference, unsupported provider, malformed declaration, or new contract. Installed JSON Schema validation reproduced authored-valid=True, composed-valid=False, composed-wrong-type-valid=True.
- Root cause: relocating a schema resource without preserving its local reference target, not missing guards in consumers. Moving `$defs` alone covers common model output but does not preserve all already-accepted local pointers, including recursive/root pointers and references within moved definitions.
- Smallest testable correction: correct reference-preserving schema composition at `Verdict::response_format`, preserving standard reference/resource semantics and reusing existing JSON Schema behavior. Do not add per-executor or SDK validation copies or silently narrow authoring. Ensure declaration validates the final composed schema. Focused proof must show matching data accepted and mismatching data inconclusive without data for both check executors, using a root-local pointer that collides with an envelope property.

### BEH-002 — INCORRECT: arbitrary TypeScript decoder refusals can escape as TypeError

- Obligation: REQ-012 / AC-006 require any typed mismatch to raise `WYRD_SDK_422_VERIFICATION_DATA_INVALID` through WyrdError. The approved parser contract is generic `parse`, with no Zod requirement.
- Location: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:282-283`.
- Producer and consumers: a supported parser `{ parse() { throw null; } }` (or `throw undefined`) refuses present stored custom data. `typedResult`, shared by Observe.verifyTyped and Execution.waitTyped/statusTyped, reads `error.issues` before producing WyrdError. A TypeScript assertion performs no runtime narrowing. Node reproduces `TypeError: Cannot read properties of null (reading 'issues')`. A malformed `issues.path` similarly makes the formatting path throw.
- Consequence: all three public typed operations can throw an uncatalogued TypeError instead of the promised WyrdError; callers cannot branch on the advertised stable error. Stored result remains unaffected.
- Smallest testable correction: keep conversion at this existing shared SDK boundary and treat caught values as unknown. Extract a field path only if it has the expected safe shape; otherwise use the existing empty path fallback. Always construct the catalogued WyrdError after a parser refusal, without echoing the caught value/message. No parser dependency or duplicate per-operation guard is needed. Focused SDK proof with an ordinary valid result and a parser throwing null/undefined should assert the exact code and unchanged raw read; include malformed optional diagnostics if inspected.

### BEH-003 — INCORRECT: handwritten data-declaration validation ignores schema composition

- Obligation: REQ-002 requires handwritten extended schemas to declare and require their object data; REQ-001 / AC-003 require incompatible check contracts refused before persistence. Compatible handwritten schemas and existing supported local references must continue working.
- Location: `crates/wyrd-spec/src/card/verifier/validate.rs:787-798` (`declares_data_object`), called at lines 583-585.
- Producer and consumers: a check Agent's handwritten portable/native Prompt schema reaches `validate_agent`. Existing `schema_path::Walk::fits` follows local refs and explicitly supports authored allOf/anyOf/oneOf. The new supplemental check instead inspects only literal root `/properties/data` and root `required`.
- Reachable false acceptance: use `{"type":"object","allOf":[<closed canonical Verdict schema declaring optional object data>]}`. `fits` accepts its allOf branch against canonical Verdict, whose new data is optional; `declares_data_object` sees no root data property and returns true. The declaration persists and runtime accepts absent data although the handwritten schema explicitly declares data without requiring it. Equivalent direct schemas are refused by the candidate's own test.
- Reachable false refusal: a handwritten required `properties.data: {"$ref":"#/$defs/JudgeData"}` with `$defs.JudgeData.type: object` fits the existing walker but the shallow `data.get("type")` test refuses it. A normal supported schema ref cannot satisfy the new check.
- Smallest testable correction: enforce the required-object custom-data obligation within the existing bounded, reference-aware schema compatibility owner rather than maintain a parallel top-level interpretation. Preserve its traversal/work limits and plain/handwritten compatibility; no schema framework or public authoring restriction. Focused declaration tests must reject optional data under supported composition before persistence and accept required object data through a local reference, for portable/native or representative shared compilation paths.

## Non-blocking observations and evidence limits

- The approved spec says documentation explains the feature in all three SDKs, but the new docs contain only Python typed examples (`verifier-recipes.md:258-268`) and no Rust/TS typed calls. Batch this documentation correction with bounded remediation; it is not a separate style-only round.
- The task evidence openly records no observed RED for scenarios 1–3. Later passing tests cannot prove test-first execution. This is a process limitation, not grounds to invent a behavioral defect or rerun broad suites.
- Exact hosted-code boundary/depth/escaping negative tables and runtime TS model authoring are less complete than the task requests; the shared validation and existing schema conversion source give substantial confidence, but the recorded implementation table overstates those specific proofs.
- The spec phrase “elevated payload-read protections” conflicts with the narrower existing Bifrost architecture: query permission governs verification outcome; only gateway-call request/response columns additionally require gateway payload-read (`architecture/bifrost-design.md:1480-1484`). No new permission scheme is prescribed here. Resolve wording by the owning authority; adding a verification payload gate would be a new security decision.

No source, tests, fixtures, configuration, exports, dependency environments, or other reports were changed. Broad test suites were skipped; the retained risk is the three concrete reachable contract defects above.

## Independent reproduction commands

These use installed interpreters/libraries only and do not modify repository source:

```sh
mise exec -- node --input-type=module -e 'for (const error of [null, undefined, "invalid"]) { try { const issue = error.issues?.[0]; console.log(issue); } catch (actual) { console.log(actual.constructor.name, actual.message); } }'
```

Observed: TypeError for null and undefined; undefined optional issue for the string.

```sh
mise exec -- uv run --project sdks/wyrd-sdk-python python -c 'import jsonschema; data={"type":"object","properties":{"reason":{"type":"integer"},"copy":{"$ref":"#/properties/reason"}},"required":["reason","copy"],"additionalProperties":False}; envelope={"type":"object","properties":{"reason":{"type":"string"},"data":data},"required":["reason","data"],"additionalProperties":False}; print("authored_valid",jsonschema.Draft202012Validator(data).is_valid({"reason":1,"copy":2})); print("embedded_valid",jsonschema.Draft202012Validator(envelope).is_valid({"reason":"judged","data":{"reason":1,"copy":2}})); print("embedded_wrong_type_valid",jsonschema.Draft202012Validator(envelope).is_valid({"reason":"judged","data":{"reason":1,"copy":"wrong"}}))'
```

Observed:

```text
authored_valid True
embedded_valid False
embedded_wrong_type_valid True
```

An initial `mise exec -- python -c ...` could not find a Python executable; the project-owned `uv run` command above then completed successfully. No test-suite pass is inferred from either small reproduction.
