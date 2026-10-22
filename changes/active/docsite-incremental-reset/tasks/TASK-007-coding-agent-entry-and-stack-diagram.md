---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 9
requirements: [REQ-009, REQ-010, REQ-011, INV-001, INV-005, AC-010, AC-011, AC-006]
depends_on: [TASK-004]
parent_task:
remediates: []
review: manual review by the user
---

## Outcome and Value

A reader who arrives with a coding agent finds the way in at the top of every
page, and a reader who wants to know where Wyrd sits sees it in one picture on
the Introduction.

Before: `llms.txt` is a footer link; `wyrd mcp install` is on a withdrawn
draft; the Introduction has an ownership table and lists Declare, Verify, Read
the verdicts. After: the header and the landing hero link to "Use with your
coding agent"; that Get started page holds both indexes and the install
command with its output; the Introduction shows the stack diagram and reads
Declare, Observe, Verify.

## What Changes and What Must Stay True

- New Get started page "Use with your coding agent" replaces
  `connect/connect-an-agent-through-mcp.md`, which is deleted once the new page
  carries its supported content. Host flags and remote servers go in its
  Advanced section; everything else in the old draft goes to reference or is
  removed.
- The Introduction's first paragraph speaks to someone shipping a support or
  Q&A agent. The stack diagram replaces the ownership table.
- `choose-your-path.md` is deleted (REQ-012); no link points at it.
- The footer `llms.txt` link stays. Brand, both themes, responsive header, and
  WCAG 2.2 AA stay (INV-001). No new components.
- Published links resolve only to published pages; every shell link test keeps
  passing.

## Approach

1. Add the header link and the hero button through the existing shell and
   landing markup, resolving the target from published content like the
   existing `start` link.
2. Write the new page from the old draft, keeping only the install, confirm,
   and Advanced material its walkthrough proves.
3. Add a `wyrd-in-your-stack` diagram spec to `src/lib/diagrams/` and register
   it in `DIAGRAMS`; embed it on the Introduction with a text alternative.
4. Rewrite the Introduction's opening and steps; delete `choose-your-path.md`
   and the old MCP draft; update the disposition record.
5. Lint the prose; hand the rendered pages to the user for review.

## Ordered Implementation Scenarios

### Scenario 1 — Every page reaches the coding-agent page

**Behavior.** The home shell and the doc shell header both link to the "Use
with your coding agent" page, and the landing hero shows it as a secondary
action. If the page is unpublished, neither link renders, so no shell link
points at a missing page.

**RED.** Extend `src/routes/shell-links.test.ts` with an assertion that both
shell headers and the landing link to the page's published path, and that
the publication-closure fixture without the page renders no such link. It
fails because no link exists. Command: [docs unit tests](#verification-and-evidence).

**GREEN.** Add the links through the existing published-entry lookup. Rerun
the whole shell-links file.

**REFACTOR.** Reuse the existing `start`-style lookup rather than a second
resolver.

## Proof Strategy for the remaining obligations

- **Diagram:** the existing `diagram data` test in
  `src/lib/diagrams/diagrams.test.ts` runs over every registered diagram and
  fails on dangling ids. Accuracy is a manual review against the
  `architecture/wyrd-design.md` client/server and deployment sections.
- **`wyrd mcp install`:** the command and its printed output match the
  existing `mcp_journey.rs` coverage and a 0.3.0 walkthrough on macOS arm64,
  recorded in evidence. Output shown on the page is captured from that run.
- **Prose:** docs/AGENTS.md page shapes (Landing for the Introduction, Install
  shape for the agent page), the humanizer lint, and the user's read-through.

## Acceptance Criteria

| Obligation | Observation | Proof |
| --- | --- | --- |
| REQ-009, AC-010 | Header and hero reach the page; the page shows both indexes and `wyrd mcp install` with real output; removing the page removes the links | Scenario 1; walkthrough evidence; built-site links |
| REQ-010, AC-011 | Introduction shows the stack diagram with every named part and a text alternative; steps read Declare, Observe, Verify | `diagram data` test; manual architecture review; a11y check |
| REQ-012 | `choose-your-path` and the old MCP draft are gone; no link breaks | `docs:linkcheck`; shell-links tests |
| INV-001, AC-006 | Header works at phone width in both themes; a11y passes | `docs:a11y`; manual check |

## Implementation Reference

| Capability | Existing owner | Gap | Extension |
| --- | --- | --- | --- |
| Header and footer links | `src/routes/+layout.svelte` (`start`, `reference` lookups, footer nav) | No coding-agent link | One more published-entry link in the header |
| Landing actions | `src/routes/+page.svelte` hero `.cta` | One primary, one GitHub button | A secondary button to the new page |
| Diagrams | `src/lib/diagrams/index.ts` `DIAGRAMS`, `WyrdDiagram.svelte`, `types.ts` `DiagramSpec` | No stack diagram | One new `DiagramSpec` |
| MCP install proof | `crates/wyrd/wyrd-cli/tests/mcp_journey.rs` `mcp_proxy_discovers_and_reads_with_shared_auth` | Acquisition at the release unproven | Released walkthrough evidence |
| Indexes | `docs/scripts/generate_llms_txt.py` | None | Regenerated by `docs:generate` |

## Verification and Evidence

- **Docs unit tests** (shell links, diagram data): `mise run docs:test`.
- **MCP journey, if its owner's setup allows a focused run:**
  `mise exec -- cargo nextest run --locked -p wyrd-cli --test mcp_journey`;
  confirm the target and Postgres setup from the crate before relying on it.
- **Prose lint:** `~/.claude/skills/humanizer/scripts/lint.sh <page>` per changed page.
- **Site gate, once before commit:** `mise run docs:check`.

## Material Stop Conditions

`wyrd mcp install` at 0.3.0 does not behave as the old draft states; the header
cannot fit the link at phone width without a shell redesign; the diagram needs
a part the architecture documents do not define.

## Authority Links

- [Approved spec revision 9](../spec.md)
- [Docs writing rules](../../../../docs/AGENTS.md)
- [Repository standards](../../../../AGENTS.md)
