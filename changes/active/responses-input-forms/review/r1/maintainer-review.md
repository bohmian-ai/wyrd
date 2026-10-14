# Independent maintainer review

## Subject and result

Base `807c4fe68`; candidate `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`.
Approved specification and original task: `changes/active/responses-input-forms/spec.md` revision 1. Review inspected the complete cumulative diff and current surrounding source. Source was not modified.

Overall: **FAIL**. Two proposed findings require independent validation. No layout, naming, private helper preference, or unrelated migration finding is proposed.

## Authority and changed-surface coverage

Applied AGENTS.md §§2–10, §11, §15–16; architecture/agent-rules.md; maintainer-style; spec-driven-development; Rust-core; PyO3-boundaries; Python-api-and-stubs; TypeScript-guide; active design's Prompt/provider-native envelope and doctrine's Skald/Python boundary. CodeGraph was used before source discovery, followed by direct source and caller searches where its returned excerpts were incomplete.

| Changed surface | Inspected owners, callers and proof | Assessment |
|---|---|---|
| `OpenAiResponsesInput::Items`, `easy_items` | Complete decoder body and `items`, `items_mut`, `user_message`, existing shorthand tests; `OpenAiResponsesRequest`, transparent `GatewayResponsesRequest`, `MessageNum` array decoder, request-builder extraction and reconstruction | Small pure helper next to the wire owner; no retained state that would earn a service struct. Schema/declaration parity gap below. |
| `OpenAiResponseItem::FunctionCallOutput`, `OpenAiFunctionCallOutput::{text,from}` | Full enum and methods, all changed writers and Python consumer; content-part enum; existing Responses loop fixture | Closed text/parts enum is conventional, `From<String>` preserves existing authoring, `Cow` has actual borrowed and owned branches. No duplicate transport or workflow owner. |
| `request_builder::tool_result_message` | Entire match and tool-result callers; Responses extraction; integration loop | Conversion is mechanical; existing error/tool JSON behavior retained. No new owner or dependency needed. |
| `Prompt::with_tool_result` | Complete method, neighboring role edit methods, retained Prompt/request owner, public Python method delegation | Conversion is mechanical and remains on existing owner. No unrelated capability change found. |
| `PyOpenAiResponseItem::as_function_call_output` | Complete Python item wrapper, producer `PyOpenAiResponsesRequest::input`, module registration/export and public prompt `.pyi` accessor | Existing signature stays `str`, but new parts projection has undocumented data loss. Rustdoc hard gate below. |
| `skald-agent/tests/integration/loop_responses.rs` | Entire modified loop test, helper builders and expected request/result; neighboring session replay test | Existing test still asserts exact native replay and outcome; changed expression is mechanical. |
| Nine generated JSON schemas | Every changed schema parsed and checked; all contain identical text/parts union and input items still reference the message variant requiring `type`, `role`, and array content | Tool result union aligns; easy input forms absent. |
| Two eval schema fixtures | Both parsed and checked with same union/message assertions | Same source-driven contract limitation, no separate cause. |
| Python `_card_types.py` | Full changed alias, function-output dataclass and codec descriptor entries; existing content-part and input aliases | Output union and codec descriptors agree; easy input representation absent. |
| TypeScript `card-types.ts` | Changed alias/interface plus full message/input/request declarations | Output union aligns; easy input representation absent. |
| Approved spec | Full original spec | No findings about task packet absence: user explicitly made this spec the task. |

The existing alternative to new parsing is the provider-native Responses wire owner and its existing text shorthand expansion. Neither previously accepted easy item objects: `OpenAiResponseItem` is a `type`-tagged enum whose `Message.content` is an array. Extending that owner is sufficient; the candidate does so with a narrowly scoped synchronous Serde hook, not a second gateway parser or workflow. Existing `OpenAiResponseContentPart` also provides the required content-part vocabulary, so the new output enum reuses it rather than duplicating parts.

## Proposed findings

### MNT-001 — Missing rustdoc for materially changed Python text projection

- Classification: **VIOLATION**; blocking under the explicit rustdoc exception.
- Location: `crates/skald/skald-prompt/src/python/openai_responses.rs:571`, `PyOpenAiResponseItem::as_function_call_output`.
- Authority: AGENTS.md §16 requires substantive rustdoc for every materially modified Rust method, and `# Errors` for every fallible method. Maintainer-style requires signatures and documentation to describe the same operation.
- Evidence: The complete method has no rustdoc. Previously it copied a string. It now calls `OpenAiFunctionCallOutput::text().into_owned()`: parts are concatenated without separators and image/file parts are skipped. Public `sdks/wyrd-sdk-python/python/wyrd/prompt/__init__.pyi:900` still describes returning the `output`, whereas for a parts-valued output the method returns only its extracted text. Wrong item kinds still produce `wrong_variant`.
- Reachability: Responses request input projects each decoded item to `PyOpenAiResponseItem`; the class is registered in the existing public prompt projection, so Python callers can invoke this method on the newly admitted parts-valued output.
- Concrete maintenance cost/consequence: A maintainer or Python caller cannot discover from the public accessor that parts, their boundaries and media are intentionally discarded, or the error conditions. More directly, the changed method violates the repository's explicit merge-blocking documentation gate.
- Smallest correction: Document this existing method's text-view semantics, part concatenation/media skipping, and wrong-kind error; update the owning declaration-generation source and regenerate declarations where needed. Do not add a second accessor or broaden the Python API solely to close this finding.
- Closure proof: Inspect source rustdoc and regenerated public declaration for the same semantics and unchanged `str` return/wrong-kind behavior; owning codegen check. This is a documentation correction to batch with substantive remediation, not a reason for a separate style-only round.

### MNT-002 — Published input types omit the newly admitted easy-message form

- Classification: **INCORRECT** public-contract parity.
- Locations: `crates/skald/skald-spec/src/wire/openai_responses.rs:291` (Serde-only hook); `crates/wyrd-spec/schemas/prompt_spec.json:3385` (derived input array); `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:2557` and `:2640`; corresponding Python input/message definitions. Same source reaches all nine generated schemas and both eval fixtures.
- Authority: Maintainer-style's typed-contract section requires public signatures/generated declarations to state the actual accepted fields; AGENTS.md §§2, 8–9 require wire contracts, generated schemas and language projections to remain aligned. Approved REQ-001 explicitly adds easy input messages to this request decoder.
- Evidence: `easy_items` accepts `{role:"user",content:"go"}` and missing-`type` array-content messages. The schema still publishes `OpenAiResponsesInput` as string or array of `OpenAiResponseItem`; its message branch requires `["content","role","type"]` and array-only `content`. TypeScript's message interface likewise requires `type:"message"` and an array. Generated Python message construction has the same required discriminator and list shape. Transparent `GatewayResponsesRequest` derives the same request schema via `OpenAiResponsesRequest`, so this is the public request contract as well as Card authoring metadata.
- Observable consequence/concrete cost: A client validating against the published JSON schema rejects the exact easy message that the new gateway decoder accepts. A TypeScript author following the generated request type cannot supply that required form without a cast. The new documentation claims easy messages are accepted, but the machine-readable contract disagrees. Future maintainers have two incompatible definitions of the request.
- Smallest correction boundary: Keep normalization on the existing Responses wire owner, and make that owner's input schema describe the easy-message alternatives that its decoder accepts. Regenerate the existing JSON/Python/TypeScript projections through the owning generator; do not add another gateway parser or hand-edit generated artifacts. Retain the existing canonical typed message representation and existing strict rejection of non-message items that do not decode.
- Focused closure proof: The captured `{role:"user",content:"go"}` and the newly supported optional-`type`/text-content variants validate against the generated request input schema; canonical typed message/function-call/output items still validate; malformed items still fail. The generated TypeScript/Python input union must expose those accepted request forms. Run codegen check plus the narrow owning schema/type checks.

## Calibration and verification limits

No findings are proposed solely for the enum's public module placement, use of a private Serde helper, `Cow`, existing owner-crate PyO3 placement, or the mechanical `.into()` changes. None creates duplicate lifecycle, transport, or durable authority.

This pass executed only read-only source/diff/CodeGraph inspection and an assert-free JSON structure inspection of all eleven changed schema files. Runtime verification and codegen results are supplied by the orchestrator; this report does not claim those lanes passed. The original spec openly records the missing Agents SDK TypeScript journey. Its acceptance consequence belongs to the task/standards reviewers, not a maintainer preference. No source edits, new tests, generators, live provider calls, or additional verification lanes were run by this reviewer.

Orchestrator verification update received after the pass: `fmt:check` passed; exact captured-body decoder test passed; `test:skald` passed 345/345. Lints and codegen remained pending at report submission. These results establish the checked runtime behavior but do not resolve either documented contract finding.
