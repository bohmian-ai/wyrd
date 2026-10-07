---
id: TASK-017
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 64
requirements: [REQ-192, REQ-204, AC-048, AC-057]
depends_on: [TASK-016]
---

# Make every client-facing test a readable public example

## Outcome and Value

Rust, Python, and TypeScript client-facing tests follow one checked-in, reviewable standard. They tell the same user stories with shared Card YAML, public SDK and in-process CLI surfaces, typed domain results, exact error codes, deployment-shaped server setup, and no internal workarounds. Every DELETE, MOVE, REWRITE, and TIGHTEN verdict in the authoritative SDK audit is resolved, every remaining test file belongs to a `mise` lane, and `TESTING.md` records the standard used for review (REQ-192, REQ-204, AC-048, AC-057).

The rewrite also supplies the client-facing journey evidence for TASK-016's AC-045..058 behavior, but it does not own or change those production contracts.

## Owners, Scope, Consumers, and Prohibited Changes

- The authoritative inventory is exactly `review/sdk-test-audit/{summary.md,rust.md,typescript.md,python-integration.md,python-unit.md}`. Every non-KEEP row receives its recorded DELETE, MOVE, REWRITE, or TIGHTEN outcome; KEEP rows remain unless consolidation preserves the same behavior more clearly.
- Repository-root `fixtures/cards/<story>/` owns checked-in Card YAML shared by all three SDKs. Additional checked-in baseline or invalid inputs may live under the existing repository fixture root when a story requires them, but the implementer owns the smallest clear arrangement.
- `TESTING.md` owns the client-test checklist. `mise.toml` and current package test configuration own lane selection.
- SDK test trees own public unit, type, and real-server journeys. Engine mathematics, caches, fences, audit staging, legacy-format behavior, binding/run lifecycle, and other audit-marked internal assertions move to their named existing Rust owner tier.
- Public test setup may use only domain-object fixtures and the three TASK-016 controls: `flush_bifrost`, `wait_for_baseline`, and `make_binding_due`.
- Prohibited: production API changes, compatibility shims, private/extension imports, raw HTTP or SQL in client journeys, subprocess CLI use, generated YAML/JSON/digests/URLs, sleeps or polling loops, multiple accepted error codes, UUID/time-suffixed fixture names, hand-edited generated stubs, Bifrost schema/storage/Iceberg/Scribe/Forge/Oracle/SQL-engine changes, and weakening or deleting behavior that the audit says KEEP or MOVE.

## Approach

1. Add the concise REQ-192 checklist to `TESTING.md`, then establish shared checked-in Card stories modelled on the referenced opsml PromptCard test: public `from_path`, domain fixtures in `conftest`/support, and direct typed assertions.
2. Rewrite the cross-language journeys against TASK-016's exact public API, using matching story/file/test names and the deployment-shaped `WyrdTestServer` environment contract.
3. Process every audit ledger row: delete tests of nothing, move internal behavior to its Rust owner, rewrite workaround-based tests, and tighten otherwise-correct tests.
4. Move Python and TypeScript type-only assertions out of runtime pytest/Vitest collection into `ty` fixtures and `expectTypeOf` compile-time tests.
5. Align Python runtime/stub test-server surfaces and `mise` membership, then prove no client-facing test file is orphaned.

## Ordered Implementation Scenarios

### Scenario 1 — The standard and shared Cards exist before test rewrites

**Behavior.** `TESTING.md` records every REQ-192 bullet as the review checklist, and each retained cross-language journey loads realistic Cards from a shared repository-root fixture story rather than constructing or editing YAML, JSON, digests, or URLs in code (REQ-192, AC-048).

**RED.** Review `TESTING.md` and the three current journey trees against REQ-192, then run `rg -n 'apiVersion: wyrd/v1|kind: (Service|Agent|Verifier|Operator|Data|Model|Prompt)' sdks/wyrd-sdk-rust/tests sdks/wyrd-sdk-python/tests sdks/wyrd-sdk-ts/wyrd/tests` to record the current generated-Card violations. This is a static RED: the approved checklist and shared fixture corpus do not yet exist.

**GREEN.** Add the checklist and only the checked-in Card stories needed by retained/revised journeys. Tests load them through public authoring/CLI surfaces and fixtures return domain objects.

**REFACTOR.** Consolidate duplicated Cards only when the resulting YAML still reads like a Card a user would author; do not hide distinct stories behind an indirection layer.

### Scenario 2 — Rust client journeys become one-story public examples

**Behavior.** Every Rust SDK and `wyrd-client` audit row is resolved. Retained SDK journeys cover one story per file/test through public APIs, including TASK-016's verification, typed Card reads, alias, CLI, token, parameterized SQL, built-in, and error behavior. Audit-marked internal/reliability assertions move to the named Rust server, Vala, Bifrost, queue, or benchmark tier (REQ-192, REQ-204, AC-057).

**RED.** Run the current audited Rust client targets independently under the repository Postgres lifecycle to capture their baseline before edits: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all"` and `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-client --features test-support --test integration -P journey --run-ignored=all"`. The audit itself is the expected-failure ledger: obsolete handles, omnibus stories, generated YAML, internal hooks, formatted SQL, and misplaced engine assertions remain.

**GREEN.** Apply every Rust audit verdict, preserving required behavior in its proper tier. Use exact named-test nextest selectors during each individual rewrite once its final test name exists; record those commands in the implementation evidence before completing the task.

**REFACTOR.** Reuse the shared fixtures and one existing Rust support owner; delete duplicated mock servers/config builders where the installed `wiremock` or current harness already covers the need.

### Scenario 3 — Python integration journeys use only public synchronous workflows

**Behavior.** Every Python integration audit row is resolved. Journeys use public `wyrd` imports, in-process CLI functions, the session test server's exported environment, domain fixtures, exact `WyrdError.code`, typed Cards/Judgments/query terminals, and no subprocess, raw HTTP, raw SQL, sleep, polling, or private test hook (REQ-192, REQ-204, AC-057).

**RED.** Run the repository-owned integration lane, which selects the audited files under one live Postgres lifecycle: `mise run py:test:integration`. The audit supplies the expected readability and boundary failures even where the old test is green. During each rewrite, use `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration <path>::<test>"` with the final path and top-level test name.

**GREEN.** Rewrite or remove each row exactly as audited, using TASK-016's API. Keep Python synchronous and make the test bodies short enough to serve as examples.

**REFACTOR.** Consolidate environment/server/receiver setup in typed pytest fixtures, use `monkeypatch` for environment changes, and keep top-level `def test_*` functions only.

### Scenario 4 — TypeScript journeys mirror the same stories and types

**Behavior.** Every TypeScript audit row is resolved. Journey names, fixture stories, and outcomes match Rust and Python where the capability is shared; `it.each` handles value tables, `vi.stubEnv` restores environment, errors match one code, and package imports never reach private `index.cjs` paths (REQ-192, REQ-204, AC-057).

**RED.** Run the complete current audited TypeScript suites through their build- and environment-owning lanes: `mise run ts:test:integration` and `mise run ts:test:unit`. The audit supplies the expected rewrite/delete/move ledger for green tests that violate the standard.

**GREEN.** Apply each TypeScript verdict against TASK-016's public declarations and shared fixtures; retain runtime tests only for runtime behavior.

**REFACTOR.** Replace repeated rejection helpers and setup with the smallest typed support module, and use native Vitest matchers rather than wrapper assertions.

### Scenario 5 — Python unit and authoring tests are typed, compact, and non-duplicative

**Behavior.** Every Python unit audit row is resolved. Data/Model loading uses public `from_path`; typed splits, interface values, Agent/Card/runtime/config/signature/query values replace dict surgery and `repr`; invalid Cards are checked-in fixtures; duplicate, name-ban, test-double, and constant-against-itself tests are deleted (REQ-192, REQ-204, AC-057).

**RED.** Run `mise run py:test:unit`; compare the collected files and tests with `review/sdk-test-audit/python-unit.md`. The audit verdict is the RED for tests that currently pass while violating the approved standard.

**GREEN.** Process the ledger without dropping unique behavior. Parameterize value tables with one assertion shape and pin every refusal to one catalog code.

**REFACTOR.** Move shared model/data objects into scoped pytest fixtures, delete imports between test modules, and keep helpers only when two or more tests genuinely share setup.

### Scenario 6 — Type-only and internal-only checks live in their correct tiers

**Behavior.** Type-only assertions run through `expectTypeOf` or `ty`, never runtime collection. Every MOVE row has equivalent non-weaker proof in its named Rust owner; engine math, query execution internals, cache/fence counters, audit staging, binding/run lifecycle, and legacy-format retirement no longer appear in SDK journeys (REQ-192, REQ-204, AC-057).

**RED.** Use `rg -n 'toMatchTypeOf|expectTypeOf|typing|verification_runs|retire_fitted_format|table_describe_count|audit_staging|PSI|X-bar|SPC' sdks/wyrd-sdk-rust/tests sdks/wyrd-sdk-python/tests sdks/wyrd-sdk-ts/wyrd/tests` and the MOVE rows in all four audit ledgers to identify misplaced proof.

**GREEN.** Re-home each MOVE with the same observable assertion strength, or retain an already-existing owner test and delete only the duplicate SDK test. Add the final type fixtures to `mise run py:typecheck` and `mise run ts:typecheck` ownership.

**REFACTOR.** Prefer an existing owner test over adding a second one; do not create a new test crate, harness, or lane for a moved assertion that already has a home.

### Scenario 7 — Every test file is selected and the harness declarations match runtime

**Behavior.** Every remaining client-facing test is selected by a module or crate unit lane or a journey lane; an `#[ignore]`d Rust journey is not covered by a family lane that skips it. `sdks/wyrd-sdk-rust/tests/integration/workflow_loading.rs` (`workflow_loading::workflow_loading_journey`), which no lane selects today, gains a journey-lane home, as do `test_error_contract.py` and example smoke coverage; the Python `WyrdTestServer` stub exactly matches the three public runtime controls; deleted and moved files leave no stale lane entries (REQ-204, AC-057).

**RED.** Compare `rg --files sdks/wyrd-sdk-rust/tests sdks/wyrd-sdk-python/tests sdks/wyrd-sdk-ts/wyrd/tests` and `mise exec -- cargo nextest list --locked -p wyrd-sdk-rust -p wyrd-client --features wyrd-client/test-support --test integration --run-ignored only` with the module selectors and file lists in `mise.toml` and package-native collection; `workflow_loading::` is the known unselected Rust journey. Run `mise run codegen:check` to expose the current Python harness-stub drift.

**GREEN.** Update the smallest existing `mise` tasks/package configuration so every file has exactly one appropriate owner lane, and regenerate declarations from source.

**REFACTOR.** Delete stale per-file lane entries when a directory-level lane already selects the file; do not add a check that merely checks another check.

## Acceptance Criteria

- `TESTING.md` contains a compact checklist covering every REQ-192 bullet, and review records each bullet as satisfied (AC-048).
- Every row in all four language audit ledgers is resolved exactly once; DELETE behavior is absent, MOVE behavior has non-weaker owner proof, and REWRITE/TIGHTEN behavior remains covered (AC-057).
- Every remaining client-facing test is selected by a repository-native module or crate unit lane or journey lane, with type-only files outside runtime collection; every `#[ignore]`d Rust journey, including `workflow_loading::workflow_loading_journey`, runs in a journey lane.
- Shared cross-language stories use checked-in Card YAML and matching story/file/test names; fixed names are idempotent.
- Client journeys use no private imports, subprocesses, raw HTTP, raw server-table SQL, generated Card text/digests/URLs, sleep, polling loop, or non-sanctioned public test-server hook.
- Every error assertion names one exact catalog code.
- TASK-016's AC-045..058 public workflows have Rust, Python, and TypeScript journey coverage wherever the spec requires that language surface.

## Expected Write Set and Consumer Closure

- Standards and lanes: `TESTING.md`, `mise.toml`, and existing package test/type configuration only where selection changes.
- Shared inputs: repository-root `fixtures/cards/**` and only the additional checked-in fixture inputs required by the audit.
- Rust client-facing tests/support: `sdks/wyrd-sdk-rust/tests/**` and `crates/shared/wyrd-client/tests/**`.
- Python tests/support/type fixtures: `sdks/wyrd-sdk-python/tests/**`; generated runtime declarations are not hand-edited.
- TypeScript tests/support/type fixtures: `sdks/wyrd-sdk-ts/wyrd/tests/**` and existing TypeScript test/type configuration.
- MOVE destinations only in existing test owners under `crates/wyrd/wyrd-server/{src/**,tests/**}`, `crates/wyrd/wyrd-testing/tests/**`, `crates/vala/**/{src/**,tests/**}`, `crates/shared/wyrd-queue/src/**`, or the existing benchmark owner named by the audit. Production behavior outside `#[cfg(test)]` is excluded.

## Dependency, Merge Order, and Conflict Resolution

TASK-017 depends on TASK-016 and merges second. It writes tests against the exact revision-64 public API; it must not add a shim, keep an obsolete name, or alter production code when TASK-016 changes a surface. A conflict is resolved by keeping TASK-016's public contract and rewriting the test to it. The intended overlap is limited to `mise.toml`, generated-declaration drift observed by tests, and any inline `#[cfg(test)]` MOVE destination inside a TASK-016-touched Rust owner; preserve TASK-016 production code and combine only the independent test module/lane edits.

`SPEC-bifrost-variant` may also touch `architecture/bifrost-design.md`, `crates/wyrd-spec/src/vala/api.rs`, `crates/shared/wyrd-client/src/bifrost/**`, `crates/wyrd/wyrd-server/src/query/**`, `crates/wyrd/wyrd-testing/tests/bifrost/**`, Bifrost SDK journey files, and `mise.toml`. TASK-017 must not edit its production/internal Bifrost files. Likely test overlap is `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e.rs`, `crates/wyrd/wyrd-testing/tests/bifrost/**`, `sdks/wyrd-sdk-python/tests/{bifrost,integration/**}`, `sdks/wyrd-sdk-ts/wyrd/tests/{integration,unit}/**`, shared fixtures, and Bifrost lane membership. Resolve by retaining this task's public-story form and audit cleanup while taking the Bifrost branch's current expected schemas/results and internal owner tests; do not restore a deleted workaround or move Variant/Iceberg assertions back into SDK journeys.

## Verification and Evidence

- During each rewrite, run the narrowest exact file/target command from the scenario and record final test names plus exact selectors. Every specifically named final Rust test must have `mise exec -- cargo nextest run --locked -p <package> --lib|--test <target> -E 'test(=<exact-name>)'` evidence; Python and TypeScript named tests must record exact pytest/Vitest path-and-selector commands.
- The audit reconciliation report must enumerate all 736 input rows by verdict and destination/result, with totals matching the five authoritative audit files; this evidence may be generated during implementation but is not a new durable planning artifact.
- Because this task changes repository-wide test organization, fixtures, lane membership, and all SDK languages, final verification is `mise run gate` once, followed by `git diff --check` and review against every REQ-192 bullet. Do not separately rerun gate component lanes as final proof.

## Material Stop Conditions

- A required audit rewrite cannot be expressed through TASK-016's approved public API without adding or changing a production contract.
- A MOVE row has no existing owner tier and placing it would require a new crate, dependency direction, persistent contract, or Bifrost internal decision.
- Matching cross-language stories require different observable behavior rather than idiomatic syntax alone.
- The audit inventory is internally contradictory or a unique KEEP/MOVE behavior cannot be identified well enough to preserve.

## Authority Links

- [Approved spec revision 65](../spec.md): REQ-192, REQ-204, AC-048, AC-057.
- Authoritative inventory: `../review/sdk-test-audit/summary.md`, `rust.md`, `typescript.md`, `python-integration.md`, `python-unit.md`.
- Ergonomic reference: `/home/thorrester/Documents/GitHub/opsml/py-opsml/tests/agent/test_promptcard.py`, its `conftest.py`, and `/home/thorrester/Documents/GitHub/opsml/py-opsml/python/opsml/cli/__init__.py` (read-only).
- `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`.
- `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md`.

## Implementation Evidence

Commit range `04d1882dc~1..a97c14f87` (78 commits, 04d1882dc through a97c14f87 inclusive) on `wyrd/verified-change-contract/TASK-017` (language branches merged: Rust `c9a2ced55`, TypeScript `d58579412`, Python `eef1a11ae`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-048 checklist covers every REQ-192 bullet | `TESTING.md` "Client-facing tests" | review against REQ-192 | PASS |
| AC-057 all 736 audit rows resolved once | Rust 116 (KEEP 46/TIGHTEN 10/REWRITE 7/MOVE 26/DELETE 27); TS 63 (9/26/13/5/10); Py integration 96 (27/35/11/10/13); Py unit 461 (183/132/70/12/64) | reconciliation reports per language | PASS |
| Every test file in a lane; type-only outside runtime | `*.test-d.ts`, `sdks/wyrd-sdk-python/tests/typecheck/`; stale `test:bifrost:unit:python` removed (`d7524b19b`) | `mise run gate`; `mise run check:ci-selection` 7/7 | PASS |
| Shared stories use checked-in YAML, matching names | `fixtures/cards/<story>/`, `fixtures/README.md` | three-SDK journeys in gate | PASS |
| Exact catalog codes per error assertion | journey and unit rewrites | gate | PASS |
| AC-045..058 workflows covered in Rust/Python/TS | `sdks/wyrd-sdk-{rust,python,ts}` integration journeys | gate | PASS |
| Public surfaces only, no non-sanctioned hooks | Role-bearing keys use the credential fixtures sanctioned by spec revision 65 (REQ-195); the `observed-service` key now comes from `cli.issue_key` | `uv run python -m pytest -q -m "integration and not identity" tests/integration/test_observe_a_run.py` 16 passed; review | PASS (remaining gaps below) |

### Declared deviations (production code)

- `b620b069f` Oracle executes bind parameters (`with_param_values`); approved by the user; journeys require it. Test `oracle::tests::bound_values_replace_placeholders_as_data`.
- `ac327a227` `WYRD_VALA_403_BIFROST_CARD_SCOPE` maps to `BifrostError::CardScopeDenied` in `wyrd-client`.
- `ffaa142b4` audit settlement deletes staged rows before the progress upsert, and freeze skips during an in-flight settle; this fixes a lock-order stall that a gate surfaced. Test `pg_audit_staging::pg_tests::audit_staging::settlement_waiting_on_staged_rows_never_stalls_a_freeze` was red before the fix and green after.
- `assemble_stubs.py` docstring lint is the MOVE destination for Python stub-docstring tests; the `Judgment.kind` stub source was fixed.

- Spec revision 65 (user-approved 2026-10-07) sanctions the test server's credential fixtures for keys that need Roles; `TESTING.md` updated to match.

- User-directed follow-ups (2026-10-07):
  - Unknown `PromptReference` kind and every caller `model_validate_json` now refuse with `WYRD_SPEC_400_VALIDATION` (`870f401cf`).
  - Python registry refusals use `WYRD_REGISTRY_400_INVALID_CARD_SPEC` / `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` (`a24d9099a`).
  - Retired kind is `WYRD_LOADER_400_INVALID_ENVELOPE` through the CLI, `register_from_path`, and `Workflow.from_path` in all SDKs, via one `From<LoadError> for WyrdError` (`9d4f064be`).
  - Out-of-range query deadline is `WYRD_VALA_400_QUERY_INVALID_DEADLINE` from client pre-IO validation and the Oracle planner alike (`cb54cbb11`, `aa0aeb52b`); tests `vala::api::query_terminal_tests::query_request_refusals_project_onto_their_catalog_codes`, `oracle::tests::query_floor_refuses_an_out_of_range_deadline`.
  - Python partials: `ModelCard.artifact_hash` read from the Card (REQ-202, `09273a2ae`); `CardRef.to_dict()` so `Split.materialized(CardRef)` works (`77807c905`); typed `ModelCardMetadata.task_type`/`interface_kind`; `ProviderName::Custom` YAML round-trips so a saved mock Agent loads and runs (`e80775a36`); public `ProviderResponse.text(...)` (`c1ddf38cf`).
  - Credential fixtures documented in the `WyrdTestServer` stub (`57d571cc4`).

### Open gaps (not fixed here)

- Python `test_workflow_loading_journey` is still multi-story.
- Rust `workflow_loading` uses `CredentialWriter`.
- Rust `gateway_inference` uses `reqwest` because no Rust OpenAI SDK exists.
- Python `Split` has no typed readback (`kind`, `card_ref`): `CardRefPy` lives in `wyrd-cards`, which depends on `wyrd-interfaces` (Split's owner), so a getter needs a crate move. `test_split_materialized_allows_authored_ref_without_space` keeps a dict because `CardRef` requires `space`.
- Mock-provider Agent run uses the default registry mock; `Agent.from_yaml` takes no `mock_provider=`.
- The TypeScript "Bifrost not started" refusal is unit-only.

### Verification

- `mise run gate` exited 0 on `aa0aeb52b` (after the follow-ups; 2706 s, zero failures).
- `mise run -c gate` exited 0 on `a97c14f87`. That run used the gate composition from that commit, with the `verify:python-sdk`, `verify:rust-sdk` and `verify:typescript-sdk` tasks. The commits after `aa0aeb52b` are:
  - `abfc6f580`: the Python Bifrost journey lane runs the current story files.
  - `535979176`: test servers wait for a routable Oracle at start.
  - `3e9b80458`: the drift journey compares median client bytes.
  - `a97c14f87`: adds the three SDK verify tasks and runs them from gate.
- `git diff --check 04d1882dc~1..a97c14f87` reported nothing.
