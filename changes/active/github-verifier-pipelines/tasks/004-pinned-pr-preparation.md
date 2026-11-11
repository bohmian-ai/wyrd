---
id: TASK-004
status: proposed
spec_revision: 6
requirements: [REQ-006, REQ-007]
acceptance: [AC-004]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-007]
depends_on: [TASK-002]
---

# Prepare pinned PR comparisons through the independent pipeline

Implement the library/adapter portion of **approved-code-review-support.md →
Library output and retained evidence / Successive commits and pipeline ownership**.
This is source preparation for task 005, not a new Wyrd review orchestrator.
The [Task 004 storage amendment](../design/approved-code-review-support.md#task-004-storage-amendment)
corrects the earlier storage prohibition. Spec revision 6 is draft; this task
remains proposed pending renewed spec approval.

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

Implement all three required library storage traits:

- `ObjectStorage` over Wyrd's configured `object_store::ObjectStore` backend:
  immutable snapshot/context bytes, scoped library keys, atomic create-only
  writes, exact reads and idempotent deletion. Preserve `AlreadyExists` and
  `NotFound` versus backend failures.
- One PostgreSQL-backed metadata owner implementing `SnapshotStore` and
  `ContextStore`. Task 004 owns the required snapshot-pointer, snapshot-record
  and context metadata tables/migrations through `wyrd-sql`, equivalent to the
  README's three-table schema. This is the explicit new-table exception alongside
  Task 002's installation table; reuse the existing PostgreSQL deployment.
- Preserve snapshot compatibility scope, tree-based conditional publication,
  atomic collection claims against upserts/pointer publication, grace periods
  and retryable cleanup. Context upserts keep original publication/expiry stamps;
  lookup distinguishes absent, expired and live. Scope by tenant, installation
  and numeric repository ID, never mutable owner/name labels.
- Follow `wyrd-sql` ownership: `TenantConn`, forced tenant RLS and a narrow
  `OperatorPool` capability for cross-tenant expiry. Use PostgreSQL
  `statement_timestamp()` and duration-derived deadlines, with no transaction
  spanning GitHub or object-store IO.

Task 002 provides verified identity and short-lived installation-token access.
PostgreSQL holds pointers/records/lifecycle metadata, never snapshot/context
object bytes or execution payloads. Do not implement these metadata traits over
Bifrost/Scribe, invent execution IDs for cache records or add a cache-coordination
service. Library cache reuse is separate from Task 005's accepted Bifrost
execution evidence; cache expiry must not remove that retained evidence.
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
Test the actual pipeline with real Wyrd object storage/PostgreSQL adapters for
cold/warm reuse, competing pointer publication, collection versus upsert/
publication, interrupted cleanup/retry, non-extending context expiry, absent/
expired/backend-failure distinctions, identity isolation and rename continuity.
An in-memory adapter alone cannot establish production custody. Task 005/006
supply the connected queued execution proof; no public Wyrd action is claimed shipped by this library task.

Library owner/tier: owning module tests and existing `pipeline_journey` integration
target with its `testing` feature. Its manifest requires that feature; use the
library's actual managed task if present, otherwise `mise exec -- cargo nextest
run --locked -p bohmian-code-review-pipeline --features testing --test
pipeline_journey` in that repository. New named focused commands are recorded
when tests exist. Wyrd adapter tier 2 belongs in its owner, using existing
`mise run test:sql` for migrations/RLS/metadata concurrency and
`mise run test:storage:matrix` for object adapters, plus the owning pipeline
integration runner, dependency/tenant checks and format/lints. No runtime proof
is claimed by planning; external repo is not edited during this task authoring.

## Obligations

Implementation proceeds under the user's 2026-11-11 direction to implement this
revised task while spec revision 6 awaits formal approval. The library branch is
the user-directed new branch `wyrd-production`, delivered through a PR into the
library's `main`.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| L1 | Missing behavior ¶2 | Public `ReviewPipeline` accepts frozen PR number, base, head, head repository ID and optional baseline | journey calls the public facade with pins |
| L2 | ¶2, Proof | Comparisons obey supplied pins when the live PR moved | moved-live-PR journey: output base/head equal pins, not live head |
| L3 | ¶2 | Full comparison is PR merge base → head | `full.base == merge_base` and files equal merge-base diff |
| L4 | ¶2 | Active comparison is baseline → head; none means full | incremental journey `active.base == baseline`; first review `active == full` |
| L5 | ¶2 | Rewritten or unavailable baseline is reported, never silently replaced | journey asserts `BaselineUnusable` reasons |
| L6 | ¶3 | Changed files carry status, old path, hunks; renames and deletions | rename/delete file assertions |
| L7 | ¶3 | Before/after source text or explicit binary/unavailable/excluded identity | binary, oversize and excluded assertions |
| L8 | ¶3 | Pinned metadata: number, URL, title, body, state, draft, base, head with fork repository, merge base | fork journey metadata assertion |
| L9 | ¶3 | Unchanged `ReviewContext` selected for the active comparison, spans carry source | context spans text and active-change anchoring |
| L10 | ¶3 | Truncation/pagination verified; reconstruct or fail explicitly | truncated comparison fails with typed error |
| L11 | ¶3 | Context-selection omissions stay coverage data | omission journey succeeds with omissions |
| L12 | ¶3 | Empty active comparison is valid | identical baseline yields empty active files |
| L13 | Proof | Cold then warm; warm reuses snapshot without rebuilding an index | warm call archive count 0 |
| L14 | ¶2 | Library takes no policy/baseline/finding/verdict/tenant/delivery decision | source review of facade signature |
| L15 | ¶2 | Existing context/refresh workflows and cache rules unchanged | existing library suites green |
| W1 | Storage bullet 1 | `ObjectStorage` over object_store: create-only `AlreadyExists`, `NotFound`, idempotent delete, backend failure | adapter tests on local and emulator backends |
| W2 | bullet 2 | Three metadata tables in `wyrd-sql` with forced tenant RLS | migration/RLS test |
| W3 | bullet 3 | Tree compare-and-swap and insert-if-absent pointer publication under competition | competing publication test |
| W4 | bullet 3 | Collection claims against upserts/publication, grace, interrupted cleanup retry | claim/upsert/retry tests |
| W5 | bullet 3 | Context upsert keeps stamps; absent/expired/live; backend failure distinct | expiry and lookup tests |
| W6 | bullet 3 | Scoped by tenant/installation/numeric repo ID; isolation; rename continuity | isolation and rename tests |
| W7 | bullet 4 | `TenantConn`, `OperatorPool` cross-tenant expiry, `statement_timestamp()`, no transaction across IO | source + expiry sweep test |
| W8 | Task 002 ¶ | PostgreSQL holds no object bytes or execution payloads | schema review |
| W9 | Proof | Actual pipeline over real Wyrd adapters cold/warm | Wyrd pg pipeline test |
| D1 | ¶ last | Design docs state adapter custody without making the cache canonical | doc diff |
| D2 | Dependency delivery | Committed, pushed library revision with PR; Wyrd pins that normal git revision | Cargo.toml/Cargo.lock rev |
