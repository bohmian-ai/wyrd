---
id: SPEC-docsite-incremental-reset
revision: 9
status: approved
---

# Reset publication and teach one verification journey

## Objective and user value

A developer can understand Wyrd, establish a working environment, and produce
and inspect a passing and failing verification without learning an entire
agent application. Existing readers can still find accurate concepts,
operational procedures, and contract reference.

This change audits publication of the current corpus and finishes the
Introduction, Setup, and Quickstart learning path. Revision 9 adds a visible
coding-agent entry, architecture and verification diagrams, a rule that keeps
design rationale out of guides, and an ordered sequence of later guides that
publish one evidence-backed slice at a time (REQ-009 to REQ-012).
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

The first such fix is the explicitly named `WyrdState.declare(path)`. Given
an authored subject Card YAML file, it resolves the file's references through
the existing loader, declares the Cards (idempotently for unchanged Cards),
and returns a ready state for the exact declared Service root. It composes
the existing declaration, complete graph pull, and hydration owners; it does
not introduce another registry, reference resolver, or artifact loader.
The Quickstart becomes:

```python
from wyrd import WyrdState

state = WyrdState.declare("support-reply.yaml")
run = state.run()
```

`WyrdState.from_path` remains offline bundle loading; it is not overloaded
with declaration or network IO. Declaration is a server write under the
caller's normal authority. The same client is used throughout declaration,
pull, hydration, and subsequent state operations. Normal read/pull and
verification permissions remain necessary; declaration does not grant them.

Default declaration keeps no files: the graph is loaded into memory and the
temporary bundle is deleted before return, including on failure or
cancellation. It never deletes caller-owned files or undoes committed server
declarations. Shutdown keeps its existing runtime meaning.

Python callers can explicitly retain an inspectable bundle with
`WyrdState.declare(path, bundle_dir=".wyrd-bundle")`. Caller-selected bundle
destinations use the existing staged publication/rollback behavior and are
not automatically deleted when state is released. Independent default calls
must not collide or overwrite live state files. Normal explicit-client and
hydration/trust configuration remain available through language-idiomatic
projections. The capability lives in `wyrd-client` and is projected to Python,
Rust, and TypeScript; Python is synchronous, Rust and TypeScript await real IO.

Models and data stay application-owned (REQ-008). A Service with a bound
assertion Verifier judges supplied JSON without Wyrd loading any model.

Same-version edits remain `WYRD_REGISTRY_409_SPEC_DRIFT` when the existing
declaration rules require it. Preserve the catalog's instruction to bump
`metadata.version` or revert the changed spec; do not silently bump, retry
with another version, or wrap away the stable error. This does not alter the
existing behavior for authored versions that are intentionally omitted.

Remove the public `WyrdState.artifacts(alias)` accessor and its public-only
artifact descriptor projections across the three SDKs. Repository callers
only inspect the accessor in tests; no current application workflow or
architecture requirement depends on state exposing a downloaded-file
inventory. Internal Artifact integrity validation remains intact. Tests must continue to prove
payload integrity at their owning boundaries rather than preserve an unused
public accessor for test convenience. Do not add a compatibility alias.
This removes a public SDK surface; unseen downstream callers may require an
API migration. Removing the accessor does not imply that every framework's
loaded model/data object is independent of its source files.

### REQ-008 — Own verification, not models or data

Wyrd owns expectations, evidence, judgment, and reaction. It identifies what
it verifies; it does not package, store, or load models or datasets.

| Kind | Revision 8 |
| --- | --- |
| Model | Reference only: `metadata` name/version plus an optional external `uri` (for example an MLflow model). No artifact upload, framework interface, signature, or loading. |
| Data | Reference only: points at an Artifact Card holding Parquet rows. No interface, splits, or loading. |
| Experiment, Mcp, Policy, Audit | Removed as Card kinds. Authorization stays role-based; audit stays the system table. |
| Prompt, Agent, Service, Artifact, Verifier, Trigger, Operator, Source | Kept. Artifact associates artifacts with Verifiers. |

A drift Verifier's `baseline_ref` points at a Data reference; each Verifier
names its own. The fitter fits from that Data's Parquet. A new baseline is a
new Data version. Baselines that are queries against a Source (run once by the
server at declaration and frozen as Parquet) are a later change, not part of
this revision.

Delete the code that exists only for the removed behavior: `wyrd-interfaces`,
SDK Model/Data hydration and interface overrides, and Model/Data artifact
upload, pull, and trust. Artifact upload and integrity remain. Update the
kind list and positioning wording in `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-design.md`, and
regenerate the derived contracts. Wyrd has no released compatibility promise
for these kinds: stored envelopes of removed kinds and of old Model/Data
shapes are removed by migration, with no compatibility read path or alias.

### REQ-009 — Make the coding-agent path visible

An outside reader looked for "Use with your coding agent" at the top of the
site and found `llms.txt` only in the footer. The site header on every page
and the landing hero link to a Get started page, "Use with your coding agent".
That page holds the `llms.txt` and `llms-full.txt` links and `wyrd mcp install`
with its printed output and a way to confirm the agent sees Wyrd's tools,
proven by a released 0.3.0 walkthrough (REQ-002). Host-specific flags and
remote servers sit in that page's Advanced section. The page replaces
`connect/connect-an-agent-through-mcp.md`. The footer link remains.

### REQ-010 — Show where Wyrd fits and how it works

The same reader looked for an architecture diagram and found none: the
existing diagram specs are no longer embedded on any published page. Diagrams
use the existing diagram specs and components, answer one stated question
each, have a text alternative, and are reviewed against their owning
architecture section before publication.

- The Introduction shows Wyrd in an existing stack: the application and its
  SDK, CI over the CLI, coding agents over MCP, the Wyrd server
  (declarations, verification engine, Bifrost), PostgreSQL, and object
  storage. It replaces the ownership table, and its steps read Declare,
  Observe, Verify.
- A new Understand page, Architecture, shows a team deployment: the
  developer-facing endpoint, server replicas, Bifrost targets, PostgreSQL,
  object storage, the identity provider, the LLM gateway and model provider,
  and the trust boundary.
- How verification works shows the parts of verification and where each
  check kind runs, the verify and submit paths, and the execution lifecycle.
  Configure the verification runtime shows the processes and limits that run
  checks.
- The Bifrost page shows its parts, deployment targets, and the gap between
  written and queryable data.

An existing diagram that no longer matches its owning architecture section is
corrected or left out. It is never published with a caveat.

### REQ-011 — Keep rationale out of guides

Text-heavy guides made setup feel complex. Guides hold steps, code, and
output. Design rationale moves to the owning Understand page, which the guide
links in at most one sentence; detail that changes no reader decision moves
to reference. Published guides are revised to this rule as they are touched,
and contributor guidance states it with the diagram rules of REQ-010.

### REQ-012 — Publish later guides one slice at a time

A Q&A or support-agent builder could not tell what Wyrd does for them; a
model-judged check is the capability that answers it. After the first path,
guides publish one at a time in this order: Author a Verifier (assertion,
Python, and model judge, with the LLM gateway page it requires), Declare a
Service, Respond to a failed verification, Record a Run and observations,
then the remaining drafts. Each slice owns a local example under `examples/`
that extends the Quickstart Service, a journey test that needs no provider
credentials, and REQ-002 evidence before publication. `choose-your-path` is
withdrawn; navigation covers its purpose.

## Invariants, boundaries, and non-goals

- **INV-001:** Retain the approved Wyrd brand, landing visual language,
  SvelteKit/mdsvex/Pagefind/Shiki foundation, responsive behavior, both themes,
  and WCAG 2.2 AA accessibility. No site redesign or platform migration.
- **INV-002:** Tenancy, authorization, and Bifrost behavior are unchanged.
  SDK, CLI, and server ergonomics may change under REQ-007 through their owning
  crates and contracts, never as a documentation-only workaround.
- **INV-003:** Preserve Wyrd vocabulary except as REQ-008 changes it, exact component identities,
  authorization requirements, redaction, and non-blocking audit semantics.
  Do not invent routes, aliases, compatibility contracts, or durable Run state.
- **INV-004:** Generated material changes through its owning source or
  generator. Existing format, link, command, contract, and accessibility gates
  are not weakened to accommodate the reset.
- **INV-005:** Later guides are in scope only as REQ-012 slices. All-language
  tutorial parity, a complete API catalog rewrite, hosted SaaS documentation,
  the Python template, and CLI selection are outside this change's
  completion boundary. The six navigation sections and their depth remain.

## Acceptance and evidence

| ID | Required observable outcome | Credible evidence |
| --- | --- | --- |
| AC-001 | All 32 baseline pages have an evidence-backed disposition and production state consistent with it. | Complete inventory, source review, final content diff. |
| AC-002 | A clean environment follows Setup and the exact Quickstart source through passing and failing results on a real server. | Fresh installation walkthrough plus executable SDK-to-server journey, including refusal with insufficient authority; no provider credentials. |
| AC-003 | The first path is independent of application examples and future CLI features. | Dependency inspection and clean execution without external example checkouts. |
| AC-004 | Production discovery contains only published pages and valid destinations. | Built route, navigation, search, links, and machine-index evidence, including removal of Setup/Quickstart in a test fixture. |
| AC-005 | Retained concepts, references, and operations claims have appropriate source or runtime evidence; unproven pages are withdrawn. | Page disposition evidence, generated drift checks, relevant catalog/procedure proof. |
| AC-006 | Brand, accessibility, and existing site behavior remain intact. | Existing docs tests, keyboard/responsive/both-theme review, `mise run docs:check`. |
| AC-008 | `WyrdState.declare(path)` prepares a ready state from authored YAML in all first-class SDKs; `from_path` remains offline bundle loading. Without `bundle_dir` no files remain after return, failure, or cancellation. Explicit retained bundles work. Reruns preserve existing declaration rules and errors remain actionable. The public `artifacts` accessor is removed. | Rust/Python/TypeScript real-server declaration-to-verification journeys, no-files-left proof, retained-bundle offline regression, export/type checks for accessor removal, and the exact Quickstart journey. Public onboarding switches only after the new API is released and its walkthrough passes. |
| AC-009 | The Card kinds and code match REQ-008 across server, wire contracts, SDKs, CLI, MCP, and doctrine; a drift Verifier fits from its Data reference's Parquet; removed kinds and Model/Data payload fields are refused with stable errors. | Drift baseline journey on a real server, refusal journeys for a removed kind and a Model payload, migration test over stored envelopes, `codegen:check`, `check:deps`, SDK export/type checks, and a repository search showing no removed kind or `wyrd-interfaces` reference outside history. |
| AC-007 | Local snippet and generated-source changes reach docs verification/publication, and the old broad content authority is superseded on approval. | CI/task dependency inspection, updated product/contributor guidance and packet status. |
| AC-010 | Every page's header and the landing hero reach "Use with your coding agent", whose `wyrd mcp install` step works at the published release. | Built-site link evidence, released 0.3.0 walkthrough, existing MCP install/proxy journey. |
| AC-011 | The Introduction, Architecture, How verification works, Configure the verification runtime, and Bifrost pages show the REQ-010 diagrams, each matching its owning architecture section. | Diagram review against `wyrd-design.md` and `bifrost-design.md`, accessibility check of the text alternatives, `mise run docs:check`. |
| AC-012 | Published guides carry steps, code, and output; their rationale lives on Understand pages. | Page review against `docs/AGENTS.md`, including the updated rationale and diagram rules. |
| AC-013 | Each REQ-012 slice publishes only with its local example and journey test passing; the model-judged check runs without provider credentials in tests. | The slice's narrow journey lane and its released walkthrough. |

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

None. Revision 9 was approved with "i agree" after the user reviewed the
coding-agent entry, the diagram set, the rationale rule, and the guide order.
Revision 8 was directed by the user with “strip the part out now,
revise the spec”, after agreeing the REQ-008 boundary in conversation. Revision 7 was approved with “agree. update the task”: state owns its
runtime resources, staging is deleted before return, required backing files
have a separate owned lifetime, and callers never manage private directories.
Revision 6 records the user's direction to delete temporary staging
after loading when it is no longer needed and remove the artifact accessor
if it lacks an actual use case. Repository research found no application
consumer or design requirement for that public accessor. Revision 5 records
the user's explicit correction to `declare`, agreement
to managed temporary storage with optional `bundle_dir`, and “ok continue”
after clarifying the boundary for application-owned models. Revision 2 was
approved on 2026-10-20 with “i agree. write the tasks”.
Exact artifact versions and page-level
evidence are implementation validation obligations, not permission to change
the release-only onboarding requirement; lack of compatible published
artifacts blocks publication and must be reported.

## Revision history and authority

- **Revision 9 — 2026-10-21 — approved by the user:** Add REQ-009 to REQ-012
  and AC-010 to AC-013 from outside-reader feedback: a header and hero entry
  for coding agents, stack, architecture, verification, and Bifrost diagrams,
  rationale moved from guides to Understand, and later guides published one
  slice at a time, starting with Author a Verifier. Amend INV-005.
- **Revision 8 — 2026-10-20 — directed by the user:** Add REQ-008 and AC-009:
  Model and Data become references, Experiment/Mcp/Policy/Audit Card kinds are
  removed, drift baselines are Data references to Parquet Artifacts, and
  Model/Data packaging code is deleted. `declare` keeps no files without
  `bundle_dir`; revision 7's state-owned payload storage is withdrawn.
  Source-query baselines are deferred to a later change.
- **Revision 7 — 2026-10-20 — approved by the user:** Separate disposable
  staging from state-owned runtime backing files. Always remove staging before
  returning a ready state, retain required payloads through dependent SDK
  lifetimes, avoid forced RAM materialization, and keep persistent bundle
  output explicit. No new public storage-management API or cache.
- **Revision 6 — 2026-10-20 — directed by the user:** Clean temporary staging
  immediately after loading when file dependencies are absent; retain only
  files still needed. Remove the public state artifact inventory, whose
  repository uses are test inspection, while preserving internal artifact
  integrity/trust/loading behavior and documenting the SDK surface removal.
- **Revision 5 — 2026-10-20 — directed and approved by the user:** Replace
  revision 4's overloaded `from_path` with `WyrdState.declare`; preserve offline
  loading. Use temporary hydration storage by default and opt-in retained
  bundles, share the capability across all SDKs, preserve error/artifact trust
  semantics, and keep external model ownership independent of verification.
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
- [Agent rules](../../../AGENTS.md)
- [Protocol authority](../../../architecture/wyrd-design.md)
- [Public doctrine](../../../architecture/wyrd-design.md)
- [Bifrost authority for retained explanations](../../../architecture/bifrost-design.md)
- [Security authority](../../../architecture/wyrd-security-posture.md)
- [Spec workflow](../../../changes/spec-driven-development.md)
- [Testing workflow](../../../TESTING.md)
- [Current docsite product guidance](../../../docs/PRODUCT.md)
- [Current contributor guidance](../../../docs/CONTRIBUTING.md)
- [Earlier approved rebuild](../developer-docsite-rebuild/spec.md)
