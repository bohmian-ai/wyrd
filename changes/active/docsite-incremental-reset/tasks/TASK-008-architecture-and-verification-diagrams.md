---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 9
requirements: [REQ-010, REQ-002, INV-001, INV-003, AC-011, AC-005, AC-006]
depends_on: [TASK-007]
parent_task:
remediates: []
review: manual review by the user
---

## Outcome and Value

Teams see how a Wyrd deployment is put together and how verification runs, in
the same style as Bifrost. Today five diagram specs exist and none is on a
published page. After this task, Understand gains an Architecture page with a
team-deployment diagram; How verification works shows its parts, the verify
and submit paths, and the execution lifecycle; Configure the verification
runtime shows the processes and limits; the Bifrost page shows its three
diagrams again.

## What Changes and What Must Stay True

- New diagrams: `wyrd-deployment` (Architecture page), `verification-parts`
  (How verification works), `verification-runtime` (Configure the verification
  runtime).
- Existing diagrams re-embedded after review: `verification-paths`,
  `run-lifecycle`, `bifrost-parts`, `bifrost-pods`, `bifrost-write-read`.
- Each diagram answers its stated question, matches its owning architecture
  section, and has a text alternative. A diagram that no longer matches is
  corrected or left out, never captioned with a warning.
- Removed Card kinds (REQ-008) and server-owner names never appear as client
  APIs; Gate, Scribe, Oracle, and Forge appear only as deployment parts.
- The Architecture page is a concept page: two opening sentences, the diagram,
  one short paragraph per part with the reason it is separate, links out.

## Approach

1. Review each existing diagram spec against `wyrd-design.md`,
   `bifrost-design.md`, and the verification runtime configuration; correct
   what drifted.
2. Add the three new specs and register them.
3. Write the Architecture page; embed diagrams on their pages.
4. Move rationale that the diagrams now carry out of the surrounding prose.
5. Hand the rendered pages to the user for review.

## Proof Strategy

Diagram data and page prose are static content; no new executable behavior,
so no manufactured RED.

- `diagram data` tests in `src/lib/diagrams/diagrams.test.ts` cover every
  registered diagram's ids; the `run lifecycle diagram` tests pin its states to
  the server enums through `WYRD_CONTRACTS`.
- `verification-runtime` limits and process names are checked against
  `crates/wyrd/wyrd-server` configuration and `reference/verification-runtime-limits.md`;
  record the source locations in evidence.
- Accuracy of each diagram: a manual review table in evidence, one row per
  diagram, naming its owning section.

## Acceptance Criteria

| Obligation | Observation | Proof |
| --- | --- | --- |
| REQ-010, AC-011 | All eight diagrams render on their named pages with text alternatives | `docs:test`; `docs:a11y`; review table |
| REQ-002, AC-005 | Every drawn part and limit matches source or doctrine | Review table with source locations |
| INV-003 | No removed kind or server owner presented as a client API | Review table; `rg` over the new specs |
| INV-001, AC-006 | Diagrams legible in both themes and at phone width | Manual check; `docs:a11y` |

## Implementation Reference

| Capability | Existing owner | Gap | Extension |
| --- | --- | --- | --- |
| Component diagrams | `bifrost.ts` `bifrostParts`, `bifrostPods`; `WyrdDiagram.svelte` | No deployment or verification-parts diagram | New `DiagramSpec` entries |
| Sequence and lifecycle | `verification.ts` `verificationPaths`, `runLifecycle` | Not embedded; unreviewed since REQ-008 | Review and embed |
| Embedding | `src/lib/mdsvex/components.js` registers the diagram components | None | Use as is |

## Verification and Evidence

- **Docs unit tests:** `mise run docs:test`.
- **Prose lint:** `~/.claude/skills/humanizer/scripts/lint.sh <page>`.
- **Site gate, once before commit:** `mise run docs:check`.

## Material Stop Conditions

An architecture section and current source disagree about a drawn part (route
the disagreement to its owner, per AGENTS.md authority); a diagram needs a
component the diagram renderer cannot draw.

## Authority Links

- [Approved spec revision 9](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Docs writing rules](../../../../docs/AGENTS.md)
