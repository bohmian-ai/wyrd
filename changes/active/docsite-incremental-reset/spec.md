---
id: SPEC-docsite-incremental-reset
revision: 4
status: approved
---

# Reset publication and teach one verification journey

## Objective and user value

A developer can understand Wyrd, establish a working environment, and produce
and inspect a passing and failing verification without learning an entire
agent application. Existing readers can still find accurate concepts,
operational procedures, and contract reference.

This change audits publication of the current corpus and finishes only the
Introduction, Setup, and Quickstart learning path. Later guides are separate
bounded changes, not unfinished acceptance obligations of this specification.
Documentation uses small, independently executable examples owned in Wyrd.
The external example repositories are optional supplemental applications.

## Authority and relationship to other changes

With the user's approval, this specification supersedes the documentation scope
and remaining content tasks of `SPEC-developer-docsite-rebuild` revision 2.
Its existing brand, site platform, navigation structure, and accessibility
commitments remain. Approval must be reflected in the older packet before
implementation proceeds; historical approval and implementation evidence remain
in the older packet.

Update `docs/PRODUCT.md` and contributor guidance within implementation to
replace the requirement to reuse one system example everywhere with small
task-specific snippets and independent supplemental applications. This is an
explicit editorial change, not a change to Wyrd's protocol or doctrine.

The Python reference application and CLI server selection belong to the
separate `SPEC-python-reference-template`. Neither its implementation nor
access to an external example checkout is a prerequisite of this change.

## Current evidence

Baseline: Wyrd `312d4028b`. The content tree has 32 pages, none draft or
planned. Setup acknowledges a published 0.1.1 / documented 0.2.0 mismatch;
Quickstart combines default-branch source with an unpinned package and embeds
the full support-desk application. These are source observations, not fresh
package-registry or runtime verification.

Existing capabilities to retain include frontmatter-derived navigation,
production draft exclusion, source-backed `CodeFromFile` snippets, generated
reference, and the documentation checks. The machine-readable index generator
still assumes Setup and Quickstart exist. The Python support-desk integration
test proves useful runtime behavior but does not prove the public installation
instructions. Compilation and typechecking alone do not prove a tutorial.

## Scope and required behavior

### REQ-001 — Decide publication for every existing page

Record one disposition for each baseline page: retain, rewrite, consolidate,
or withdraw. Each disposition names its reason, evidence, and resulting
publication state. Retain useful source as drafts while a replacement is
pending; remove obsolete or redundant prose once it has a verified replacement.
Do not restore historical deleted article sets.

The following table fixes editorial scope. Paths are relative to
`docs/src/content/docs/`. Retention is conditional on the evidence required by
REQ-002; this table is not an accuracy certification.

| Existing page | Disposition in this change |
| --- | --- |
| `get-started/introduction.md` | Rewrite now: purpose, boundaries, first action. |
| `get-started/setup.md` | Rewrite now: one coherent acquisition and connection path. |
| `get-started/quickstart.md` | Replace now: one small deterministic verification journey. |
| `build-and-verify/declare/define-a-service.md` | Preserve as draft input to a later consolidated declaration guide. |
| `build-and-verify/declare/bind-a-verifier.md` | Preserve as draft input to the same later guide. |
| `build-and-verify/declare/declare-and-inspect.md` | Preserve as draft input to the same later guide. |
| `build-and-verify/observe/record-a-run-and-observations.md` | Retain if verified; otherwise draft for its own focused rewrite. |
| `build-and-verify/observe/write-and-query-custom-data-with-bifrost.md` | Draft for a later independent guide. |
| `build-and-verify/verify/author-a-verifier.md` | Retain if verified and focused; otherwise draft for an assertion-first guide. |
| `build-and-verify/verify/verify-a-request-in-real-time.md` | Draft for a later application-lifecycle guide. |
| `build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md` | Reuse the smallest useful material in Quickstart; draft the remainder pending a distinct later guide. |
| `build-and-verify/verify/monitor-drift.md` | Draft for a later independent guide. |
| `build-and-verify/verify/respond-to-a-failed-verification.md` | Draft for a later independent guide. |
| `connect/set-up-the-llm-gateway.md` | Retain if verified; otherwise draft. |
| `connect/call-wyrd-from-a-stock-client.md` | Retain if verified; otherwise draft. |
| `connect/connect-an-agent-through-mcp.md` | Retain if verified; otherwise draft. |
| `operate/deploy-wyrd-for-a-team.md` | Retain the verified supported procedure with explicit limits; otherwise draft. |
| `operate/grant-and-inspect-permissions.md` | Retain if verified; otherwise draft. |
| `operate/troubleshoot-a-deployment.md` | Retain accurate recovery guidance with its actual scope; otherwise draft. |
| `operate/configure-verification-runtime.md` | Retain if source-backed; otherwise draft. |
| `understand/cards-versions-and-relationships.md` | Retain after doctrine review and bounded trimming. |
| `understand/runs-observations-and-evidence.md` | Retain after doctrine review and bounded trimming. |
| `understand/how-verification-works.md` | Retain after doctrine review; trim procedural and reference inventories. |
| `understand/bifrost.md` | Retain after review against the Bifrost authority. |
| `understand/identity-and-authorization.md` | Retain after review against security and protocol authority. |
| `reference/verifier-card.md` | Retain if it matches owning contracts; otherwise draft. |
| `reference/extraction-output.md` | Retain if it matches owning contracts; otherwise draft. |
| `reference/agent-tools-and-authorization.md` | Retain if it matches owning contracts; otherwise draft. |
| `reference/verification-runtime-limits.md` | Retain if it matches owning configuration; otherwise draft. |
| `reference/mcp-tools.md` | Retain if it matches the runtime catalog; otherwise draft. |
| `api/errors.md` | Retain source-generated reference, verified through its generator. |
| `api/openapi.md` | Retain the accurate entry to the served OpenAPI contract. |

Accuracy review may make bounded corrections, remove stale claims, or draft a
page. Discovering a substantial rewrite does not expand this change: record
the follow-up outcome and withdraw the page until separately completed.

### REQ-002 — Earn publication with the appropriate evidence

Published procedures have complete prerequisites, supported commands, expected
results, and the meaningful failure/recovery behavior for their task. Their
actual instructions and source snippets are exercised in the declared
environment. Existing relevant journey evidence may be reused only when it
covers those instructions and versions; uncovered acquisition steps still
need proof. A full procedure cannot be certified by regex, compilation, or
typechecking alone.

Concepts are reviewed against doctrine; reference against owning schemas,
APIs, configuration, or the runtime catalog; operational procedures against
their supported deployment and recovery behavior. Unknown accuracy causes
withdrawal, not a claim of verification. A partial page may remain published
only when its supported portion completes a useful task and its limits are
explicit at the point of use.

### REQ-003 — Finish the first three pages

Introduction explains verification and assurance, the declared-component to
judgment relationship, and the boundary between Wyrd and application runtime.
It leads directly to Setup and Quickstart without an inventory of every kind.

Setup supplies one exact, compatible, available released SDK/CLI/server set,
external PostgreSQL prerequisites, connection confirmation, and accurate stop,
persistence, and reset guidance. It does not require the external example
repository, provider credentials, or a locally built testing-enabled wheel.
Do not silently mix default-branch sources, published packages, or future CLI
options. Verify artifact availability during implementation; if no released
set supports the current required contract, report that as a publication
blocker rather than publishing an impossible stable installation path.

Quickstart uses the minimum declarations needed to bind one assertion-based
Verifier to an exact subject, supplies explicit JSON evidence, executes a
passing and a failing verification, and inspects the result's subject,
Verifier identity, verdict, and reason. A failed verdict is a successful
verification response, distinct from a refused or broken request. Explain
that supplied evidence demonstrates judging declared criteria; it does not
claim to have measured a model or application that was never executed.

The tutorial uses YAML for declarations and Python for execution. Its
authored files are one YAML file per Card (the Verifier, and the subject that
references it by relative path) and one complete Python script with all
imports. After starting
the server once, a reader runs that script to declare, load through the
existing SDK, and verify the supplied evidence; no separate deployment script
or manual bundle preparation is required. Repeating the script works without
manual cleanup. Generated local bundle files are explained when relevant and
are not additional authored prerequisites. Keep PostgreSQL and the running
server explicit rather than hiding them in the snippet.

The example requires no model, LLM gateway, app server, browser UI, background
judge, scheduled verification, or custom analytical table. Inspection of the returned
result is sufficient; asynchronous retention is not represented as an immediate
query-completeness guarantee. Add other language walkthroughs separately when
their exact runtime paths are proven; do not remove existing SDK capabilities.

### REQ-004 — Own small executable documentation examples locally

The Wyrd repository owns the tutorial's minimal runnable source and its tests.
Displayed code and tested code have one source wherever practical, extending
the existing embedding and test mechanisms. Samples are independently
understandable with stated prerequisites; readers need not reconstruct a
sample from multiple distant excerpts of an application.

An external example may be linked as further reading but is never fetched,
built, mirrored, synchronized, or required to render, test, or follow the
docsite. Removing obsolete documentation dependencies does not authorize
deleting useful application regression coverage.

### REQ-005 — Make withdrawal consistent across every discovery surface

Withdrawn material is absent from production routes, navigation, landing
actions, search results, related links, `llms.txt`, and `llms-full.txt`.
Published links resolve to published content. `planned` alone is insufficient
because it currently leaves the page reachable and searchable.

Generation and build tolerate omitted onboarding or reference pages without
hardcoded missing-page assumptions. Empty navigation sections stay hidden.
Retain the six existing sections and current navigation depth; the first
learning path remains the prominent entrance. No empty placeholders are added
to advertise deferred coverage.

### REQ-006 — Keep the editorial workflow incremental

Contributor and product guidance establish one outcome per guide, explicit
evidence before publication, and one completed learning slice before another
is added. Preserve a concise disposition record for deferred subjects without
creating implementation tasks for them under this specification.

Documentation source dependencies introduced or changed here participate in
the relevant documentation checks and publication triggers. A source-backed
snippet or generated-reference change must not leave deployed content stale
merely because its source lies outside `docs/`.

### REQ-007 — Fix ergonomic gaps the first journey exposes

Rebuilding the docs is also an ergonomics review. When a tutorial needs
ceremony a developer should not have to write, fix the owning SDK or server
surface in this change rather than documenting the workaround.

The first such fix: `WyrdState.from_path` accepts an authored Card YAML file
as well as a pulled bundle. Given a Card file, it loads and resolves the file's
reference graph locally, declares it (idempotently for unchanged Cards), and
returns a hydrated state for the declared root, so the Quickstart is:

```python
from wyrd import WyrdState

state = WyrdState.from_path("support-reply.yaml")
run = state.run()
```

Bundle loading keeps its current offline behavior. Declaration from a Card
file is a server write under the caller's normal declaration authority, and
the public documentation says so. The capability lives in the shared Rust
client owner and is projected to the Python, Rust, and TypeScript SDKs.

## Invariants, boundaries, and non-goals

- **INV-001:** Retain the approved Wyrd brand, landing visual language,
  SvelteKit/mdsvex/Pagefind/Shiki foundation, responsive behavior, both themes,
  and WCAG 2.2 AA accessibility. No site redesign or platform migration.
- **INV-002:** Tenancy, authorization, and Bifrost behavior are unchanged.
  SDK, CLI, and server ergonomics may change under REQ-007 through their owning
  crates and contracts, never as a documentation-only workaround.
- **INV-003:** Preserve Wyrd vocabulary, exact component identities,
  authorization requirements, redaction, and non-blocking audit semantics.
  Do not invent routes, aliases, compatibility contracts, or durable Run state.
- **INV-004:** Generated material changes through its owning source or
  generator. Existing format, link, command, contract, and accessibility gates
  are not weakened to accommodate the reset.
- **INV-005:** Later guides, all-language tutorial expansion, a complete API
  catalog rewrite, hosted SaaS documentation, the Python template, and CLI
  selection are outside this change's completion boundary.

## Acceptance and evidence

| ID | Required observable outcome | Credible evidence |
| --- | --- | --- |
| AC-001 | All 32 baseline pages have an evidence-backed disposition and production state consistent with it. | Complete inventory, source review, final content diff. |
| AC-002 | A clean environment follows Setup and the exact Quickstart source through passing and failing results on a real server. | Fresh installation walkthrough plus executable SDK-to-server journey, including refusal with insufficient authority; no provider credentials. |
| AC-003 | The first path is independent of application examples and future CLI features. | Dependency inspection and clean execution without external example checkouts. |
| AC-004 | Production discovery contains only published pages and valid destinations. | Built route, navigation, search, links, and machine-index evidence, including removal of Setup/Quickstart in a test fixture. |
| AC-005 | Retained concepts, references, and operations claims have appropriate source or runtime evidence; unproven pages are withdrawn. | Page disposition evidence, generated drift checks, relevant catalog/procedure proof. |
| AC-006 | Brand, accessibility, and existing site behavior remain intact. | Existing docs tests, keyboard/responsive/both-theme review, `mise run docs:check`. |
| AC-008 | The Quickstart script hydrates its state directly from the authored subject YAML, and bundle loading is unchanged. | SDK journeys for Card-file and bundle loading in each first-class SDK; the Quickstart journey and released walkthrough using the new form once released. |
| AC-007 | Local snippet and generated-source changes reach docs verification/publication, and the old broad content authority is superseded on approval. | CI/task dependency inspection, updated product/contributor guidance and packet status. |

Use the narrowest existing journey lanes for changed executable examples and
`mise run docs:check` for site verification. Planning identifies exact focused
commands from the selected source/tests; this spec does not invent test names.
A green site build alone does not close AC-002. REQ-007 changes follow the
repository's normal journey and contract gates, and no unrelated repository-wide gate is required merely to draft
or approve this specification.

## Expensive-to-reverse decisions

Documentation and full application examples have independent source ownership
and release responsibilities. Existing navigation and visual authority stay.
The first learning journey is Python and deterministic; other first-class
languages remain supported without unproven tutorial parity. Unverified pages
lose publication rather than gaining warning labels while still looking like
ordinary runnable guides. Exact snippet locations and private test mechanics
remain implementation choices.

## Open material decisions

None. Revision 2 was approved by the user on 2026-10-20 with “i agree. write
the tasks” after agreeing to YAML declarations and a complete Python script.
Exact artifact versions and page-level
evidence are implementation validation obligations, not permission to change
the release-only onboarding requirement; lack of compatible published
artifacts blocks publication and must be reported.

## Revision history and authority

- **Revision 4 — 2026-10-20 — directed by the user:** Nothing found while
  rebuilding the docs is out of scope; add REQ-007 and AC-008 so
  `WyrdState.from_path` also accepts an authored Card YAML file, and relax
  INV-002 to allow owner-crate ergonomic fixes.
- **Revision 3 — 2026-10-20 — directed by the user:** Author the subject and
  Verifier as two YAML files joined by a relative path reference instead of
  one multi-document file, whose only working link is the loader-internal
  `sibling` form.
- **Revision 2 — 2026-10-20 — approved by the user:** Record the agreed
  developer experience: start the server once, then run a complete local
  YAML-and-Python example, including declaration and a clean rerun. The user
  requested the four-task packet on this basis. No SDK or server changes are
  authorized.
- **Revision 1 — 2026-10-20 — draft:** Capture the requested independent,
  incremental docsite reset, full page disposition, and first small journey.

- [Repository standards](../../../AGENTS.md)
- [Agent rules](../../../architecture/agent-rules.md)
- [Protocol authority](../../../architecture/wyrd-design.md)
- [Public doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Bifrost authority for retained explanations](../../../architecture/bifrost-design.md)
- [Security authority](../../../architecture/wyrd-security-posture.md)
- [Spec workflow](../../../architecture/references/languages/spec-driven-development.md)
- [Testing workflow](../../../architecture/references/languages/testing-workflows.md)
- [Current docsite product guidance](../../../docs/PRODUCT.md)
- [Current contributor guidance](../../../docs/CONTRIBUTING.md)
- [Earlier approved rebuild](../developer-docsite-rebuild/spec.md)
