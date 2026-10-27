---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verifier-artifact-workspaces
spec_revision: 3
requirements: [REQ-002, REQ-007, REQ-009, REQ-010, REQ-011, REQ-013, INV-004]
depends_on: []
parent_task: TASK-001
remediates: []
---

# Preserve repairable upload and filesystem-tool errors

Implement with `$wyrd-implement`. This task packages **HUMAN-ERROR-1** and
**HUMAN-ERROR-2**, the user's explicit additions to review. They intentionally
have no FIND IDs. All original FIND-TASK-001-1..7 are closed and must remain so.

Authority/context: [approved spec revision 3](../../spec.md), [original task](../../tasks/TASK-001-reusable-artifact-workspaces.md),
[R1 remediation](../task-001-r1/TASK-001-R1-complete-artifact-workspaces.md),
[current verdict](verdict.md), [validated diagnoses](findings-validation.md).
Original base: `1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44`;
prior candidate: `da7622b52994946e03b9ba241032b99d096063cf`;
reviewed cumulative candidate: `0d30e262aafa07766ca73cd0613c71c861b9ddd0`.
The user asked this review to address indistinguishable upload refusals and
tool input refusals reported as 500. This instruction expands R1's deliberately
limited error scope. Existing Validation and structured projections suffice;
do not add catalog variants merely to distinguish individual tests.

## Upload refusal diagnosis and correction

**HUMAN-ERROR-1.** Expected nonregular-source, empty-collection,
supported-extraction and generated-output-collision refusals in
`wyrd-loader/src/upload.rs:75–99,140–160` use invalid-envelope diagnostics.
Nested nonregular entries originate in shared `manifest.rs:84–110`.
`error.rs:161–173` discards their structured identity into null context;
the LoadError conversion at `:47–54` supplies the same outer envelope family.
Its catalog remediation concerns missing Card fields, which cannot repair
these sources. Existing Python negative journeys prove the same outer/inner
code pair for different required repairs. Literal callers must parse prose.

Classify these refusals at the existing source owners using
`WyrdError::Validation`, with distinct structured rules for nonregular source,
empty collection, refused supported extraction and generated-path collision.
Include the relevant field, safe source path, expected constraint and collision
output path as applicable. A single bounded extraction-refusal category with
known format/constraint is enough; a parser-error taxonomy is unnecessary.

Use the shared collector's existing Diagnostic preservation mechanism for
nested nonregular entries, and preserve that validation class at the upload
boundary. Ordinary declaration may retain its existing outer loader wrapper
and receive the structured inner diagnostic. Do not globally rewrite LoadError
conversion, duplicate traversal, infer categories from prose or add SDK-specific
classifiers. Rust/CLI/Python/TS already project code/details from the shared
owner; discarded producer information cannot be repaired at those consumers.
Preserve actual IO, source-change/integrity and serialization error meanings.

This is bounded classification and metadata at existing owners. Dropping
refusals or rewriting prose cannot supply machine-readable repair identity;
new lifecycle or error-enum machinery is unnecessary.

## Tool refusal diagnosis and correction

**HUMAN-ERROR-2.** Tool-facing Workspace search/scope/glob validation at
`hosted/workspace.rs:551–575,711–736,832–843`, file/text checks at `:740–792`,
and extraction/output checks at `:645–697` reduce expected semantic refusals
to `BlockError::Terminal { reason }`. `hosted/tools.rs:244–270` maps both those
returned errors and blocking task failures to generic Invocation. The model
sees `SKALD_TOOL_500_CALL`; RecordedTool records `WYRD_TOOL_500_CALL`.
Malformed patterns, directory/non-UTF8 reads or invalid output destinations
are therefore presented as implementation failures, with repair meaning only
in prose. These are required normal negative flows, not speculative hardening.

Preserve typed semantic Validation from the tool-facing checks to invocation,
then reuse the existing `structured` projection used by sibling Card/Bifrost
tools. Provide meaningful rule/field and safe path/expected constraint for
regex/glob syntax, scope/path existence and confinement, regular UTF-8 reads,
supported/parseable bounded extraction, output confinement/no-overwrite and
applicable scratch limits. Classify at the producer rather than parsing
Terminal strings. `ToolInvalidInput` specifically means JSON/schema decoding
failed; do not repurpose it for every semantic filesystem refusal.

Keep genuine IO, serialization, task-join and implementation failures as
execution errors. Terminal also serves block-facing materialization and
declarative extraction; retain their retry/admission semantics through their
existing consumers instead of blanket-mapping Terminal to client error.
Do not globally redesign BlockError or Skald error codes.

One shared consumer correction is necessary: Agent `run_loop` at
`skald-agent/src/loop_runtime.rs:591–599` currently emits code/prose but drops
existing StructuredInvocation safe_details. Forward those available details
in the existing failed-tool JSON, retaining its code/error fields and existing
unstructured/callback failure behavior. They are already explicitly designated
safe for an agent. ConversationTurn::ToolResult and provider request projections
accept open JSON; there is no new successful output, endpoint or verification
result schema. Do not invent details from prose or expose unsanitized internal
causes. RecordedTool already preserves recognized catalog codes; keep its
existing record/error shape and assert code agreement with the model response.

The minimum correction is source classification plus existing error metadata
forwarding. A consumer-only status rewrite loses the distinction among repairs,
while new per-tool variants, a parser taxonomy or loop redesign would add
unneeded machinery.

## Preserved behavior and scope limits

Keep authorization/audit before parsing and work, current tenant/resource
authority, generic execution failures, typed Busy/capacity refusal, block retry
classification, bounded search results and binary skipping, explicit extraction
atomic/no-overwrite publication, writer ownership, admission and cleanup.
Preserve the real opaque code.run and search/read/query journeys and their
content-sensitive final assertions. Retain fixed checked-in Cards, full Python
authoring, exact-code tests, shared parser/upload/declaration and docs closure.

No new endpoint, catalog variant, public result/error schema, parser format,
cache, lifecycle owner, dependency, SDK traversal, global code migration or
unrelated tool correction. Update direct affected documentation and mandatory
changed-item rustdoc if needed. The dirty TASK-007 edit is unrelated and must
not be overwritten. If implementation exposes a material new contract or
security decision, report the precise authority conflict rather than silently
expand scope.

## Acceptance and focused proof

| Directed item | Required observation |
|---|---|
| HUMAN-ERROR-1 | Public upload errors retain Validation and distinct structured rules for nonregular source, empty collection, refused supported extraction and generated collision; safe paths/constraints identify the repair without prose parsing. Top-level and nested links use the same category. Failed fresh publication leaves no Card. |
| HUMAN-ERROR-1 | Existing missing-source IO retains its IO family; ordinary declaration retains its outer loader convention and structured inner validation. Shared Python/CLI/TS projections need no duplicated classifier. |
| HUMAN-ERROR-2 | Existing real read/search/extract negatives produce canonical Validation with rule/field details in the actual model tool messages; ToolCallRecord carries the same catalog code. |
| HUMAN-ERROR-2 | Genuine execution/IO and generic invocation errors retain execution classification; denied calls, Busy, no-protected-content and no-partial-output behavior remain unchanged. |

Follow RED/GREEN for new executable corrections; record exact failures and
diagnoses rather than revising assertions blindly. Use existing upload
negative matrices, Workspace tests and provider mock/real-host journeys. Do not
add a new harness or run full journey suites/broad aggregates for remediation.
Select existing mise tasks first and serialize Cargo-backed work.

Exact existing Rust selectors for the affected owners:

```sh
mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=upload::tests::uploads_refuse_invalid_sources_and_reports)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::workspace_reads_lines_and_extracts_on_request) | test(=verification::hosted::workspace::tests::workspace_search_matches_lines) | test(=verification::hosted::workspace::tests::workspace_refuses_bad_paths_and_limits)'
mise exec -- cargo nextest run --locked -p wyrd-skald-tool --lib -E 'test(=toolerror::tests::generic_invocation_code_is_unchanged) | test(=toolerror::tests::tool_errors_project_onto_the_catalog)'
```

Real public proof, once after combined changes:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_upload_preserves_files_and_versions tests/integration/test_verifier_building_blocks.py::test_artifact_upload_extracts_before_registration tests/integration/test_verifier_building_blocks.py::test_agent_file_tools_read_without_extraction tests/integration/test_verifier_building_blocks.py::test_agent_file_search_supports_regex_across_files tests/integration/test_verifier_building_blocks.py::test_agent_extract_tool_writes_explicit_output'
```

These exercise the actual model-visible tool response and recorded result,
not only direct Workspace errors. If additional loop or genuine-IO proof is
necessary, add it to an existing owner and record its collected exact selector
before running it. Preserve generic/Busy/authorization tests in meaning; select
additional affected tests only when actual edits reach those paths. Build the
existing server/Python wheel via original task setup when needed.

Run `mise run fmt:check`, `mise run lints`, `mise run py:format:check`,
`mise run py:lints`, `mise run py:typecheck` and `mise exec -- git diff --check`
for the expected write set. Run docs/codegen/TS checks only if corresponding
sources or outputs change. Append actual commands, results, failure diagnoses
and remaining evidence limits here. Subsequent review checks these two closures
and affected consumers, carrying forward the seven already closed findings.
