# Focused error-boundary review — FAIL

Subject: `0d30e262aafa07766ca73cd0613c71c861b9ddd0`; prior candidate
`da7622b52994946e03b9ba241032b99d096063cf`; original base
`1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44`. HEAD matched the subject.
Static review only; no implementation, test execution, commit, or push.

This report covers the two explicitly human-directed additions in `scope.md`.
They are **HUMAN-ERROR-1** and **HUMAN-ERROR-2**, not new or retroactive FIND
IDs. It does not reopen the seven original findings or re-review their closure.
The remediation diff leaves these error-producing paths unchanged apart from
ExtractError rustdoc; the user's instruction is the reason for reopening them.

Authority: root AGENTS; Python and TypeScript AGENTS for the projections;
`agent_docs/errors.md`, `agent_docs/agent-surfaces.md`, maintainer/Rust guides;
`architecture/wyrd-design.md` Agents/tools/workspaces (1758–1798) and error
catalog (2266–2285); approved spec revision 3 REQ-002, REQ-007, REQ-009–013,
INV-004; original task and R1 remediation, including the prohibition on adding
error variants merely to improve assertions. Current human direction expands
review scope; it does not require inventing variants.

| Obligation | Source evidence | Existing proof assessed | Result |
|---|---|---|---|
| Upload refusals are repairable without parsing prose; public metadata means what its catalog says | upload.rs:75–99,140–160; manifest.rs:84–110; loader/error.rs:47–54,161–173; spec/error.rs:308–320,348–362 | upload unit negative matrix and Python upload journeys prove refusal, but intentionally assert identical outer/inner codes | FAIL: HUMAN-ERROR-1 |
| Expected filesystem input refusals keep validation classification to the model and recorded consumer | workspace.rs:551–575,645–697,740–792,832–843; tools.rs:244–270; skald loop_runtime.rs:591–599; blocks/tools.rs:310–314 | Python file/search/extract journeys assert SKALD_TOOL_500_CALL for these expected refusals | FAIL: HUMAN-ERROR-2 |
| Reuse the existing catalog and preserve unrelated lifecycle/admission/error semantics | spec/error.rs Validation and ToolInvalidInput/ToolInvocationFailed; tools.rs structured; toolerror.rs StructuredInvocation and From<&ToolError> | Existing projection tests cover structured errors and generic invocation codes | Existing mechanisms suffice; no spec revision required |

## HUMAN-ERROR-1 — upload preparation destroys repairable refusal identity

Classification: INCORRECT, human-directed correction. The supported scenario is
an author using `Cards.upload_artifact` or the CLI on an empty directory, a
symlink/nonregular source, a corrupt supported PDF with extraction enabled, or
an extraction-output collision. These are required negative flows, not arbitrary
hardening: REQ-002/009/010 require precisely these refusals.

`ArtifactUpload::prepare` defines a common `refused` closure at
`crates/shared/wyrd-loader/src/upload.rs:75–80`. All of those local branches call
`Diagnostic::invalid_envelope`; corrupt extraction goes through the same closure
at 152–154. Nested links instead fail in shared `manifest::collect_files` at
84–110. That function is also used by `build_artifact_manifest` for ordinary
Card declaration; top-level source checks and nested traversal must therefore
be addressed together, without changing declaration's outer error convention.

`Diagnostic::invalid_envelope` constructs LoaderInvalidEnvelope with null
details (`loader/src/error.rs:161–173`), so its `context` has no typed refusal
identity. `From<LoadError>` wraps all diagnostics in the same outer variant
(47–54). The public catalog says that variant means missing Card-envelope
fields and recommends supplying apiVersion/kind/metadata/spec
(`wyrd-spec/src/error.rs:348–362`). Those fields cannot repair these sources.
Only the prose differs among the four failure classes. This violates the
explicit requirement in agent-surfaces to avoid prose parsing where a typed
field carries the meaning, and errors.md's repairable-validation guidance.

The consumer path does not recover the information: shared
`Cards::upload_artifact` (client/cards/handle.rs:308–324) propagates preparation's
WyrdError unchanged; Python `PyCards::upload_artifact` (state/mod.rs:2019–2051)
uses that method and `wyrd_utils::py` projects code/details to exception
attributes (py.rs:404–435). TypeScript native cards.rs:181–205 uses the same
shared method; `NativeLifecycleResult::outcome/from_wyrd` and NativeWyrdError
(native/src/lib.rs:161–260) retain its metadata; the facade's projectedError
(wyrd/src/index.ts:405–430) retains details. Rust and CLI likewise consume the
shared preparation result. Repairing each SDK separately would duplicate the
wrong owner and cannot reconstruct discarded categories safely.

Minimum decision-complete correction: reuse **WyrdError::Validation**
(`WYRD_SPEC_400_VALIDATION`, 400), already used in this same prepare method for
authored extraction reports. At preparation's source checks expose structured
`rule`, `field`, safe source `path`, and expected constraint/remediation context
for nonregular source, empty collection, refused supported extraction, and
generated-path collision. Use distinct rule values for those four classes;
include the colliding output path for the collision. The extract refusal can
remain one category, with the known format and expected parseable/bounded
document constraint: categorizing every parser failure is unnecessary.

For nested nonregular entries, put the classification at shared collect_files
using the existing Diagnostic::from_wyrd_error mechanism, which already retains
Validation details as diagnostic context. The upload boundary must preserve that
validation classification in its public error rather than turn it back into a
missing-envelope refusal; ordinary declaration may retain its documented outer
LoadError wrapper and gains the correct inner diagnostic. Preserve IO diagnostics
as IO. Do not infer any class from message substrings, rewrite global LoadError
conversion for all callers, rescan the tree in an SDK, add new catalog variants,
or change refusal/publication behavior. A single source-level classification
plus the existing public Validation mechanism is sufficient.

Consequence/materiality: an SDK/agent cannot choose the required repair or use
the advertised remediation without parsing filesystem/parser prose. The
existing Python tests at 861–883 and 1014–1034 explicitly demonstrate the lost
identity by asserting `(INVALID, INVALID)` for corrupt and colliding inputs.
This affects the capability's normal negative flows, not availability or data
integrity of successful uploads.

Focused closure proof: extend the existing loader upload negative matrix to
assert catalog validation and four distinct structured rules, including both
top-level and nested symlinks. Assert an IO refusal retains its existing family.
Update the existing Python upload journey assertions to observe the same rules
through WyrdError.details and retain the no-registered-Card assertions. Reuse the
existing TypeScript/CLI upload proof to check projection of a representative
structured refusal if those assertions exist; no new harness or broad journey
suite is needed for remediation.

## HUMAN-ERROR-2 — tool-input refusals become implementation failures

Classification: INCORRECT, human-directed correction. A real Agent can submit
valid JSON containing an invalid regex/glob, a missing scope, a directory or
non-UTF-8 file to fs.read, malformed/unsupported bytes to fs.extract, or an
existing/outside-work output destination. These are required supported negative
flows under REQ-007/011/013 and the owning architecture section.

The first loss occurs in the Workspace producer: search validation
(workspace.rs:551–575), file/text/path checks (740–792, host at 370–376), glob
(832–843), and extraction/output checks (645–697) all reduce refusals to
`BlockError::Terminal { reason: String }`. The extraction adapter
`hosted/mod.rs:40–42` does the same. Terminal is a block retry classification,
not a public validation class; it also represents non-input/internal failures.

`Invocation::call` then maps both blocking task joins and returned workspace
errors to the same generic `invocation(error.to_string())` in tools.rs:244–270.
ToolError::Invocation reports `SKALD_TOOL_500_CALL`
(skald-tool/src/toolerror.rs:65–73). In the actual model loop,
skald-agent/src/loop_runtime.rs:591–599 returns that code with only prose.
RecordedTool separately converts the same error through WyrdError at
vala-verify/src/blocks/tools.rs:310–314, yielding `WYRD_TOOL_500_CALL` in the
record. Thus neither consumer represents a repairable argument refusal, and
the model and durable record use different code namespaces for the same one.
Python journey constant TOOL_FAILED at 1183 and assertions at 1304/1359 pin the
observed failure classification instead of disproving it.

Existing alternatives were checked in full: InvalidInput means JSON failed
deserialization, corresponding to `WYRD_TOOL_422_INPUT` in the public catalog
(spec/error.rs:3368–3384); reusing it for every semantic workspace failure would
misstate that catalog meaning. Invocation/`WYRD_TOOL_500_CALL` means tool
implementation execution failed (3385–3398). StructuredInvocation already
carries code/status/title/remediation/safe_details; tools.rs `structured`
(585–595) builds it from a WyrdError, and Card/Bifrost sibling tools already use
that route. ToolError's existing From<&ToolError> preserves recognized catalog
codes. General Validation is the existing semantic-request refusal family.
RegistryInvalidArtifactPath is specifically manifest-relative-path validation;
AgentInvalidArgument concerns agent constructor/method arguments. Neither owns
these workspace tool operations. No new filesystem catalog is necessary.

Minimum decision-complete correction: have the **tool-facing Workspace
operations** preserve repairable validation as WyrdError::Validation with
structured rule/field/safe path/expected constraint. Carry that typed distinction
from the checks listed above to Invocation::call and reuse `structured` for it.
Include regex/glob syntax, scope/path existence/confinement, regular-text-file
requirements, supported/parseable bounded extraction, no-overwrite output and
scratch limits. Classify at their producer; do not parse BlockError prose or
blanket-convert Terminal into bad input. Preserve task join failures and genuine
IO/serialization/implementation failures as execution errors. Keep block
materialization, declarative extraction, retry and admission semantics in their
existing BlockError owners; adapt shared private path checks where those owners
consume them rather than globally changing BlockError semantics. Existing
extract_refused remains suitable for declarative extraction and need not be
rewritten just to fix the tool-facing boundary.

One shared consumer projection must also retain the existing safe_details in
the failed-tool branch of Agent::run_loop: otherwise all Validation errors once
again reach the model with the same code and only prose explaining the rule.
Forward the JSON-safe details already supplied by StructuredInvocation, while
preserving existing code/error behavior for unstructured errors and callback
failures. This adds no successful tool-result field, result schema, endpoint,
or verification-record schema; ToolCallRecord.error remains the existing
catalog-prefixed string. This is the smallest-contract assessment: the inspected
loop builds an open JSON failure value and already owns safe_details on the
error. Independent validation should confirm that forwarding those existing
details does not encounter a separately closed failure-envelope contract; none
was found in these paths. If such an owner exists, resolve that owner before
prescribing a schema expansion. No broader loop redesign is required.
A structured filesystem failure then carries the same
catalog code in the model result and record. Do not rewrite unrelated generic
Skald codes or require a global canonicalization migration.

Consequence/materiality: a literal agent sees a server execution failure for
repairable ordinary mistakes and must parse prose to decide which argument to
change. This violates the repository's agent-facing error obligations and affects
the explicitly required negative interactions. It does not prove retries are
currently performed or data is leaked; neither claim is necessary.

Focused closure proof: update the existing search/read/extract negative cases
to assert canonical validation code and distinct structured field/rule values,
including directory/binary reads, malformed regex and glob, missing scope,
malformed/unsupported extraction, and invalid/existing output. One existing
model-loop journey must observe those details in the actual tool response and
the same catalog code in its ToolCallRecord; a direct Workspace unit test alone
does not cover the projection loss. Keep the existing authorization precedence,
no-protected-content, no-partial-output, genuine execution-error, generic
ToolError projection and Busy/capacity tests unchanged in meaning. Run only the
focused affected tests and owning lanes; no new harness or full journey suite.

## Correction burden and review boundary

Both additions require source classification and projection corrections, not
new error enums or one catalog variant per negative test. Existing Validation
payloads and StructuredInvocation avoid a material public-contract decision;
the selected behavior restores existing error semantics and agent-repair
requirements. **SPEC_REVISION_REQUIRED is not supported by these paths.**
Changing extraction's string error into a comprehensive typed parser taxonomy,
reworking every Skald error code, changing BlockError retry policy or changing
verification result schemas would exceed this correction and is unnecessary.

The loader/shared-client/SDK and hosted Workspace/Invocation/RecordedTool/model
paths above were inspected, including sibling error producers and existing
catalog alternatives. No runtime checks were run; cited green implementation
evidence is not independently reproduced here. Independent validation by the
round's ponytail reviewer remains required. Overall scoped result: **FAIL** on
HUMAN-ERROR-1 and HUMAN-ERROR-2.
