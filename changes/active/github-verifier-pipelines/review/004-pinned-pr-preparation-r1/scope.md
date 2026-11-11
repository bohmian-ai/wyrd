# Task 004 first review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Base: `81aca5479` (storage amendment recorded, before Task 004 implementation).
Candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Range: `git diff 81aca5479...359a19f41597b8cfdc673373409b0d96e8298447`.
Library repository: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Library base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
Library candidate: `d380f57fa53429eaa4f3e7272ce651e1825a8b38`.

First task review; no prior acceptance or findings carried forward. Reviewed
source remains immutable. Only reports may be written in this review directory.

## Authority

Original task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`
at base; candidate appends obligations/evidence. Spec: same packet `spec.md`,
revision 6, currently draft. Revision 5 was approved; the user explicitly directed
Task 004's storage amendment. Determine whether current authority suffices for
acceptance without silently treating draft revision 6 as approved. Implementation
claims in the task ledger are hypotheses, not verified review evidence.

Read root/nested AGENTS, spec-driven development, maintainer style, relevant
architecture sections and TESTING. CodeGraph before code navigation if indexed.

## Navigation map

- Library ReviewPipeline::pinned, comparison.rs, pull_request.rs, provider types;
  tests/pipeline_journey.rs. Public facade consumes pins/baseline, returns source.
- Wyrd server components/github/review_cache.rs: blob and metadata adapters;
  tests/integration/pg_code_review_cache.rs: actual pipeline through adapters.
- wyrd-sql queries/code_review_cache.rs and migration
  20261111000000_code_review_cache.sql; integration pg_code_review_cache and
  pg_migration: tenant metadata, CAS, collection, lifecycle and expiry.
- wyrd-storage handle.rs::create_object and tenant_path.rs: create-only writes
  and cache key namespace; integration/handle_crud.rs and mise GCS lane.
- Server storage_e2e.rs download-plan fix; trace actual auth boundary and diagnosis.
- Cargo manifests/lock: library dependency and tier ownership.
- architecture/wyrd-design.md and bifrost-design.md: cache/evidence custody.

Reviewers must expand this map through callers/consumers; it is not coverage proof.
Available verification results and process exceptions are committed in the task's
Evidence/Commands/Process exceptions sections. No runtime checks yet repeated by
this review; do not modify source or run mutation tests.

## Specialist applicability

| Contract / invariant; producers → consumers | Selection and source basis |
|---|---|
| REQ-006/007, AC-004; caller pins → library → adapters → extraction consumer | behavior-rev: comprehensive acceptance matrix and library/Wyrd seam |
| INV-004/005; snapshot/context writes → warm reads/collection/retry | invariant-rev: CAS, claim, expiry and publication are materially shared lifecycle state |
| Dependency tiers, rustdoc, SQL capabilities, test lanes; manifests/source → shipped owners | repo-rev: server/storage/SQL/library and architecture span multiple rule owners |
| New adapter/query owners; existing storage/SQL/pipeline → new consumers | maintainer-rev: substantial ownership changes; compare existing complete owners and duplication |
| Interrupted writes/collection, dependency failures → subsequent reads/retry | system-rev: durable recovery and error propagation materially affected |
| INV-001/004; metadata transactions → pointers/GC/expiry | domain-rev-data: persistent data, concurrency and RLS are one coherent SQL boundary |
| Tenant object keys/installation identity → storage and metadata | domain-rev-security: separate trust/scoping review of adapter identity and operator expiry authority |

No UI, Python, TypeScript or SDK public operations change; no language/UI specialist
selected. No model invocation or judging semantics change; no AI-domain specialist.
Fresh independent ponytail validation follows all selected discovery reports.
