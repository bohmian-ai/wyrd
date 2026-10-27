---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verifier-artifact-workspaces
spec_revision: 3
requirements: [REQ-001, REQ-006, REQ-007, REQ-009, REQ-010, REQ-013, INV-004, AC-006, AC-007, AC-008, AC-011]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7]
---

# Complete reusable Artifact workspaces

Implement this bounded remediation with `$wyrd-implement`. Preserve the existing
shared upload, storage, parser, workspace and authorization owners while closing
the validated behavior, projection and proof gaps. This task authorizes no new
product behavior beyond approved revision 3.

Authority: [approved spec](../../spec.md), [original task](../../tasks/TASK-001-reusable-artifact-workspaces.md),
[verdict](verdict.md), and [independently validated ledger](findings-validation.md).
Review base is `1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44`; reviewed candidate is
`da7622b52994946e03b9ba241032b99d096063cf`. This is the first remediation; there
are no prior finding IDs to replace. Later uncommitted edits were excluded from
review and must be inspected against these outcomes before receiving credit.
Do not overwrite the unrelated dirty TASK-007 edit or other contributors' work.

## Diagnoses and selected correction boundaries

### Search must retain a discoverable hit for long regex matches

**FIND-TASK-001-1.** In server `verification/hosted/workspace.rs:620–624`,
`Workspace::search` passes the entire regex span to `snippet` (`:935–938`). Its
bound grows by the match length. A normal approximately 70 KiB record beginning
`REFUND`, searched with `REFUND.*`, produces a hit larger than the default 64 KiB
tool-result budget. `Collected::push` rejects the first hit, returning empty
matches with truncation, although bounded path/line/snippet metadata would fit.
Short literal search on the same record works. Inputs and scratch share this
producer. This violates REQ-013 bounded snippets and blocks search-then-read.

Bound the existing snippet projection independently of full match length,
preserving UTF-8 boundaries, actual locators, source IDs and the whole-result
collector. Correcting an SDK consumer is insufficient because the host has
already discarded the hit. Enlarging budgets, new options, pagination or another
search owner is unnecessary. One local projection correction and its focused
existing workspace test close the gap.

### Python must forward the approved authoring values and validation errors

**FIND-TASK-001-2.** Python `src/state/mod.rs:2015–2043` accepts only convenience
fields and constructs otherwise default Metadata/ArtifactSpec. Labels,
annotations, ordinary spec metadata and existing optional spec fields cannot
reach the shared upload owner, unlike Rust and TypeScript. Python therefore
cannot express REQ-001's full authoring operation or exercise REQ-010's authored
report-conflict behavior. The matching stub is at
`python/wyrd/cards/__init__.pyi:521–556`.

The same boundary uses bare serde `?` to deserialize name into Metadata. Invalid
CardName input selects `WyrdPyError::from(serde_json::Error)` in
`wyrd-utils/src/py.rs:85–95`, projecting a routine caller mistake as Internal.
Existing Python `parse_name` preserves Validation instead. This materially
misdirects Python users/agents about a repairable authoring error.

Expose full Metadata and ArtifactSpec mappings using the existing Python-object
conversion and domain deserialization, mapping caller conversion errors to the
existing Validation code and offending field. If convenience arguments remain,
they must lower unambiguously to those same domain values. Align native signature,
stub and affected callers. Leave global serde-to-Internal conversion, shared
`Cards::upload_artifact`, preparation/report validation and declaration untouched.
Those owners already preserve authored values and reject report conflicts; a new
Artifact holder or Python validation/preparation workflow would duplicate them.

### Complete the two explicitly required real-host observations

**FIND-TASK-001-3.** Committed Python building-block tests at `:1151–1247` mount,
list and refuse text reads of opaque bytes but never send them through code.run.
The search story at `:1250–1339` declares search/read/extract and never queries
Bifrost. Existing separate sandbox/query tests do not demonstrate the exact new
paths required by AC-006 and AC-011. These are missing acceptance proofs, rather
than established production dispatch defects.

Extend the existing raw-file journey with a declared/granted code.run operation
that reads mounted opaque bytes and returns a deterministic byte-derived value
checked against the upload. Extend the search story to perform real search →
read → bifrost.query → final, with explicit tool and ordinary query-domain grants.
Assert actual response content and order before response-sensitive completion.
Reuse existing sandbox, real query fixture and provider mock; keep storage,
authorization, tools and query real. A second runner/harness or parser fallback
is unnecessary. Preserve existing search/extraction semantic cases.

### Make existing refusal assertions distinguish the intended errors

**FIND-TASK-001-4.** New Python refusal assertions at `:900`, `:916`, `:956/962`,
`:1431`, `:1441/1443` and `:1451` accept substrings, any WyrdError, alternative
codes or prose. They can pass on Internal or the wrong refusal boundary, contrary
to the mandatory exact-code rule in TESTING.md and Python AGENTS.

Bind each existing raised-error case to its one expected catalog code, and the
field/rule where needed. Use already structured tool errors when available.
Retain no-Card, no-disclosure and before-model assertions. Do not add a new public
tool-error schema or error variant merely to strengthen tests; plain tool failure
representation is not a new runtime defect in this finding. This requires
assertion corrections at existing proof owners rather than new machinery.

### Author the journey Cards as reusable repository fixtures

**FIND-TASK-001-5.** `fresh` (`:855–857`), `write_probe` (`:1026–1086`) and Probe
setup (`:1090–1122`) construct random Card identities and serialize Agent,
Verifier and Service documents in test code. Selection/read/search/extraction
and refusal stories consume that generator. TESTING.md explicitly requires
checked-in Card YAML, fixed meaningful identities and fixtures returning domain
objects. Generated uploads and fresh markers are data and remain appropriate.

Remove the Card document builder and use checked-in positive and deliberately
malformed Card variants in the existing fixture tree, with fixed distinct story
names and exact references matching ordinary uploaded Artifact identities.
Use existing declaration/pull fixture conventions to return domain objects.
Do not rewrite fixtures at runtime, add template substitution, weaken refusals,
mock runtime boundaries or replace the content-sensitive upstream. Replacing one
shared setup boundary closes the violation without redesigning the suite.
Private fixture layout remains implementation-owned.

### Finish the bounded rustdoc and public usage obligations

**FIND-TASK-001-6.** Three mandatory changed-item rustdoc omissions remain:
`wyrd-loader/src/extract.rs:68` leaves the public ExtractError payload undocumented;
`wyrd-loader/src/upload/tests.rs:29–35` omits the one-submission invariant and
possible indexing panic in `paths`; client `cards/handle.rs:278–310` omits upload
cancellation, partial progress and retry semantics. Document these at their
current owners. Trace the existing declaration saga: it is non-atomic, may leave
pending or completed work and mints a new saga key per call. Do not promise
unconditional same-key replay, add rollback or change cancellation. These are
bounded prose corrections batched with functional work, requiring no new tests.

**FIND-TASK-001-7.** Public content changes document tools, selection and extraction
output, but none documents `artifact upload`/`artifact files` or SDK upload and
inventory workflows. Architecture and stubs do not fulfill the spec's final
authority paragraph and original task's public CLI/SDK guidance obligation.

Extend appropriate existing public documentation with plain upload, optional
preparation, exact Active identity/version behavior and authoritative inventory
discovery, linking existing workspace/tool references. Use final Python signatures
and existing docs components; reuse runnable journey-backed sources for copyable
examples. Regenerate the machine-readable copy through its owner. A new docs
system or expansive guide is unnecessary.

## Preserved behavior and non-goals

Keep exact references and UID consistency, existing pending/reconciliation and
version rules, selected-only verified bytes, combined resource limits,
read-only inputs, bounded scratch and per-invocation cleanup. Keep accepted
caller tenancy, tool and downstream grants, nonblocking audits, source/citation
identity, explicit extraction and all existing raw/regex/glob semantics. Keep
one shared parser and thin SDKs. No new endpoint, scope root, cache, result/error
schema, parser format, shell, lifecycle owner or new security/concurrency decision.

Do not add speculative lowered-limit, audit or cancellation requirements to
close this task. Their recorded evidence limits remain visible. Do not fabricate
a historical RED run: for new executable corrections follow the repository's
RED/GREEN discipline where possible, and accurately record verification-only
proof for missing observations and documentation. New failures need diagnosis
before changing tests or production behavior.

## Acceptance mapped to findings

| Finding | Required observable closure |
|---|---|
| -1 | A regex spanning a long UTF-8 line returns its path/actual line/source in a bounded snippet; serialized result fits its limit. Literal and many-hit truncation cases remain valid. |
| -2 | Python full authored Metadata/ArtifactSpec survives upload/get with extraction metadata added; authored extraction reports are refused; invalid names raise exact `WYRD_SPEC_400_VALIDATION` with the field before registration/connection. |
| -3 | Actual sandbox result derives from opaque mounted bytes; captured real search/read/query responses precede response-sensitive final completion. |
| -4 | Each existing structured refusal asserts its exact expected code while preserving nondisclosure and lifecycle assertions. |
| -5 | New stories use checked-in Card fixtures and fixed meaningful identities; source data/fresh marker and real runtime proof are retained. |
| -6 | All three named rustdoc omissions describe the actual field/invariant/interruption behavior. |
| -7 | Public CLI/SDK documentation enables upload and inventory discovery with current signatures and accurate default/opt-in semantics; generated copy agrees. |

## Focused verification

Inspect existing mise tasks before running commands. Coordinate Cargo-backed
work sequentially. Use existing server/Python build setup from the original
task when native/server code changes. Run shared selectors once after combined
changes, not once per finding. Do not run a full journey suite or `gate` for this
remediation; integrated change review owns broad aggregates.

Search regression, exact existing selector:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::workspace::tests::workspace_search_matches_lines)'
```

Run these seven affected Python journeys through the existing repository-managed
database/server setup. This selects the bounded changed stories in the file,
not its complete building-block suite:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_artifact_upload_preserves_files_and_versions tests/integration/test_verifier_building_blocks.py::test_artifact_upload_extracts_before_registration tests/integration/test_verifier_building_blocks.py::test_artifact_workspace_selects_files_and_directories tests/integration/test_verifier_building_blocks.py::test_agent_file_tools_read_without_extraction tests/integration/test_verifier_building_blocks.py::test_agent_file_search_supports_regex_across_files tests/integration/test_verifier_building_blocks.py::test_agent_extract_tool_writes_explicit_output tests/integration/test_verifier_building_blocks.py::test_artifact_workspace_refusals_preserve_authority'
```

Place the invalid-name boundary assertion in the affected upload journey or a
narrow existing public Python test owner; if a new named selector is added,
record its exact collected command before executing it. Do not invent another
harness. Rerun a retained Service/SDK journey only if a changed shared fixture
or dependency affects its accepted behavior; record that reason.

For the expected write set, run `mise run fmt:check`, `mise run lints`,
`mise run py:format:check`, `mise run py:lints`, `mise run py:typecheck`,
`mise run docs:check`, and `mise exec -- git diff --check`. Run the existing
machine-copy generator/check via its documented owner. If an example is added
or changed, run its exact focused journey and record the selector. No TS or wire
generator changes are expected; run their owning checks only if the actual
correction touches those outputs/contracts. Append exact results and diagnoses
to this remediation task without claiming closure from uncommitted edits or
unexecuted commands.
