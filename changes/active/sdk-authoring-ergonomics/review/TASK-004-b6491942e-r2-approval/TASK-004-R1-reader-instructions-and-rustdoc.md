---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-sdk-authoring-ergonomics
spec_revision: 8
requirements: [REQ-005, REQ-006, REQ-008, REQ-009, REQ-010, REQ-014, REQ-016, REQ-017, INV-001, INV-002, INV-003, AC-006, AC-007, AC-008, AC-010, AC-012, AC-013]
depends_on: [TASK-004]
parent_task: TASK-004
remediates: [FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4]
---

# Finish the Python reader instructions and changed-item rustdoc

## Outcome and subject

A reader can follow Author in code from setup through the complete modular
Python example, find the canonical typed AgentRunConfig construction, and use
plain JSON Schemas without confusing a named-input mapping with a whole schema.
The two changed Rust test items also satisfy the mandatory panic-doc contract.
Preserve the approved Table and active-table APIs and their behavior.

- Approved authority: [SDK-authoring spec revision 8](../../spec.md) and
  [verification-contract revision 72](../../../verified-change-contract/spec.md).
- Original task, retained unchanged: [TASK-004](../../tasks/TASK-004-language-authoring-docsite.md).
- Original review: [r1 verdict](../TASK-004-b6491942e-r1/verdict.md) and
  [validated ledger](../TASK-004-b6491942e-r1/findings-validation.md).
- Original base: `352ca157856217b6083e131b5d68cafe50cf36f5`.
- Prior/cumulative candidate: `b6491942ea845055b89f8a955bc12e9cd7157cc3`.
  A later task review records the new candidate and reviews the fix delta plus
  affected consumers against the original cumulative subject.

The maintainer explicitly approved revision 8 after the first review. That
approval resolves the authority decision in FIND-TASK-004-1; it does not waive
the remaining findings or approve task completion. For the named Table,
optional-Pydantic and active-table additions only, revision 8 and this task
supersede the parent's documentation-only production/interface stop condition.
No new runtime change is needed to preserve those already reviewed additions.
The original task remains the acceptance context for all other behavior.

## Diagnosis and selected corrections

### FIND-TASK-004-2: complete the modular reader path

The guide at `docs/src/content/docs/build-and-verify/declare/author-in-code.md:124–154`
shows only two Verifier modules and main, then directs a setup-only reader into
`examples/finance-release/service/`. Setup never obtains that directory.
`examples/finance-release/service/main.py:3–8` imports both components' Agent
modules, which the page neither embeds nor supplies a retrieval route for.
The existing modular journey has the whole repository directory and therefore
cannot establish that the rendered instructions provide all required files.
The same guide merely names AgentRunConfig; its embedded files and linked
Verifier reference provide no construction, despite the parent's explicit
canonical typed-config teaching requirement.

Keep the current modular example and show its two existing Agent modules
alongside the existing Verifier/main embeds. Give explicit save paths and the
directory/run instructions needed to recreate that complete five-module tree
from the rendered page. Retain the existing gateway prerequisite. This local
addition avoids repository-checkout prerequisites, a download framework or a
second application. Expose the existing canonical AgentRunConfig construction
from `examples/finance-release/finance_release.py` through a bounded source
embed with enough context to understand where the value is passed. Use the
existing CodeFromFile capability and actual tested source, not hand-written
copies or a new configuration model.

This closes a required procedure that otherwise stops before declaration;
it is not a preference about page layout. No SDK constructor or modular
runtime change is required. The existing source files and their journey own
the behavior; improve only the reader's ability to obtain and run them.

### FIND-TASK-004-3: distinguish input-slot maps from whole schemas

At guide line 25, “pass a JSON Schema dictionary wherever the program passes a
model” is false for `inputs=Inputs` in `examples/author-in-code/author.py`.
`verifier_inputs` in `sdks/wyrd-sdk-python/src/verify/definitions.rs:383–423`
expands a class's properties into slots, but a mapping goes through `entries`
as slot-name/value pairs. Replacing Inputs with its whole object schema creates
keys such as type/properties/required rather than the required answer slot.
The documented optional-dependency path therefore cannot reproduce the basic
verification flow. Table and ResponseFormat schema positions accept whole
schemas and must not be described as having the same input-map grammar.

Replace the blanket instruction with the precise existing forms: Verifier
inputs map each slot name to its schema (answer maps to a string schema),
Table and ResponseFormat take the corresponding whole object schemas, and
evidence/rows remain plain dictionaries. Retain the existing typed basic and
modular examples. Keep the explanation bounded; any copyable source block
must follow docs rules and come from an exercised examples file. Do not add a
second whole-input-map dictionary interpretation to the SDK or change the
shared adapter to accommodate incorrect prose.

Extend the existing Pydantic-blocked unit scenario only as needed to pin the
exact answer-slot and plain Table schema forms described by the page. That
existing test already runs a separate interpreter with Pydantic unavailable;
reuse it rather than introducing an environment or test harness. A focused
canonical slot/schema assertion protects this instruction correction; existing
actual-reader and modular journeys supply the relevant execution regression.

### FIND-TASK-004-4: document panic conditions on surviving test items

`crates/shared/wyrd-client/src/authoring/tests.rs:1918–1940`
(`tables_match_their_canonical_entries`) calls expect/assert without a panic
section. `crates/shared/wyrd-client/src/state.rs:2528–2614`
(`TestBundle::complete_service_with_tables`) creates a temporary directory and
serializes/writes fixture data but only links to its wrapper. The wrapper's
panic section does not document this new helper. Root rules and
`agent_docs/rust.md` explicitly require panic docs on changed tests/helpers;
green runtime checks cannot waive the merge gate.

Add accurate # Panics sections for these two items, matching their complete
bodies. Preserve assertions and fixture failure behavior. These are local
documentation edits batched with the guide fixes; no additional runtime test
or separate style-only remediation round is justified.

## Ownership, preserved behavior and non-goals

Write set: the Author in code page, existing example regions only if needed
for source embeds, the existing Python authoring unit/integration test owners
only as needed for focused proof, the two named Rust doc comments, and generated
documentation corpus through its owning generator. Reuse the existing Python
fixtures/local judge and WyrdTestServer; never require live provider credentials.
Read applicable docs/Python rules and root/agent-doc guidance before editing.

Preserve all three reader programs' declare/exact-ref/replay/pass-fail/write/
drain/readback behavior, component-specific binding/refusal assertions,
canonical Verdict envelope, named evidence, optional Pydantic, and the
approved shared Rust Table/schema/lifecycle owners. Preserve row-only default
writes, explicit write_to/writeTo, shared switching, sole-table startup refusal,
Card/Run attribution, admission, retries, permissions and tenancy. No new
production API, lifecycle, parser, validator, writer, dependency, configuration
option, compatibility alias, release bump or draft-page requirement is in scope.
The user explicitly accepted the current 0.4.1 release timing.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| FIND-TASK-004-2 | From setup plus the rendered page, a reader can save all five modular files at the correct paths, run main with the documented gateway prerequisite, observe the captured component verdicts, and reach an actual canonical AgentRunConfig construction. Existing component binding/refusal assertions remain. |
| FIND-TASK-004-3 | The guide distinguishes named slot maps from whole Table/ResponseFormat schemas. With Pydantic unavailable, its named answer slot and plain table schema validate through the existing owners; dictionary evidence/rows retain the existing flow. |
| FIND-TASK-004-4 | Both named changed Rust items have accurate panic sections, with no weakened assertions or altered fixture behavior. |

This is documentation plus regression proof of existing APIs. No manufactured
RED or production behavior change is required. Validate the rendered-page file
paths and imports in a clean reader directory using only its save/run steps;
do not count the repository's injected sys.path as proof of page completeness.
Existing real-server tests supply local provider responses. Record that reader
check separately from runtime journey results and do not claim a live-provider
or documented-release run that was not performed.

Run these exact focused selectors for the named existing proof, after ordinary
Python setup only if its extension is stale:

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/test_authoring.py::test_plain_schemas_do_not_require_pydantic
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject tests/integration/cards/test_authoring_journey.py::test_author_in_code_example'
```

Confirm nonzero selection and retain current assertions. If an additional test
is actually named, record its exact selector before executing it. Do not run
all SDK journey suites or the broad gate for this remediation.

For the bounded write set, run docs:generate, docs:check:commands,
docs:linkcheck and docs:build. Confirm generated corpus matches the source via
the owning generator and report the diff. Full docs:check includes committed-
output drift and broader docs checks; it and full journey suites run at change
review, not as an additional task-remediation demand. Never claim that gate
passed merely because its component checks passed, or bypass its drift step.
Commit/stage only on explicit instruction.

```bash
mise run docs:generate
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:build
mise exec -- git diff --check
```

If Python source/tests change, run py:format:check, py:lints and py:typecheck.
Rust fmt:check covers the changed doc comments; executable Rust tests are not
required solely for panic docs. No TypeScript implementation changes or
contract generation are expected. The fresh review checks this task's closure
and affected paths while carrying forward the first review's unchanged evidence.

## Handoff

Route directly to `$wyrd-implement` with this file and the prior validated
ledger. This task packages the three existing findings and adds no new review
finding, product decision or implementation approval beyond revision 8.

## Implementation evidence

Candidate: `e43d4bab8` on top of `b6491942e`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-004-2: five modular files savable from the page, run step, canonical AgentRunConfig | `author-in-code.md` step 5 embeds both `agent.py` files beside the Verifier and `main.py` embeds, each tab titled by its save path under a new `quickstart/service` directory, and runs `uv run main.py` from `service`; gateway prerequisite kept. `finance_release.py` gains the `run-config` ANCHOR region around `policy_judge`, embedded with `CodeFromFile region="run-config"` | Reader check: the five files copied by their tab paths alone (no `__init__.py`, no repository `sys.path`) into a clean directory import `main` and its four component modules with `PYTHONPATH` unset. `test_multi_component_service_verifies_each_subject` passed (binding/refusal assertions unchanged). `docs:build` resolves the region | PASS |
| FIND-TASK-004-3: slot maps vs whole Table/ResponseFormat schemas | Install note now says each input slot maps to its own schema (`inputs={"answer": {"type": "string"}}`), `Table` takes the row's whole object schema, `write` takes a dictionary row | `test_plain_schemas_do_not_require_pydantic` (Pydantic blocked) now asserts the `answer` slot schema and a plain `Table("refund_replies", reply)` entry; existing ResponseFormat whole-schema assertion retained; `test_author_in_code_example` passed | PASS |
| FIND-TASK-004-4: panic docs on changed Rust test items | `# Panics` on `tables_match_their_canonical_entries` and `TestBundle::complete_service_with_tables` matching their bodies; assertions and fixture behavior unchanged | `fmt:check` | PASS |

Commands, all exit 0: the two task selectors (1 passed; 2 passed under
`with-test-postgres.sh`), `docs:generate` (regenerated `llms-full.txt`
committed), `docs:check:commands`, `docs:linkcheck`, `docs:build`,
`py:format:check`, `py:lints`, `py:typecheck`, `fmt:check`, `git diff --check`.

Limits: the reader check validates paths and imports, not a live gateway run;
the modular journey uses the repository's local judge. Full `docs:check` and
journey suites are left to change review. No production API, SDK runtime,
TypeScript, release bump or other non-goal changed.
