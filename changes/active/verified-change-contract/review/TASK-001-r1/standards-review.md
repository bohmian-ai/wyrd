# TASK-001 r1 — Repository-standards review (repo-rev)

Subject: base `5293546f3` → candidate `2e09ae81213cb608253b75de276e3c946353ac35` (HEAD confirmed unchanged at start and end of review).
Scope: repository-rule compliance only (not task acceptance, not simplicity).

## 1. Authority coverage

| Changed surface | Applicable authorities |
|---|---|
| `crates/wyrd-spec` (card/verifier.rs, drift.rs, trigger.rs, operator.rs, mod.rs, envelope.rs, error.rs, graph/*, refs/mod.rs, reference.rs, examples/gen_schemas.rs, tests/fixtures) | AGENTS.md §2, §3, §4, §5, §12, §15, §16; agent-rules.md (imports, bare names, clippy-allow, generated artifacts, no plan/task refs, rustdoc, sync default); wyrd-design.md; references/doctrine/positioning-and-vocabulary.md; references/languages/rust-core.md; references/languages/errors.md |
| Generated schemas / openapi / `.pyi` / `error-codes.ts` / `docs/public/llms*.txt` | agent-rules.md (never hand-edit generated); AGENTS.md §8, §11 (`codegen:check`) |
| `crates/shared/wyrd-loader`, `crates/shared/wyrd-client` (state.rs, eval removal) | AGENTS.md §2 (client-tier deps), §3, §5, §6, §16; agent-rules.md; `check:client-tier` |
| `crates/wyrd/wyrd-server` (cards resolve/service, eval component removal, Cargo.toml, tests) | AGENTS.md §4, §5, §6, §9, §16; agent-rules.md (TenantConn, audit, imports, bare names); references/languages/errors.md |
| `crates/wyrd/wyrd-cli` (eval removal, tests, fixtures) | AGENTS.md §4, §9, §11, §12, §16; references/languages/errors.md |
| `crates/wyrd/wyrd-cards`, `crates/skald/skald-agent` | AGENTS.md §4, §16 |
| vala crates (vala-core deletion, vala-drift, vala-eval, vala-bifrost-redux tables, vala-sql queries/row types/tests) | AGENTS.md §3, §12 (retiring checks/tests), §16; agent-rules.md; references/domain/drift-monitoring.md, evaluation.md, olap-serving.md |
| SQL migrations (`wyrd-sql` 20260601000025, `vala-sql` 20260910000028) | agent-rules.md (tenancy/RLS); AGENTS.md §15 (wyrd-sql / vala-sql ownership); append-only migration convention |
| Python SDK (`src/state/mod.rs`, stubs, tests) | AGENTS.md §7, §8, §16 (top-level `def test_*`); references/languages/pyo3-boundaries.md, python-api-and-stubs.md; `check:pyo3-scope` |
| TS SDK `error-codes.ts` | references/languages/typescript-guide.md; generated artifact rule |
| wyrd-ui (types.ts, ServiceDefinition.svelte, mock, test) | AGENTS.md §2 (UI not source of truth); wyrd-ui skill (styling only) |
| Docs (`docs/src/**`, `docs/scripts/generate_api_docs.py`) | AGENTS.md §11 (`docs:check`), §12 (no legacy names); wyrd-design.md |
| `mise.toml`, `scripts/test-families.sh`, root `Cargo.toml`/`Cargo.lock` | AGENTS.md §11, §12 "Adding And Retiring Checks"; agent-rules.md (mise lanes) |
| Architecture authorities (`wyrd-design.md`, `bifrost-design.md`, `wyrd-doctrine.mdx`, references/*) | references/README.md authority hierarchy; AGENTS.md §14 |
| Commits `5293546f3..HEAD` | AGENTS.md §13 Git identity |

## 2. Rule-by-rule results

| Rule (source) | Result | Evidence |
|---|---|---|
| Git identity, no AI co-author trailers (AGENTS §13) | PASS | All 16 commits authored `Thorrester <sjforrester32@gmail.com>`; all commit bodies empty (`git log 5293546f3..HEAD --format='%an %ae%n%b'`). |
| Rustdoc on every new/materially modified item; `# Errors`/`# Panics` (AGENTS §16; agent-rules — hard blocker) | **FAIL** | New items are documented (verifier.rs types, `SlotValue` methods, `binding_validation_errors`, `check_activation`, `EffectiveSpecs`, `WyrdState::verifier`, new tests/helpers). Materially modified undocumented items remain — see SR-3. |
| Rustdoc accuracy of invariants (AGENTS §16) | FAIL (minor) | SR-7. |
| Struct-centered style (AGENTS §5; agent-rules) | **FAIL** | New wyrd-spec free fns are stateless validators (PASS). New server workflow is a dependency-threading free fn — SR-4. |
| Top-of-module `use` only; bare type names in signatures (agent-rules) | **FAIL** | SR-5. |
| WyrdError derive for public errors, no hand-written code/status (AGENTS §4) | PASS | 7 new catalog variants in `crates/wyrd-spec/src/error.rs` all use `#[wyrd_error(code, status, title, remediation)]`; only accessor match arms extended. |
| Stable error catalog stays truthful / no surfaces for removed features (AGENTS §9, §12) | FAIL | SR-6 (dead CLI error codes remediating deleted flags). Eval pull-protocol codes were correctly removed from catalog, docs generator, and `error-codes.ts`. |
| No `unwrap` on fallible IO in non-test code; `expect` only for invariants (AGENTS §4) | PASS | Added `unwrap`/`expect` occur only in `#[cfg(test)]` / `tests/`. |
| No `#[allow]` / `#[ignore]` added to pass a gate (AGENTS §12; agent-rules clippy-allow) | PASS | One `#[allow(clippy::large_enum_variant)]` at `crates/wyrd-spec/src/card/verifier.rs:43` preceded by a `// justification:` block (accepted by `scripts/check_clippy_allow.py` block walk); same-lint precedent at `crates/wyrd-spec/src/envelope.rs` `Card`. No `#[ignore]` added. |
| wyrd-spec IO-free, async-free, PyO3-free (AGENTS §2, §7) | PASS | No tokio/sqlx/fs/reqwest/pyo3/async in added wyrd-spec lines; `check:pyo3-scope` rc=0. |
| Client-tier deps (AGENTS §2) | PASS | Only dependency removals; `check:client-tier` rc=0. |
| Async only at IO (AGENTS §6) | PASS | `validate_effective_bindings`/`EffectiveSpecs::load` await registry reads; `check_activation`, loader validation, composition checks are sync. |
| TenantConn/RLS, no raw PgPool, callee does not commit (agent-rules) | PASS | New reads use `get_card_by_uid` over `&mut TenantConn<'_>`; no commit/rollback in callee. No new authorization decision → no audit obligation. |
| Migrations are new append-only files following owner patterns | PASS | `crates/wyrd/wyrd-sql/migrations/20260601000025_verifier_card_kind.sql` (drop/re-add `cards_kind_check`, last in sequence) and `crates/vala/vala-sql/migrations/20260910000028_drop_drift_alerts.sql` (last in sequence); shipped migrations untouched. `test:sql` rc=0. |
| Generated artifacts not hand-edited (agent-rules) | PASS | `gen_schemas.rs` registers the new/removed schema names; `codegen:check` rc=0 (schemas, openapi, `.pyi`, `error-codes.ts`). |
| Retiring checks/tests requires the property to be unreachable (AGENTS §12) | PASS with defect | `check:alert-router-schema-drift`, the vala-core codegen lines, and `vala-core` family entry: property unreachable (crate deleted). Deleted tests `pg_eval_v1_protocol.rs`, `eval_server_protocol.rs`, `pg_drift_alerts.rs`, vala-core tests: behavior deleted with them (route, CLI server/agent modules, `drift_alerts` queries+table, crate). But retirement is incomplete — SR-1. |
| Mise lanes keep working / verification scope (AGENTS §11) | **FAIL** | SR-1 (`test:bifrost` aggregate names a deleted test target). |
| No plan/task/agent references in code (agent-rules) | **FAIL** | SR-2. |
| No legacy names / compat aliases (AGENTS §2, §12) | FAIL (minor) | No `serde(alias)` or compat routes added; `kind: Drift`/`Eval` rejected by tests. Residual retired vocabulary — SR-8. |
| Test tier placement (agent-rules; TESTING) | PASS | New tests inline `#[cfg(test)]`; Postgres tests remain in `pg_*` targets; journeys updated (CLI `card_lifecycle`, Python `test_state_journey.py`). |
| Python tests top-level `def test_*` only (AGENTS §16) | PASS | No classes added in changed Python tests. |
| PyO3 rules (AGENTS §7) | PASS | `sdks/wyrd-sdk-python/src/state/mod.rs` only renames an accessor/kind mapping; no new pyclass, no stored `PyErr`. |
| Comments/docstrings not added to untouched code (AGENTS §16) | PASS | None observed. |

### Verification evidence (orchestrator, `scratchpad/verify/summary.txt`)

rc=0: `diffcheck`, `fmt` (+ clean `fmt_status`), four named nextest tests, `wyrdspec_lib`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `test:shared`, `test:cards:unit`, `test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, `test:sql`, `py:test:unit`, `py:typecheck`.
Pending at report time: `ts:typecheck`, `ts:test:unit`, `lints`, `test:wyrd`.
Not scheduled: `check:clippy-allow-audit`, `check:unwrap-audit`, `docs:check`, `test:bifrost` (would expose SR-1).

## 3. Material findings

### SR-1 — REGRESSION: `test:bifrost` lane references a deleted test target
- Rule: AGENTS.md §11 (`test:bifrost` covers every Bifrost tier) and §12 (a change is not done until targeted tasks pass; retirements must be complete).
- Location: `mise.toml:256` (`test:bifrost:integration:server:inner` runs `--test pg_eval_v1_protocol`); reached via `scripts/run-bifrost-tests.sh:11` from `test:bifrost` (`mise.toml:205`, in the aggregate at `mise.toml:1401`). Stale path also in `.github/scripts/detect-changes.sh:64`.
- Consequence: `crates/wyrd/wyrd-server/tests/pg_eval_v1_protocol.rs` and its `[[test]]` entry were deleted, so cargo fails with "no test target named `pg_eval_v1_protocol`". `test:bifrost:integration:server` fails, and `test:bifrost` and `gate` fail with it.
- Correction: remove `--test pg_eval_v1_protocol` from `mise.toml:256` and `pg_eval_v1_protocol|` from the regex at `detect-changes.sh:64`. Proof: `mise run test:bifrost:integration:server` exits 0, and `git grep pg_eval_v1_protocol -- mise.toml .github scripts` is empty.

### SR-2 — VIOLATION: code references the task plan
- Rule: agent-rules.md "Never mention references to plans, tasks, or other agents in the codebase."
- Location: `crates/wyrd-spec/src/graph/composition.rs:435-436`, rustdoc of test `publication_validation_rejects_duplicate_targets` at `:438`: "The name is retained from the retired publication check because the task verification list names it".
- Consequence: an ephemeral plan artifact is baked into permanent code, together with retired "publication" vocabulary.
- Correction: delete that sentence. The test name also carries retired vocabulary; renaming it (and `validate_rejects_duplicate_component_publication_targets` in `crates/shared/wyrd-loader/src/validate.rs`) needs the task's verification list updated in the same change. Proof: `git grep -n -i 'task verification\|the task' -- crates` returns nothing.

### SR-3 — VIOLATION (hard blocker per AGENTS §16): materially modified Rust items lack rustdoc / `# Errors`
- Rule: AGENTS.md §16 and agent-rules.md: every new or materially modified item, including private items, needs rustdoc, and every fallible fn needs `# Errors`.
- Locations:
  - `crates/wyrd-spec/src/card/drift.rs`: `DriftMethod::name` (:309) and `DriftSignal::variant_name` (:319) lost variants; `DriftValidationError::details` (:338) gained a `ConditionNotStatistical` arm; `validate_signal_method` (:440), `validate_signal` (:459), `validate_condition` (:490) and `validate_profile_presence` (:505) were rewritten. All are undocumented, and the fallible ones have no `# Errors`.
  - `crates/vala/vala-drift/src/baseline/mod.rs:143` `signal_variant`: arms removed, no rustdoc.
  - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:122`: new nested `fn indexed` inside `owned_bindings`, no rustdoc.
  - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:20-21`: `pub async fn resolve_card_references` now also fails with `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION` and `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`, but its doc is a single line with no `# Errors` and no statement about partial progress.
  - Lower severity: the trait-impl methods `utoipa::PartialSchema::schema` / `ToSchema::name` / `ToSchema::schemas` at `crates/wyrd-spec/src/card/verifier.rs:105,120,124` have no rustdoc. Pre-existing `impl utoipa::PartialSchema` sites (for example `envelope.rs` `Spec`) set the same pattern, but §16 treats existing code as drift, not precedent.
- Consequence: the change fails a merge-blocking documentation criterion.
- Correction: add rustdoc covering intent, workflow role and invariants to each listed item, plus `# Errors` on the fallible ones. `resolve_card_references` should list all three error classes: unresolved dependency, binding validation, and registry IO.

### SR-4 — VIOLATION: new dependency-threading workflow as a free function
- Rule: AGENTS.md §5, Required Struct-Centered Rust Style, and agent-rules.md: "Reject module-level orchestration that threads shared dependencies or context through free functions".
- Location: `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:66`. `async fn validate_effective_bindings(conn, submissions, resolved)` is a new multi-step, IO-backed workflow. It calls `EffectiveSpecs::load(conn, resolved)` (:227), which takes the same `conn` and `resolved` again on every call.
- Consequence: the new code extends the functional drift that §5 says must not serve as precedent, and the shared state is passed around rather than owned.
- Correction: let `EffectiveSpecs`, or a binding-validator struct, own `resolved` and the sibling index, and expose the workflow as an inherent method such as `validator.validate(conn, submissions)`. Keep `check_activation` as a stateless helper. Proof: no new free function in `resolve.rs` takes both `conn` and `resolved`.

### SR-5 — VIOLATION: function-scoped `use` and fully qualified paths in signatures
- Rules: agent-rules.md "All `use` statements live at the top of the module" and "Bring types in with `use` and use bare names in signatures".
- Locations:
  - Function-scoped `use`: `crates/wyrd-spec/src/card/verifier.rs:58`, inside `eval_spec_openapi_schema`, and `:82`, inside `implementation_branch`. Both are `use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};`, relocated from `envelope.rs` into this new file.
  - Fully qualified signature types:
    - `crates/wyrd-spec/src/card/verifier.rs:159`: `Result<(), crate::card::drift::DriftValidationError>`.
    - `verifier.rs:57,80,105`: `utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>`.
    - `crates/wyrd-spec/src/graph/composition.rs:140`: `&InlineableRef<crate::card::trigger::TriggerSpec>`.
    - `crates/wyrd-spec/src/refs/mod.rs:54,66,78,93,122`: `crate::reference::CardRef`.
    - `refs/mod.rs:109`: `Option<&std::path::Path>`.
    - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:329-330`: `&wyrd_spec::reference::InlineableRef<T>`, even though `InlineableRef` is already imported at `:9`.
    - `crates/wyrd/wyrd-server/src/components/cards/service.rs:1157-1158`: `&[wyrd_spec::card::verifier::VerificationBinding]`.
- Consequence: the module dependency manifest is incomplete, and signatures hide which crate owns each type.
- Correction: add `#[cfg(feature = "server")] use utoipa::openapi::{RefOr, schema::{ObjectBuilder, Schema, Type}};` and the other types to the top-of-module `use` blocks, then use bare names.

### SR-6 — DRIFT: stable CLI error catalog still advertises removed flags
- Rule: AGENTS.md §9 (no compatibility surfaces for old surfaces) and §12 (retire what is unreachable). references/languages/errors.md requires stable codes to be accurate cross-surface contracts.
- Location: `crates/wyrd/wyrd-cli/src/error.rs:43` `SimulatedUserScriptRequired`, `:53` `ScriptedTurnMissing`, `:78` `ServerRequiresAgentUrl`, `:88` `ServerRequiresToken`, `:98` `ServerRejectsRecords`, `:108` `RecordsRequireSubject`, `:214` `AgentTurnFailed`. After `eval/server.rs`, `eval/agent.rs` and `validate_args` were deleted, `git grep` finds only these definitions.
- Consequence: the public codes (for example `WYRD_CLI_400_SERVER_REQUIRES_AGENT_URL` and `WYRD_CLI_502_AGENT_TURN`) stay in generated error docs, and their remediation tells users to pass `--server`, `--agent-url`, `--simulated-user-script` and `--agent-timeout-secs`, which no longer exist.
- Correction: delete the unreachable variants, then regenerate. Proof: each name appears 0 times in `git grep`, and `mise run codegen:check` and `mise run docs:check` pass.

### SR-7 — DRIFT (minor): inaccurate invariant rustdoc
- Rule: AGENTS.md §16: rustdoc must describe the relevant invariants correctly.
- Location: `crates/wyrd-spec/src/card/trigger.rs:26-29` says the Schedule/Drift and ObservationsReady/Eval pairing is enforced by `crate::graph::composition`. `graph/composition.rs` has no activation check. The only enforcement is `check_activation` in `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:153-171`.
- Correction: point the doc at server registration binding validation, which raises `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`.

### SR-8 — DRIFT (minor): residual retired vocabulary on public surfaces
- Rule: AGENTS.md §2 kind list (Verifier replaces Drift/Eval) and §12 (no legacy names).
- Locations:
  - `docs/src/lib/components/CardTileGrid.svelte:22,24` still renders `Eval` and `Drift` kind tiles and has no `Verifier` tile, while `docs/src/content/docs/cards/index.svx` now lists Verifier and 15 kinds.
  - The `UnboundVerifierPeer` message at `crates/wyrd/wyrd-server/src/components/cards/service.rs:1342` still says "has no submitted publisher".
  - The details key `"publisher_kinds"` is still used at `service.rs:1350` and `crates/shared/wyrd-loader/src/order.rs:134`.
  - The UI mock still labels the relation `'publication'` (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/mock/cards/service.ts` relationships).
- Correction: replace the two tiles with one `Verifier` tile, and rename the publisher wording and details key to verifier-binding terms. Proof: `mise run docs:check`, plus the owning server and loader tests.

## 4. Overall verdict

**FAIL**. SR-1 breaks a repository test lane. SR-2 through SR-5 violate hard repository rules: the no-plan-references rule, merge-blocking rustdoc, struct-centered style, and imports/bare names. SR-6 through SR-8 are drift that should be fixed in the same remediation.
