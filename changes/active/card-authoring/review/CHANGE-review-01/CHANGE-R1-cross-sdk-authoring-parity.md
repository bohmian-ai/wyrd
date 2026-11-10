---
id: CHANGE-card-authoring-R1
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 6
spec_approval: approved
findings: [FIND-CHANGE-001, FIND-CHANGE-002, FIND-CHANGE-003, FIND-CHANGE-004, FIND-CHANGE-005]
---

# Close the remaining cross-SDK authoring gaps

Python's fluent wrappers can build the finance graph, but equivalent canonical
option objects still reject nested authored values and canonical Prompt nulls.
The new Rust/Python generic constructors also exceed the approved kind
inventory, and Python lost an existing file-declaration keyword. Fix these
four public contracts at their current shared and binding owners, and repair
the stale media-slot fixture found by full verification; preserve the working
declaration, publication and execution behavior.

## Authority and immutable review context

- Approved spec: `changes/active/card-authoring/spec.md`, revision 6.
- Original tasks: `changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md`,
  `TASK-002-python-authoring.md`, `TASK-003-typescript-authoring.md`, and the
  approved R3/R5/R6 follow-on tasks. Current spec/architecture supersede
  historical mechanics in those tasks.
- Integrated base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Reviewed target: `17916872ad526d3f3b474c86534d6227e2bc922c`.
- Independent source findings/acceptance: adjacent `integrated-review.md`;
  orchestration/verdict and execution evidence: adjacent `verdict.md` and
  `verification.md`.
- Preserve prior task verdicts/remediations and their stable IDs. These are
  new aggregate findings, not reassigned prior findings or a reopening of
  unrelated accepted behavior.

Apply root/Python AGENTS, shared Rust and maintainer/error guides, and the
owning Client model/In-memory authoring/Prompt/Registry lifecycle architecture.
Route this bounded task directly to `$wyrd-implement`; it requires no new spec
approval and does not authorize unrelated cleanup.

## FIND-CHANGE-001 — Nested Python canonical arguments bypass native capture

REQ-001/002/008/009 and AC-001/002/005/007 require canonical objects and fluent
construction to agree, including by-value Cards at permitted reference slots.
Python `src/authoring.rs:154` recognizes native objects only at the argument
root; mappings then use JSON conversion. `binding` at `:191` and `canonical!`
at `:203` also use JSON for canonical mappings, and `Check.drift` at `:1044`
serializes its complete spec. Consequently these required shapes immediately
raise `WYRD_SPEC_400_VALIDATION` with a JSON serialization error:

- `Check.drift(..., {"signal": {"baseline_ref": CardRef.data(...)}, ...})`;
- `Agent(..., verified_by=[{"verifier": authored_verifier}])`;
- `Service(..., components=[{"alias": "judge", "ref": authored_agent}])`.

Separate Component/Binding wrappers work but do not fulfill canonical mapping
support. Serializing an exact literal ref avoids the exception but cannot
preserve an authored Card's sibling capture. The same common producer affects
nested workspace/code Artifact selections and inline canonical bodies, so a
special case in each consumer would repeat the defect.

Extend the existing Python argument conversion boundary to compose canonical
mappings/sequences containing native values through the existing shared
`FieldValue` and canonical-object `FromIterator` conversions. Route canonical
Component/Binding/Step/Requirement/Action/Check and other reference-bearing
arguments, including drift, through that boundary. Preserve supported inline
bodies, authoring CardRefs and ordinary registered CardRefs as well as authored
Cards and Artifact selections. Update recursive authored stub argument types
so these forms are supported by the type checker.

The shared Body/Captures/loader/Graph owners must remain the only capture,
identity, semantic validation and declaration owners. Python only adapts its
values. Retain plain-JSON trust-boundary error handling, schema/model-class
conveniences and the existing callback-safe setter conversion order: user
conversion must not run while the receiver remains borrowed. Keep native
snapshots by value, external versus sibling intent, field replacement,
duplicate-add refusal and deferred file reads. Do not introduce a Python graph,
resolver, schema library, scheduler, temporary-file bridge or upload loop.
This bounded input adaptation is necessary; a guard at declaration cannot
recover native values already rejected by JSON serialization.

Acceptance and focused proof: canonical mappings and corresponding fluent
values emit equal canonical content and dependency intent for the three
reproduced forms and representative inline/Artifact nesting. Subsequent child
edits do not alter a capture; replacing an enclosing field drops obsolete
captures while another live field retains its own; attempted incremental
duplicates still refuse. Exercise a canonical-object graph through both
existing Python declaration owners in the existing parity journey, checking
registered identities/content and actual local Artifact selection/publication.
Keep existing callback/schema, permission/error and fixture assertions.

## FIND-CHANGE-002 — Python rejects canonical nullable Prompt collections

REQ-001/002/008/009 and AC-001/005/007 preserve canonical Prompt values. Shared
Rust and TypeScript support `messages: null` and `body: null`; Python's Prompt
dispatch (`src/authoring.rs:517` and the class macro's collection setter arms)
unconditionally invokes list/map conversion. Both constructor keywords and
setters reject `None` immediately, and the PromptFields/setter stubs exclude
it. A caller cannot clear an earlier request form to author the other form
using the canonical value. This is distinct from nested native-value capture.

Accept exactly `None` at the two existing Python fields and forward it to
shared `Prompt::clear_messages`/`clear_body`. Both constructor keywords and
same-key setters must use the same adaptation. Preserve ordinary iterable/map
replacement and add-after-null behavior; expose no new public Python clear
method, sentinel or null syntax. Update the precise stub unions. Do not
convert None to an empty collection or validate Prompt semantics in PyO3.
The shared helpers already implement the required replacement, so no shared
nullable framework or Prompt compiler change is needed.

Acceptance and focused proof: constructor/setter nulls round-trip exactly,
clear previous content, allow subsequent additions, and retain shared deferred
validation when neither/invalid request form is complete. Direct None calls
pass type checking. Compare canonical output with existing Rust/Node cases
and use a valid resulting Prompt in the existing Python parity proof.

## FIND-CHANGE-003 — New generic constructors expose unauthorized kinds

The locked twelve-kind contract, REQ-001/INV-002 and AC-007 exclude newly
exposed Policy/Source authoring. The new public shared
`CardDefinition::new` (`authoring/mod.rs:968`) accepts any existing CardKind;
new Python `PyCardDefinition` (`src/authoring.rs:674`, constructor `:699`)
projects it. Rust SDK inventory at `tests/integration/authoring.rs:1506`
explicitly constructs Policy/Source; Python inventory at
`tests/unit/cards/test_authoring.py:344` constructs Policy. A direct Python
Policy construction validates successfully. The original base has no
public authoring module: old internal enums do not authorize this new surface.
TypeScript removed the equivalent generic route during TASK-003 remediation.

Confine raw shared construction to the existing in-crate malformed-input
diagnostic tests; it must not remain a public production constructor or an
unused production helper. Delete the new Python generic
facade and its public exports/stub/native registration/definition-dispatch
entries. Remove only the prohibited external Policy/Source inventory cases.
Preserve the public erased CardDefinition type, conversion from the twelve
named values, validation/snapshot/declaration operations, and every authorized
named construction path.

The only external production caller of the raw constructor is the deleted
Python facade; the other external Rust call is the prohibited inventory loop.
This selects deletion/narrow visibility rather than another kind allowlist,
new enum, early domain check or compatibility alias. Do not overhaul legacy
wire/server CardKind variants, schemas or stored content.

Acceptance and focused proof: all twelve named kinds remain public and retain
canonical conversion/declaration coverage; Python has no generic constructor
export or stub; external Rust cannot use the raw constructor; shared malformed
canonical diagnostic tests still pass. No public test claims successful new
Policy/Source authoring. Keep internal erased conversion and all legitimate
Cards/state callers compiling.

## FIND-CHANGE-004 — Python renamed an existing file-call keyword

REQ-006 and AC-008 preserve existing file callers/options. Base
`WyrdState.declare` exposes `path`, accepting `path=...`. The integrated PyO3
signature (`src/state/mod.rs:185`) and stub
(`python/wyrd/state/__init__.pyi:140`) renamed it to `card`. The formerly valid
call now raises unexpected-keyword TypeError before the loader. Existing
positional journey calls do not test that contract. The approved breaking
changes concern the new authoring vocabulary and slot-level required field;
they do not authorize renaming this existing file-call parameter.

Restore the original public parameter name `path` while keeping its widened
path-or-authored-Card value and existing options/client/bundle behavior. Match
runtime docs and stubs. This is a signature correction, not an alias or
compatibility parser. Reuse the existing declarable conversion and shared
file/Card declaration engine unchanged.

Acceptance and focused proof: the existing real Python parity journey declares
its file form with `WyrdState.declare(path=..., client=...)` and retains
positional authored-Card success and existing options. A missing path using
that keyword reaches the ordinary loader error, rather than argument binding
failure. Stub/runtime signatures agree and direct keyword use typechecks.

## FIND-CHANGE-005 — A positive Artifact admission fixture retains a removed slot field

REQ-003/AC-009 remove and reject slot-level `required`; the broad verification
gate must still exercise the existing Artifact admission limits. Full Wyrd
family execution fails in
`pg_hosted_artifacts::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit`.
Its `two_extracts` fixture at
`crates/wyrd/wyrd-server/tests/integration/pg_hosted_artifacts.rs:1262`
adds a dynamic boolean `required` to both media slots. The first of its six
calls panics at line 1272 while decoding the Verifier, before any admission
assertion: unknown field required, expected formats or max_bytes. This is a
current integration regression from the approved grammar change, not a reason
to restore optional-slot metadata or loosen validation.

Update that one existing fixture to emit canonical media slots without the
removed field, removing the obsolete fixture argument that only selected that
flag. Update its optional-slot explanation to the approved evidence-omission
semantics. Preserve all formats, per-slot max_bytes, combined/individual limits,
double-read charging and positive/negative admission assertions. The helper's
only consumer is this test; no production engine or new fixture owner is
needed. Sibling search established that remaining boolean slot-level required
cases deliberately test refusal, while required arrays belong to JSON Schema:
neither category may be deleted or loosened.

Acceptance and focused proof: the exact failing real-server test must reach
and pass every existing admission/limit assertion with canonical slot input.
Explicit invalid-slot refusal tests and JSON Schema required behavior remain
intact. Do not ignore, suppress, skip, delete or weaken the failing test, widen
the deserializer, add a retry, or change a timeout to clear this failure.

## Required verification follow-up — post-session PTY observation

The selected installed-CLI gate also fails in
`tests/integration/test_local_development.py::test_installed_cli_guided_setup`.
Its `interrupt_hidden_question` helper checks terminal echo using the slave
descriptor after `child.communicate()` has reaped the controlling-session
child. On this supported host that descriptor returns ENOTTY (25), so the
assertion cannot observe the state. Source comparison establishes that this
test/helper and the CLI terminal owner are unchanged by this change; this is
a current verification blocker, not an additional product-regression FIND ID.
The root repository rule forbids waiving a selected red gate as pre-existing.

Independent stdlib-only probes established the cause and correction: after
the same controlling-session child exits, the master descriptor remains
queryable; it reports ECHO enabled when the child restored it and disabled
when the child deliberately left it off. Change only the post-exit echo
observation to that existing master descriptor. Preserve the controlling-session
setup, pre-exit echo-disabled observation, both signals, expected exit 130,
and every no-disclosure/no-artifact assertion. No CLI/product behavior change,
new PTY framework, OS branch, retry, timeout increase, catch-and-ignore or
weakened assertion is justified.

Use the existing local-development lane's CLI/server and production-UI setup,
then run the exact guided-setup selector below. Its prior failure stopped the
remaining local-dev commands, so do not claim their coverage from this review.
Final integrated verification must execute the full lane after correction.

## Preserved behavior and non-goals

Keep shared canonical grammar, duplicate diagnostics, local capture/replacement,
exact external/sibling references, twelve named kinds, selected-client tenancy,
stable errors and the existing sole-Artifact publication/declaration/pull tail.
Nothing rolls back an Artifact already published before later graph failure.
Preserve fixed constructor fields and ordinary Action/Check types, honest missing
evidence, JSON Schema semantics, native stage planning, existing finance outcomes,
citations/versions and retained Node lifetime/type behavior.

No new wire fields/endpoints, schemas, dependency, feature, receiver type,
compatibility alias, runtime scheduler, registration/upload engine, migration,
security/tenant/persistence behavior or automatic heavy Data/Model upload.
No correction of unrelated legacy CardKind definitions. No new fixture platform,
full journey suite requirement or optional hardening. Preserve mandatory Rustdoc
on every changed Rust item and update authored stubs at their existing owners.

## Focused execution and evidence

Use the current tests/fixtures and their existing setup. Confirm any added test
selector before recording evidence. Rebuild the Python extension once, then:

- Extend the existing Python authoring offline file for nested canonical/native
  composition, null handling, authorized exports and callback/snapshot/type
  regressions; run
  `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/test_authoring.py tests/unit/test_public_exports.py`.
- Extend the existing Python parity journey for canonical kwargs, valid nullable
  Prompt and the existing `path=` call. Run only
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_local_artifacts_and_declaration_parity"`.
  Retain the shared story and its existing positive/negative assertions; no
  broad integration suite is required for remediation.
- Run `py:format:check`, `py:lints` and `py:typecheck` for actual Python/stub
  edits. Run `fmt:check`, `lints` and `check:deps` for shared/PyO3 changes;
  do not overlap Cargo commands. Preserve all mandatory item rustdoc.
- For the shared visibility change, run the existing exact canonical diagnostic
  proof:
  `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::yaml_and_code_share_diagnostics)'`,
  plus only additional exact shared tests materially changed by the correction.
  Confirm actual names from source before execution.
- Retain the Rust public inventory's twelve-kind positive proof; run its exact
  existing selector with repository setup:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::public_paths_author_every_native_family)'"`.
- For the positive media-slot fixture, run the exact failing test with normal
  managed dependencies and tracing:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit)'"`.
- For the independent gate-fixture observation correction, after its existing
  setup run only
  `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup`.
  Preserve all terminal, signal, secrecy and filesystem assertions; the
  independent restored/unrestored-echo probe establishes that the descriptor
  change retains the property rather than bypassing it.
- No generated wire output is expected to change. Run existing generator/check
  tooling only if consumed outputs actually change; never hand-edit generated
  artifacts. Finish with `mise exec -- git diff --check`.

Record each FIND-CHANGE ID with its actual source correction and focused
observation. The next review checks closure, changed code and affected shared/
foreign consumers against this target, carrying unchanged integrated acceptance
and existing task closures forward. Full integrated suites run at change review,
not as repeated remediation loops. Durable task-review packet/evidence binding
must also be resolved under the caller's commit authorization before a future
immutable PASS can retire the active packet; this task does not authorize commits.

## Implementation evidence

Candidate: commits `0f2f89f69..HEAD` on `docs` over reviewed target `17916872a`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-CHANGE-001: canonical mappings carry native values equal to fluent forms | `sdks/wyrd-sdk-python/src/authoring.rs` `field` composes `dict`/`list`/`tuple` through shared `FieldValue` `FromIterator`; new `object` keeps the str-key `WYRD_SPEC_400_VALIDATION` refusal; `binding`, `canonical!` (Component/Step/Requirement/Action/Check) and `Check.drift` route mappings through it; stub `FieldInput` is recursive and canonical positions take `Mapping[str, FieldInput]` | `test_canonical_mappings_carry_native_values` (verifier binding, component ref, drift `baseline_ref` via both CardRef types, nested ArtifactFile/inline/file code source; snapshot, field replacement, live-field retention, duplicate refusal, key/NaN refusal), `test_canonical_mapping_callbacks_may_edit_their_receiver`; parity journey declares `canonical_release` through `Cards.declare` (local 1.0.0 equals file graph; local directory 6.0.0 publishes and mounts) and `WyrdState.declare` (registered 1.0.0, verified twice) | PASS |
| FIND-CHANGE-002: `None` at Prompt `messages`/`body` | `nullable { messages: clear_messages, body: clear_body }` arm in the shared `set` dispatch used by constructor keywords and setters; `PromptFields`/setter stubs take `| None` | `test_prompt_request_forms_take_canonical_nulls` (mirrors Rust/Node cases: round-trip, clear, add-after-null, neither form refused at validation); journey registers `report-reviewer-prompt` 6.0.0 with `body=None` | PASS |
| FIND-CHANGE-003: no public generic constructor | `CardDefinition::new` deleted; shared diagnostic tests build `CardDefinition(Card::with_spec(..))` in-crate; `PyCardDefinition`, its registration, exports and stub deleted; Rust Policy/Source and Python Policy inventory cases removed | `authoring::tests::yaml_and_code_share_diagnostics`; `authoring::public_paths_author_every_native_family`; `test_authoring_exports_no_generic_card_constructor`; `test_every_native_family_constructs_canonical_specs` | PASS |
| FIND-CHANGE-004: `WyrdState.declare(path=...)` | PyO3 signature, rustdoc, error field and stub restored to `path` | journey `from_file = WyrdState.declare(path=..., client=WyrdClient())`; missing path by keyword reaches the loader (`WYRD_LOADER_400_INVALID_ENVELOPE` naming `missing.yaml`); positional authored-Card/reader calls retained; `py:typecheck` | PASS |
| FIND-CHANGE-005: canonical media slots in admission fixture | `two_extracts` emits one canonical slot for `a` and `b`; flag argument removed; doc states omitted evidence stays missing | `pg_hosted_artifacts::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit` with `WYRD_LOG=info,wyrd_server=debug` | PASS |
| PTY post-exit echo observation | `interrupt_hidden_question` reads post-exit echo from the master descriptor | `test_installed_cli_guided_setup` after `py:setup`, bins and production UI build | PASS |

Commands (all exit 0): the exact selectors above; `pytest -q tests/unit/cards/test_authoring.py tests/unit/test_public_exports.py` (42 passed); parity journey selector (1 passed); `py:format:check`, `py:lints`, `py:typecheck`; `fmt:check`, `lints`, `check:deps`; `git diff --check`. No generated output changed.

Diagnosis — first guided-setup run: **Symptom** `OSError: [Errno 28] No space left on device` copying `target/debug/wyrd-server` into the test home. **Evidence** `df` showed 996 MiB free on the data volume; `target/debug/incremental` held 26 GB. **Cause** host disk exhaustion, not code. **Fix site** none in source; the regenerable incremental cache was cleared and the unchanged selector passed.

Non-goals held: no shared capture/graph/validation change, no new wire field, schema, dependency, feature, alias, or Python graph/resolver; no TypeScript change; CardKind legacy variants untouched.
