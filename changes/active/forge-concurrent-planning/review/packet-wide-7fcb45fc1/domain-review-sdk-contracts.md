# SDK, contracts, and Python domain review

## Immutable subject

- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Original implementation base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Reviewed ranges: the cumulative `c1508b375..7fcb45fc1` contract/SDK/Python
  surfaces and the complete remediation delta `e8d3cca13..7fcb45fc1` for those
  surfaces.

The checkout had advanced past the candidate, so every source assertion below
was made against the immutable commit with `git show`, `git grep`, or
commit-scoped diffs.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Approved compaction contract | Spec revision 11 REQ-011 through REQ-013; TASK-004; TASK-004-R1; prior `FIND-TASK-004-1` and `FIND-TASK-004-2` | PASS |
| Canonical wire and generated schemas | `crates/wyrd-spec/src/vala/api.rs:143-170,390-421,2649-2719`; checked-in request/description schemas and schema goldens | PASS |
| Shared and first-class Rust SDK | `crates/shared/wyrd-client/src/bifrost/{mod.rs:54-67,table.rs:57-65,229-248}`; `sdks/wyrd-sdk-rust/src/lib.rs:27-77` | PASS |
| Python compaction projection | `sdks/wyrd-sdk-python/src/bifrost/mod.rs:598-644`; runtime package, source stub, generated stub, unit/public-typing and integration journey changes | PASS |
| TypeScript compaction projection | `sdks/wyrd-sdk-ts/native/src/lib.rs:480-522`; source wrapper, generated declarations, unit and integration journey changes | PASS |
| Python runtime/stub parity | TASK-006, TASK-006-R1, prior `FIND-TASK-006-1`; source and generated stubs for agent, errors, testing, state, model, data, prompt, cards, and root header; owning PyO3/Rust symbols; `test_public_api_parity.py` | PASS |
| Callback behavior | Prior `FIND-TASK-006-2`; `skald-agent/src/{callbacks.rs:14-25,loop_runtime.rs:385-461,502-573,python.rs:797-836}`; Rust callback tests and top-level Python parity tests | PASS |
| Test ownership and generated ownership | AGENTS.md sections 7, 8, and 11; PyO3, Python/stub, TypeScript, and testing references; remediation evidence and final aggregate evidence | PASS |

Applicable repository authority was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/bifrost-design.md`, and the routed Rust, PyO3, Python/stub,
TypeScript, maintainer-style, spec-driven-development, and testing references.

## Prior-finding closure

| Finding | Source validation | Proof assessment | Result |
|---|---|---|---|
| `FIND-TASK-004-1` | `CompactionTypeWire` now uses `kebab-case`; its four values are `auto`, `full`, `small-files`, and `files-with-delete`; underscore spellings have no alias; request/description prose consistently says omission uses `small-files`. Python and TypeScript parse through this same serde owner. Searches of public schema/SDK surfaces found underscore spellings only in tests that explicitly reject them; remaining repository uses name internal Forge task strategies/metrics rather than the public compaction type. | Canonical tests exercise every accepted value, both rejected underscore values, schema contents, and omission prose. First-class language tests and journeys exercise the projection. | PASS |
| `FIND-TASK-004-2` | `wyrd_client::bifrost` re-exports the existing `CompactionTypeWire` at `mod.rs:64-67`; the thin Rust SDK re-exports `wyrd-client` and its compile test imports only `wyrd_sdk::bifrost`; the Rust journey imports through `wyrd_client::bifrost`, not `wyrd_spec`. No wrapper enum or direct SDK dependency was added. | `sdk_bifrost_names_the_compaction_type` names all variants and builds a `TableConfig`; the public journey uses the re-export. | PASS |
| `FIND-TASK-006-1` | The complete recorded parity inventory is reconciled at hand-authored sources and copied into generated outputs: `Role.System`; keyword-only `SessionTurn` with string `role`, `call_id`, and `model_*`; Agent mutators and callback registration; plain `Exception(*args)` direct error construction; the eleven testing methods; `WyrdState.from_path` `None` defaults; keyword-only `ModelCardMetadata` without phantom fields; required model `HuggingfaceInterface.hf_task`; `DataCardMetadata.to_dict`; Prompt helpers, real `PromptCard` members, and no phantom metadata/member; `AgentCard.kind`; and `run_wyrd_cli`. Inspected owning PyO3 signatures agree with these declarations. Source and generated stub copies agree on the changed declarations. | The new top-level parity test directly exercises the central Role, SessionTurn, error, Agent, callback, and PromptCard rows. The remaining declaration rows are source-owned and covered by the recorded one-off runtime/member scan, `codegen:check`, Python unit suite, and typecheck. | PASS |
| `FIND-TASK-006-2` | `ChainResult::Abort` now retains the held value. An `after_model` abort returns a `CallbackAborted` run before the response enters the conversation; an `after_agent` abort converts the held completed run to `CallbackAborted` while retaining iterations and conversation; an `after_tool` abort creates one failed tool result and the loop continues. The three former callback `unreachable!` consumers are absent. Python exceptions enter this owner as `CallbackOutcome::Abort`; catalog-coded `WyrdError` arguments retain their code and ordinary exceptions become the stable validation error. | Rust tests assert the exact three outcomes. Top-level Python tests raise from all three hooks and assert the public `FinishReason`, error code, held iteration/conversation behavior, failed tool result, and continued run. | PASS |

## Deleted-test assessment

`test_model_public_surface.py` remains present. The remediation removed only
`test_model_stub_args_docs_use_name_type_description_format` and its private
parser. That test required every `Args:` line to repeat the type already
expressed by the stub signature. TASK-006 explicitly prohibits duplicating a
type signature as filler prose, so retaining that assertion would enforce the
opposite contract. The adjacent public-docstring coverage remains, and
signature correctness is covered by `py:typecheck`; this is a justified test
retirement, not a weakened gate.

All added Python tests are top-level functions. Generated `.pyi` and schema
files have source owners, and the recorded `codegen:check` passed; no generator
redesign or hand-authored compatibility alias entered the remediation.

## Findings

No material SDK, contract, PyO3, Python, or TypeScript finding remains.

## Verification limits

- I performed commit-scoped source, caller/consumer, generated-output, test,
  and cumulative/remediation diff inspection, plus `git diff --check` on both
  reviewed ranges; both diff checks exited zero.
- I did not rerun build or test commands because the shared checkout is past
  the immutable candidate. The candidate records focused Rust/Python/TypeScript
  tests, all three language journeys, `codegen:check`, `py:typecheck`,
  `check:pyo3-scope`, and the final `mise run gate` as passing. The final gate
  ran at `30d31ebac`; the only later candidate changes are the unrelated
  Bifrost-variant spec draft and the evidence commit itself, so no later SDK or
  runtime source escaped that aggregate.
- The unrelated `changes/active/bifrost-variant/spec.md` addition is outside
  this domain review; packet-level scope review owns it.

## Overall result

**PASS**
