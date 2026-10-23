---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verifier-response-data
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-007, REQ-011, REQ-012, AC-003, AC-004, AC-005, AC-006, AC-008]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6]
---

# Preserve declared schema semantics and safe custom-data errors

Implement this task through `$wyrd-implement`. It repairs the independently validated TASK-001 gaps under the existing approved behavior; it does not reopen the product design.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Approved specification: `changes/active/verifier-response-data/spec.md`, revision 1.
- Original task: `changes/active/verifier-response-data/tasks/TASK-001-verdict-formats-and-custom-data.md`.
- Original base: `4d0f7005888a899910008399d1b5e15e2a3207f9`.
- Reviewed cumulative candidate: `9e56fc340cb5fd769d3b34f47433e2719339fdfb`.
- Rebased base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`, the head of PR 205 (flat Card identity), which merges before this change. The reviewed commits were rebased onto it unchanged in content; the reviewed candidate is now `031a892be`, and `ebd078712` drops Card `space` from this change's fixtures and Card reads. A later task review audits `fb1fd9234...<remediated candidate>`.
- Verdict: `changes/active/verifier-response-data/review/task-001-20261022-02/verdict.md`.
- Validated diagnosis: `changes/active/verifier-response-data/review/task-001-20261022-02/findings-validation.md`.
- Attempt 01 was blocked by the candidate update, with no accepted FIND IDs. This is the first remediation task.

Read root/nested AGENTS, applicable topic guides, CodeGraph, testing/workflow guidance, and owning Prompt/Verifier, Bifrost Variant/query-permission and security authorities. Preserve all user edits and the committed ty 0.0.91 upgrade. No review authorizations permit committing or pushing.

## Outcome

Wyrd generates and validates JSON Schema in one dialect, 2020-12: `schemars` is upgraded to 1.x so generated schemas match the installed `jsonschema` validator. Named custom-data formats retain the meaning of the schema the author supplied. Compatible handwritten declarations use the existing schema-analysis owner. Every decoder refusal produces the normal Wyrd error without disclosing returned dictionary keys. The required hosted-code edge cases have real execution proof, and missing rustdoc/docs are closed in the same task.

## Diagnoses and selected corrections

### FIND-TASK-001-1: align schema generation with validation, then preserve the author's resource

`Verdict::response_format` in `crates/wyrd-spec/src/verification/execution.rs:598–620` removes `$schema`, hoists definitions and embeds the remaining schema at `/properties/data`. Two defects follow:

- **Dialect.** The envelope comes from `schemars` 0.8, which stamps draft 7 on every generated schema. The installed `jsonschema` 0.46 validator reads that stamp, so the author's data schema is validated as draft 7 once embedded. A 2020-12 `prefixItems` constraint, which Pydantic emits for every tuple field, is silently ignored: invalid data is accepted and retained. Standalone, the same schema is validated as 2020-12 (the validator's default), so the two paths disagree.
- **Reference root.** A local `#/properties/reason` that pointed at an integer custom field now resolves to the envelope's string reason. Valid `{reason:1,copy:2}` is refused while invalid `{reason:1,copy:"wrong"}` can be accepted.

Existing simple/model-definition cases exercise neither failure.

**Selected correction (owner decision).** Upgrade the workspace `schemars` dependency from 0.8 to the current 1.x release in this task, so generation and validation share the 2020-12 dialect repo-wide. This is not deferred to a separate change. Then repair the resolver on top of it:

1. Bump `schemars` in the root `Cargo.toml` (keep the `chrono` and `uuid1` equivalents under their 1.x feature names) and migrate every consumer to the 1.x API. The 0.8-only API (`RootSchema`, `SchemaObject`, `SingleOrVec`, `InstanceType`, `schemars::gen`, `visit::`) is used in `wyrd-spec` (12 files), `skald-spec`, `wyrd-loader`, `wyrd-semver` and `wyrd-types`. `#[schemars(...)]` attributes (59) and `schema_with` helpers such as `crate::schema::skipped_option` move to their 1.x forms. Do not keep 0.8 alongside 1.x or add a compatibility shim.
2. 1.x emits `$defs` and `#/$defs/...` instead of `definitions`. Update every reader that assumes `definitions` (27 references across crates/SDKs). Do not add a translation layer. Author-supplied schemas that use `definitions` stay valid: they are accepted as authored, as `skald-spec` authoring tests already require.
3. Regenerate every derived contract, generated type and docs mirror through the existing owners (`codegen:check`, docs generators). Generated output changes are expected and must come only from the generators. Re-check provider wire handling in Skald (OpenAI, Anthropic and Google structured-output compilation) against the new generated shapes. A provider-incompatible shape is fixed at the shared generator or compiler, never per provider call site.
4. In `Verdict::response_format`, embed the author's data schema so it keeps its own reference root and any dialect it declares. Use the standard 2020-12 embedded-resource mechanism the installed validator already supports. Do not use keyword translation, flattened definitions, new validators, per-executor guards or a new authored option. With no `$schema`, the author's data is validated as 2020-12, identical to standalone validation. Keep authored Card identity/hash unchanged. Genuinely unsupported provider/schema combinations fail through existing compiler errors before persistence, without weakening the emitted contract.

Both Prompt compilation (standalone/inline/referenced) and `CodeCheck::response_schema` consume this owner; both Agent and code execution validate through `verdict_from`. Fixing readers or repeating validation at each caller cannot repair an incorrectly produced judgment. Preserve these existing workflows and their dependency direction.

### FIND-TASK-001-2 and -4: normalize errors without payload-derived paths

`typedResult` in `sdks/wyrd-sdk-ts/wyrd/src/index.ts:281–292` casts an arbitrary thrown value and reads `.issues` before constructing WyrdError. A supported parser throwing null/undefined, or malformed optional diagnostics, escapes as TypeError. All three typed operations share this converter; existing Zod mismatch tests do not cover generic rejection.

The same diagnostic assumption leaks custom data. `verdict_from` (`crates/vala/vala-verify/src/blocks.rs:201–205`) copies raw instance paths into requirement reasons. For a valid dictionary schema with integer values, returned `{"customer-secret@example.test":"wrong"}` fails at a path containing the payload key. The engine keeps the reason and the result writer retains/publishes it even though `data` is dropped. Rust (`execution.rs:515–522`), Python (`sdks/wyrd-sdk-python/src/verify.rs:340–365`) and TS conversion copy arbitrary map-key paths into SDK errors too.

Correct the shared server rejection projection once, using only a schema-owned location or safe fixed root. Do not add scrubbing to HTTP, MCP, persistence or each executor. At generic SDK conversion boundaries, use the existing empty relative path to identify the safe data root. Arbitrary model/parse/Serde interfaces cannot prove whether a string segment is a static field or a returned dictionary key. Remove unsafe diagnostic extraction instead of introducing model introspection, schema-bearing decoder APIs, registries, path parsers or redaction machinery. Keep requirement identity and the exact `WYRD_SDK_422_VERIFICATION_DATA_INVALID` error. Do not copy the thrown value/message into public fields.

This single conservative TS projection also closes nullish/malformed diagnostic handling: every parser rejection reaches WyrdError. Existing assertions expecting a literal model field may change to the safe root for this diagnosed privacy correction; retain marker-free message/details, whole-conversion failure, ordinary reread and no-rerun assertions. Stored judgments, execution deadlines, identity, transport and polling remain with their existing owners.

### FIND-TASK-001-3: reuse bounded schema compatibility

`validate_agent` and `declares_data_object` at `crates/wyrd-spec/src/card/verifier/validate.rs:583–585,789–797` use a shallow root-property interpretation alongside the existing reference/composition-aware `schema_path::fits`. Wrapping an optional-data handwritten Verdict in supported allOf or a root-local reference bypasses the required-data rule. Conversely, required object data behind a local reference is falsely refused because its literal property lacks `type:object`. Current declaration cases only use direct shapes.

Delete the shallow parallel check. Use the existing bounded compatibility owner to require a handwritten schema to fit one of the two approved complete closed shapes: plain Verdict without data, or Verdict with required non-null object data. Derive these target shapes from the canonical Verdict owner; do not copy another definition or create a second walker. Preserve traversal budgets, plain/compatible handwritten forms, output-reference `admits`, code actions, runtime validation and citation behavior. Extend the existing HTTP declaration test to prove an incompatible composed Card is absent after refusal.

### FIND-TASK-001-5: complete hosted-code execution proof

`fixtures/cards/verifier_building_blocks/scored-review.yaml:96–111` only produces wrong-type BAD-DATA and coarse 70,000-character HUGE-DATA failures. The Rust/Python/TS invalid-output stories use those cases. They do not execute missing/null/undeclared data, unknown envelope fields, exact 65,536/65,537 compact-byte values or 16/17 containers through hosted code. Pure limit tests and Agent-only cases do not prove that code's return/readback path preserves the required boundaries. This finding claims missing proof, not an established production truncation defect.

Extend the existing checked-in fixture and focused SDK case tables with those cases, including specified UTF-8, required JSON escapes and wide-array variants. Complete corresponding Agent cases where needed. Reuse the existing deployment/local provider/setup. Exact boundaries must retain the data through ordinary readback; malformed/one-over values must be inconclusive with absent data and safe reasons, while valid siblings survive. Do not generate Cards in SDK test bodies, add another harness, weaken bounds or add options. If proof reveals a real failure, diagnose and fix the existing producer owner within the approved contract; stop only for a genuinely new material decision.

### FIND-TASK-001-6: document the local Rust model

The new local `Judge` type and `score` field in `crates/wyrd-spec/src/verification/execution.rs:840–841` lack rustdoc. Root rules explicitly include private/test items. Add concise intent/field documentation and preserve the existing test. Static inspection/format proof suffices; add no behavior test or style-only task.

### Batch bounded documentation corrections

Use existing journey-backed examples and language-tab conventions to explain code/Agent format placement, per-requirement data and existing typed calls in Rust, Python and TypeScript; the current recipe only shows Python. Explain the conservative safe-root decoding location. Regenerate the existing docs mirror through its owner.

Clarify the spec's inherited payload-protection wording against the narrower Bifrost authority: verification outcomes inherit table query-read and tenant scoping; only gateway call payload columns need the additional gateway payload permission. This is an editorial clarification of existing inheritance, without changing approved behavior/revision or introducing a new result permission. Missing historical RED for scenarios 1–3 stays disclosed; do not manufacture past failures.

## Preserved constraints and non-goals

- Keep all approved public names, operation signatures, envelope/result placement, fixed limits, error code and authoring forms.
- Preserve citation/pass rules, aggregation, actual/detail meanings, identity, dispatch/status, durability/visibility/retention and tenant/permission controls.
- Reuse compiler, bounded schema analysis, shared execution validation, result projection and ordinary SDK workflows. No new transport, polling loop, store, migration, Cargo feature/dependency, config, decoder registry or mandatory model library. The `schemars` 0.8 → 1.x version upgrade is the one selected dependency change; it adds no new crate or feature.
- No broad unrelated refactor, new permission, compatibility alias, option to weaken validation, changed preparation-action contract, partial typed result or verification rerun.
- Preserve the ty/auth update and other user edits. Do not restore lockfiles or sync environments speculatively.

## Acceptance mapped to findings

| Finding | Required observation |
| --- | --- |
| FIND-TASK-001-1 | The workspace builds against `schemars` 1.x only (`cargo tree -i schemars` shows a single 1.x version); generated envelope and contracts declare 2020-12; regenerated contracts are clean under `codegen:check`. Original-root refs (including envelope-name collision and definitions) and effective supported dialect constraints agree before/after composition; good data retained, bad data inconclusive/absent in code and Agent paths; unsupported combinations refused without persistence/weakening. |
| FIND-TASK-001-2 | Generic parsers rejecting via null, undefined, Error or malformed optional diagnostics always yield normal exact-code WyrdError with requirement and safe root; raw read/call count unchanged. |
| FIND-TASK-001-3 | Supported composed/ref handwritten plain and required-object forms accepted; optional/non-object data forms refused before save; existing reference traversal limits retained. |
| FIND-TASK-001-4 | Secret markers in nested dynamic keys appear in neither serialized server reasons nor SDK error message/details; invalid server data absent, accepted stored data unchanged after typed mismatch. |
| FIND-TASK-001-5 | Real hosted code and required Agent cases cover malformed output and exact/one-over compact byte/container limits, including Unicode/escapes/wide arrays; all three SDK focused story tables demonstrate expected result behavior. |
| FIND-TASK-001-6 | Local Judge type/field have substantive rustdoc; existing assertions retained. |

## Focused execution and closure proof

Follow RED -> GREEN -> REFACTOR for the changed behaviors, one existing-owner scenario at a time. Add/select the regression at its owner and observe the diagnosed failure before its correction. Pure schema/privacy tests support rather than replace the focused code/Agent execution proof. Record actual exact selectors and results; no full journey suite or broad aggregate is required for this remediation.

Reuse these existing exact selectors, extending their cases where appropriate:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::named_verdict_formats_validate)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::custom_verdict_data_obeys_limits)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::typed_data_decodes_whole_results)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_validates_custom_data)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::named_verdict_formats_preserve_authored_forms)'
```

The `schemars` upgrade changes every generated schema, so it also needs the unit lanes of each crate that generates or reads them. Run these once, after the migration compiles:

```bash
mise exec -- cargo tree --locked -i schemars
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-skald-spec -p wyrd-loader -p wyrd-semver -p wyrd-types -p wyrd-version -p wyrd-client --lib
mise run test:skald
mise run codegen:check
```

Use the original task's repository-managed isolated Postgres wrapper, migrations and existing worker/SDK builds for the focused real-server selectors. Cargo commands remain sequential. The wrapper definition is:

```bash
with_verification_postgres() {
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
    'mise run db:migrate:all:inner && exec "$@"' verification-tests "$@"
}
```

```bash
with_verification_postgres cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::verdict_declarations_validate_before_persistence)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::invalid_custom_verdict_data_is_inconclusive)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::custom_verdict_data_survives_readback)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::typed_verdict_data_mismatch_keeps_original_result)'
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_invalid_custom_verdict_data_is_inconclusive
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_custom_verdict_data_survives_readback
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_typed_verdict_data_mismatch_keeps_original_result
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^invalid_custom_verdict_data_is_inconclusive$'
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^custom_verdict_data_survives_readback$'
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^typed_verdict_data_mismatch_keeps_original_result$'
```

If a focused new owner test is needed, record its exact selector instead of inventing a nonexistent name. Do not add repeated HTTP/MCP read guards or rerun full surface journeys merely because errors ultimately travel over those surfaces; their source correction is the shared projection. Rebuild affected SDK/server binaries through existing owning tasks before relying on the updated runtime tests.

After the focused cases pass, run required formatting and only checks covering the actual write set: Rust format, workspace Clippy (`mise run lints`, because the `schemars` migration touches every schema-deriving crate), the existing Python binding Clippy lane if touched, Python/TS format/lint/type lanes, codegen drift verification for changed contract/schema generation, and docs command/link/mirror checks for the documentation edits. Existing task owners are in mise.toml; use them rather than adding scripts. Appropriate narrowed Rust checks include:

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-spec -p vala-verify --all-features --all-targets -- -D warnings
mise run lint:wyrd-sdk-python
mise exec -- cargo clippy --locked -p wyrd-server --features test-support --test integration -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-ai --test integration -- -D warnings
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run codegen:check
mise run docs:check:commands
mise run docs:linkcheck
mise exec -- git diff --check
```

Run `mise run check:deps` after the dependency change. Do not run `gate`, full SDK journey suites, broad docs/example journeys or crash/load exercises for this task. Those integrated checks belong at change review. Confirm generated consumers/mirror via their existing generators, not hand edits. Finish with per-finding source/proof evidence and a cumulative diff audit; hand the immutable new candidate back to `$wyrd-task-review` with this original base, original task, prior verdict/ledger and remediation task.

## Implementation evidence

Candidate: `fb1fd9234...3db82f534` (commits `d76028fa1`, `d39ddb9d3`, `39b642c4b`, `999d05d03`, `37f9cfd5c`, `c0fb06b2e`, `3db82f534` on top of the reviewed `031a892be`/`ebd078712`/`4b723032f`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: one 2020-12 dialect | `Cargo.toml` `schemars = 1.2` (`chrono04`, `uuid1`); every 0.8 API consumer migrated to `Schema`/`json_schema!`; `$defs`/`const` readers in `gen_schemas.rs`, `scripts/gen_ts_types.py`, `docs/src/lib/diagrams/diagrams.test.ts`, `gateway/openai.rs` OpenAPI walk, `mcp/principals.rs`; `card-types.ts` regenerated | `cargo tree --target all -e normal,build,dev` shows only `schemars v1.2.1` (the lockfile's `0.9.0` is `serde_with`'s unbuilt optional feature, already on the base); `codegen:check`, `docs:test`, `test:skald` (334), unit lanes for wyrd-spec, skald-spec, loader, semver, types, version, client, vala-verify (1328); `pg_openapi_contract` 23/23; MCP journey 27/27 | PASS |
| FIND-1: author's resource kept | `Verdict::response_format` (`verification/execution.rs`) embeds `data` with its own `$id` (`verdict-data` unless authored) and keeps its `$schema`; no hoisting, so definitions named like the Verdict's no longer collide; a declared draft whose syntax 2020-12 refuses is refused at `response_format.data...` through `check_output_schema` | `vala-verify blocks::tests::verdict_data_keeps_its_schema_resource` (RED: `/data/copy` resolved to the envelope reason; GREEN: `#/properties/reason`, `prefixItems`, draft-07 `$ref` siblings, `$defs.Citation`); `blocks_agent::check_agent_validates_custom_data` (Agent path); `validate::tests::named_verdict_formats_validate` (draft-07 tuple `items` refused) | PASS |
| FIND-2/4: safe errors | `verdict_from` (`vala-verify/src/blocks.rs`) names only `schema_path`; Rust `RequirementResult::decode_data`, Python `decode_data` (`sdks/wyrd-sdk-python/src/verify.rs`), TS `typedResult` report `path: ""` for every rejection and never copy the thrown value | `blocks::tests::verdict_refusal_omits_returned_keys` (RED: reason held `/data/customer-secret@example.test`); `execution::tests::typed_data_decodes_whole_results` (nested dynamic key); TS `tests/unit/observe.test.ts` null/undefined/Error/malformed/keyed issues (RED: 3 failed; GREEN 77/77); three `typed_verdict_data_mismatch_keeps_original_result` journeys | PASS |
| FIND-3: bounded compatibility | `declares_data_object` deleted; `validate_agent` requires a handwritten schema to `fits` one of `verdict_shapes()`, both generated by `Verdict::response_format`; a named format is generated, so trusted | `named_verdict_formats_validate` (RED: required object data behind `$ref` refused; GREEN: `$ref`/`allOf`/root-`$ref` forms accepted, optional/nullable forms refused); `pg_verification_routes::verdict_declarations_validate_before_persistence` (`vr-composed-optional-data` refused and absent) | PASS |
| FIND-5: hosted-code proof | `fixtures/cards/verifier_building_blocks/scored-review.yaml` code check markers: `MISSING-DATA`, `NULL-DATA`, `EXTRA-DATA`, `EXTRA-FIELD`, `EXACT-`/`OVER-BYTES` (UTF-8, `\n`, `\"`, `\u0001`), `EXACT-`/`OVER-WIDE`, `DEEP-16`/`DEEP-17` | Rust `invalid_custom_verdict_data_is_inconclusive`, `custom_verdict_data_survives_readback` (direct, wait, status, retained Bifrost), `typed_verdict_data_mismatch_keeps_original_result`: 3/3; Python same three: 3 passed; TypeScript same three: 3 passed | PASS |
| FIND-6: rustdoc | local `Judge` and `score` in `execution::tests::typed_data_decodes_whole_results` | `mise run lints` | PASS |
| Docs | `verifier-recipes.md` typed read in Python/Rust/TypeScript `LangTabs`, per-requirement data, safe-root `path`; `architecture/wyrd-design.md` and Python stubs state the empty path; spec payload-permission wording clarified (revision unchanged); `llms-full.txt` regenerated by `docs:generate` | `docs:check:commands`, `docs:linkcheck`, `docs:test` (102), `docs:build` | PASS |

Format and lint: `mise run fmt`, `mise run lints`, `lint:wyrd-sdk-python`, clippy for `wyrd-spec`/`vala-verify` (all features, all targets), `wyrd-server --test integration`, `wyrd-ai --test integration`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `check:deps`, `git diff --check`: all pass.

Non-goals held: no new crate, feature, option, store, migration, permission, alias, or validator; Card identity/hash unchanged; user edits to `.agents/skills/wyrd-plan/SKILL.md`, `.agents/skills/wyrd-spec/SKILL.md`, and `changes/spec-driven-development.md` are uncommitted and untouched.

### Diagnoses

- **Verifier extract format tag.** Symptom: `card::verifier::extract` tests refused every document format after the upgrade. Evidence: generated single-value enum tags are `{"const": "pptx"}` in 1.x. Cause: `extract.rs` read `/properties/format/enum/0`, a production read. Fix site: the pointer in `extract.rs`, the only reader of that tag (diagnostician-confirmed with the test-shape updates below).
- **Generated shape assertions.** Symptom: 13 unit assertions failed on `definitions`, `enum`, `oneOf`, and unwrapped doc text. Evidence: 1.x emits `$defs`, `const`, `anyOf` beside a skipped untagged variant, and keeps doc line breaks. Cause: the assertions pinned 0.8 output; their intent (the named shape exists and is closed) holds. Fix site: each assertion, reading the 1.x keyword (diagnostician-confirmed).
- **OpenAPI transparent root.** Symptom: `pg_openapi_contract::gateway_ingress_publishes_typed_contracts` lost a component body. Evidence: 1.x emits a transparent newtype root as a `$ref` to its named definition. Cause: `component()` expected the body inline. Fix site: `component()` follows a root `$ref` into the generated components, the one place components are projected.
- **MCP required order.** Symptom: `mcp::principals` journey compared `required` lists unequal. Evidence: 0.8 sorted `required` through a `BTreeSet`; 1.x keeps declaration order. Cause: the helper compared an unordered JSON Schema set as a list. Fix site: the test helper sorts, since `required` has no order.
- **Base lint.** Symptom: `lint:wyrd-sdk-python` (no `testing` feature) reported `from_registry_card` unused. Evidence: its only caller is `testing_cli`, compiled under `testing`; introduced by the base's `fd5463d78`. Fix site: `#[cfg(feature = "testing")]` on the helper.

### Material limits

- A live OpenAI strict structured-output call with an embedded `$id` was not exercised; local providers and the jsonschema validator accept it. A provider refusal would surface through the existing compile/route errors.
- The recipe's typed-read snippets follow the page's existing inline-snippet convention; their calls mirror the three SDK journey stories rather than being embedded from an example program.
- Historical RED for TASK-001 scenarios 1-3 remains as disclosed by the original task.
