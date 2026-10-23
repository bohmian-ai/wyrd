# Independent behavior review — TASK-001

Result: **FAIL**. Proposed material findings: **BEH-1** and **BEH-2**, continuing prior **FIND-TASK-001-1** and **FIND-TASK-001-5** respectively.

## Subject and method

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`; candidate `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`. Read the actual cumulative `git diff fb1fd9234...d2ffc5987`, approved revision-1 spec, original TASK-001, attempt-02 verdict/validated ledger, and R1 task/evidence. HEAD remained the candidate at the final inspection. No new discovery reports or implementation conversation summary were consumed. The three excluded workflow edits were preserved; workflow authority was read with `git show d2ffc5987:changes/spec-driven-development.md`.

CodeGraph preceded source navigation (`mise exec -- codegraph explore "NamedResponseFormat Verdict response_format RequirementResult verdict_from typedResult"`). Expanded the supplied navigation map to the compiler's native/portable lowering, PromptSpec/Agent inline decoding, check/action split, handwritten compatibility, both check executors and Judged, ordinary/typed SDK operations, result storage projections, schema migration consumers, fixtures, SDK journeys and HTTP/MCP tests. Read root and applicable docs/Python/TS rules, maintainer guide, testing map, owning Prompt/Verifier/client authority and relevant storage/security boundaries. Other review roles retain their independent standards/domain coverage.

Caller-to-result traces:

1. YAML/model/schema authoring → NamedResponseFormat → Verdict::response_format → Prompt compiler or CodeCheck::response_schema → AgentExecutor/CodeExecutor → verdict_from → Judged::checked/report → ordinary outcome serialization/Scribe/results → SDK/HTTP/MCP/Bifrost readers.
2. Ordinary verify/wait/status → complete ordinary result → one local conversion owner per language → every present requirement data → whole success or catalogued local error; no new execution/storage/polling path.
3. Schema migration → schemars 1.x generated definitions/const/root refs → TypeScript lowering, OpenAPI conversion, MCP schemas and extraction-format reader. The migration is the R1-selected solution rather than an unsolicited dependency addition.

## Acceptance matrix

Recorded tests below are evidence inspected in source and task records unless explicitly described as rerun. Green checks are not treated as proof of untested cases.

| Requirement / criterion / constraint | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: scalar and expanded plain Verdict generate canonical closed required outcome/reason/citations | `execution.rs::Verdict::response_format`; authored compiler `compile_with`; required citations and deny_unknown_fields derive | `named_verdict_formats_preserve_authored_forms`, `named_verdict_formats_validate`; plain-judge/policy-desk fixtures and SDK readback stories | PASS |
| Portable/native and inline/referenced Prompt forms; retain/hash authored form; unresolved Skald names cannot render | `authoring.rs::compile_resolved`, `named_output`; `prompt.rs::authored/render`; `PromptSpec::new/deserialize`, `agent.rs::inline_prompt` | compiler round-trip/native-conflict/bind cases and plain SDK fixtures | PASS |
| Existing compatible handwritten schemas; require declared non-null object data through supported refs/composition | `validate_agent` reuses `schema_path::fits` against canonical `verdict_shapes`; literal parallel walker removed | `named_verdict_formats_validate` includes local refs/allOf/root refs, optional/nullable refusal; HTTP composed refusal/absence | PASS |
| AC-003: unknown names, malformed schemas/native conflicts/incompatible declarations fail with existing structured errors before save | resolver/schema meta-validator, native conflicts, CodeCheck response-schema validation; original declaration persistence owner retained | pure compiler/contract cases; `pg_verification_routes::verdict_declarations_validate_before_persistence` | FAIL for accepted schema whose effective resource changes: BEH-1 |
| REQ-002: code check format beside existing call fields; default plain contract; preparation code arbitrary JSON | CodeCheck wraps CodeCall; strict custom decode removes only response_format; Action::Code remains CodeCall; `response_schema` defaults canonical plain | preparation code fixture `tally`; declaration refusal tests; code and step-requirement SDK data | PASS |
| REQ-002 / AC-008: Python model and TS schema-source authoring; Rust existing schema representation | Python `response_format_from_py` reuses schema coercion; TS `verdictFormat` reuses `jsonSchemaText`; native Rust map | Python `test_verdict_format_keeps_its_name_and_takes_a_model_for_data`; TS type fixtures/helper source; original generation/typecheck records | PASS within recorded proof |
| REQ-007 / AC-004: exact whole schema in both check executors; invalid response inconclusive/absent | both executors call `verdict_from`; terminal failures become Judged::inconclusive with data None | shared invalid-output tests, Agent negative table, hosted-code malformed SDK tables | FAIL: draft-04 accepted data can violate original schema, BEH-1 |
| REQ-007 / AC-005: inclusive compact UTF-8 byte count, required escapes, 16 containers, no truncation; both kinds' execution proof | canonical check_data_bounds uses compact serde_json encoding and object/array depth; both executors share it | pure boundary tests; exact/over hosted-code UTF-8/escape/wide/depth cases in all SDKs; Agent claimed exact case is 65,535 bytes and over case 65,549; no Agent wide cases | FAIL required Agent proof: BEH-2; no production arithmetic defect asserted |
| Invalid output diagnostics omit contents/dynamic returned keys | `verdict_from` emits schema_path and fixed messages, bounds only counts; typed decoders report safe empty root | `verdict_refusal_omits_returned_keys`; Rust nested-map mismatch; TS generic throw table; typed mismatch journeys | PASS |
| Existing citations, Agent uncited-pass downgrade, aggregation, actual/detail, dispatch/status preserved | executor citation processing follows accepted verdict; Judged carries data beside existing fields; aggregate logic unchanged | Agent citation/uncited cases; step/plain/custom fixtures; existing engine regression changes only initialize None | PASS |
| REQ-011 / AC-002: per whole/step requirement, direct/queued/status/HTTP/MCP/published Bifrost; old/plain omit | generic RequirementResult optional field; Judged report; storage still full outcome Variant; existing result/read owners | three SDK readback stories incl retained equality, HTTP runtime tests, MCP custom roundtrip | PASS for correctly accepted data; subject to BEH-1 |
| REQ-011 / AC-007: existing tenancy/query-read/retention/durability, no new metadata/log/audit projection | production storage/result owners unchanged except response data; outcome remains tenant-owned; no new store/column/permission | foreign-tenant/scoped HTTP journey; MCP reads and retained comparisons; narrower Bifrost authority unchanged | PASS within changed boundary |
| REQ-012 / AC-006: actual typed values through verify/wait/status, Rust absolute deadline; ordinary shared transport/lifecycle | Rust ordinary operation then decode_data; Python ordinary verify/wait/status then model_validate; TS ordinary operation then typedResult | custom_verdict_data_is_typed_on_return in three SDKs; Rust wait_until_typed case; runtime and static type fixtures | PASS |
| All present data decoded (failed included); absence skipped; mixed union; no partial returned result | Rust collect<Result>; Python local list conversion before return; TS map before return; no outcome filter | mixed Scored enum/decoder stories, failed well-formed data, plain polite absence, no-result status proof | PASS |
| Exact normal SDK error for arbitrary parser rejection; no payload or decoder diagnostic; raw read unchanged/no rerun | TS catch ignores thrown value; Rust/Python discard decoder diagnostics; all return existing error catalog | TS null/undefined/Error/malformed/keyed exceptions; SDK mismatch/readback/invocation-count cases | PASS |
| Python positional-only verifier/type; evidence slot data_type/group_id/input forms preserved | `#[pyo3(signature = (verifier, data_type, /, *value, group_id=None, **named))]` delegates ordinary verify | Python typed journey/typecheck slots | PASS |
| AC-008: public exports, stubs, generated TS/runtime/HTTP/MCP schemas, docs/authority/examples match | generic types projected through existing facades; error catalog generated; Prompt/Verifier authority and recipes updated; three language tabs | recorded codegen/type/lint/docs checks; source migration consumer inspection | PASS except the exact-schema promise in BEH-1 |
| R1 schemars 1.x/2020-12 migration preserves supported data dialect/root semantics | generator and consumers migrated; data embedded resource with $schema retained | root-ref/collision/prefixItems/draft07 tests; actual draft04 experiment below disproves complete closure | FAIL: BEH-1 |
| R1 missing rustdoc and documentation corrections | Judge type/score field now documented; safe-root docs; Python/Rust/TS recipe tabs | source inspection and recorded lint/docs proof | PASS |
| Scope/non-goals/reuse: no new transport/store/polling/registry/permission/features/configurable bound; dependency direction retained | host callback in Skald, canonical resolver in spec, runtime in existing Vala/server, local SDK conversion | cumulative manifest/source diff; migration matches explicit R1 direction | PASS |
| Historical TDD evidence | task discloses RED absent for original scenarios 1–3; R1 diagnoses compiler/assertion changes | recorded original/R1 evidence inspected; not reconstructable retrospectively | PASS as candid disclosed historical limit, not reconstructed proof |

## Prior finding closure

| Prior ID | Source-based disposition |
| --- | --- |
| FIND-TASK-001-1 | **OPEN (BEH-1)**. 2020-12/07 local refs, named definition collision and later-draft constraints are now covered and fixed. Draft4 is still accepted by check_output_schema, but resolver always inserts `$id`, which is not that draft's identifier. Same original source invariant, not a separate consumer defect. |
| FIND-TASK-001-2 | **CLOSED**. TS typedResult's unconditional catch creates WyrdError without inspecting arbitrary thrown values. The malformed/nullish diagnostic table covers the former crash path. |
| FIND-TASK-001-3 | **CLOSED**. Literal declares_data_object check deleted; handwritten schemas fit one of canonical-derived plain/required-object shapes using existing bounded compatibility owner; composed/ref cases and before-save absence covered. |
| FIND-TASK-001-4 | **CLOSED**. Shared server failure uses schema-owned path; all generic client decoders use root path and do not inspect exception locations/values; dynamic dictionary-key tests exist. |
| FIND-TASK-001-5 | **PARTIALLY CLOSED (BEH-2)**. Hosted-code fixture and three SDK stories now cover required malformed/exact/one-over cases; corresponding Agent exact/one-over/wide proof remains incomplete, including an arithmetic error in its claimed exact case. |
| FIND-TASK-001-6 | **CLOSED**. Local Judge and score now have substantive rustdoc. |

## BEH-1 — Draft-04 data schemas still resolve local references against the Verdict envelope

Classification: **INCORRECT**. Continue **FIND-TASK-001-1**. Obligations: REQ-001/002/007, AC-003/004, R1's preservation of supported effective dialect/root semantics.

Location: `crates/wyrd-spec/src/verification/execution.rs:603–620`, particularly unconditional `$id` insertion at 618–619; supported draft declaration in `crates/skald/skald-spec/src/prompt.rs:246–269`.

A caller declares this accepted object schema:

```json
{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","required":["reason","copy"],"properties":{"reason":{"type":"integer"},"copy":{"$ref":"#/properties/reason"}}}
```

Public `Prompt.openai_chat(..., response_format={"kind":"verdict","data":schema})` compiles and renders it. The resolver retains its draft declaration but only injects `$id: "verdict-data"`. Draft4 uses `id`; `$id` does not create its expected resource. The installed referencing 0.46.4 implementation confirms the distinction at `src/draft.rs:113–114` (`Draft4` analyzes `"id"`, Draft6/7 `"$id"`). Official JSON Schema documentation also specifies the draft-4 spelling: [Structuring schemas](https://json-schema.org/understanding-json-schema/structuring?highlight=defs).

Fresh read-only experiment used the public installed Python binding to emit the actual compiled schema to `/tmp/behavior-draft4-schema.json`, then compiled `/tmp/behavior-schema-probe.rs` against the existing repository-built jsonschema and serde_json rlibs (no Cargo or repository source edits):

```text
mise exec -- rustc --edition=2024 /tmp/behavior-schema-probe.rs -L dependency=target/x86_64-unknown-linux-gnu/debug/deps -L dependency=target/debug/deps --extern jsonschema=target/x86_64-unknown-linux-gnu/debug/deps/libjsonschema-d4dbafcac2763e43.rlib --extern serde_json=target/x86_64-unknown-linux-gnu/debug/deps/libserde_json-b8fd2d102c1e5672.rlib -o /tmp/behavior-schema-probe
mise exec -- /tmp/behavior-schema-probe

                               copy: 2     copy: "wrong"
authored standalone schema     true        false
actual compiled envelope       false       true
same envelope + draft4 id       true        false
```

Both response instances have `data.reason: 1`, envelope `reason: "judged"`, outcome failed and empty citations. The candidate's Rust validator resolves the copy reference to the envelope string reason: valid declared integer copy is rejected and invalid string copy is accepted. This is stronger than the Python validator comparison (which raised unresolvable on draft4); the finding relies on the actual Rust library used by production.

Reachability and shared cause: the producer is the shared Verdict resolver. PromptSpec and inline Agent compilation use it; CodeCheck::response_schema and declaration validation use it too. The output passes check_output_schema (meta-validation only, no user-ref resolution). At execution `AgentExecutor::execute` validates the compiled Prompt response_type, while hosted CodeExecutor validates CodeCheck's schema; both call the unchanged `verdict_from` jsonschema::validator_for. For a failed response no pass-citation downgrade interferes; accepted data flows through Judged::checked/report into persisted ordinary results. No read-side correction can restore the lost judgment semantics. Existing sibling handwritten schemas are not rewritten by this resolver and must remain unchanged.

Observable consequence: invalid data can produce a real failed/passed judgment and be retained, while valid check data becomes inconclusive. Declaration does not reject this supposedly unsupported composition; source and the public compiler both accept it.

Smallest testable correction: keep the existing resolver and standard embedded resource mechanism, but choose/preserve the resource identifier appropriate to the supported effective draft (`id` for Draft4, `$id` for later drafts), retaining authored identity/dialect and original local-reference meaning. Do not add a walker, another validator, flattening or consumer guards. The experiment adding draft4 `id` demonstrates that the installed library already supports the needed standard semantics. Extend the existing resource-root regression table with anonymous and authored-id draft4 schemas, integer good/string bad values; exercise the affected code/Agent executor paths through their existing focused proof. Unsupported combinations must fail before persistence through existing compiler errors rather than silently changing meaning. Preserve fixed bounds and all typed/storage behavior.

## BEH-2 — Claimed Agent exact-boundary proof is not exact and lacks wide-array execution

Classification: **MISSING verification** (not a demonstrated production bounds bug). Continue **FIND-TASK-001-5**. Obligations: REQ-007, AC-005, original scenario 2 and R1 required corresponding Agent cases.

Location: `crates/vala/vala-verify/tests/integration/blocks_agent.rs:603–642`, `check_agent_validates_custom_data`. `note(bytes)` produces `"é".repeat(bytes / 2)`. `exact` is 65,536 minus the 21 compact bytes of `{"score":1,"note":""}`, i.e. 65,515. Integer division therefore produces 65,514 bytes and the complete accepted object is **65,535**, not 65,536. Its refused size value adds the 14-byte `s3cr3t-payload`, producing **65,549**, not 65,537. A small independent standard-library JSON calculation reproduced those totals with ensure_ascii=False/compact separators; the same arithmetic follows directly from the Rust source.

The inventory beyond this function does not supply the missing Agent proof: the Rust/Python/TS `EXACT-BYTES`, `EXACT-WIDE`, `OVER-BYTES`, `OVER-WIDE`, `DEEP-*` journey markers are implemented by the **well-formed hosted-code** check in scored-review.yaml. Their local scored Agent providers only handle BAD-DATA/HUGE-DATA; helpful remains a small valid Agent result for exact markers. Agent integration has 16/17-container and malformed/plain cases, and a small escaped-string case, but no exact/one-over compact escaped-string or wide-array case. Shared pure check_data_bounds tests prove arithmetic, and code journeys prove hosted execution; neither establishes the explicitly required Agent execution boundary cases.

Observable consequence: the recorded claim that exact Agent boundaries are covered is false; an Agent-specific acceptance/read path regression at the boundary is not protected by the required evidence. Production uses a shared bound owner, so no production correction is proposed on this evidence alone.

Smallest testable correction: extend the existing Agent executor test table with actual compact-byte assertions for 65,536 and 65,537 objects (including UTF-8/required escapes and wide arrays), keeping accepted data equality and terminal safe rejection assertions. Reuse the existing executor/provider fixture and check_data_bounds; no new harness or redundant SDK suite is necessary. Keep existing malformed/16/17/citation cases intact. Run the existing exact selector `mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_validates_custom_data)'`. Hosted-code fixture/journey cases need no repetition unless changed.

## Verification limits

Fresh proof was read-only public Prompt compilation/rendering, actual installed Rust-validator experiments, independent JSON byte arithmetic, and cumulative source/diff inspection. No repository source was modified; only this report is owned. No Cargo test, Postgres/server journey, provider network request, broad gate, formatting/codegen mutation, or new report from another reviewer was run/consumed. Original/R1 green checks remain recorded claims supported by inspected tests, with the two gaps above explicitly falsified. The draft04 probe links existing built artifacts rather than rebuilding them; emitted schema structure was checked against the candidate resolver and the installed 0.46.4 validator source. Live strict provider support for embedded resources remains the previously disclosed implementation limit.
