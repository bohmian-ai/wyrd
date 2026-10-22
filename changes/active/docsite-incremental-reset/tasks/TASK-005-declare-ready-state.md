---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 7
requirements: [REQ-003, REQ-004, REQ-007, INV-002, INV-003, INV-004, INV-005, AC-002, AC-003, AC-008]
depends_on: []
parent_task:
remediates: []
---

## Outcome and Value

One call, `WyrdState.declare("support-reply.yaml")`, resolves and declares the
authored Service graph and returns a state ready for `state.run()` and
verification. It replaces reader-written declare → pull → offline-load
ceremony in all three first-class SDKs. `from_path` stays offline.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement`. Add the capability to shared `wyrd-client` and thin
Rust/Python/TypeScript projections. Python's `declare(path, client=None,
*, bundle_dir=None, ...)` uses existing hydration/trust arguments; TypeScript
uses `await WyrdState.declare(path, { client?, bundleDir? })`; Rust exposes
async `WyrdState::declare(path)` and the idiomatic explicit-client/configured
projection. Retain the same resolved client throughout the operation and state.
Do not add declaration IO to any `from_path`/`fromPath` variant.

`WyrdState` owns every resource needed to remain usable; developers never
manage private directories. Card metadata lives in memory. Delete all download/
hydration staging before returning a successful `declare`, for both metadata
and artifact-backed states. Required Model/Data backing files live at stable
paths in private state-owned runtime storage, retained by dependent states,
Runs and SDK holders and cleaned best-effort after the last dependent owner
releases them. Loaders must receive paths that remain usable after staging
removal; do not load against staging and then invalidate those references.
Do not force large models/datasets into RAM merely to eliminate files. A
metadata-only state retains no private directory. Shutdown must not invalidate
live runtime resources.

Explicit `bundle_dir` is caller-owned persistent output, using existing staged
replacement and rollback. No persistent default `.wyrd-bundle`, public cleanup
API, zero-disk guarantee, new cache, registry, loader, or transport. Separation
of staging and runtime backing is an ownership/lifetime requirement; private
directory layout and transfer mechanics remain implementation-owned.

Application-managed models/data, including MLflow models, remain independent:
the minimal Service/Verifier graph judges JSON without Wyrd Model/Data Cards.
Existing Wyrd artifact hydration remains supported with its integrity and
trust requirements. No MLflow adapter or local-artifact download optimization.

Remove public `WyrdState.artifacts` and public-only `HydratedArtifact`
projections. No application/example consumer or design requirement was found;
current calls in Rust/TS/Python/CLI tests inspect downloaded payloads. Preserve
that integrity proof at the existing bundle/storage/hydration owners. Internal
artifact metadata and loader directories remain necessary and must not be
deleted merely because the public accessor disappears. This is an intentional
SDK API removal; document it without a compatibility alias.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Declare exact graph and prepare state | `wyrd-client/src/cards/handle.rs`: `Cards::declare_from_path`, `pull`; `cards/pull/mod.rs`: `CardGraphPuller::pull`; `state.rs`: `WyrdState::from_path_with` | Quickstart's former manual flow; Rust `declare_and_pull.rs`; TS `declare-and-pull.test.ts` | No one-call authored-YAML constructor | Compose these existing workflows under `WyrdState::declare`, resolving one client | No replacement graph traversal, registration or hydration |
| Staging and runtime resource lifetime | `state.rs`: shared `WyrdState` index; `observe/mod.rs`: `Run` retains state; pull staging/publication owner | Shared artifact validation; Python Model/Data hydrate via `artifact_dir_by_key`; Rust/TS artifact-path journeys | Bundle ownership is external and does not distinguish disposable staging from required runtime files | Delete staging before return; give runtime values shared ownership of stable required payload paths; reuse installed `tempfile` | A storage lifetime owner is earned; no new cache, lifecycle service or dependency |
| Language projections | Python `src/state/mod.rs`: `PyWyrdState::from_path`; `hydrator.rs`: `PythonStateHydrator`; TS native `cards.rs` and public `index.ts`: `WyrdState`; Rust SDK re-exports `wyrd-client` | Python public state tests/stubs, Rust SDK integration target, TS SDK integration target | New entry point and typing absent | Thin projections and existing hydrate path, with ordinary runtime/async boundaries | No Python-only orchestration or foreign-runtime tests in Rust |
| Useful failure and sample | `wyrd-spec/src/error.rs`: `RegistrySpecDrift` already has version-bump remediation; existing loader/auth errors | `examples/quickstart/{support-reply.yaml,refund-limit.yaml,quickstart.py}` and Python docsite journey | Script currently incorrectly calls authored YAML through `from_path`; implicit bundle assumption in refusal test | Switch to `declare`, update journey, preserve catalog errors and existing version rules | No new error code, silent bump, alias, or compatibility path |
| Remove unused public inventory | `wyrd-client/src/state.rs`: `WyrdState::artifacts`, `HydratedArtifact`; Python/TS descriptors and forwarding | Rust/TS declaration journeys; Python state journey; CLI `tests/card_lifecycle.rs`; Python authored stub example | File inspection API exists for tests without an application consumer | Remove public method/projections and adapt test consumers; preserve internal inventory, `artifact_dir_by_key` and trusted-manifest reconstruction | No replacement public file-inspection API |

Owner paths above are relative to `crates/shared/` for `wyrd-client`, and
`sdks/wyrd-sdk-{python,ts,rust}/` for language surfaces. Paths guide ownership;
private layout and fixture mechanics remain implementation-owned.

## Approach

1. Add the shared async operation using existing declaration, complete pull,
   and validated offline hydration; preserve the current Service root boundary.
2. Establish stable state-owned backing paths for required Model/Data payloads,
   complete hydration, and delete staging before return. Own runtime files
   across dependent handles, failures and cancellation. Reuse persistent pull
   behavior for opt-in output.
3. Expose the operation and its existing client/trust inputs through each SDK;
   update authored stubs, exports/docs, generated N-API declarations and typing.
4. Remove the unused public artifact accessor/projections and update its
   consumers without weakening internal integrity/loading tests. Change
   Quickstart to `declare`, preserve both referenced YAML files, and prove the
   scenarios below through actual SDK → server → SDK journeys.
5. Record that public docs must wait for a compatible released SDK carrying
   this API. Preserve TASK-001's historical evidence and rerun affected proof;
   do not certify this API using its earlier 0.2.0 manual-flow walkthrough.

## Ordered Implementation Scenarios

Commands J-Python/J-Rust/J-TypeScript and R-Python below are exact focused
commands. Rust/TS new selectors are planned additions to inspected existing
targets. Extend these journeys scenario by scenario; extra focused tests may
be added by the implementer with their exact commands recorded in evidence.

### Scenario 1 — Authored YAML becomes a ready verification state

**Behavior.** Resolve the relative Verifier file, declare the Service graph,
and return a state whose exact root and Verifier produce passing and failing
JSON judgments in every SDK. No provider or Wyrd Model/Data Card is needed.

**RED.** Run J-Python against the sample's corrected `declare` call before
implementing the API; add J-Rust and J-TypeScript to expose missing parity.
Missing-method/type failures are the expected starting evidence.

**GREEN.** Compose the current shared owners; project the same operation to
each language. Return only fully validated/hydrated state, using the same
client for all calls. Run all three J commands.

**REFACTOR.** Consolidate orchestration in the shared owner and reuse current
language hydration/runtime helpers, retaining green journeys.

### Scenario 2 — Repeat calls and refusals preserve server semantics

**Behavior.** Unchanged reruns follow existing version/idempotency rules;
different same-version content is refused with `WYRD_REGISTRY_409_SPEC_DRIFT`
and existing bump/revert guidance. Declaration/read/invocation permissions
remain separate. Missing/malformed references and unauthorized calls return
their existing structured errors; no implicit privilege or version escalation.

**RED.** Extend the three J journeys with repeat, pinned-version conflict and
permission-negative assertions. Run them plus R-Python. Use pinned versions
for the drift case: the sample's omitted versions retain their existing rules.

**GREEN.** Preserve errors from existing owners and reuse the catalog's
remediation. Adapt R-Python so it no longer assumes default persistent output;
its invocation refusal must still reach the server with an already usable
state, rather than fail earlier during declaration. Rerun Scenario 1 proof.

**REFACTOR.** Remove duplicated validation/error translation; preserve stable
codes, redaction, authorization and non-blocking audit behavior.

### Scenario 3 — Staging disappears while runtime resources remain usable

**Behavior.** No download/hydration staging remains when a default call returns,
including for Model/Data graphs. Metadata-only states retain no directory.
File-backed model/data operations remain usable through stable state-owned
payload paths after staging removal and after the original state wrapper is
released while a dependent SDK owner remains. Default calls do not collide.
Final dependent release cleans owned runtime files. Failure/cancellation
publishes no partially ready state and cleans staging/unclaimed runtime files
without rolling back committed declarations. An explicit bundle destination
persists after state release and still loads offline. Failed replacement
preserves prior output under the current staged pull semantics.

**RED.** Add assertions for staging absence before return, continued file-backed
Model/Data use, dependent-owner lifetime, independent calls, retained output
and failure/cancellation cleanup to the J journeys. Reuse current artifact
fixtures and interface support; exercise Python holder lifetime through Python,
Node lifetime through Node, and shared ownership through Rust. Use deterministic
synchronization, not sleeps. Checking metadata-only cleanup is insufficient.

**GREEN.** Hydrate against stable backing-file locations with the smallest
shared runtime storage owner; delete staging before return. Compose existing
pull publication and preserve caller-owned retained output. Keep required
runtime files available until dependent SDK owners release them, including
after shutdown. Run J commands and Python state regressions.

**REFACTOR.** Reuse installed ownership/temporary-directory mechanisms; avoid
duplicated per-language cleanup loops, a new cache, or retention configuration.

### Scenario 4 — Offline loading and artifact safeguards remain intact

**Behavior.** `from_path`/`fromPath` still loads existing bundles with no network,
and does not reinterpret YAML as a registration request. Valid Wyrd Model/Data
graphs keep existing complete-bundle and Python interface/trust checks. An
externally managed model is never discovered, registered or loaded implicitly.

**RED.** Add offline/incorrect-input and artifact regressions to the J journeys;
extend Python hydration proof only for behavior newly reached through `declare`.
Already-correct offline/trust behavior uses green regression evidence, not a
manufactured production failure.

**GREEN.** Reuse the validated bundle and Python hydrator, including existing
trusted-hash/loader arguments. Run all earlier focused proof and existing
Python state tests. Do not bypass artifact validation to shorten the snippet.

**REFACTOR.** Share existing construction logic without changing offline
semantics, eager Python loading, or foreign model ownership.

### Scenario 5 — Remove public artifact inventory without losing integrity proof

**Behavior.** The three SDK public state APIs no longer expose `artifacts`
or public-only artifact descriptors. Declaration, Model/Data hydration, trusted
manifest validation and tampered/missing-payload refusals retain their existing
proof. Rust/TS state projections remain metadata views; removing this accessor
does not turn them into framework model loaders.

**RED.** Add export/type/surface assertions through the existing Python state
and TS unit targets, and Rust SDK surface checks, establishing that the public
inventory is still exposed. Run the supporting scoped commands below. Keep
integrity regressions green while moving test-only inspection to its existing
bundle/hydration owner. Record exact selectors for new named checks.

**GREEN.** Remove public forwarding, method and descriptor exports/stubs/
registration as applicable. Keep shared internal artifact metadata needed by
`validate_artifact`, `artifact_manifest_hash_by_key`, and Python loaders.
Update existing Rust/TS/Python/CLI test consumers to prove supported file or
runtime behavior through the appropriate owner. Rerun all earlier scenarios.

**REFACTOR.** Delete obsolete public-only projection machinery. Do not create
a replacement facade to preserve the removed inspection API indirectly.

## Acceptance Criteria

- Three SDKs expose the explicit operation; the current Quickstart imports
  `WyrdState` from `wyrd`, calls `declare`, and passes its real-server journey.
- Relative references, exact identities, pass/fail, rerun, pinned drift and
  insufficient authority work with the same principal and stable errors.
- Staging is absent before successful return for metadata and Model/Data graphs;
  state-owned backing files support later runtime operations and dependent
  handles without user directory management or forced full-RAM loading.
  Failure/cancellation cleanup and explicit retained output satisfy REQ-007.
- Offline constructors and Model/Data integrity/trust regressions pass.
- Public artifact inventory/projection types are removed across SDKs; internal
  artifact validation and loader access remain covered. Caller-owned files and
  previously valid persistent output are not removed by private cleanup.
- No external model integration, new server route, migration, permission,
  SDK-specific durable behavior, or new dependency is introduced.
- Typing/export/generated-declaration checks pass; released documentation is
  not updated to promise an API unavailable in its declared release.

## Expected Write Set and Consumer Closure

Shared `wyrd-client` Cards/state/lifetime owners; Rust SDK re-export/docs as
needed; Python state wrapper/hydrator and public state/root stubs/exports;
TypeScript native Cards/state, public state/options and generated declarations;
the existing Quickstart source and its journey; focused SDK journeys and state
regressions, CLI lifecycle test consumers, and public descriptor registrations/
exports/stubs. Update the owning client architecture/API documentation to name
the online constructor separately from offline loaders. Preserve the user's
working sample edits while correcting the constructor during implementation.

## Verification and Evidence

Runner ownership is confirmed from `mise.toml`, Rust's `tests/integration/main.rs`
target, Python's integration markers, and the TS package/Vitest configuration.
Setup: `mise run build:server`, `mise run py:setup`, `mise run ts:build`, and
`mise run ts:build:testing`. Keep Cargo-backed setup/checks sequential.

**J-Python** — existing exact Quickstart journey:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats
```
**R-Python** — existing exact refusal journey:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_refuses_insufficient_authority
```
**J-Rust** — planned test in the current SDK integration target, using its
existing ignored journey convention:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration --run-ignored ignored-only -E 'test(=declare_and_pull::declares_ready_verification_state)'
```
**J-TypeScript** — planned test in the current integration file/runner:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/declare-and-pull.test.ts -t '^declares a ready verification state$'
```

Supporting regressions and boundary checks:
```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^state::/)'
mise exec -- cargo nextest run --locked -p wyrd-ai --lib -E 'test(=tests::sdk_root_exposes_the_supported_surface)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::cli_bundle_loads_typed_wyrdstate_after_server_shutdown)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::multi_card_service_pull_publishes_complete_and_metadata_bundles)'"
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run check:deps
mise run codegen:check
git diff --check
```
Apply pinned Ruff checks to `examples/quickstart/` as well. Regenerate N-API
declarations through `ts:build`, never hand-edit them. Keep new Python/Node
lifetime behavior in those runtimes. Enable tracing before diagnosing failures.
Document cancellation/partial-progress, staging cleanup before return, and
private runtime-storage lifetime. Include a test that performs file-backed
Model/Data work after staging is gone; do not infer readiness from loader return
or Python object creation alone.
Do not run broad multi-language journey aggregates as this task's iteration
proof. TASK-001/002 own the new released-artifact walkthrough once available.

## Material Stop Conditions

Stop if readiness requires a new server contract, changing supported root
kinds, permissions, artifact trust, offline behavior or external-model ownership.
The public artifact accessor removal is explicitly authorized; unanticipated
application consumers require reassessing the use-case evidence, not silently
deleting their workflow.
Temporary-storage lifetime/private plumbing and shared composition are in scope;
local-artifact reuse and in-memory hydration are deferred, not blockers. Lack
of a release blocks publication, not implementation/testing of this additive
SDK capability. Preserve the existing recorded macOS startup blocker for its
own scope; do not claim this constructor fixes it.

## Authority Links

- [Approved spec revision 7](../spec.md), REQ-007/AC-008
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../AGENTS.md)
- [Protocol](../../../../architecture/wyrd-design.md) and [doctrine](../../../../architecture/wyrd-design.md)
- [Spec workflow](../../../../changes/spec-driven-development.md)
- [Implementation execution](../../../../changes/implementation-execution.md)
- [Testing workflows](../../../../TESTING.md)

## Implementation Evidence

Reuse map: `Cards::declare_from_path` (loader + `./path` resolution), `Cards::pull`
(staged rename publication), and `WyrdState::from_path_with` (load and integrity)
are composed by `WyrdState::declare_with` in `wyrd-client/src/state.rs`. Without
`bundle_dir` the bundle is pulled into a `tempfile` directory (existing
dependency) that is dropped before return. Spec revision 8 withdrew the
state-owned payload storage revision 7 required; it was implemented and then
stripped, so no `runtime_files` field remains in `wyrd-client` or `wyrd-cards`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Three SDKs expose `declare`; Quickstart uses it | `state.rs` `declare`/`declare_with`/`DeclareOptions`; Python `src/state/mod.rs` `declare` + `.pyi`; TS `native/src/cards.rs` `declare_wyrd_state`, `index.ts` `WyrdState.declare`; `examples/quickstart/quickstart.py` | `test_docsite_quickstart.py` (3 pass); Rust `declares_ready_verification_state`; TS `declares a ready verification state` | PASS |
| Refs, identities, pass/fail, rerun, drift, authority, missing file | same | Rust/TS/Python journeys assert `WYRD_REGISTRY_409_SPEC_DRIFT`, `WYRD_PERMISSION_403_DENIED_RBAC`, `WYRD_LOADER_400_INVALID_ENVELOPE` | PASS |
| No files without `bundle_dir`, including on failure; retained `bundle_dir` | `declare_with` drops its temporary directory before return | `test_docsite_quickstart_verifies_and_repeats` (tmp empty); `test_refused_declaration_keeps_no_files`; `test_declared_bundle_dir_persists_and_loads_offline`; Rust/TS `bundleDir` offline load | PASS |
| Offline constructors and integrity/trust regressions | `from_path` unchanged | `wyrd-client` `state::` (46 pass); `test_from_path_never_declares_a_card_file`; Python state journey; CLI `cli_bundle_loads_typed_wyrdstate_after_server_shutdown`, `multi_card_service_pull_publishes_complete_and_metadata_bundles` | PASS |
| Public artifact inventory removed across SDKs | `HydratedArtifact` private, `artifacts` removed in Rust/Python/TS | Rust `compile_fail` doctests; `test_state_declares_and_lists_no_artifact_files`; TS `"artifacts" in state` false | PASS |
| No route, migration, permission, durable SDK behavior, or dependency | diff audit | `check:deps`, `codegen:check` | PASS |
| Typing/export/generated declarations; docs not ahead of release | regenerated `index.d.ts`/`index.d.cts`; public docs untouched; `architecture/wyrd-design.md` names `declare` | `py:typecheck`, `py:lints`, `ts:typecheck`, `ts:lints`, `ts:format:check`, `lints`, `fmt`, `git diff --check`; `ts:napi:check` diffs the regenerated declaration against the index and passes once it is staged | PASS |

Commands: exact nextest/pytest/vitest selectors above, run under
`scripts/postgres/with-test-postgres.sh` where a server is needed; `mise run
py:test:unit`.

Findings:
- The loader requires an exact `metadata.version` on any Card referenced by a
  `./path` ref (`wyrd-loader/src/parse.rs`), so `examples/quickstart/refund-limit.yaml`
  pins `version: 1.0.0`. Candidate ergonomic gap.
- TypeScript `observe.verify` reads a plain object as named slots; Python passes
  it to the single slot. The TS journey uses `{ positional: true }`.
- Public docs wait for a release that carries `declare`.
