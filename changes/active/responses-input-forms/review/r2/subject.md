# Immutable cumulative remediation review

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
- Original base: `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`.
- Candidate: `100adbc04a39ca22022fb013b3c0e93bfe903c3a`; tree `3bb993d4c32769cda67650eb02b991f981ebc3c2`.
- Candidate is a detached Git object snapshot of HEAD plus all tracked working-tree changes, made using a temporary index. Branch HEAD remains `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`; the user's staging area and source were not changed. Untracked files comprise only review artifacts, excluded from source snapshot.
- Approved spec and original task: `changes/active/responses-input-forms/spec.md`, revision 1. User explicitly identifies spec as task.
- Complete cumulative diff: `candidate.diff`; latest remediation diff: `remediation.diff`.
- Prior verdict/ledger: `../r1/verdict.md`, `../r1/findings-validation.md`. Prior remediation task and appended execution evidence frozen as `remediation-evidence.md` in this directory. Do not substitute implementation summary for source.
- Available verification: commands/results reported in remediation-evidence.md; reviewer/orchestrator reruns are recorded separately. No test assertions/timeouts/skips were weakened in the reported remediation; verify against actual diff.

## Common navigation map (starting points, not coverage limits)

Original runtime symbols: `OpenAiResponsesInput::Items`, `easy_items`, `OpenAiResponseItem::FunctionCallOutput`, `OpenAiFunctionCallOutput::{text,from}` in `crates/skald/skald-spec/src/wire/openai_responses.rs`; existing agent tool-result writer and Prompt writer; Python `PyOpenAiResponseItem::as_function_call_output`; captured-body decoder test and existing Skald Responses loop.

Remediation changes: `input_items_schema` and its `schema_with` on Items; feature-gated `input_items_schema_admits_easy_messages`; Python accessor rustdoc plus authored `stubs/prompt.pyi` and generated stub; existing served test `pg_openapi_contract::gateway_ingress_publishes_typed_contracts`; existing TS `gateway-inference.test.ts` Responses receiver/deployment and two real Agents SDK tests; TS workspace devDependencies and lockfile. Generated input schema/types now include named `OpenAiEasyInputMessage`, alongside canonical items. Nine JSON schemas, two fixtures, Python and TS generated contracts changed.

Trace existing gateway route `responses` → transparent `GatewayResponsesRequest` → `typed_body` → `GatewayInvocation` → native adapter. Input decoder normalizes typed values but gateway forwards original JSON. Trace schema generation/conversion into served OpenAPI, generated language types, and canonical item/output sibling consumers. Trace real SDK journey through existing serverTest/Receiver/scoped tokens and native provider.

Authorities: AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx; router-selected Rust/PyO3/Python/TS/testing/contracts/security references; spec-driven-development and maintainer-style. CodeGraph first, current disk source reflects snapshot; `git show <candidate>:<path>` is available if necessary. Source and candidate identity must remain immutable.

Sensitive domain: gateway request trust boundary and tenant isolation; domain reviewer follows authorization and original JSON forwarding. No persistent data/concurrency implementation added, verify that conclusion from full diff.

Prior finding closure requires source evidence for FIND-responses-input-forms-1 (real SDK journey), -2 (Python rustdoc/stub), -3 (input contract parity). Preserve stable IDs if reopened. Evaluate complete original acceptance, not just these closures.

Owner raised whether schema unit and served OpenAPI test overlap. Assess under actual boundaries and repository testing guidance, without editing tests during review or turning optional preference into a finding. Missing pre-fix journey rerun is disclosed; do not rebuild/check out baseline for authorship or manufacture required RED history.
