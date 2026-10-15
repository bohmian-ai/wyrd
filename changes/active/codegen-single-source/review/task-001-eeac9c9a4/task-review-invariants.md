# TASK-001 independent invariant review

Result: **FAIL**

Subject: base `5bcd9e4872f1376f1722922fa63b67384243be04` → candidate `eeac9c9a45b599b1727086ba30582d3f02b3230a`. HEAD was checked against the candidate. No sibling discovery reports were read. Reviewed source was not edited. No Cargo command was run.

## Proposed finding

### INVREV-001 — INCORRECT / REGRESSION — response lowering loses serialized field presence

- Severity: Important.
- Violated obligations: REQ-007 (correct optionality/nullability to the Rust contract), INV-003 (preserve public typing precision), AC-006.
- Root location: `scripts/gen_ts_types.py:205–225`, especially the output-mode `default` heuristic at 215–220. Derived examples: `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:6655–6683` and `6718–6748`.
- Producer-to-sink evidence: Rust `WorkflowRun` and `WorkflowStepResult` (`crates/wyrd-spec/src/card/workflow.rs:775–839`) derive Serialize and have Option fields with **no** `skip_serializing_if`. Their keys are serialized even when the value is null. `NativeWorkflow::run` (`sdks/wyrd-sdk-ts/native/src/workflow.rs:203`) passes the snapshot through `NativeLifecycleResult::outcome` and serde JSON projection (`native/src/lib.rs:181–184`). The public `Workflow.run` returns that projection as `WorkflowRun` (`wyrd/src/index.ts:1804–1805`). The new lowerer interprets the deserialization schema's missing `required` entry as optional unless `default` is present. Consequently it publishes `started_at?: string | null`, `workflow?: CardRef | null`, and optional step `text`, among others, although those keys are always present. The base declared these response keys required and nullable.
- Sibling authorities/consumers: the same producer affects `CardRegistrationOutcome.artifact_hash/card_blob_uri` (`registry/submission.rs:102–119`), `ProviderCredentialView.rotated_at/revoked_at` (`gateway/credential.rs:192–210`), `PrincipalSummary` nullable fields and `PrincipalPage.next` (`auth/tenant_principals.rs:224–249`), and their existing public Cards/Gateway/Principals consumers. The converse appears in Bifrost table fields: `canonical_physical_fingerprint` explicitly skips None (`vala/api.rs:129–130`), yet the generated output adds null to the previously precise optional-string type; `tests/types/table-description.test-d.ts:12` was changed to accept this widening. One schema-direction problem owns these cases; separate consumer guards are inappropriate.
- Observable consequence: valid existing TS applications fail to compile, e.g. `const started: string | null = run.started_at`, while the server/runtime still guarantees the original shape. Types also admit response values Rust never serializes. This is a public-contract precision regression, not merely a stylistic preference.
- Independent proof: an in-memory TypeScript program was checked using the installed compiler through `mise exec -- node --input-type=module`; it imported the candidate `WorkflowRun` and assigned `run.started_at` to `string | null`. The diagnostic was `Type 'string | null | undefined' is not assignable to type 'string | null'`. No fixture or generated source was written.
- Smallest required correction: preserve Rust serialization presence/nullability in the existing transient Rust-contract producer and existing TS lowerer, rather than inferring it solely from deserialization `required`/`default`. Keep input defaults and omission support, keep skipped output fields optional and non-null when appropriate, and keep always-serialized nullable output fields required. Regenerate the existing module. Do not restore handwritten wire interfaces or introduce a second schema tree/generator. Add focused compile assertions for a required-nullable response field and a skipped optional response field; retain the table-description assertion's original precision. Audit the sibling response roots through the same correction.

### Existing dependency limit relevant to correction

The pinned dependency is schemars 0.8.22 (`Cargo.toml:121`). Its `SchemaSettings` (`schemars-0.8.22/src/gen.rs:24–51` in the local Cargo registry) has no serialization/deserialization contract mode. `option_nullable` and `option_add_null_type` only change Option schema representation; they do not recover field presence. The existing derive (`schemars_derive-0.8.22/src/schema_exprs.rs:449–480`) computes requiredness for deserialization and does not retain `skip_serializing_if` as schema metadata. A visitor over the emitted bundle cannot recover information already lost. Do not recommend flipping a nonexistent settings switch, globally requiring nullable fields, or manually patching generated types. The shared producer must retain authoritative serialization metadata before lowering; if doing so requires a new generator framework or dependency change, the task's material stop condition applies and that approach must return for authority revision rather than silently accept weaker typing.

## Acceptance matrix

`PASS` below means the reviewed implementation satisfies the obligation on inspected source and the available verification evidence; implementation-run claims are distinguished from independently run checks below.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 persisted schema/fixture retirement, UI examples retained | Complete diff removes schema/fixture JSON; gen_schemas default mode writes UI examples/error codes only; UI import retained | Diff inventory; implementation codegen claim | PASS |
| REQ-002 five MCP schemas from owners; preserve catalog behavior | gateway gated descriptors use existing principals::schema_of for exactly the five DTOs; tool metadata, permission paths, dispatch retained | Existing focused gateway catalog test reported passing | PASS |
| REQ-003 docs cleanup and meaningful enum check | llms generators retain content index; inactive generators/inventory retired; diagram test uses Rust bundle definitions | Deleted tests inspected and limited to snapshot equality; docs:check reported passing | PASS |
| REQ-004 native complete CardEnvelope retrieval/identity/aliases | PyCards::get returns retained Card plus requested identity pinned to server UID; shared assert_selector_identity validates; getters serialize retained components; hydrate_envelopes still rejects empty aliases | Updated public real-server retrieval journey checks identity, native kind, mappings, empty aliases, dumps; integration claims | PASS |
| REQ-005 annotation/tool/callable behavior retained | annotation_to_schema and decorator remain; only unused output converter retired; tool overloads retain callable signatures | Source caller tracing; existing unit lane reported passing | PASS |
| REQ-006 one authored stub source and shared pure-Python definitions | Templates/assembler retired; inline private definitions reexported; root Card replacement removed; native declarations retained | Independent py:lints and py:typecheck PASS; moved runtime bodies compared against base | PASS |
| REQ-007 all existing TS wire contracts Rust-derived with precise fields | New transient roots feed existing lowerer; public index aliases generated contracts; summary timestamps/enums added; output field-presence defect INVREV-001 | Existing type fixtures cover CardSummary/discrimination, but omit required-nullable parity; independent compiler reproduces regression | FAIL |
| REQ-008 TS root/subpath/native declaration pipeline retained | package.json still points root at dist/index.d.ts and otel subpath declaration; NAPI source/generation unchanged | ts:napi:check reported passing; independent existing TS compiler lane PASS | PASS |
| REQ-009 clean pipeline/packaging/CI/rules | mise codegen no longer touches authored stubs; py setup no assembly dependency; check_stubs preserves docs/layout/exports; CI/docs and governing text updated | Independent Python lint/typecheck PASS; implementation codegen/docs/CI-selection checks reported PASS | PASS |
| REQ-010 required three lints/typechecks and one scenario | Single retrieval scenario retained; no added behavioral scenario matrix or dependency | Review verification records all three lints, Python typecheck and TS compiler PASS | PASS |
| INV-001 server ownership, validation, errors, permissions, audit, tenancy, durable formats unchanged | Server diff changes schema acquisition and type-name imports only; client identity guard retained; no durable engine/state changes | Complete production diff and existing MCP dispatch/authorization inspected | PASS |
| INV-002 deliberate Python break only; no compatibility path/runtime wire changes | Generated API removed without fallback/alias; registry reads remain read-only with no artifact/hydration; native authoring remains | Retrieval/hydration consumers updated; existing integration claims | PASS |
| INV-003 one declaration per owner and precision retained | Shared Python protocols/native reexports consolidated; TS output broadens guaranteed response fields | INVREV-001 source/compile evidence | FAIL |
| INV-004 reuse dependencies/feature boundaries; no new framework | No dependency/feature changes; existing schemars, lowerer, NAPI and tsc reused | Manifests/diff inspected | PASS |
| INV-005 no weakened meaningful checks/serde/validation | Snapshot-only Rust modules removed; behavioral serde/validation tests remain. Table output assertion widens to accommodate faulty projection | INVREV-001; snapshot removals reviewed in full | FAIL |
| AC-001 artifact removal plus UI consumer | REQ-001 source | Inventory/codegen evidence above | PASS |
| AC-002 MCP catalog semantics | REQ-002 source | Focused catalog result above | PASS |
| AC-003 docs/index/enum agreement | REQ-003 source | docs:check claim and transient enum consumer inspected | PASS |
| AC-004 complete exact envelope; registry vs hydration aliases | REQ-004 source | Public journey and existing hydration validation | PASS |
| AC-005 generated Python surface retired; exports/tool behavior work | REQ-005/006 source | Independent Python checks plus unit/integration claims | PASS |
| AC-006 Rust-derived precise TS contracts/root/native declarations | REQ-007/008 source | INVREV-001 proves missing response precision despite green existing checks | FAIL |
| AC-007 all language lint lanes unchanged and passing | mise lane bodies and manifest lint configuration retained | Independent review verification PASS for all three | PASS |
| AC-008 pipeline/docs/rules closure and reproduction | REQ-009 source | Implementation reproduction checks reported PASS | PASS |
| Non-goals: no new capabilities/endpoints/Bifrost behavior/kinds/data migration/deploy/publish/Git actions/unrelated UI | Complete diff is contract/tooling/SDK projection cleanup; no such new runtime surfaces | Diff and manifests inspected | PASS |

## Invariant and failure-path coverage

- Registry retrieval follows the existing Rust Cards owner and error/identity validation before constructing the Python envelope. Missing refs, transport failures and mismatched identity still return typed errors. Allocation/JSON conversion remain at the Python boundary. Returning empty registry aliases does not relax hydrated-state construction validation.
- Pure-Python Bifrost, SessionMemory, Card, gateway payloads, observe and testing CLI definitions were compared with their pre-move runtime definitions. The move reuses their implementation and native calls; no duplicate transport or lifecycle owner is introduced. Bifrost stream exception/cancellation cleanup stays present.
- MCP request descriptor production now derives the very same DTOs execution parses and reuses an existing schema helper. Names, effects, permission filtering, request execution and errors remain in their previous owners.
- Contract extraction retains the existing Rust example and schema derives; lowering retains one TS module, reachable definitions and discriminated Cards. Known contract fields remain typed, with the response presence/nullability defect identified above.
- Docs depend on transient definitions, not deleted JSON paths; codegen checks the surviving consumed outputs; production/testing package boundaries are unchanged.
- No independent optional architecture, extra schema service, compatibility layer, or broader test scenario is requested as a correction.

## Verification limits

Read `verification.md` only as command evidence supplied by the orchestrator, not as a discovery conclusion. It records independently successful Rust/Python/TypeScript lints, Python typecheck and the existing TS compiler body. Implementation task evidence additionally claims unit/integration, focused gateway, codegen, docs, NAPI and format checks; these were not rerun here. The exact reported RED/GREEN retrieval assertion matches the changed implementation and test. No failure-driven test retry/skip change was found in the inspected behavioral tests.

No broad gate, fresh native build, runtime journey, docs generation or Cargo check was run by this reviewer. Residual risk is generated SDK contract precision not exercised by the existing green compile fixtures.
