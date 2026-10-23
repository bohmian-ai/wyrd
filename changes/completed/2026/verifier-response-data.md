# Verdict formats and typed custom verification data

- Change: `verifier-response-data`
- Specification: `SPEC-verifier-response-data`, approved revision 1
- Completed: 2026-10-22
- Reviewed target: `6709046e2` (TASK-001 with remediations R1 and R2; base `fb1fd9234`)
- Delivery reference: not supplied

Authors no longer copy Wyrd's Verdict JSON Schema into Prompts. `response_format: verdict` (or `{kind: verdict}`) compiles the closed Verdict from the canonical `Verdict` type for portable and native Prompts, inline or referenced. `{kind: verdict, data: <schema>}` adds a required custom `data` object. Code checks declare the same forms on the check beside `entrypoint`; omitting it keeps the plain Verdict. Cards keep and hash the authored form. Compatible handwritten schemas still work when they explicitly require an object `data`.

The server validates every Agent and hosted-code response against the exact composed schema. Malformed output, missing, wrong, null or undeclared data, and unknown envelope members make the requirement inconclusive with no data and a reason naming the schema location or violated limit, never the payload. Data is bounded at 65,536 compact UTF-8 bytes (required escapes count) and 16 containers, inclusive. Valid data appears as `RequirementResult.data` for whole-Verifier and step requirements through direct, queued wait/status, HTTP, MCP, SDK, and retained Bifrost outcomes, under the outcome's existing tenant isolation and query-read permission. It never affects aggregation, citation rules, or dispatch.

Rust, Python, and TypeScript expose `VerificationResult<T>` with typed verify, wait, status (and Rust `wait_until_typed`). Every present data object is decoded; absent data stays absent. A mismatch returns `WYRD_SDK_422_VERIFICATION_DATA_INVALID` with the requirement and the safe root path, never the payload, and never reruns or changes the stored result.

## Lasting invariants

- A `data` schema is JSON Schema 2020-12 without a root `$id`. It is embedded as its own resource under a private id so its local `$ref`s resolve as they do standalone; any other declared draft or authored root `$id` is refused before the Card persists.
- Limits are fixed and server-owned; rejected data is never truncated or retained.
- Skald owns the compiler but no format names; Wyrd resolves `verdict`.
- Typed decoding is client-local and never changes server behavior.

## Decisions and deviations

- `schemars` moved to 1.x so the generated Verdict is 2020-12 and can embed a data schema as its own resource rather than hoisting definitions.
- The owner chose to refuse other drafts and authored root `$id` rather than preserve them, after review attempt 03 showed that draft-04 and authored identities could rebind data references to the envelope.
- Review attempts 01–03 returned FIX_REQUIRED and were closed by R1 and R2. The owner directed completion after R2 without a further independent re-review or final change review; R2's closure rests on its implementation evidence only.

## Acceptance closure

| Criterion | Evidence |
| --- | --- |
| AC-001 named formats, handwritten compatibility | Compiler/validator tests (`validate::tests::named_verdict_formats_validate`); SDK building-block journeys |
| AC-002 data through every read | `custom_verdict_data_survives_readback` in Rust, Python, TypeScript; HTTP/MCP route tests |
| AC-003 refusal before persistence | `pg_verification_routes::verdict_declarations_validate_before_persistence`, including older-draft and authored-`$id` cases |
| AC-004 invalid output inconclusive | `blocks_agent::check_agent_validates_custom_data`; `invalid_custom_verdict_data_is_inconclusive` in three SDKs, including plain code returning undeclared data |
| AC-005 exact bounds | Agent and hosted-code 65,536/65,537-byte UTF-8/escape and wide-array cases, 16/17 containers |
| AC-006 typed returns | `custom_verdict_data_is_typed_on_return` and mismatch stories in three SDKs; Python/TS typechecks |
| AC-007 isolation | `custom_verdict_data_is_tenant_isolated` and scoped foreign-read refusals |
| AC-008 authoring, exports, docs | Python/TS model authoring; stubs incl. returned `CodeCheck.response_format`; docs and generated `llms-full.txt` |

## Current owners

- [Wyrd design: Prompt and Verifier](../../../architecture/wyrd-design.md), [Bifrost storage](../../../architecture/bifrost-design.md), [security posture](../../../architecture/wyrd-security-posture.md).
- [Verdict contract and resolver](../../../crates/wyrd-spec/src/verification/execution.rs), [declaration validator](../../../crates/wyrd-spec/src/card/verifier/validate.rs), [Prompt schema checks](../../../crates/skald/skald-spec/src/prompt.rs).
- [Verdict validation](../../../crates/vala/vala-verify/src/blocks.rs), [Agent check tests](../../../crates/vala/vala-verify/tests/integration/blocks_agent.rs), [route tests](../../../crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs).
- [Shared fixture](../../../fixtures/cards/verifier_building_blocks/scored-review.yaml) and SDK stories: [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs), [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py), [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/verifier-building-blocks.test.ts).
- [Verifier Card reference](../../../docs/src/content/docs/reference/verifier-card.md).
