# Independent contracts domain review

Result: **PASS**. No material proposed findings in this domain.

## Subject and boundary

Reviewed immutable detached source `/tmp/wyrd-task006-review-622800c7a`, candidate `622800c7ad18c8b28cb5a2ea84cd568e2a085cb6`, exclusive base `b323327f4cbb690628303d3a6b5249a67af50c9b`. The complete cumulative diff includes the first implementation, both remediations, and r11 withdrawal; the latest asset correction was not treated as the whole subject. Source had no `.codegraph`, so ordinary Git/source navigation was appropriate. No current-round peer report or conclusion was read.

Boundary: the retired public evaluation contracts and their replacements, shared Rust engine primitives, Rust/Python/TypeScript accessor/export and declaration consistency, GenAI projections, served HTTP/MCP proof, and the latest product-reference producer/direct consumers. Database migration correctness, security implementation and fresh visual rendering are separate domain responsibilities; their public refusals and contract implications were inspected here.

## Authority and source coverage

| Boundary | Governing authority | Source / consumers inspected |
|---|---|---|
| Contract retirement and allowed exceptions | Approved spec r11 REQ-017–022, INV-001/002/008/009, AC-009/011–016; original TASK-006; prior immutable R1/R2 remediation and execution evidence | Cumulative Git diff; `wyrd-spec` Card/vala module retirement, verification contracts, IDs/operators/planning, schema generator, error catalog and generated TS types/errors |
| Current Verifier ontology | Wyrd design §Verifier; doctrine verification/object model; AGENTS ownership/contract rules; reference router and evaluation reference | `card/verifier/mod.rs`, `verification.rs`, execution contract and reference traversal consumers; current engine and server-host imports |
| SDK contracts and documentation | AGENTS §§7–9/11; Python API/stub and TypeScript references; maintainer-style contract documentation guidance | `wyrd-client/src/state.rs::verifier`; Python native state accessor, authored state stub; TS public accessor, native bridge and generated declarations; original/R1 typing and runtime evidence |
| Agent-facing contract proof | Agent-harness reference, AGENTS runtime OpenAPI/MCP proof rules; r11 served-assertion requirement | `pg_openapi_contract.rs` matcher and five-operation/local-upload test; server OpenAPI removal; MCP query negative journey and actual `list_all_tools` assertion; Rust/Python/TS public negative/current controls |
| GenAI preservation | Spec REQ-019 standard exception and INV-009; telemetry-observation reference | `gen_ai_evaluation_result.rs`, `gen_ai_span.rs`, trace re-exports/tests/attribute constants; cumulative rename diff and consumer search |
| Engine primitive preservation | REQ-019 source-backed retention; no replacement engine under INV-001/009 | Base and candidate JSONPath, assertion, actual capture/hash, trace projection, stage walk/ledger; operators, engine, scope/Agent/drift/tools imports, moved plan/IDs/operator contracts and surviving tests |
| Reference producer and direct consumers | REQ-017/018/022, R2 selected correction and explicit exclusions | Product README; `cards.svg`; Experiment runs/outputs/mobile SVG XML, retained E-04/E-07/E-16/EM-02 text in current source; comparison to current Verifier declaration |

## Contract and preservation matrix

| Obligation | Implementation evidence | Proof assessed | Result |
|---|---|---|---|
| Delete obsolete authoring/execution contracts without aliases | `card/eval.rs` and `vala/eval` modules removed; schema generator stops exporting CardEvalSpec; obsolete error variants and generated TS error union entries removed; retained paths resolve through `verification` | Cumulative source and import/export searches; committed codegen/spec/typing checks | PASS |
| Preserve one current declarative Verifier | `VerifierSpec` remains subject-less inputs/steps/requirements/decide/context_capture, without implementation discriminator; shared execution contracts remain current | Source compared to design §Verifier and approved spec | PASS |
| Correct accessor documentation at all projections | Rust returns borrowed pure spec; Python returns kind-checked persistent envelope; TS returns registered envelope. Docs correctly explain each shape and direct `spec` fields, and Python stub no longer claims startup describes obsolete observation table | Native bodies, authored stub, TS wrapper/native declaration inspection; recorded R1/type checks | PASS |
| Move live helpers while deleting unused workflow machinery | Current engine calls retained assertion/JSONPath/capture/trace/walk owners; base-to-candidate engine diff changes imports/types/docs, not judgment algorithm. Plan removes obsolete EvalTask wrapper/error conversion; ID module removes scenario-only identity, retaining TaskId/JsonPath validation | Fresh in-memory body comparison: extract_jsonpath_from/extract_jsonpath_values/values_to_value, evaluate_single/evaluate_item_mode, capture_actual/hash_context_value, project_spans/project_one and walk all equivalent after approved type/error rename and comment/whitespace normalization; recorded engine49/P9/P10 proof | PASS |
| Preserve GenAI standard semantics and content | Projection is `GenAiEvaluationResult` and `evaluation_results`; field shape, value validation and external `gen_ai.evaluation.*` constants preserved; no old projection alias. Canonical stored span/OTel payload owners are not rewritten by this rename | Source rename comparison and consumer search; recorded P8a–c; span test asserts two entries, equality after roundtrip, and no abbreviated alias key | PASS |
| Served HTTP contract rejects retired product identifiers | Test fetches actual `/openapi.json`, asserts the current operations and local upload, then applies retired_vocabulary to serialized served document. Matcher distinguishes abbreviation/product names from prose and permitted standard projection | Full matcher/test diff; original and R1 exact served/principals execution records, including restored-tag RED/prose allowance | PASS |
| Runtime MCP/public SDK paths refuse old identities and retain controls | MCP journey fetches real catalog, rejects old tool tokens, seeds stale catalog entry, refuses describe/query and compares absent namespace; SDK journeys use public language clients, reject stale grants and query/describe/write-table old names, then query current verification table | Source of P4–P7 and adjacent exact Verifier grant/current behavior controls; committed exact focused GREEN evidence | PASS |
| Finish reference-model retirement at producer and consumers | C-08/E-17 removed in both themes; E-07 has no scored-task aggregate or canonical evaluation-record promise. E-04/E-07/E-16/EM-02 section links now point only at retained Summary/Metrics/Files/Provenance/Traces. E-07 quality Verifier pass and existing Verifier link fit current ontology | Fresh XML parsing and artboard-ID/text inspection; R2 committed XML/count/render record. Preserved metrics, stage timing, output inventory, sessions/traces and provenance content inspected | PASS |
| Honor r11 scanner withdrawal and R2 exclusions | No permanent vocabulary scan required or reinstated; served contract test retained. No request to remove general Workflow display debt or E-07 review-loop link | r11 source/evidence, R2 task, cumulative diff | PASS |

## Prior findings within this lens

- FIND-TASK-006-1: repository scanner obligation is withdrawn by r11; remaining served-OpenAPI contract assertion is present. Historical scanner evidence is not current acceptance authority.
- FIND-TASK-006-2: accessor docs now describe direct building blocks and distinguish spec versus envelope return shape. No stale implementation-discriminator instruction remains at the reviewed accessors.
- FIND-TASK-006-3: bounded changed helper/contract documentation now explains the retained behavior, including actual hashing bytes and cancellation of the shared walk. This review does not broaden that sweep to byte-equivalent moved tests or import-only callers.
- FIND-TASK-006-4: current XML producer and retained direct section consumers no longer teach the obsolete scored-task/Workflow evaluation result model. R2 expressly excludes general C-01/C-14/Service Workflow cleanup and E-07 unrelated review-loop link, which were not reopened.

## Verification limits

No build, generator, migration, live server, external call, or test suite was run during this review. GREEN execution counts are committed implementation/R1/R2 records checked against the corresponding source, not fresh executions. Fresh work was read-only Git/source inspection, in-memory base/candidate helper comparison, and parsing/extracting current SVG artboard IDs/text. No fresh image was rendered by this role; both-theme readability remains with the independent UI role and supplied R2 evidence, with its fallback-font/cropped-sheet limits.

Full SQL/Bifrost/Verifier journey qualification remains integrated change-review work. Historical P2 RED is absent, P3 claim branch is unseeded, and public corrupt-role INTERNAL/retired SQL invalid-query mappings remain explicit evidence limitations, not claims of a distinct new error contract. TASK-005 prerequisite FIX_REQUIRED is not converted into PASS by this domain review. No source, task, authority or Git mutation occurred.

No new finding ID is proposed. This is a contracts-domain PASS, not a complete change or prerequisite verdict.
