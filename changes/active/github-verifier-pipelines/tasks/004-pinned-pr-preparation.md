---
id: TASK-004
status: proposed
approved_spec_revision: 4
requirements: [REQ-006, REQ-007]
acceptance: [AC-004]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-007]
depends_on: [TASK-002]
---

# Prepare pinned PR comparisons through the independent pipeline

Implement the library/adapter portion of **approved-code-review-support.md →
Library output and retained evidence / Successive commits and pipeline ownership**.
This is source preparation for task 005, not a new Wyrd review orchestrator.

## Actual input and missing behavior

Current `bohmian-code-review-pipeline/src/review.rs::ReviewPipeline::context`
accepts `(GitHubClient, Identity, pr_number)` and returns PrContextResult
(record/lifetime/ReviewContext). It cold-builds only when no snapshot pointer
exists, otherwise reuses the snapshot; `PrContext::run` overlays source, ranks
and packs context. It does not accept Wyrd's frozen base/head and comparison
baseline or return both complete/active diffs and retained validation source.
Its `ObjectStorage`, `SnapshotStore` and `ContextStore` protocols include scoped
identity, create-only objects, conditional pointer publication and collection/
expiry. Reusing only the ranker would duplicate its complete acquisition workflow.

Extend the public **ReviewPipeline** facade and its owned generic provider/
comparison stages to accept frozen repository/PR base/head identities, including
fork head repository identity, plus an optional previous-reviewed head. The
caller supplies those pins and baseline; the library verifies/acquires objects,
computes PR merge-base→head and baseline→head comparisons and selects supporting
context for the active comparison. It must not decide Wyrd policy/baseline,
findings, verdict, tenant association or GitHub delivery. Keep existing context
and refresh workflows and cache/version rules rather than adding another index.
The library may report unusable comparison history so Wyrd can select the
approved full-review fallback; it must not silently compare another source.

The library output supplies the contract's changed-file/hunk structure, exact
before/after blob source or explicit binary/unavailable/excluded identity, pinned
metadata, separate complete/active comparisons and **unchanged ReviewContext**.
Verify pagination/truncation and reconstruct required absent source from verified
objects or fail; context-selection omissions remain honest coverage data. Empty
active comparison is valid source evidence, not a verdict.

Implement Wyrd's storage/persistence adapters over existing storage and canonical
Bifrost/Scribe owners, preserving each actual trait's conditional publication,
identity, cleanup/expiry and unavailable-versus-absent behavior. Task 002 provides
verified connection and short-lived installation-token access. These adapters
must not put payloads in PostgreSQL or invent fake execution IDs for cache data.
No new PostgreSQL tables or cache-coordination service is authorized. Cache reuse
is a library concern; accepted execution evidence custody is task 005's concern.
Amend the applicable **wyrd-design.md → Verifier** boundary and
**bifrost-design.md → Table and row identity / Durability and visibility / Ingest**
only as needed to state adapter custody without making library cache canonical.

## Dependency delivery and consumer

Library work belongs to the independent repository and the user-approved
`wyrd-production` branch only. Do not resurrect either deleted historical App
branch/commit or infer an approved old SHA. Revalidate the branch and deliver a
reviewed committed/published library revision carrying the generic facade; the
exact future revision is implementation-owned. If unavailable, report that
external prerequisite explicitly; a fabricated pin or Wyrd-side substitute does
not satisfy this task. Wyrd App signing/callback/connection policy stays in Wyrd.

Output is that reviewed facade plus Wyrd-compatible adapters and a resolved
normal dependency revision. Task 005 invokes it using task 006's accepted source
pins and Wyrd's selected baseline, then retains the returned evidence. Library
publication ACKs satisfy its own store protocols, not Wyrd execution acceptance.

## Proof

**AC-004 (acquisition portion):** Exercise the actual extended public pipeline
cold and warm against its routed provider transport, including fork pins,
full/incremental comparisons, moved live PR, rewritten/unavailable history,
renames/deletions, binary/unavailable source, truncation and context omissions.
Assert comparisons obey supplied pins, returned spans include source and warm
reuse does not rebuild a parallel index. Assert limits/failures stay explicit.
Test Wyrd adapters against real storage/Bifrost dependencies for conditional
publication, tenant separation and cache expiry; an in-memory adapter alone
cannot establish production custody. Task 005/006 supply the connected queued
execution proof; no public Wyrd action is claimed shipped by this library task.

Library owner/tier: owning module tests and existing `pipeline_journey` integration
target with its `testing` feature. Its manifest requires that feature; use the
library's actual managed task if present, otherwise `mise exec -- cargo nextest
run --locked -p bohmian-code-review-pipeline --features testing --test
pipeline_journey` in that repository. New named focused commands are recorded
when tests exist. Wyrd adapter tier 2 belongs in its owner, using existing
`mise run test:vala`/`mise run test:storage:matrix` only for the adapters actually
touched, plus dependency/tenant checks and format/lints. No runtime proof is
claimed by planning; external repo is not edited during this task authoring.
