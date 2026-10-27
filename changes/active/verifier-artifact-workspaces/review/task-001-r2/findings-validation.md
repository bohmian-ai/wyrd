# Independent remediation validation

Subject: original base `1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44`, prior
candidate `da7622b52994946e03b9ba241032b99d096063cf`, cumulative candidate
`0d30e262aafa07766ca73cd0613c71c861b9ddd0`. HEAD matched the candidate.
Review covers the actual prior-to-candidate diff and affected consumers, all
seven prior findings, and the two explicitly human-directed error additions.
Prior review artifacts committed in this range are acceptance context. The
unrelated dirty TASK-007 is excluded. No implementation or commits were made;
this reviewer owns only this report and ran no builds or tests.

Read the full task-review skill, round scope and error report, original
verdict/ledger/remediation, approved revision-3 spec and original task. Applied
root and Python/TS/docs AGENTS, spec-driven workflow, agent_docs index and
Rust/error/maintainer/agent-surface guides, architecture map and owning
Artifact/Agent/error sections, security principles and TESTING rules.
CodeGraph preceded textual source discovery. This is focused closure
validation, not a second comprehensive discovery pass.

## Prior finding closure

Every original FIND ID is preserved. Closure is against the original issue;
the subsequent human instruction about error semantics is recorded separately.

| Prior finding | State | Independently checked source and proof |
| --- | --- | --- |
| FIND-TASK-001-1 | CLOSED | `hosted/workspace.rs:551–629,935–941`: search still owns path/line/source collection; snippet now takes at most 160 UTF-8-safe bytes independently of regex length. `Collected` still bounds serialized results. `workspace/tests.rs:214–347` adds the long multibyte `REFUND.*` case and checks one source-bearing actual-line hit within the result budget; existing literal/case/glob cases remain. The task records RED at the one-hit assertion and GREEN for the exact selector. |
| FIND-TASK-001-2 | CLOSED | Python `state/mod.rs:1992–2052` accepts Metadata and optional ArtifactSpec mappings, reuses `observe::json_value`, validates CardName and maps conversion errors through existing `invalid_argument` before `this.cards(py)` can connect. Metadata bump is explicitly separated into the existing version-bump argument. Stub `cards/__init__.pyi:521–573` matches. All upload callers under the Python SDK use the final signature. Journeys at `test_verifier_building_blocks.py:886–1035` assert persisted labels/annotations/spec metadata, retained metadata with extraction, authored-report refusal and exact field-bearing Validation before connection. Global serde/Internal conversion and shared upload/declaration stay unchanged. |
| FIND-TASK-001-3 | CLOSED | Raw-file journey at `:1190–1240` invokes real `code.run` with mounted opaque bytes and asserts `repr(OPAQUE.hex())`; fixture `raw-desk-check.yaml` mounts the upload and shared file-probe declares code.run. Search journey `:1243–1327` flushes real Bifrost, then invokes search → read → query, asserts the hit/read line and earlier retained result count, and checks call order. `Probe.play` gets its results from the completed Agent output. Unchanged `conftest.py:239–276` reads each real provider tool message before generating the next call/final echo, so final assertions remain response-sensitive. Ordinary query/tool grants are supplied by `grants` at `:325`. |
| FIND-TASK-001-4 | CLOSED | Existing raised refusals now assert one exact outer code and applicable first diagnostic code/rule at `:939–973,1014–1034,1375–1416`; authority tests retain no-Card, before-model and no-disclosure checks. Actual tool results assert their current single code at `:1225–1232,1304,1359,1420–1423`. This closes the original assertion requirement, which explicitly prohibited new error variants/schema solely for stronger assertions. HUMAN-ERROR-1/2 are the user's later direction to change those existing error semantics, not evidence this correction failed. |
| FIND-TASK-001-5 | CLOSED | New Cards are checked-in `fixtures/cards/artifact_workspaces/*.yaml`: fixed story Services, Verifiers and two Agent documents, including deliberate malformed variants. `Probe.__init__` at `:1102–1116` declares those files and pulls the domain bundle; runtime Card serialization/random-name helpers are removed. Upload identities and exact fixture references agree. Generated source bytes/fresh content markers remain legitimate test data. Shared file-probe keeps real host calls and response-sensitive completion. |
| FIND-TASK-001-6 | CLOSED | `extract.rs:68–73` documents ExtractError payload; upload `tests.rs:29–38` documents the one-submission invariant and indexing panic; client `cards/handle.rs:297–307` documents background preparation, pending/partial/Active state after submission, reconciliation and fresh-key retry. Full shared upload body and saga `mod.rs:43–83` confirm preparation precedes declare, keys are minted per saga and transfer/completion can leave durable progress. No rollback/replay promise was added. |
| FIND-TASK-001-7 | CLOSED | Recipes `verifier-recipes.md:377–420` explains CLI upload/default raw bytes/opt-in preparation, exact Active identity/version behavior, inventory discovery and current Python/Rust/TS operations; existing journey-backed uploaded-artifact fixtures are embedded with CodeFromFile. Python signature matches final boundary; TS native/shared operation is unchanged. Machine-readable copies contain the new content. Source supports the guide's version/refusal statements through the existing publication journey and shared declaration owner. |

No original finding remains OPEN. No new independent blocker was discovered,
so no additional focused discovery validation was required.

## Human-directed proposal disposition and deduplicated ledger

| Proposal | Independent validation | Retained identity |
| --- | --- | --- |
| Upload refusal identity/remediation loss | CONFIRMED; minimum correction uses existing Validation and Diagnostic, preserving ordinary declaration's outer loader convention and IO diagnostics | HUMAN-ERROR-1 |
| Expected filesystem tool refusals projected as execution failures | CONFIRMED; source classification plus existing structured tool projection and safe-details forwarding suffice | HUMAN-ERROR-2 |

These are distinct producer boundaries, with common reuse of the existing
catalog. They are human-directed additions, not retroactive or new FIND IDs.
Their materiality follows agent-surface/error obligations and the explicit
request to address these risks; they were deliberately excluded from the first
remediation's production-error scope.

### HUMAN-ERROR-1 — CONFIRMED / INCORRECT — upload refusals lose repair identity

Authority: spec REQ-002/009/010 and INV-004; Artifact architecture; errors.md's
repairable-validation fields and agent-surfaces.md's no-prose-parsing rule.

Read the complete `ArtifactUpload::prepare`, `collect_files`, `manifest_of`,
LoadError conversion, Diagnostic constructors, shared Cards upload, Python
wrapper/projection and TS native/facade projection. Producer `upload.rs:75–99`
uses invalid-envelope diagnostics for top-level nonregular/empty sources;
`:140–160` does so for collisions and failed extraction. Nested nonregular
sources originate in `manifest.rs:84–110`; this collector is shared with
ordinary declaration's `build_artifact_manifest`. The invalid-envelope
constructor at `error.rs:161–173` carries null context; `From<LoadError>` at
`:47–54` adds the same outer envelope code. Catalog `wyrd-spec/error.rs:348–362`
describes missing Card fields, which cannot repair these sources.

Reachable supported examples are the existing upload journeys' symlink,
empty directory, corrupt supported PDF and generated-path collision. Their
identical code pairs at Python `:950–956,1030–1034` prove that only prose
identifies the remedy. This materially prevents literal callers from deciding
which input to repair. It does not establish a successful-upload integrity
defect or permission bypass.

Shared client `handle.rs:308–324` propagates preparation errors; Python
`wyrd-utils/py.rs:404–435` and TS native `lib.rs:161–260` plus facade
`index.ts:405–430` already preserve catalog details. SDK-specific classifiers
cannot recover the discarded identity without duplicating or parsing source
logic. No correction belongs in those projections.

Smallest correction: classify these expected refusals at preparation using
existing `WyrdError::Validation`, with distinct structured rules and relevant
field, safe source path, expected constraint and collision output path. One
bounded extraction-refusal class with known format/constraint is sufficient;
do not create a parser taxonomy. Put nested nonregular classification in the
existing collector via `Diagnostic::from_wyrd_error`, which already preserves
Validation context. At the upload boundary retain that typed validation rather
than relabel it as a missing envelope. Preserve ordinary declaration's existing
outer LoadError convention, genuine IO diagnostics, digest-change refusal and
serialization failure meanings. No global LoadError rewrite, SDK rescan,
message matching, new variant, dependency, publication phase or lifecycle owner.

The correction burden is bounded source metadata/classification at existing
owners and existing tests. Deletion cannot retain required refusals; merely
changing prose or loosening assertions cannot give machines repair identity.
The existing Validation payload and Diagnostic context avoid a material new
public-contract decision.

Closure proof: extend the existing upload negative matrix to assert catalog
Validation plus distinct rules for the four categories and both top-level and
nested links. Pin the existing missing-source IO family as preserved. Update
the two Python upload journeys to assert those identities through public
WyrdError.details and preserve absence of registered Cards. Existing TS error
projection is source-proven; no new TS harness is required. A representative
assertion in an already affected CLI/TS test is sufficient if one is edited.

### HUMAN-ERROR-2 — CONFIRMED / INCORRECT — semantic tool refusals become 500s

Authority: spec REQ-007/011/013, Agent tools architecture, public error and
agent-repair rules. Full `Workspace::list/read/search/extract`, private
host/file/text/scope/glob functions, `Invocation::call/run/workspace`,
`RecordedTool::invoke`, `Refusal::observe`, ToolError conversion and Agent
`run_loop` were read, including relevant sibling operations/consumers.

Production path: malformed regex/glob/missing scope at
`workspace.rs:551–575,711–736,832–843`, directory/non-UTF8 reads at `:740–792`,
and unsupported/malformed extraction or unsafe/existing output at `:645–697`
become `BlockError::Terminal { reason }`. Filesystem dispatch in
`tools.rs:244–270` turns both these returned errors and task-join failures into
generic Invocation. Model failure projection at `loop_runtime.rs:591–599`
therefore emits `SKALD_TOOL_500_CALL`, while `RecordedTool` at
`vala-verify/blocks/tools.rs:310–314` records catalog `WYRD_TOOL_500_CALL`.
Current Python journeys at `:1225–1232,1304,1359` exercise and pin this path.
These are ordinary required input mistakes; classifying them as execution
failures and discarding structured context forces the model to parse prose.
No claim of actual retry amplification or content leakage is needed.

Catalog alternatives checked: `ToolInvalidInput` is failed JSON/schema
deserialization (`error.rs:3368–3384`), not semantic filesystem validity;
ToolInvocationFailed is implementation failure (`:3385–3398`). Existing
Validation is the correct general semantic refusal. `structured` at
`hosted/tools.rs:585–595` already constructs StructuredInvocation, used by
Card/Bifrost siblings. ToolError's conversion (`toolerror.rs:101–151`) already
preserves recognized catalog codes and safe details in the record projection.

Smallest correction: preserve semantic Validation from the tool-facing
Workspace producers to `Invocation::call`, then reuse `structured`. Distinct
rules/fields describe regex/glob validity, scope/path existence or confinement,
regular UTF-8 reads, supported/parseable bounded extraction, output
confinement/no-overwrite and applicable scratch constraint. Do not convert
every Terminal or every failed task to Validation: Terminal also serves
materialization/internal work, and join/IO/serialization failures are not
caller-validation evidence. Shared host checks also serve materialization and
mounts; adapt their block-facing consumers to retain existing BlockError
semantics rather than changing the whole block/error policy. Preserve current
search skipping of binary files, extraction atomic/no-overwrite publication,
writer ownership, and genuine execution-error meanings.

#### Existing safe-details forwarding is permitted and necessary

Independently checked `ToolError::safe_details` and its source field
(`toolerror.rs:17–18,87–93`): the value is explicitly scrubbed context safe to
expose to an agent. Hosted `structured` populates it from the existing public
Wyrd problem details. The sole loss before the actual model is the
`ChainResult::Replaced(Err(error))` branch at `loop_runtime.rs:591–599`.
Forwarding those present details in that existing failed-tool JSON value is
necessary: changing only the code to generic Validation would still require
prose to distinguish regex, directory, encoding or output errors.

This is permitted within the current contract. `ConversationTurn::ToolResult`
in `conversation.rs:114–124` owns an unrestricted `serde_json::Value`, and
`append_tool_result` accepts that same open value. `request_builder.rs:375–414`
serializes/forwards it for all four provider protocols; none parses a closed
`{code,error}` schema. Journal ToolResult likewise receives the JSON value in
the already-read loop. There is no new successful output, endpoint, durable
verification-record schema or code enum. It is an additive projection of
metadata already designated for agent consumption, under the user's express
error-scope instruction; no material new public-contract choice is required.
Keep existing code/error fields and unstructured/callback failure behavior;
only include available safe details, rather than inventing details from prose
or converting every shared failure into a client error.

Adjacent behavior must remain: authorization/audit precede parsing and
execution (`tools.rs:193–202`); CodeRun Busy retains its typed cause
(`:307–318`) and `Refusal::observe` (`blocks/tools.rs:193–210`) stops the
invocation on process capacity refusal. Declarative extraction still uses
`extract_refused` (`hosted/mod.rs:40–42`) and its retry class; mount IO,
admission, cancellation and shared workspace lifetime stay with existing
owners. No global Skald code migration or tool-record redesign is needed.

Closure proof: existing Python raw/search/extract journeys must see canonical
Validation with distinct structured rule/field data in actual model tool
messages and assert the same code prefix in ToolCallRecord. This covers the
shared projection loss without a new harness. Keep denied/no-disclosure,
failed-output absence and generic execution/Busy behavior intact. Extend
existing focused Workspace tests for exact classification. The correction is
bounded classification and forwarding, not a new execution framework.

## Reconciled acceptance and carried-forward evidence

| Obligation | Evidence and effect of remediation | Result |
| --- | --- | --- |
| Publication and full SDK authoring, REQ-001–003/AC-001/007 | Original shared upload/transfer proof carried forward; Python source, persisted authored values and docs now close -2/-7 | PASS for original task |
| Fixed identity and selected-only byte/inventory integrity, REQ-004–005/INV-001/003 | Carry forward r1 invariant/security source tables: refs visitor, engine attachment selection, TenantEvidence inventory/select/admit and byte-reader tests. No remediation production edit touches those owners | PASS |
| Real Service/Agent interactions, REQ-006–008/AC-002–006/010 | Existing Service/citation proof carried; actual opaque sandbox result and real search/read/query completion close -3; shared grant/fixture changes are inspected | PASS |
| Shared preparation/parser and explicit atomic extraction, REQ-009–012/AC-008–009 | Original parser/preparation/publication and workspace proof remains; Python authored report input now exists. Machine repair classification reopened only by explicit human request | Original task PASS; HUMAN-ERROR-1/2 OPEN |
| Search REQ-013/AC-011 | Bounded multibyte regex hit and mixed-tool real result proof close -1/-3; path/glob/case/binary semantics unchanged | Original task PASS; HUMAN-ERROR-2 OPEN |
| RBAC/tenancy/audit and citation identity | Carry forward r1 domain-security and invariants source proof, including exact name grants and ordinary downstream permission. Remediation does not change permission/tenant/source owners; migrated journeys preserve denied/no-disclosure results | PASS within prior disclosed execution limits |
| Lifecycle, admission, cancellation and cleanup | Carry forward original owner tracing and existing tests; only runtime production remediation is bounded snippet projection. Cancellation rustdoc matches saga; no new runtime owner/cache/parser | PASS within prior disclosed limits |
| Thin clients/shared owners and non-goals, INV-004 | Python reuses existing conversion then shared upload; snippets remain Workspace projection; checked-in fixtures/docs reuse established systems | PASS |
| Mandatory rustdoc, fixtures, exact assertions, docs | Changed source and final signatures inspected; -4/-5/-6/-7 closed. Reported checks are evidence, not independently reproduced runs | PASS for submitted remediation |
| New explicit error scope | Both shared producers remain unchanged in this candidate; exact present-code assertions do not satisfy the newly requested repair semantics | OPEN: HUMAN-ERROR-1/2 |

Implementation evidence reports eight focused Python journeys, exact search
nextest test and workspace tests, fmt/lints, Python format/lints/typecheck,
docs and diff checks passing. No source change here invalidates those reported
results; they were not independently rerun. The historical GREEN-first limit
remains; -1 alone has a newly reported RED. No retroactive RED is demanded.

## Remaining correction scope and focused proof

Only HUMAN-ERROR-1/2 remain. Reuse existing loader, workspace, tool and loop
owners described above; update their direct tests/docs and affected Python
assertions. Do not rerun the full original journey suite or broad gate merely
because another review round began. No new catalog variants or generated wire
outputs are required by these corrections.

Exact existing focused Rust selectors to update/run when those owners change:

```sh
mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=upload::tests::uploads_refuse_invalid_sources_and_reports)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::workspace_reads_lines_and_extracts_on_request) | test(=verification::hosted::workspace::tests::workspace_search_matches_lines) | test(=verification::hosted::workspace::tests::workspace_refuses_bad_paths_and_limits)'
mise exec -- cargo nextest run --locked -p wyrd-skald-tool --lib -E 'test(=toolerror::tests::generic_invocation_code_is_unchanged) | test(=toolerror::tests::tool_errors_project_onto_the_catalog)'
```

Exact public proof reuses five affected journeys, once after combined changes:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_upload_preserves_files_and_versions tests/integration/test_verifier_building_blocks.py::test_artifact_upload_extracts_before_registration tests/integration/test_verifier_building_blocks.py::test_agent_file_tools_read_without_extraction tests/integration/test_verifier_building_blocks.py::test_agent_file_search_supports_regex_across_files tests/integration/test_verifier_building_blocks.py::test_agent_extract_tool_writes_explicit_output'
```

Use existing fmt/lints and Python format/lints/typecheck lanes for the actual
write set, plus diff check. If an additional loop-focused or genuine-IO test is
needed, choose its owning existing test owner and record its exact selector
before execution; no new test harness or guessed selector is prescribed here.
Preserve unchanged Busy/authorization tests in meaning and select them only if
the actual correction affects those paths. No docs/codegen/TS lane is required
unless its corresponding source/output is edited.

No second unsuccessful remediation has occurred: all seven prior issues are
closed, and the remaining work is explicit human expansion rather than fix
regression or failed convergence. This report supplies validation and closure,
not the orchestrator's final verdict.

Skipped executable verification and platform/load/crash experiments. Prior
audit/cancellation/lowered-limit execution limits remain; neither this report
nor reported green checks certify those unexecuted scenarios anew.
