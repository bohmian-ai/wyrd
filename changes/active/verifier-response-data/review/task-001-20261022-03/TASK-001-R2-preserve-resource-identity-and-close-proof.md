---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verifier-response-data
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-007, REQ-011, REQ-012, AC-003, AC-004, AC-005, AC-008]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-5, FIND-TASK-001-7, FIND-TASK-001-8]
---

# Preserve custom-schema identity and finish executor proof

The shared Verdict resolver still changes the meaning of some accepted schemas.
A draft-04 object requiring integer `reason` and `copy`, where `copy` refers to
`#/properties/reason`, becomes a schema accepting a string copy after embedding:
the reference reaches the Verdict envelope's string reason. Correct that
producer once, finish the remaining real-executor proof, and repair the Python
returned-check declaration and two Rust documentation omissions.

The existing schema compiler, validator, executors, fixtures and SDK journeys
remain the owners. This task changes no product behavior or approved limits.

## Subject and authority

- Approved [specification revision 1](../../spec.md).
- Original [TASK-001](../../tasks/TASK-001-verdict-formats-and-custom-data.md).
- Prior [attempt-02 verdict](../task-001-20261022-02/verdict.md),
  [validated ledger](../task-001-20261022-02/findings-validation.md), and
  [TASK-001-R1 with implementation evidence](../task-001-20261022-02/TASK-001-R1-preserve-schema-and-safe-data.md).
- This review's [validated ledger](findings-validation.md),
  [focused schema follow-up](followup-review.md), and [verdict](verdict.md).
- Cumulative base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`.
- Reviewed candidate: `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`.
- Repository: `/home/thorrester/Documents/GitHub/wyrd`.

Implement against the resulting candidate while preserving the existing user
workflow edits. Read root/nested rules, the owning Prompt/Verifier architecture,
TESTING.md and existing mise tasks. Prior findings 2, 3, 4 and 6 are closed;
retain their behavior and regression tests. Route this task to `$wyrd-implement`.
A later review audits the complete original-base-to-new-candidate range.

## FIND-TASK-001-1 — Fix the resource producer, not its consumers

**Diagnosis.** `Verdict::response_format` at
`crates/wyrd-spec/src/verification/execution.rs:603–625` inserts `$id` regardless
of the effective draft and assumes an authored ID creates an independent
resource. `check_output_schema` in Skald accepts draft-04, which uses `id`.
It also accepts meta-schema-valid IDs that resolve to the enclosing resource.
Installed-Rust-validator reproductions independently show both failures:

| Custom schema | Standalone good integer / bad string copy | Candidate embedding |
|---|---|---|
| Anonymous draft-04 | accept / refuse | refuse / accept |
| Draft-04 authored `id` or 2020-12 authored `$id` equal to empty, `#`, `.`, or `/` | accept / refuse | refuse / accept |
| Usable nonempty relative or absolute identity | accept / refuse | accept / refuse |

The data schema declares required integer `reason` and `copy`, with
`copy: {"$ref":"#/properties/reason"}`. The enclosing response has string
`reason: "judged"`. Use a failed outcome with empty citations to isolate schema
validation from Agent pass/citation policy. The installed reference library
also suppresses ID siblings of root `$ref` in classic drafts; keyword presence
alone therefore cannot establish an independent resource.

Prompt construction, inline Agent compilation, Python named authoring, and
`CodeCheck::response_schema` all consume this one resolver. Agent and hosted-code
execution then call the same `verdict_from`, which validates the wrong emitted
schema. Accepted bad data reaches ordinary requirement results and persistence;
valid data can instead become inconclusive. Prior fixes covered default/later
anonymous resources and definitions collisions, but did not cover these inputs.

**Selected correction.** Keep standard resource-preserving embedding at
`Verdict::response_format`. Use the effective supported draft (2020-12 when
absent) and the installed dependency's existing draft and normalized URI
resolution semantics through the existing Skald schema boundary. An absent
effective identifier receives the existing private resource identity with the
appropriate keyword. Preserve authored dialect and usable relative/absolute
identities. Establish an effective resource distinct from the envelope before
emitting the composed schema.

If an authored identity or classic root-reference composition cannot preserve
an independent scope through this mechanism, refuse that composition before
persistence through the existing field-specific `SkaldError::PromptInvalid`
path. Do not silently replace an authored identity, ban a supported draft or
relative IDs generally, or accept an ineffective identity. Use standard URI
resolution/comparison rather than a blacklist of example strings. This guard
belongs at the producer that promises reference-root preservation; downstream
guards cannot repair a judgment made against the wrong schema.

Reuse the installed jsonschema/referencing draft/URI facilities already exposed
through Skald. No new dependency, resolver, registry, schema walker, dialect
translator, definitions hoisting, or runtime validation copy is needed. Keep
Cards' authored form/hash and existing cross-dialect syntax refusals unchanged.

**Focused observations.** Extend the existing resource regression table with
anonymous draft-04 and usable authored draft-04 identities, good integer/bad
string copy responses, and usable relative/absolute identities across default
and later drafts. At declaration, prove ineffective authored IDs and classic
root-ref cases take the selected compiler refusal rather than emitting a
changed schema; preserve the authored form. Extend the existing HTTP refusal
case table to show an invalid Card is absent after refusal. Exercise the
corrected schema through the existing Agent and hosted-code focused proof:
good data survives ordinary readback; bad data is inconclusive and absent.
Keep the current default/draft-07/2020-12 collision and constraint regressions.

## FIND-TASK-001-5 — Close the remaining executor cases

**Diagnosis.** In `blocks_agent.rs:604–642`, the purported exact Agent object
has a 21-byte compact envelope and a string made by repeating two-byte `é`
with integer division. Its actual size is 65,535 bytes; the rejected secret-
prefixed case is 65,549, rather than the required adjacent 65,536/65,537.
Small escaped-string and depth cases do not supply exact escaped/wide-array
Agent proof. SDK exact markers now exercise hosted code, while their Agent
providers return small ordinary scores.

Separately, `scored-review.yaml:95–143` always declares custom data on its
hosted-code check. `EXTRA-DATA` adds a forbidden property inside that declared
object. It does not test envelope `data` under a plain/default code contract.
All three SDK negative tables reuse that fixture, so none supplies that case.
These are missing acceptance proof, not demonstrated production bounds or
storage bugs.

**Selected correction.** Extend the existing Agent executor table with actual
65,536/65,537 compact-byte values and explicit length self-checks, including
UTF-8/required escapes and wide-array variants. Assert exact accepted data and
safe terminal refusals with no rejected data; retain the existing 16/17,
malformed and citation cases. Extend the existing checked-in building-block
fixture with a plain/default code-check negative case returning otherwise valid
envelope data, and extend the three existing SDK negative-story tables to
observe inconclusive outcome, absent data, safe reason and valid siblings.
Retain the extra-property case because it tests a different schema error.

Use the existing provider, executor, fixture and repository Postgres/server
setup. No production guard, new harness, extra option, or repeated full SDK
suite is justified. Existing hosted accepted-boundary/published-readback proof
stays intact; rerun its focused stories only when their fixture changes.

## FIND-TASK-001-7 — Describe returned code formats in Python

**Diagnosis.** Python `verify/__init__.pyi:218–226::CodeCheck` ends at `inputs`.
Rust `CodeCheck.response_format` at `card/verifier/mod.rs:477` is serialized into
`RequirementResult.check` by the engine; Python returns it unchanged. Rust and
generated TypeScript expose it. A Python caller narrowing `check['kind']` to
`'code'` gets ty's `invalid-key` for `check['response_format']`. Existing typing
fixtures cover the old shape, so passing typechecks missed this omission.

**Selected correction.** Add the optional scalar/expanded named wire format to
the existing authored Python CodeCheck TypedDict, reusing existing schema/value
types. Returned data schemas are JSON, not Python model instances. Preserve
absence for historical/default checks. Extend the existing
`tests/typecheck/verify_contracts.py` with supported scalar/expanded declarations,
absence, narrowing and optional-format access. No native registration, runtime
conversion, new model, transport or separate stub tree is needed.

## FIND-TASK-001-8 — Finish the new-item rustdoc

**Diagnosis.** The new Rust SDK `exact_data` at
`verifier_building_blocks.rs:1455` intentionally asserts exact byte lengths but
has no `# Panics` section. The new local `PATH` constant at
`gateway/openai.rs:549` has no rustdoc explaining its shared placement/extraction
pointer role. Root rules explicitly make missing new-item rustdoc blocking.
These are distinct from the now-documented Judge/score of closed FIND-6.

**Selected correction.** Add concise panic and constant role/invariant docs
beside these items. Preserve the assertions and converter behavior. Static
inspection and applicable format/lint checks suffice; add no behavior test or
suppression for this documentation change.

## Execution sequence and preserved behavior

First add/select the schema-root regression at the existing owner and observe
the diagnosed wrong good/bad validation before fixing the producer. Then prove
the selected declaration refusal and both runtime consumers. Next correct the
existing Agent boundary values and add the missing plain-code negative case;
assert input byte sizes before relying on execution outcomes. Finally capture
the Python narrowed-access type failure, repair the declaration/type fixture,
and batch the rustdoc corrections. Record RED, minimum GREEN and relevant
regressions for executable/type behavior. Documentation uses static proof;
no historical RED or diagnostician report may be fabricated. If a new failure
requires changing a test/assertion, preserve the required independent diagnosis.

Keep all public operation names/signatures, fixed limits, error code and safe
root paths. Preserve plain/handwritten formats, compatible schema authoring,
CodeCall preparation actions, citation/Agent pass rules, aggregation, actual/
detail, status/dispatch, result identities, ordinary/typed reads, tenant/query
permissions and durability/visibility/retention. No new store, column, migration,
permission, compatibility alias, inferred format, decoder registry, mandatory
model dependency, compiler or polling loop. Fixes remain within revision 1;
stop for specification authority only if a material decision is actually needed.

## Acceptance mapping

| Finding | Required result |
|---|---|
| FIND-1 | Every emitted supported resource retains standalone local-reference meaning; draft-04 uses its effective identity; usable authored relative/absolute IDs survive. Proven ineffective-ID/root-ref compositions are refused before save without authored rewriting. Existing Agent and code proof accepts/retains good data and refuses/omits bad data. |
| FIND-5 | Actual 65,536/65,537 Agent UTF-8/escape/wide cases have explicit byte assertions and expected outcomes. Real plain/default hosted code returning undeclared envelope data is inconclusive/absent through the three focused SDK negative stories; valid siblings and existing extra-property coverage remain. |
| FIND-7 | Python's returned Check projection accepts optional scalar/expanded formats and absence; narrowed optional access passes existing type/stub checks. |
| FIND-8 | New helper panic and constant invariant rustdoc are present; assertions/behavior remain unchanged. |

## Focused verification

Use existing mise tasks/builds to prepare changed server/worker/SDK artifacts.
Run Cargo-backed commands sequentially. Existing exact owner selectors:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::tests::verdict_data_keeps_its_schema_resource)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::named_verdict_formats_validate)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_validates_custom_data)'
```

For focused server/SDK proof, reuse the original task's repository-managed
isolated Postgres lifecycle and migrations:

```bash
with_verification_postgres() {
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
    'mise run db:migrate:all:inner && exec "$@"' verification-tests "$@"
}
with_verification_postgres cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::verdict_declarations_validate_before_persistence)'
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::invalid_custom_verdict_data_is_inconclusive)'
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_invalid_custom_verdict_data_is_inconclusive
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^invalid_custom_verdict_data_is_inconclusive$'
```

When the shared fixture's accepted schema changes, also run its existing
accepted readback stories (otherwise the current recorded proof is sufficient):

```bash
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::custom_verdict_data_survives_readback)'
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_custom_verdict_data_survives_readback
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^custom_verdict_data_survives_readback$'
```

Run Python `py:typecheck` and `py:lints` for the authored stub/type fixture.
Finish Rust format and focused Clippy for changed owners/test targets; apply
Python/TS format/lint/type lanes to their changed test tables. If any generated
contract changes, run its existing generator and `codegen:check`. No schema
library upgrade, new dependency or broad generation change is required here.
Use only the narrowest affected checks; do not run `gate`, full SDK suites,
broad docs suites, crash/load tests or live-provider qualification for this
remediation. Record exact actual selectors, source/closure evidence per finding,
verification limits and the immutable resulting candidate.

## Implementation evidence

**Owner decision on FIND-1.** The owner chose refusal over preservation: a
Verdict `data` schema is JSON Schema 2020-12 only and carries no root `$id`.
This replaces the selected correction's "preserve authored dialect and usable
identities" with the refusal branch it already allowed, now applied to every
other draft and every authored root `$id`. No draft/URI resolution, resolver,
or walker was added; the 2020-12 recheck branch the earlier design needed is
deleted.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: emitted resource keeps standalone local-reference meaning; ineffective compositions refused before save | `Verdict::response_format` (`crates/wyrd-spec/src/verification/execution.rs`) refuses a `$schema` other than 2020-12 at `response_format.data.$schema` and a root `$id` at `response_format.data.$id`, then always embeds under the private `verdict-data` id | `vala-verify blocks::tests::verdict_data_keeps_its_schema_resource` (anonymous and declared-2020-12 `#/properties/reason` copy, `prefixItems`, `$ref` siblings, `definitions` collision); `wyrd-spec card::verifier::validate::tests::named_verdict_formats_validate` (draft-07, draft-04, absolute and `#` root `$id` refused at their field for code checks and Prompt decoding); `wyrd-server pg_verification_routes::verdict_declarations_validate_before_persistence` (`vr-older-draft-data`, `vr-authored-id-data` refused and absent); `blocks_agent::check_agent_validates_custom_data` rooted case | PASS |
| FIND-5: actual 65,536/65,537 Agent UTF-8/escape and wide cases with byte assertions | `crates/vala/vala-verify/tests/integration/blocks_agent.rs` builds `text(1_505/1_506)` and `wide(0/10)` and asserts their compact sizes before use | `vala-verify --test integration blocks_agent::check_agent_validates_custom_data` | PASS |
| FIND-5: plain/default hosted code returning undeclared envelope data is inconclusive/absent in three SDKs | `plainly-checked` requirement in `fixtures/cards/verifier_building_blocks/scored-review.yaml` (`ENVELOPE-DATA` marker); Rust, Python, TS negative stories assert inconclusive, no data, no `s3cr3t`, `well-formed` sibling data kept | `wyrd-ai verifier_building_blocks::{invalid_custom_verdict_data_is_inconclusive, custom_verdict_data_survives_readback, custom_verdict_data_is_typed_on_return}`; Python `test_verifier_building_blocks.py::` same three; TS `verifier-building-blocks.test.ts` same three | PASS |
| FIND-7: Python returned CodeCheck accepts scalar/expanded formats, absence, narrowed access | `CodeCheck.response_format: NotRequired[Literal["verdict"] \| _VerdictFormat]` in `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi`; `tests/typecheck/verify_contracts.py` adds both forms and `code_check_formats_are_optional` | RED: old stub gives `invalid-return-type` under `py:typecheck`; GREEN: `py:typecheck`, `py:lints` (incl. `check_stubs.py`) | PASS |
| FIND-8: panic and constant rustdoc | `# Panics` on `exact_data` (`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs`); rustdoc on `PATH` (`crates/wyrd-spec/src/gateway/openai.rs`) | static inspection; `fmt`; clippy | PASS |

Commands: `mise run fmt`; `cargo clippy --locked -p wyrd-spec -p vala-verify --all-targets`; `cargo clippy --locked -p wyrd-server --features test-support -p wyrd-ai --tests` (no warnings); `mise run py:format`, `py:lints`, `py:typecheck`; `mise run ts:format:check`, `ts:lints`, `ts:typecheck`; `mise run py:setup`, `ts:build`, `ts:build:testing` before the SDK stories; journeys under `with_verification_postgres`; `mise run docs:generate` after the `reference/verifier-card.md` sentence on 2020-12/no root `$id` (regenerates `llms-full.txt`); `git diff --check`.

Non-goals kept: no new dependency, resolver, store, column, permission, option, or production bound change; limits, error code, safe root paths, and the extra-property case are unchanged. Not run: `gate`, full SDK suites, `docs:check`, live-provider qualification. The pre-existing workflow edits stay uncommitted and untouched.
