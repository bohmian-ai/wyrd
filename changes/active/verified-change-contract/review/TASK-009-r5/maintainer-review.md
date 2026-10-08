# TASK-009 r5 Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Candidate observed before and after review: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Task packet: TASK-009 plus R1, R2, R3, and the human-approved R4E replacement

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Shared Rust Run owner | `Run`, `Run::new`, `Run::for_card`, `Run::correlation`, `WyrdState::run`, and `WyrdState::run_for_card`; shared unit tests and Rust SDK journey callers | Cohesive owner/method shape. Initial selection reuses the hydrated graph and preserves immutable sibling views. Naming, domain types, and rustdoc state the local/no-IO and identity invariants. |
| Python PyO3 boundary | `PyWyrdState::run`, `PyRun::{new,for_card,__enter__,__exit__,observe}`, native module registration, and public `wyrd.state` / `wyrd.observe` imports | Thin projection over shared Rust. `__enter__` and `__exit__` keep Python OTel work at the foreign-runtime edge, preserve conventional context-manager signatures, and document failure and durability behavior. |
| Python OpenTelemetry owner | Import-time optional bindings and `_SCOPE_KEY`; `_RunCorrelationProcessor`; `install_run_correlation`; `_enter_run`; `_carries_card_ref`; `_exit_run`; all direct PyO3 and test callers | One existing module owns provider registration and execution-local scope behavior. Names and docstrings expose the token-free stack, never-retry registration, fail-open boundary, and mismatched-exit rule without adding a new abstraction or dependency. |
| Python typing and dependency projection | Generated `_wyrd.pyi`, `observe` and `state` source stubs and installed stubs; `pyproject.toml`; `uv.lock` | Runtime signatures and generated declarations agree: keyword-only optional Card selection, context-manager return types, conventional `__exit__`, and unchanged optional production OTel dependency. The OTLP exporter is development-only. |
| TypeScript projection | `NativeWyrdState::run`, `NativeRunOpen`, `Run`, `WyrdState.run`, generated `.d.ts` / `.d.cts`, and integration callers | N-API remains a thin projection over shared Rust. The public wrapper narrows the internal nullable optional argument to `card?: string`, documents failures, and keeps `Run` view semantics discoverable. |
| Tests and journey | Shared Rust initial-selection tests; Rust SDK journey helper; Python unit scope/provider/failure/concurrency cases; Python authenticated OTLP journey helpers and joins; TypeScript integration assertions | Test names describe caller outcomes, helpers isolate real stages, and the suite covers public signatures, nesting, task isolation, registration idempotency, optional failures, generated parity, and persisted correlation. No new harness was introduced. |
| Active Run architecture | `changes/active/verified-change-contract/architecture/logic/run_api.md`, including the revised token-free scope contract and its relationship to spec revision 46 | The body accurately describes the candidate, but its status header is stale; see `MAINT-009-1`. |
| Adjacent callers/regression surface | Existing Rust, Python, and TypeScript `run()`, `for_card` / `forCard`, Eval active-span, Drift, and Bifrost journey callers found repository-wide | Existing no-argument `run()` calls remain source-compatible. New initial-Card calls are confined to the approved surfaces. No duplicated transport, lifecycle, or durable behavior was introduced. |

## Material findings

### `MAINT-009-1` — the changed Run authority claims an obsolete specification revision

- **Location:** `changes/active/verified-change-contract/architecture/logic/run_api.md:3`
- **Governing principle:** maintainer documentation must describe the current typed contract and must remain subordinate and traceable to the approved specification; the task-review subject is specification revision 46.
- **Evidence:** the header still says `Approved client interface through specification revision 34`, while the same changed document now specifies revision-46 behavior: the import-time private context key, token-free tuple stack, top-pair exit rule, and at-most-once provider registration. `spec.md:3` identifies revision 46, and its REQ-151 contains those same requirements.
- **Concrete maintenance cost:** a maintainer or later reviewer using the document's own status cannot tell whether the newly written token-free contract is approved authority or post-revision drift. That ambiguity is especially costly here because the document explicitly records a deliberate departure from normal OpenTelemetry attach/detach guidance.
- **Smallest testable correction:** update the status line to identify specification revision 46. Verify the document and spec report the same revision and leave the substantive Run API text unchanged.

## Calibration notes

- `test_observe_surface.py::_wyrd_processors` reads OpenTelemetry SDK private fields to count processors. This is brittle across an OTel SDK upgrade, but it is test-only, the dependency is locked, and the test directly proves an approved idempotency obligation for which the SDK exposes no public inventory. I do not treat it as a material finding.
- The single shared duck-typed processor and module-level registration cache are the minimum existing-module solution for the approved behavior; introducing provider wrappers, protocols, or a separate correlation service would add maintenance cost without improving the contract.

## Verification notes

- Reviewed the complete base-to-candidate source diff, all materially changed symbols in their owning modules, repository-wide callers, generated declaration parity, and the focused and journey tests named above.
- Reviewed the candidate's recorded successful focused, unit, integration, typecheck, codegen, boundary, format, and lint evidence in the task/remediation packet.
- Per discovery instructions, no Cargo, pytest, or mise command was run by this reviewer. `git diff --check` for the immutable range returned clean.

## Overall result

**FAIL** — implementation layout, ownership, naming, types, tests, and generated projections are maintainable, but the changed Run authority's stale revision status is a material documentation inconsistency with the revision-46 subject.
