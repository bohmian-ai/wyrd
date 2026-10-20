---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-006]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Outcome and Value

Introduction → Setup → Quickstart is a complete, short learning path: understand
Wyrd, start it once, and run the proven two-file YAML/Python example. The pages
teach the actual released workflow established by TASK-001, with complete code
and visible results rather than excerpts from an application.

## Owners, Scope, Consumers, and Prohibited Changes

Use `$wyrd-implement` and the repository's technical writing guidance. Own the
three onboarding pages and their direct source/text/index consumers. Reuse
the site's existing components and approved visual language. No site redesign,
new components solely for this tutorial, SDK feature, all-language walkthrough,
model gateway, app server, or later guide is included.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Teach the first path | `docs/src/content/docs/get-started/{introduction,setup,quickstart}.md` | Current Setup's release mismatch; Quickstart application embeds; TASK-001 source/runtime evidence | Three pages do not teach a minimal proven released path | Rewrite only these pages from the executable sample and installation evidence | None; existing authoring components suffice |
| One displayed and executed source | `docs/src/lib/mdsvex/highlight.js`: `embedCodeFromFile`; `page-text.js`: `pageText` | `docs/src/lib/mdsvex/page-text.test.ts`; `docs/scripts/page_text.mjs` | Text assertions currently require the old app and multi-language lifecycle | Embed the complete new files and update superseded onboarding assertions | No new code loader or duplicated snippet store |
| Agent-facing entrance agrees | `docs/scripts/generate_llms_txt.py`: `START`, `render_start`, `render_llms_full` | `docs/src/lib/indexes.test.ts` | Start descriptions still promise the support-desk application | Update editorial descriptions and generated output to the small journey | Missing-page behavior stays with TASK-003 |

## Approach

1. Use TASK-001's actual versions, installation evidence, and local source;
   remove the stale release mismatch without replacing it with an assumption.
2. Rewrite Introduction around verification, assurance, declared criteria,
   exact identities, and Wyrd's boundary with the application's runtime.
3. Rewrite Setup around one released acquisition/startup path, external
   PostgreSQL, saved connection confirmation, and accurate stop/persistence/
   reset guidance. Clearly distinguish the persistent server terminal from
   the terminal running snippets.
4. Replace Quickstart with both files shown completely, the command to run
   Python, expected pass/fail output, and the meaningful refusal/recovery
   explanation. Explain YAML's reusable declarations and the supplied-evidence
   boundary without requiring users to learn a full application.
5. Update direct text assertions, start descriptions, generated indexes, and
   the three onboarding rows of the disposition record TASK-003 completes.

## Proof Strategy

TDD is not applicable to prose, editorial constants, generated output, and
updates to assertions tied to superseded prose. TASK-001 owns the executable
sample's behavioral tests; this task must not fork that implementation.

Prove displayed source parity through the existing embed/page-text path and
repeat the exact published instructions in a clean released environment. If a
necessary behavioral defect is discovered, route it to its existing owner and
use that behavior's Red-Green-Refactor discipline rather than hiding it in
static text checks. Publication/discovery logic changes remain TASK-003.

## Acceptance Criteria

- Each page completes its own purpose and leads to the next useful action.
  The first path is Introduction → Setup → Quickstart.
- YAML and Python are complete, copyable, and identical to TASK-001's tested
  source. No external app checkout, missing imports, manual bundle preparation,
  separate deployment script, or future CLI behavior is required.
- Setup exposes PostgreSQL and server prerequisites and uses one exact,
  compatible released set; stop, persistence, reset, and connection claims
  match execution evidence.
- Quickstart shows exact subject and Verifier identities, verdicts and reasons;
  failed judgment differs from refusal. It makes no unexecuted application or
  immediate retention/query-completeness claim.
- Onboarding disposition rows name evidence and resulting publication state.
  An unproven procedure is withheld, not labeled runnable.
- Existing brand, responsive behavior, both themes, accessibility, copy
  controls, and machine-readable content remain supported.

## Expected Write Set and Consumer Closure

The three onboarding pages; onboarding-specific assertions in
`docs/src/lib/mdsvex/page-text.test.ts` and `docs/src/lib/indexes.test.ts`;
editorial machine-index descriptions; generator-owned outputs; onboarding rows
in the shared page disposition record. Retain useful unrelated application
tests. Paths identify consumers, not a private implementation allowlist.

## Verification and Evidence

These are existing test targets and selectors confirmed from their current
Vitest source/configuration. Update their assertions to the approved tutorial:

```bash
mise run docs:install
mise exec -- pnpm --dir docs exec vitest run src/lib/mdsvex/page-text.test.ts -t '^pageText renders the quickstart with its embedded source bodies and no component markup$'
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts -t '^machine-readable indexes follow published pages both indexes open with the canonical start route$'
```

Generate indexes before checking their new content; regenerate through
`mise run docs:generate`, never by editing output files. Run both exact
TASK-001 journey selectors from its Verification section after any sample
edit, using its setup. Record a clean walkthrough of the final rendered
instructions against released bytes; prior runtime evidence is reusable only
for unchanged instructions, versions, and source.

```bash
mise run docs:svelte:check
mise run docs:check
git diff --check
```

The docs aggregate includes generation/drift, unit, command, link, build, and
accessibility checks. Its generated-output check expects reviewed generated
changes to be committed through the caller's ordinary authorized workflow;
record any resulting pending commit prerequisite, never bypass drift checks.
Review keyboard navigation, copy controls, narrow/wide layouts, and both
themes. Static text checks and a green build cannot replace the walkthrough.

## Material Stop Conditions

TASK-001 cannot prove the released journey; a page needs a prohibited product
change or unsupported CLI option; or an editorial change would alter the
agreed evidence model. Correcting prose, embed location, or sample filenames
within the approved experience is implementation-owned.

## Authority Links

- [Approved spec revision 2](../spec.md) and [executable prerequisite](TASK-001-minimal-local-verification.md)
- [Repository standards](../../../../AGENTS.md) and [agent rules](../../../../architecture/agent-rules.md)
- [Protocol](../../../../architecture/wyrd-design.md) and [doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Spec workflow](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
