# TASK-004 independent maintainer review

Result: **FAIL**

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
- Candidate: `07adb7907951f343544e550f6cabc5b7aa3aadea`.
- Packet: approved specification r7 and original TASK-004, supplied in `subject.md`; r7 TASK-005 coordination is respected. The later packet-only HEAD is not treated as an implementation revision.
- External demo: frozen `/tmp/wyrd-task004-external-07adb7907`, compared with `external.diff`; its changes are not an independently committed implementation subject.
- Read-only source/diff audit. CodeGraph was used first for shared execution/input ownership and result producers/consumers. No implementation, generator, or other review report was changed or consulted.

Authorities applied: `AGENTS.md`, `architecture/agent-rules.md`, the reference router, `languages/maintainer-style.md`, Rust core, PyO3 boundaries, Python API/stubs, TypeScript guide, testing workflows, spec-driven development, and the applicable verifier/SDK sections of Wyrd design, doctrine, Bifrost design, and the approved packet. Findings below concern maintenance of the requested public capability, not a redesign or an additional product contract.

## Changed-surface coverage

| Surface | Owners, changed symbols, callers and proof inspected | Assessment |
| --- | --- | --- |
| Shared evidence authoring and projection | `verify/mod.rs`: `File`, `FileBytes`, file construction/format/pin, `Upload`, `ArtifactRef`, `Input` conversions, `Inputs`, `Submission`, `Target`, `Projected`, `RunEvidence`, projection/request/execute/enqueue/pin, samples/type/error helpers; `Observe::{verify,submit,target,drift_value}` and `Run` correlation; owner tests covering named values, files, retry keys and waits | Cohesive Rust-owned projection; closed public input/submission types; client remains the network owner. |
| Hydrated binding and retained lifecycle | `state.rs::BoundVerifier`/`bound_verifier`, retained client resolution; `observe/lifecycle.rs` startup/close ownership; drift projection and callers in all languages/capacity tests | New slot conversion is explicitly staged for TASK-005's schema cutover; no parallel graph loader or lifecycle owner introduced. Removal of Eval destination follows removed emits. |
| File IO and storage reuse | `storage::WyrdStorageClient::upload_evidence`, existing upload dispatch/hooks/FileSource; shared `sha256_file`; `cards/saga/hash_artifacts.rs::hash_source` and registration caller | Existing transfer executor is reused. Evidence planning needs no Artifact registration/completion lifecycle, so the narrow executor extension is sufficient; new PUT transport, upload manager or registry workflow was not introduced. Hashing is shared with existing registration. |
| Execution/reconnect | `verify/execution.rs` construction, connect/connect_with, id/status/wait; existing Cards/client connection conventions; Rust/Python/Node callers and scripted status tests | One client and execution identity per handle; polling/replay policy remains shared. Deadline/recovery behavior belongs to the system/domain audits; no second SDK polling implementation found. |
| Rust SDK | `src/lib.rs` exports and export test; all changed journey families, support helpers, real-time/gateway attribution, scheduled drift/Operator, support desk, building blocks, action timeline, observation/local development | Thin re-export; domain identifiers and typed results retained. New helpers have cohesive setup ownership. Existing provider journeys are retained while removed LLM-judge cases are removed with the old surface. |
| Python boundary and package | `src/verify.rs`, `src/observe/mod.rs` converters and methods, aggregation; `python/wyrd/{verify,observe}` runtime/stubs and root exports; unit exports/input tests and integration callers | Correct foreign-runtime placement and shared-runtime calls. Public fixed response structures lose their type information: MNT-001. Two new wrapper fields lack required rustdoc: MNT-002. |
| TypeScript boundary and declarations | `native/src/verification.rs` input conversion/File mapping/NativeSubmission/NativeExecution/client connect; `native/src/cards.rs::NativeRun`; authored `src/index.ts` input/submission/namespace/Observe operations, native declarations, generated wire types/errors and their source roots | Thin native projection with typed public execution/result contracts. Generated declarations correspond to the changed Rust binding shapes. Boundary JSON conversion remains in its existing serializer. |
| Test and harness consumers | Shared Observe tests; Python integration fixture/provider and scheduled-result helpers; Node server/assistant/provider helpers and action-timeline tests; three-language verification/building-block/drift/support-desk journeys; changed `wyrd-testing` capacity/observe-ingest and MCP test callers; mise selectors | Purpose remains real public client/server workflows; transport doubles prove client policy separately. Full execution is deliberately deferred by TASK-004. No fresh harness dependency is introduced. |
| Examples and fixtures | Matched support-desk entrypoints, service policy-reader/policy-judge/policy-check and PDF; shared verification/drift/observation/registration fixtures; frozen external demo request path, outcome projection, reconnect script, vendor build/package setup and authored tests | File and background execution behavior uses SDK owners. Examples retain their existing application/MCP explanation shape. External build provenance and integrated execution remain limits, not inferred proof. |
| Docs and architecture | Changed verify/submit, evaluation/drift, observation and evidence guides; extraction/runtime/tool reference pages; MCP table; diagram/landing text; Wyrd/Bifrost/design/doctrine/reference updates and packet revisions | Contracts are discoverable through the expected guide/reference paths; r7 server-owned seams are explicit. No separate compatibility usage path introduced. |
| Tooling and dependencies | Schema roots, `gen_ts_types.py`, generated card/error/native declaration diff, `mise.toml`, manifests/lockfile, deleted Python eval package | Reuses generation and verification lanes; added Rust media re-export dependency has a concrete public API caller. |

## Material proposed findings

### MNT-001 — Python discards the declared types of fixed execution and breakdown records

- Classification: **MISSING** (public typed contract).
- Location: `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:89`, `:94`, `:99`, `:134`; corresponding producers in `sdks/wyrd-sdk-python/src/verify.rs:139`, `:211`, `:223`, `:235`.
- Obligation: REQ-011 requires typed results in all three SDKs; TASK-004 scenario 3 and acceptance require typed inputs/results/handles and authored Python typing. `languages/maintainer-style.md` says documentation cannot replace a missing/broad type and warrants a finding when the public signature hides the result shape. `languages/python-api-and-stubs.md` requires precise public signatures unless an API truly accepts arbitrary input.
- Producer-to-consumer evidence: Rust `verification/execution.rs` defines closed `VerificationExecutionState` fields and `VerificationResult`/`VerificationOutcome`, `RequirementResult`, `StepResult`, `InputDeclaration` schemas. Shared `Execution::status` returns that typed state. The new PyO3 getters serialize these values to ordinary Python dictionaries, which is a legitimate projection; the new authored stubs then declare the entire closed status as `dict[str, Any]` and entire requirement/step/evidence records as dictionaries of `Any`. Consequently `execution.status()["stauts"]`, an invalid lifecycle comparison, and a misspelled requirement/citation/provenance field all escape Python checking. The new Python building-block journeys' `requirement` and `step` helpers propagate those untyped records, whereas equivalent Rust and TypeScript consumers get named schemas and closed status/outcome literals.
- Concrete cost/consequence: a user cannot discover or safely refactor the advertised execution and attributed breakdown contract from the Python API; schema changes cannot expose mistyped consumers through the package's typecheck. This concerns fixed metadata and structure, not the deliberately arbitrary JSON in an action output or captured actual value.
- Existing mechanism: Python already uses `TypedDict` and discriminated unions for served records, for example `python/wyrd/operators/__init__.pyi::_OperatorConnectionViewBase` and provider variants; `wyrd.cards.JsonValue` exists for genuinely open JSON leaves. No Python parser/model or parallel implementation is needed.
- Smallest correction: keep the current Rust serialization and dictionary runtime representation; author and export precise typed dictionary/union projections of the existing wire response records once, use them for status/requirements/steps/evidence, and retain open JSON typing only at actual arbitrary-data leaves. Reuse existing `CardRef` wire mapping/JSON aliases where applicable; do not annotate the serialized status `result` as the native `VerificationResult` object, because it is a dictionary there. Keep public exports and stubs synchronized.
- Focused proof: a small Python typing fixture that accepts the real status/outcome/requirement/citation fields and rejects wrong fields/types, plus existing runtime result/export coverage and `py:lints`/`py:typecheck`. No real-server journey or new runtime test harness is needed for this typing correction.

### MNT-002 — New PyO3 wrappers omit required field rustdoc

- Classification: **VIOLATION** (explicit rustdoc gate).
- Locations: `sdks/wyrd-sdk-python/src/verify.rs:98` (`PyExecution`'s tuple field) and `:162` (`PyVerificationResult`'s tuple field).
- Obligation: `AGENTS.md` §16 and `architecture/agent-rules.md` explicitly require substantive rustdoc on every new/materially modified Rust field regardless of visibility; the review skill explicitly retains this as blocking.
- Evidence: both newly introduced tuple fields are written inline without a field doc comment. Their surrounding struct docs document the Python-facing object, but neither field documents the wrapped shared client's ownership or the immutable result whose getters it feeds. By contrast this same file's `PyFile::inner` and `PyArtifactRef::inner` fields are explicitly documented.
- Concrete cost: the native wrapper's ownership boundary is undocumented at the retained field; more importantly the mandatory changed-item documentation gate remains unmet even with green Clippy. This is one bounded documentation correction to batch with implementation remediation, not a request for a style-only review round.
- Smallest correction: add field-level rustdoc describing the shared execution/client owner and the wrapped attributable result; keep the tuple shape and behavior. No rename, abstraction, or unrelated documentation sweep.
- Focused proof: inspect both field docs and run the narrow formatting/check lane for the touched Rust boundary. Runtime behavior does not change and needs no new test.

## Non-blocking notes

`crates/shared/wyrd-client/src/observe/mod.rs:249` retains an `Observe::drift` rustdoc link after removal of that method. Correct it to the surviving lifecycle/error description in the same edit when convenient. This is not a separate material finding or a basis for FAIL.

No preference-only findings are proposed. In particular the new `verify` module and `Execution` handle are approved capability boundaries; introducing them is not itself complexity drift. The evidence upload extension reuses the complete applicable byte-transfer owner rather than merely its low-level HTTP leaf.

## Verification limits

Reported green checks were treated as supplied claims, not independently rerun logs. No Cargo/build/codegen command or full journey suite was run during this read-only audit. Full SDK/server journeys are intentionally deferred until TASK-005 under the task's coordination; this report does not turn authored journeys into executed proof. The external demo remains a frozen uncommitted snapshot, so its package build and end-to-end behavior cannot be attributed to a committed external candidate.

Overall **FAIL** for MNT-001 and MNT-002, subject to the required independent finding validation. No implementation changes were made.
