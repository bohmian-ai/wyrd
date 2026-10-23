# Independent findings validation — TASK-001

Recommendation: **FIX_REQUIRED**. Four deduplicated findings remain: two
continuations of prior findings and two newly identified omissions. The
corrections fit approved revision 1; no new product, public-call, security,
resource-ownership, or persistence decision is required.

## Subject, authority, and coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`.
- Candidate: `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`.
- Subject: complete cumulative three-dot range, not just `4b723032f..d2ffc5987`.
- Authority: approved `changes/active/verifier-response-data/spec.md` revision
  1; original `tasks/TASK-001-verdict-formats-and-custom-data.md`; root and
  applicable docs/Python/TypeScript `AGENTS.md`; architecture map and owning
  Client model, Prompt, Agent, Verifier/results, Bifrost Variant/result, and
  security tenant/privacy sections; agent_docs index, Rust, maintainer, errors,
  agent surfaces; TESTING.md. Workflow authority was read from the candidate,
  excluding the three user working-tree workflow edits recorded in subject.md.
- Inputs read: subject, attempt-02 verdict/validated ledger/R1 and recorded
  evidence; every attempt-03 discovery report (behavior, invariants, standards,
  maintainer, system, security, persistent data), claim-comparison,
  verification, and followup-review. No intended verdict was supplied.

CodeGraph preceded code navigation. Independently inspected cumulative changes
and the materially changed production owners, declarations, generators, tests,
fixtures, and downstream readers. Correction-owner bodies and relevant callers
were read directly, rather than treating report agreement as proof. The shared
schema compatibility owner was expanded beyond the supplied navigation map.
The reported schema and Python typing failures were independently reproduced
below; Agent byte arithmetic was independently recomputed. No Cargo operation,
source edit, formatter, generator, environment repair, commit, or broad check
was performed. Only this review report is a repository write.

Coverage is complete for this validation: all seven required discovery reports
and the focused follow-up exist, all proposals have a disposition, prior IDs
1–6 were reassessed, and no unresolved authority/caller conflict remains.
Scoped discovery PASS statements are reconciled with the narrower cases their
source and tests actually cover. They do not establish closure for an untested
supported dialect or identifier.

## Producer and consumer map

1. YAML/JSON/schema-model authoring produces `NamedResponseFormat`. The sole
   Wyrd resolver is `Verdict::response_format` in
   `crates/wyrd-spec/src/verification/execution.rs`. PromptSpec construction
   and decoding call `card/prompt/spec.rs::compile_prompt`; inline Agent
   decoding in `card/agent.rs::inline_prompt` reuses it. Skald's existing
   `AuthoredPrompt::compile_with/compile_resolved`, portable lowering, and
   `named_output` emit provider settings and retain the authored form. The
   Python builder's `named_prompt` also calls this Wyrd owner.
2. Code declaration validation and `CodeCheck::response_schema` use the same
   resolver. Preparation actions retain `CodeCall`; reference visitors and
   input resolution were adapted to the check wrapper without adding formats
   to actions. Handwritten Agent schemas use existing bounded
   `schema_path::fits` against canonical plain/required-object shapes;
   `admits` remains the sibling output-reference mechanism.
3. Agent and hosted-code check executors are the only production callers of
   `vala-verify/src/blocks.rs::verdict_from`. They supply the full response and
   actual compiled declaration. The validator precedes Verdict decoding and
   bounds. Engine `judge` converts terminal refusal to
   `Judged::inconclusive/bare` (`data: None`); accepted output passes through
   `checked/cited/report`. Citation policy and aggregation retain their owners.
4. Direct and queued results retain the complete outcome through existing
   `ResultPayloadBuilder::build/summary` and Scribe staging. Result/status
   readers use the existing tenant-authorized selected result. There is no
   new store, column, lower-privilege projection, retry loop, or result writer.
5. Rust's typed operations call ordinary verify/status/wait/deadline-wait then
   the shared `decode_data` chain. Python's three typed operations use one
   `decode_data` over a fresh projection; TypeScript uses one `typedResult`.
   Every present data value, including failed requirements, is converted;
   missing data is skipped. Conversion cannot write or rerun verification.
   The returned `check` is the authored declaration, including code format.
6. The schemars migration reaches manual schemas, `$defs`/`const` consumers,
   the TS lowering generator/generated contracts, MCP schema publication,
   OpenAPI transparent-root projection, extraction format tags, and diagram
   tests. Existing generation and response owners were extended; there is no
   parallel compiler or schema engine. SDK/server/MCP tests and checked-in
   fixtures carry the public proof; pure cases complement them.

## Proposal dispositions

| Source proposal | Disposition | Validated result |
| --- | --- | --- |
| BEH-1 | **REVISED** | Correct defect; broaden the same resource-identity cause to authored ineffective IDs and legacy root-reference suppression. Continue FIND-TASK-001-1. A draft-key-only repair is insufficient. |
| followup-review resource findings/recommendation | **REVISED** | Its independent-resource diagnosis is confirmed. Use existing draft/URI semantics at the producer; preserve usable relative/absolute identities and refuse only compositions the producer cannot preserve. Do not create a general identity policy or string blacklist. Same FIND-1. |
| BEH-2 | **CONFIRMED** | Agent nominal exact/over objects are 65,535/65,549 bytes and no Agent exact wide-array case exists. Verification gap, FIND-TASK-001-5. |
| DATA-1 | **CONFIRMED** | Same Agent arithmetic/coverage cause as BEH-2, not a persistent-storage defect. FIND-5. |
| INV-1 | **CONFIRMED** | The hosted-code EXTRA-DATA case adds a property inside declared data; no plain-code undeclared-envelope-data case exists. Same required proof inventory, FIND-5. |
| MNT-001 | **CONFIRMED** | Python returned CodeCheck TypedDict omits implemented response_format. New FIND-TASK-001-7. |
| REP-1 | **CONFIRMED** | New exact_data helper lacks panic docs; new local PATH constant lacks rustdoc. New FIND-TASK-001-8, distinct from prior Judge/score omission. |

System and security reports have explicitly empty material ledgers. Their
availability/isolation conclusions remain supported within the changed paths;
their broad schema-closure statements are narrowed by FIND-1. Invariant
review's overall Agent-boundary PASS is superseded by the independently
verified arithmetic, without inventing a production limit defect.

## Validated deduplicated ledger

### FIND-TASK-001-1 — Embedded schemas can still inherit the Verdict resource

**Status:** REVISED, still open. **Classification:** INCORRECT.
**Sources:** BEH-1 and focused follow-up; continuation of prior FIND-1.
**Obligation:** REQ-001/002/007 and AC-003/004: supported declarations keep
their exact meaning; unsupported composition is refused before persistence,
never silently weakened.

**Location:** `crates/wyrd-spec/src/verification/execution.rs:603–625`,
especially unconditional `$id` insertion at 618–619. Existing dialect and
syntax boundary: `crates/skald/skald-spec/src/prompt.rs:246–272`.

`check_output_schema` accepts built-in drafts 4/6/7/2019-09/2020-12 and defaults
to 2020-12. It meta-validates, deliberately without resolving user refs. The
resolver preserves `$schema` and inserts `$id: verdict-data` only when `$id`
is absent. Draft-04 uses `id`, so its inserted `$id` is ineffective. An
authored later-draft ID may itself identify the enclosing resource. Presence
of an ID keyword therefore does not prove the resolver's promised root.

An accepted object schema requires integer `reason` and `copy`, with
`copy: {"$ref":"#/properties/reason"}`. Standalone, both are integers. In
the faulty composition the pointer instead names the Verdict's string reason.
Failed output with empty citations avoids any pass/citation interference:
integer copy becomes inconclusive, while string copy becomes a genuine
accepted judgment and ordinary retained data.

I independently wrote `/tmp/ponytail-identity-probe.rs`, using a minimal
resource context with the same string envelope reason, the exact candidate
insertion operation, and the repository-built jsonschema 0.46.4/serde_json
rlibs. Each authored schema passed installed meta-validation and independently
accepted integer/refused string data standalone. Actual installed-validator
results were:

| Effective dialect / identity | Candidate integer / string | After only correcting ID keyword |
| --- | --- | --- |
| Draft-04, anonymous | refuse / accept | accept / refuse |
| Draft-04, authored `id` empty, `#`, `.`, or `/` | refuse / accept | still refuse / accept |
| 2020-12, authored `$id` empty, `#`, `.`, or `/` | refuse / accept | still refuse / accept |
| Either dialect, usable `authored-data` or `https://example.org/data` | accept / refuse | unchanged |
| Anonymous default/2020-12 | accept / refuse | unchanged |

This fresh probe does not depend on the earlier reviewers' emitted schema
file. Source comparison establishes the canonical resolver applies the same
operation. The full check executor and projection bodies establish reachability
through both runtime consumers, whole/step results, and direct/queued reads.
No SDK or persistence correction could recover the original meaning afterward.

Installed `referencing-0.46.4/src/draft.rs:86–93,113–115` selects draft-04
`id`, later `$id`; `spec/ids.rs:11–38` ignores an ID beside a classic-draft
root `$ref` and treats fragment-only legacy IDs differently from a resource
base. `jsonschema/src/compiler.rs:29,853` supplies the anonymous root base;
`referencing/src/uri.rs::from_str/resolve_against` provides standard normalized
URI resolution. These are already re-exported by the installed `jsonschema`
dependency used in Skald; no added URI package is necessary. Official
[draft-04 Core §7.2](https://json-schema.org/draft-04/json-schema-core#section-7.2)
and [2020-12 Core §8.2.1](https://json-schema.org/draft/2020-12/json-schema-core#section-8.2.1)
support the dialect-specific identity and parent-base resolution rules.

**Smallest selected correction:** retain standard embedded resources at the
shared `Verdict::response_format` producer. Reuse Skald's existing schema
boundary and installed draft/URI interpretation to establish an effective,
distinct resource before emitting the envelope. An absent effective identifier
receives the existing private identity using the effective draft's keyword;
preserve authored dialect and usable relative/absolute IDs. Do not silently
overwrite an authored identity. For an authored identity or legacy root-ref
composition whose independent scope cannot be preserved by this mechanism,
return the existing field-specific `SkaldError::PromptInvalid` before save.
Account for root `$ref` suppressing classic ID siblings; checking keyword
presence alone cannot establish effectiveness.

This is the already-authorized unsupported-combination refusal, rather than a
new public restriction on every draft or every relative ID. The producer owns
the invariant, so the guard belongs there. Use standard URI comparison after
resolution, not a blacklist of observed strings or a new walker, draft
translator, resolver, registry, validator, provider option, or runtime guard.
Keep plain/handwritten formats, authored Card hashing, compiler lowering,
citations, bounds, persistence, and typed operations unchanged.

**Focused closure proof:** extend the existing resolver/shared-validator table
with anonymous draft-04 and usable authored draft-04 IDs, integer good/string
bad data, plus default/later-draft usable relative and absolute identity
regressions. Cover ineffective authored IDs and classic root-ref suppression
at the same declaration boundary: either exact preserved validation or the
selected pre-save compiler refusal, without rewriting authored form. Exercise
the corrected resolved schema in the existing focused Agent and hosted-code
execution proof, with retained good data and inconclusive/absent bad data.
Reuse `blocks::tests::verdict_data_keeps_its_schema_resource`, existing
`named_verdict_formats_validate`, Agent integration, and the existing code
fixture/journey; add no broad suite or harness.

### FIND-TASK-001-5 — Required executor proof still has uncovered cases

**Status:** REVISED, partially closed. **Classification:** MISSING verification.
**Sources:** BEH-2, DATA-1, INV-1; continuation of prior FIND-5.
**Obligation:** original scenario 2, REQ-002/007, AC-004/005, and R1's explicit
both-kind malformed/exact/one-over execution-proof requirement.

**Locations:** Agent `crates/vala/vala-verify/tests/integration/blocks_agent.rs:604–642`;
checked-in `fixtures/cards/verifier_building_blocks/scored-review.yaml:95–143`;
Rust SDK invalid-case table at 1530 onward and corresponding Python 670 onward
and TypeScript 729 onward.

Two residual cases belong to one required proof inventory:

- Agent `note(bytes)` repeats two-byte `é` using integer division. The empty
  `{score:1,note:""}` object is 21 compact bytes; its 65,515-byte remainder
  rounds down. Fresh compact UTF-8 arithmetic gives **65,535** accepted bytes
  and **65,549** refused bytes after `s3cr3t-payload`, not 65,536/65,537.
  The small escape example and depth cases do not establish adjacent byte
  boundaries, and no Agent wide-array boundary exists. The SDK marker tables
  drive code boundaries; their Agent providers script only BAD-DATA/HUGE-DATA
  and otherwise return a small ordinary score.
- All hosted negative markers run `well-formed`, which always declares custom
  data. EXTRA-DATA returns `{score,category,extra}` under that declaration, so
  it tests an extra **data property**, not envelope `data` under a plain/default
  code format. Plain requirements in this fixture are Agents. HTTP/MCP code
  helpers likewise declare custom data. Repeating the same fixture in three
  SDKs does not supply the missing case.

**Consequence:** required boundary and plain-code negative acceptance claims
are stronger than their actual proof. This does **not** demonstrate incorrect
production byte arithmetic, truncation, retained rejected data, or a storage
bug. Shared production validation appears correct for these cases.

**Smallest selected correction:** extend the existing Agent table with
explicit compact byte-length self-checks and actual 65,536/65,537 values,
including the required UTF-8/escape and wide-array variants. Keep equality on
accepted data and safe terminal refusal on invalid data. Extend the existing
checked-in fixture with a plain/default code check whose negative case returns
otherwise valid envelope data; use existing negative SDK tables to verify
inconclusive outcome, absent data, safe reason, and valid sibling survival.
Keep the existing extra-property test: it protects a different schema error.
No production mechanism or extra test harness follows from these proof gaps.

**Focused closure proof:** run the existing exact Agent selector
`blocks_agent::check_agent_validates_custom_data` and the three existing
`invalid_custom_verdict_data_is_inconclusive` SDK selectors after extending
their cases. Hosted accepted-boundary readback is already implemented and
recorded; repeat it only if its fixture changes. No full SDK suite, repeated
HTTP/MCP journey, load/crash exercise, or invented historical RED is necessary.

### FIND-TASK-001-7 — Python's returned code-check declaration omits its format

**Status:** CONFIRMED. **Classification:** MISSING public declaration.
**Source:** MNT-001. **Obligation:** AC-008/original scenario 5 and Python
AGENTS public API/stub precision and parity.

**Location:** `sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:218–226`
`CodeCheck`, consumed by `RequirementResult.check` at 281. Producer:
`crates/wyrd-spec/src/card/verifier/mod.rs:477`.

The new Rust CodeCheck serializes optional response_format. Engine report
clones the authored check into RequirementResult; ordinary/typed Python getters
and status keep that declaration intact. The authored Python CodeCheck
TypedDict ends at inputs, while Rust and generated TypeScript expose the new
member. Independent `/tmp/ponytail-check-parity.py` imports Check, narrows
`kind == "code"`, and reads response_format. Installed ty reports:
`error[invalid-key]: Unknown key "response_format" for TypedDict CodeCheck`.
This is a reachable implemented return shape rejected by its advertised type.

**Smallest selected correction:** add the optional scalar/expanded named wire
format member to the existing authored CodeCheck declaration, using JSON
schema values for returned data schemas, and preserve absence for old/default
checks. Reuse existing schema/value types; do not add a runtime model,
conversion, native registration, transport, or second stub tree. Add the
custom declaration/narrowing case to the existing verify_contracts type fixture.

**Focused closure proof:** the same narrowed access and supported scalar/
expanded declarations type-check, absence remains accepted, and existing
`py:typecheck`/stub lint passes. Existing runtime source/journeys already
establish the producer; no new server journey is needed for this stub correction.

### FIND-TASK-001-8 — New Rust items miss required documentation

**Status:** CONFIRMED. **Classification:** VIOLATION (explicit rustdoc gate).
**Source:** REP-1. **Obligation:** root AGENTS and agent_docs/rust.md require
rustdoc for new/private/test items and `# Panics` when applicable; the review
skill explicitly retains this blocking exception to the style threshold.

**Locations:** new `exact_data` in
`sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:1455`, and
new local `PATH` in `crates/wyrd-spec/src/gateway/openai.rs:549`.

The complete exact_data body asserts both encoded byte lengths at line 1475;
its docs contain no Panics section. Its readback journey caller relies on that
self-check. The new PATH const has no rustdoc and is used for both generated
component-definition placement and extraction in the full `component` body.
These are new omissions; the earlier local Judge/score docs are now present
and are not reopened under FIND-6.

**Smallest selected correction and proof:** add the missing concise panic
contract and constant role/invariant documentation beside the existing items.
Preserve assertions and behavior; no suppression, deletion of a check, new
helper, runtime change, or behavior test is justified. Source inspection and
applicable format/lint proof suffice. Batch these with the functional fixes.

## Prior finding closure

| Prior stable ID | Disposition and independent source evidence |
| --- | --- |
| FIND-TASK-001-1 | **OPEN, revised above.** Hoisting/dialect erasure and definition collisions were corrected, but producer resource identity is still ineffective for supported draft-04/ID cases. |
| FIND-TASK-001-2 | **CLOSED.** TS typedResult catch inspects no thrown value. Its only three typed callers reuse the converter; the null/undefined/Error/malformed/keyed exception table protects exact normal error mapping. |
| FIND-TASK-001-3 | **CLOSED.** Parallel declares_data_object interpreter is deleted. validate_agent uses canonical-derived two complete shapes and existing bounded fits. Full fits/resolve/fits_object bodies retain ref/composition/budget behavior; accepted required/ref/allOf and refused optional/nullable cases and HTTP absence assertion exist. |
| FIND-TASK-001-4 | **CLOSED.** Shared server refusal reports schema_path, not instance path/display value. Rust and Python conversion discard decoder diagnostics; TS catch does too. All report a safe fixed root with requirement identity. Dynamic-key source tests and raw-read/no-rerun SDK proofs exist; result writers add no unsafe diagnostic copy. |
| FIND-TASK-001-5 | **PARTIALLY CLOSED, revised above.** Hosted exact/one-over UTF-8/escape/wide/depth and malformed cases plus direct/queued/retained readback now exist. Agent adjacent-byte/wide and plain-code undeclared envelope data remain unproved. |
| FIND-TASK-001-6 | **CLOSED.** execution.rs:840–846 now documents the local Judge and score. FIND-8 covers different new items and does not relabel that closed cause. |

## Reconciled acceptance matrix

PASS here means source plus inspected recorded proof support the obligation;
it never claims a fresh execution of every listed test.

| Requirement / criterion / constraint | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: scalar/expanded plain, portable/native, inline/referenced, authored form retained | shared host callback/compiler/PromptSpec/inline Agent; unresolved render refusal | compiler table, plain fixtures, SDK readback | PASS |
| REQ-001/002 / AC-003: invalid names/schemas/conflicts and incompatible contracts refused before save, exact supported meaning | resolver, meta-validation, CodeCheck and declaration validation | existing declaration/HTTP absence cases; fresh identity probe | FAIL: FIND-1 |
| REQ-002: check-only formats, required object/non-null data, plain closed shape, whole/step checks; actions arbitrary | CodeCheck versus CodeCall; canonical shapes and fits | contract and composed/ref cases; scored step fixture | PASS source, FIND-5 missing negative proof |
| REQ-002/007 / AC-004: entire exact schema both kinds; invalid inconclusive/no data | two verdict_from callers; terminal → bare projection | schema/executor/SDK negatives; fresh resource inversion | FAIL: FIND-1 and FIND-5 proof |
| REQ-007 / AC-005: compact bytes, escapes, root-inclusive depth, no truncation, both-kind proof | shared fixed check_data_bounds; existing provider/sandbox bounds | pure boundaries; hosted exact/readback cases; fresh Agent byte calculation | FAIL proof: FIND-5; production bound logic supported |
| Preserve citations, Agent pass rule, aggregation, dispatch/status, actual/detail | executor continuation, Judged and unchanged aggregation owners | citation/uncited cases; plain/scored step fixtures | PASS |
| REQ-011 / AC-002: ordinary direct/queued/status/HTTP/MCP/published Bifrost, old/plain absent | optional generic field; complete outcome serialization and ordinary readers | three SDK readback, HTTP/MCP, retained outcome equality | PASS for correctly validated data, subject to FIND-1 |
| REQ-011 / AC-007: existing tenant/query-read/retention/durability/privacy | existing tenant owners/outbox/results; no extra projection | foreign/scoped HTTP/MCP and safe diagnostic cases | PASS in changed boundary |
| REQ-012 / AC-006: actual typed values through all operations, Rust deadline, ordinary workflows | shared Rust conversion, Python model_validate, TS parse | typed success/mixed/failed/absent and status/type fixtures | PASS |
| REQ-012: whole conversion failure, stable safe-root error, no payload/rerun/mutation | shared conversion owners, no execution call on mismatch | arbitrary TS throws, Rust keyed case, SDK reread/call counts | PASS |
| Python positional-only verifier/type, data_type slot and group/input behavior | native signature delegates ordinary verify | Python typed journey and type fixture | PASS |
| REQ-002 / AC-008 model/schema authoring, no mandatory model dependency | Python coercion, TS verdictFormat/jsonSchemaText, Rust schema map | authoring unit/type cases and recorded checks | PASS |
| AC-008 public projections/runtime schemas/generated types/docs agree | six-layer existing classes, generated TS and error catalog, owning docs | source/type evidence and recorded generation | FAIL: FIND-7; otherwise supported |
| Migration preserves consumer contracts and tiers | manual schemas, generators, OpenAPI/MCP/extract readers | inspected migration checks and recorded unit/codegen evidence | PASS except resource semantics FIND-1 |
| Changed-item mandatory rustdoc | prior Judge fixed; new helper/constant omissions | direct source inspection | FAIL: FIND-8 |
| Scope/non-goals/reuse/availability | existing owners; no new store/transport/polling/check kind/config; no shared-process crash policy | cumulative diff and system/domain coverage | PASS |
| Historical TDD/diagnosis provenance | missing historical RED disclosed; test-set ordering retains members | recorded evidence only; retained independent diagnosis report absent | Historical limits/nonblocking process deviation below |

## Non-blocking deviations and rejected escalation

Standards identifies missing retained independent diagnostician evidence for
historical assertion updates. The diagnosis text exists, but the independently
authored report is not present. This remains an unverified process obligation;
it is not retroactive independent proof. The actual MCP required-set change
sorts while retaining every required member; schema assertion adaptations follow
the changed generator. No weakened behavioral assertion or public-contract
consequence was established. Under this review skill's blocking threshold,
that process deviation alone does not create a FIND or demand implementation
reruns. Do not manufacture a historical report.

Handwritten recipe snippets instead of tested embeds, Python additions in an
existing owner-crate Python section, a function-local import, and an interface
where a type is preferred are recorded standards deviations. Their proposed
escalation to material blockers is **REJECTED**: no demonstrated behavioral,
security, tenancy, durability, public-contract loss or unearned code deletion
follows solely from those placement/wording/shape issues. They receive no FIND
ID or remediation work. Rustdoc remains the explicit exception, FIND-8.

## Focused remediation lanes

Use these existing exact selectors, extending their current owner tables:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::tests::verdict_data_keeps_its_schema_resource)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier::validate::tests::named_verdict_formats_validate)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::check_agent_validates_custom_data)'
```

For the changed checked-in hosted-code fixture and negative readback tables,
use the original task's existing repository-managed Postgres wrapper and worker/
SDK setup, with only these selectors:

```bash
with_verification_postgres cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::invalid_custom_verdict_data_is_inconclusive)'
with_verification_postgres uv run --project sdks/wyrd-sdk-python python -m pytest -q -m integration sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py::test_invalid_custom_verdict_data_is_inconclusive
with_verification_postgres pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^invalid_custom_verdict_data_is_inconclusive$'
```

Reuse the schema-construction regression in the existing focused Agent/code
proof for FIND-1 rather than introducing another journey. If pre-save refusal
is exercised in the HTTP declaration table, its existing exact selector is
`pg_verification_routes::verdict_declarations_validate_before_persistence`
(`wyrd-server --features test-support --test integration --test-threads=1`).
Run that focused selector only when extending its case table, under the same
Postgres wrapper. All Cargo-backed operations remain sequential.

For FIND-7 run `mise run py:typecheck` and `mise run py:lints`; these already
own `tests/typecheck/verify_contracts.py` and authored-stub parity. Formatting/
lint checks cover only the resulting write set: Rust fmt and focused Clippy for
changed owning crates/test targets, Python/TS format/lint/type lanes only when
those fixtures/declarations change. FIND-8 alone needs source/format/lint proof,
not behavioral reruns. Existing codegen verification applies only if the repair
changes generated contracts; this remediation needs no schema-library upgrade,
new dependency, broad gate, or full SDK suite.

## Verification and final recommendation

Fresh independent proof:

- `/tmp/ponytail-identity-probe.rs` compiled with `mise exec -- rustc
  --edition=2024` against repository-built jsonschema/serde_json rlibs and ran
  successfully. The reproduced inversion table above is actual Rust validator
  behavior, not Python JSON Schema behavior or reviewer consensus.
- `mise exec -- sdks/wyrd-sdk-python/.venv/bin/ty check --project
  sdks/wyrd-sdk-python --extra-search-path sdks/wyrd-sdk-python/python
  /tmp/ponytail-check-parity.py` produced the exact expected unknown-key
  diagnostic, independently confirming FIND-7.
- A `mise exec -- python3` compact UTF-8 JSON calculation independently
  reproduced Agent overhead 21 and total sizes 65,535/65,549.
- HEAD remained the candidate; existing user edits and reports were preserved.

Recorded original/R1 successful focused checks remain evidence claims,
cross-checked against source and assertions; this validation does not recertify
their executions. No Cargo/server/Postgres/SDK suite, formatting, generation,
live provider, fault injection, or broad gate was rerun. Rlib probes do not
constitute fresh full executor journeys. Live strict-provider acceptance of
embedded schema IDs, historical RED for original scenarios 1–3, and retained
independent diagnosis provenance remain disclosed limits.

**FIX_REQUIRED:** FIND-TASK-001-1, FIND-TASK-001-5,
FIND-TASK-001-7, FIND-TASK-001-8. No missing report, unresolved conflict, or
candidate movement requires BLOCKED; the minimal selected corrections do not
require SPEC_REVISION_REQUIRED.

Skipped broad/runtime re-execution and live providers; schema meaning and two
required proof cases remain unresolved in the candidate until remediation.
