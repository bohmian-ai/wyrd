---
id: TASK-004
kind: implementation
status: complete
spec: SPEC-docsite-incremental-reset
spec_revision: 9
requirements: [REQ-004, REQ-006, INV-001, INV-002, INV-003, INV-004, INV-005, AC-003, AC-006, AC-007]
depends_on: [TASK-003]
parent_task:
remediates: []
---

## Outcome and Value

Future changes to local snippets and generated-reference sources receive the
necessary documentation verification and publication. Contributor guidance
keeps guides independently executable, evidence-backed, and incremental. The
old broad content packet no longer competes with this approved scope.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement`. Own documentation dependency closure in current CI/task
owners plus product/contributor guidance. TASK-001 establishes the sample and
journey; TASK-003 establishes the final publication/source dependencies.

No new CI orchestration framework, source mirror, universal dependency
manifest, external-example sync, new product API, or tasks for deferred guides.
Keep current conservative CI fallbacks and required gates. Generated content
continues to come from its owning sources.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Changed sources select docs proof | `.github/scripts/select-ci.py`: `DOCS`, `select`; `.github/scripts/detect-changes.sh` | `.github/scripts/tests/test-detect-changes.sh`: `check_selection`; `.github/workflows/lints-test.yml`: docs job and CI matrix Run lanes; `mise.toml`: `check:ci-selection` | Current docs classification does not explicitly cover all inputs scanned by error generation or the new sample journey | Extend current classification and existing self-tests for the actual dependency closure | No second selector or check that checks another check |
| Source changes republish derived output | `.github/workflows/publish-docs.yml`: push paths, Generate docs, Build docs | `docs/scripts/generate_api_docs.py`: `error_catalog` scans `crates/**/src/**/*.rs` and `sdks/**/src/**/*.rs`; `generate_llms_txt.py`: `page_bodies`; `highlight.js`: embed dependencies | Publish paths currently cover docs, root Cargo version, and the workflow only | Include actual local snippet and generated-reference inputs in current publication triggers | Existing workflow remains the sole publication path |
| Narrow executable-docs verification | `mise.toml`: `docs:check`, `py:setup`, `build:server`, Python integration task pattern | TASK-001 exact docsite tests; current Postgres wrapper and pytest integration runner | Site checks do not execute Python examples; broad example checks build Rust examples only | Wire a narrow documentation journey lane to the existing Python runner/setup; select it for sample changes | A scoped task wrapper is justified; no replacement harness, release installer, or broad journey suite |
| Incremental authoring authority | `docs/PRODUCT.md`: Product Principles; `docs/CONTRIBUTING.md`: frontmatter/generated-pages guidance | Approved spec; both change packets and existing proposed tasks | Product still requires one shared application; contributors need accurate publication/evidence rules | Update existing guidance and verify supersession metadata already recorded during planning | No new editorial workflow system or deferred task packet |

## Approach

1. Trace final snippet, embed, generator, schema, and release-version inputs
   after TASK-003. Make their existing verification/publication dependencies
   explicit without speculative future inputs.
2. Extend CI selection through the ordered scenario below and add the smallest
   scoped task needed to run TASK-001's exact documentation journey with its
   existing environment owner. Keep server-dependent tests out of the fast
   docs unit lane.
3. Update publication trigger paths and task/workflow wiring. These changes
   are configuration/static proof unless executable selection logic changes.
4. Update PRODUCT and CONTRIBUTING: one completed outcome per guide, YAML
   declarations plus complete local execution snippets, independent optional
   applications, evidence by page class, and consistent draft withdrawal.
5. Verify old packet supersession, preserve historic evidence and retained
   brand/platform commitments, and reconcile the coverage map below against
   actual task evidence. Add no deferred-guide implementation tasks.

## Ordered Implementation Scenarios

### Scenario 1 — Documentation source changes select their required checks

**Behavior.** Changes to the actual local sample and generated-reference
inputs select docs verification, and executable sample changes select the
scoped real-server journey. Package consumer closure and conservative fallback
behavior remain intact (REQ-004/006, INV-004, AC-007).

**RED.** Extend the inspected selector self-tests with representative inputs
from the final sample location and both source roots scanned by
`error_catalog`, including a source outside `wyrd-spec`. Assert the effective
required proof, including when a full-gate fallback already supplies it. Run
the existing executable self-test target and capture missing selection:

```bash
mise exec -- bash .github/scripts/tests/test-detect-changes.sh
```

**GREEN.** Extend the current selector and task wiring to cover the actual
dependencies. Keep unrelated lane selection and fallback semantics intact.
Rerun that command and the scoped journey from TASK-001; do not add the whole
Python integration suite to a docs-only task.

**REFACTOR.** Simplify current source classification and reuse current
environment/test tasks. Do not introduce duplicate orchestration or a new
dependency-inventory service for a few known inputs.

## Acceptance Criteria

- A local snippet change selects both the relevant docs checks and its narrow
  executable journey. Generated-reference source changes under crates and
  SDKs select docs proof even when outside current docs/wyrd-spec patterns.
- Effective publication triggers cover the same source inputs, so deployed
  generated reference and snippets cannot remain stale solely because their
  source lives outside `docs/`.
- PRODUCT and CONTRIBUTING describe independent runnable snippets, evidence
  requirements, withdrawal, and one completed learning slice before the next.
  No shared-app prerequisite or new deferred-guide tasks remain authoritative.
- Old packet statuses and supersession links identify this revision as the
  active documentation scope, preserve history, and retain brand/platform/
  navigation/accessibility commitments. This is supersession, not a claim
  that old task implementation passed review.
- Existing CI fallback, required jobs, source-driven generation, and all
  current docs gates remain effective.

## Expected Write Set and Consumer Closure

`docs/PRODUCT.md`, `docs/CONTRIBUTING.md`; `.github/scripts/select-ci.py` and its
existing tests/wrapper as needed; `.github/workflows/publish-docs.yml` and
`.github/workflows/lints-test.yml` consumers as needed; narrow task wiring in
`mise.toml`; supersession metadata
only if its recorded authority needs correction. The sample's location comes
from TASK-001, not a new parallel source tree. No production Rust/SDK contract
changes, external integration, or site styling is required.

## Verification and Evidence

Selection logic uses TDD above. Product/contributor prose, workflow trigger
configuration, task declarations, and status metadata use static/configuration
inspection plus the existing selector and docs regressions; no manufactured
RED is required for those artifacts. Show representative source-to-check and
source-to-publication mappings, including code outside `docs/`, and inspect
both CI execution and publication entrypoints.

```bash
mise exec -- bash .github/scripts/tests/test-detect-changes.sh
mise run check:ci-selection
mise run docs:check
git diff --check
```

Also execute both exact TASK-001 selectors with its documented setup, through
the newly wired scoped task and directly during iteration as applicable.
Record the resulting narrow task name and command; private wrapper mechanics
remain implementation-owned. Pin formatter/linter use to the changed Python
selector and any Python test/task support; SDK Python edits also run
`mise run py:format` and `mise run py:lints`.

Do not install released artifacts on every credential-free docs unit run.
TASK-001/002 retain distinct acquisition proof. Integrated change review owns
the final aggregate required by AGENTS.md for shared CI/build/test changes;
it does not become an additional implementation task or a repeated task-level gate.

### Obligation ownership across the task packet

| Spec obligation | Owning task(s) | Proof |
|---|---|---|
| REQ-001, AC-001 | TASK-002 onboarding; TASK-003 complete inventory | 32 rows, appropriate evidence, final production state |
| REQ-002, AC-005 | TASK-001/002 first path; TASK-003 retained corpus | Released walkthrough, real SDK/server journey, source/catalog/procedure evidence or withdrawal |
| REQ-003, AC-002 | TASK-001 executable path; TASK-002 teaching | Pass/fail/refusal/rerun, exact source, final released instructions |
| REQ-004, AC-003 | TASK-001 sample; TASK-002 embedding; TASK-003 site independence; TASK-004 CI | Same local source executed and displayed, build without application dependency, scoped CI proof |
| REQ-005, AC-004 | TASK-003 | Omitted-page fixture, production routes/links/nav/search/indexes |
| REQ-006, AC-007 | TASK-004 | Guidance, source-trigger closure, supersession metadata; deferred outcomes only |
| REQ-007, AC-008 | TASK-005 capability; TASK-001/002 released follow-through | Three SDK journeys, temporary/persistent lifetime proof, offline/artifact regression |
| INV-001, AC-006 | TASK-002/003; TASK-004 preserves checks | Existing site, both themes/responsive/keyboard review, docs gate |
| INV-002, INV-003 | All tasks | Only the approved REQ-007 ergonomic extension; vocabulary, auth, identities and audit preserved |
| INV-004 | All tasks | Existing checks retained; generated sources regenerated, not hand-edited |
| INV-005 | All tasks | Later guides only as REQ-012 slices; no all-language/SaaS/template/CLI-selection expansion |

## Material Stop Conditions

Required source coverage needs weakening existing gates or changing package/
release semantics; publication would need an external application checkout;
or scope grows into a replacement CI system. Routine trigger/task wiring and
the current selector's source classification are implementation-owned.

## Authority Links

- [Approved spec revision 9](../spec.md)
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../AGENTS.md)
- [Spec workflow](../../../../changes/spec-driven-development.md)
- [Implementation execution](../../../../changes/implementation-execution.md)
- [Testing workflows](../../../../TESTING.md)
- [Earlier packet](../../developer-docsite-rebuild/spec.md)

## Implementation evidence (spec revision 9)

Capability map revalidated against current source:

- Sample changes already select their checks: `examples/` is in `DOCS`, and the
  `PYTHON`, `TYPESCRIPT`, and `wyrd-rust-examples` owner rules select
  `py:test:integration`, `ts:test:integration`, and `test:examples`, each of
  which runs its language's quickstart journey (`test_docsite_quickstart.py`,
  `examples/tests/quickstart.rs`). Existing self-tests pin this. No narrow
  journey task was added: each lane is the owning language suite, and narrowing
  it would drop journeys that share example Cards (for example
  `test_support_desk.py`), weakening a conservative selection.
- Gap closed: `error_catalog` reads every `crates|sdks/**/src/**/*.rs`, but only
  `wyrd-spec` sources selected docs, so an error added elsewhere left
  `api/errors.md` stale until an unrelated docs change failed its drift check.
- Gap closed: publication fired only on `docs/**`, so changed examples,
  `openapi.yaml`, or error sources did not redeploy.
- `examples/quickstart/stored-verdicts.sql` selects docs only; its journey in
  `test_local_development.py` is `local_dev` and is excluded from CI by design,
  so that proof stays with the released walkthrough (TASK-001/002).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Local snippet change selects docs checks and its journeys | Existing `DOCS`/`PYTHON`/`TYPESCRIPT` rules | `docs example Card runs every language's journeys`, `docs example Rust program runs the examples journeys only` | PASS |
| Generated-reference sources outside `wyrd-spec` select docs proof | `.github/scripts/select-ci.py` `DOCS` | New cases: crate error source, SDK error source (RED: 2 failed; GREEN: 62 passed), Rust test source leaves docs alone | PASS |
| Publication triggers cover the same inputs | `.github/workflows/publish-docs.yml` push paths | Static inspection: `examples/**`, `openapi.yaml`, `crates/**/src/**/*.rs`, `sdks/**/src/**/*.rs` | PASS |
| PRODUCT and CONTRIBUTING describe runnable snippets, evidence, withdrawal, one slice at a time | `docs/PRODUCT.md` principle 3; `docs/CONTRIBUTING.md` "Publishing a page"; `docs/AGENTS.md` rationale and diagram rules (REQ-010/011) | Static inspection | PASS |
| Old packet superseded with history preserved | `developer-docsite-rebuild/spec.md` already `superseded`, linking this packet | Static inspection; no edit needed | PASS |
| Existing fallbacks and gates remain effective | No fallback or gate changed | `test-detect-changes.sh` 62/62, `mise run check:ci-selection` 7/7, `git diff --check` | PASS |

Commands: `mise exec -- bash .github/scripts/tests/test-detect-changes.sh`,
`mise run check:ci-selection`, `git diff --check`. `mise run docs:check` is
deferred to the pre-commit run. Cost: any Rust `src/` change now also runs the
docs job; that is the price of keeping the generated error reference current.

## Review verdict

PASS: manually reviewed and approved by the user on 2026-10-21.
