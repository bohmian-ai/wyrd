---
id: TASK-005
spec: SPEC-verifier-review-pipelines
spec_revision: 24
status: proposed
contract_status: complete
previous_id: TASK-004
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001-R5, TASK-002, TASK-003, TASK-004]
req: [REQ-016, REQ-017, REQ-018, REQ-020, REQ-041, REQ-042, REQ-043, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-059]
inv: [INV-002, INV-010, INV-014, INV-017, INV-018]
ac: [AC-010, AC-012, AC-033, AC-034, AC-035, AC-036, AC-017, AC-047, AC-048, AC-049, AC-051]
---

# Pull-request extraction using bohmian-code-review

## Outcome and Value

Implement extractor: pull_request by extending and calling the library's public ReviewPipeline and existing GitHubClient. The engine automatically persists its result as PR metadata plus one durable directory containing the complete diff/changed source and supplemental context. No new GitHub client or Wyrd extraction pipeline.

## What Changes and What Must Stay True

[Replacement TASK-004](TASK-004-installable-github-app.md) supplies the verified
connection to the deployment-owned App. This task consumes that connection and
current repository authority; it does not register Apps, accept tenant keys,
create another OAuth flow or rediscover which tenant owns an installation.
SaaS and self-hosted/GHES use that same authority adapter with different operator
configuration. Installation/user setup tokens never enter the engine, mounted
evidence or persisted run state; background acquisition uses scoped installation
credentials resolved by Wyrd immediately before the operation.

Keep the independent library's generic App/Check/webhook commit `9fdf6dc` and
extend the immutable descendant delivered by TASK-004. Do not revert it, return
to the rejected Wyrd implementation or create a second GitHub client. Extend
ReviewPipeline and its generic storage traits/implementations for reusable review
functionality only. Wyrd owns its SQL/object-store adapters, tenant grants,
connection lifecycle, durable maintenance scheduling and cancellation.

Task 001 retains the extractor's pinned bundle and exact completed File
associations in accepted/recovered run state. Terminal reads need no old
PostgreSQL payload row, live library context or new GitHub acquisition. Library
snapshot/context CAS, expiration, maintenance and offboarding remain independently
owned; their policy does not remove accepted evidence roots.

| Implement | Required implementation logic |
|---|---|
| **Library pinned acquisition** | Extend ReviewPipeline's public facade to accept/verify pinned PR identity and pair real ReviewContext with complete evidence. Extend existing GitHubClient PR metadata/diff/tree/blob operations; reuse its host, identity, request, pagination and retry checks. Keep index/overlay/ranking/packing/publication/readback unchanged. |
| **Wyrd consumer adapters** | Implement ObjectStorage create/read/delete through existing object storage and SnapshotStore/ContextStore through tenant SQL. create must preserve conditional immutable-object/integrity behavior; publish_snapshot_pointer is compare-and-swap; unreferenced atomically claims eligible records against pointer swaps/upserts; context upsert preserves original database-clock publication/expiry stamps and exact identity lookup enforces expiry. Host/tenant-qualified identity requires versioned persisted formats and incompatible-cache cold-build; no second snapshot cache. |
| **Extract selector and bundle** | Add exactly pull_request to existing extract validation; refuse from for it and resolve bound/manual identity through Task 004 before source IO. output equals pr.json. Have the engine automatically persist one evidence directory containing pr.json, diff.patch, ordered changes.json, changed/before/, changed/after/ and unchanged context.json using Task 001. |
| **Completeness and pinning** | Pin target/base, merge base, head/head repository/tree and metadata. Verify every changed blob and context revision. Reconstruct missing patches using library verified objects or fail; never treat absence/truncation as no change. Include additions/deletions/renames/copies/binary markers and bytes; reject unsupported unsafe entries atomically. PR movement is moved/superseded, never silent retargeting. |
| **Shared reads and budgets** | Commit one accepted bundle per execution and reuse it for every reviewer/recovery. Reviewers use existing filesystem tools over the mounted immutable bundle; no repository read/search capabilities or additional source fetches are exposed. Preserve library context readback through ReviewPipeline::read; retained evidence remains readable without GitHub. Full model admission includes prompts, diff/source, tools/conversation and response, separately from context_tokens; never truncate silently. |
| **Library maintenance** | Default-branch work admitted by Task 004 runs public refresh; warm context does not auto-refresh. Use library expiration/collection with grace/nonoverlap and quiesce before offboarding. |

## Approach

1. Deliver pinned acquisition through existing library ReviewPipeline/GitHubClient and its public journeys.
2. Implement intended library store adapters and pin the delivered immutable Git revision/lockfile.
3. Add automatic selector/identity validation and atomically publish the complete evidence bundle.
4. Mount the retained bundle through existing filesystem tools and reuse accepted evidence after recovery.
5. Connect durable refresh/cleanup and prove lifetime, revision, completeness and resource-limit behavior.

## Ordered Implementation Scenarios

Scenarios follow a reviewer and a repository update. Evidence variants, adapter
crashes, source/permission failures and maintenance races remain focused test
cases, not separate implementation phases.

### Scenario 1 — Review complete pinned PR evidence and reuse it after recovery

**Behavior.** After the ordinary install/select/connect flow from TASK-004, a
developer selects the `extract: pull_request` action for a bound or manually
selected PR without authored root inputs or `from`. The
existing library pipeline acquires the exact PR; output is `pr.json` and one
accepted evidence directory holds complete diff/source plus supplemental context.
Reviewers read/search that same immutable mount with existing filesystem tools.
After interruption, subsequent reviewers restore the bundle with GitHub unavailable
rather than acquire it again. Document extraction remains unchanged.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Metadata/title/body, merge-base diff, source, context spans/omissions and exact repository/base/head/tree pairing | One complete, attributable bundle through the public library facade, not supplemental context alone. [Acquisition cases](#proof-1). |
| Minimal selector, verified shared-App connection or self-hosted operator App, bound/manual target, missing/foreign/disabled/unallowed identity or invalid `from` | Exact automatic output/mount; invalid/denied requests make no provider call. [Public extraction journey](#proof-2). |
| Add/delete/modify/rename/copy, binary, zero changes or missing patch; missing blob, pagination exhaustion, unsafe source or limits | Complete correct inventory/bytes or explicit failure with no reviewer/accepted partial bundle; no fabricated verdict for empty changes. [Changed-evidence cases](#proof-3). |
| Moving base/head, mismatched context or metadata-only edits | No mixed-revision acceptance or mutable captured snapshot; later generations use their own pins. [Revision cases](#proof-4). |
| Cold/warm build, concurrent pointer/object updates, interrupted commit, host/tenant identity and incompatible library format | Intended adapters preserve library CAS/database-time/expiry/integrity and scope separation. [Adapter cases](#proof-5). |
| Three reviewers, crash after extraction, removed live context, disabled GitHub and out-of-mount filesystem reads | One acquisition; identical digests/citation lineage, no hidden provider reads and exact descriptor-backed inspection. [Reuse cases](#proof-6), [retained-state cases](#lifecycle-proof). |
| Allowed or inaccessible fork; mid-run repository/integration revocation before a protected tool | Source is data, no fork credential/code execution, explicit missing-source failure, next protected call refused and active work cancelled. [Fork/authority cases](#proof-7), closing Task 002's repository seam after Task 004. |
| Context fits its budget but full model request exceeds admission; slow concurrent acquisition/materialization | No silent truncation, distinct context/request bounds, bounded staging/IO and unrelated progress. [Budget cases](#proof-8). |

**RED.** Extend the [public extraction journey](#proof-2) through real reviewer
mounts and recovered reuse, with supporting library/adapter/negative cases above.
Assert complete original bytes and provenance, exact acquisition counts, zero
reviewer calls on incomplete evidence and unchanged document extraction. The
automatic complete paired bundle and its Wyrd adapters are missing today.

**GREEN.** Extend/call the whole public ReviewPipeline and existing GitHubClient;
implement intended tenant/host-qualified store adapters. Bind Task 004's exact
identity, retain one extraction result through Task 001, and mount it using
existing evidence/Workspace owners and Task 002 tools. No Agent publish call is
introduced for extraction.

**REFACTOR.** Keep acquisition/reconstruction library-owned and Wyrd adapters
thin; no new checkout, PR cache, parser, index/ranker, reviewer source API or
replacement orchestration. Keep all journey cases green.

### Scenario 2 — Refresh repository context for a subsequent review

**Behavior.** A repository maintainer pushes a default-branch update. Work already
admitted by Task 004 invokes the library's incremental refresh. A subsequent PR
review uses the refreshed snapshot/context under its own exact pins; a warm PR
request alone triggers no refresh. Previously accepted evidence remains unchanged.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Duplicate event or interrupted refresh retry | One logical refresh converges through existing library/store semantics; no false completion while work remains pending. [Maintenance cases](#proof-9). |
| Context expiration, collection grace/nonoverlap and offboarding during a build | Existing library lifetime applies; quiesce before removal, preserve active work and accepted execution evidence. [Maintenance cases](#proof-9). |
| Next review after refresh versus a warm request | New context uses the refreshed snapshot; source identities stay exact, and old accepted bundle is still readable. [Public refresh-to-review journey](#refresh-to-review-journey). |

**RED.** Add the [public refresh-to-review journey](#refresh-to-review-journey),
supported by [maintenance integration cases](#proof-9), to observe push → refresh
→ subsequent review. Assert provider/refresh counts, updated context, immutable
older evidence and safe offboarding. TASK-004 deliberately leaves maintenance pending; this task supplies the
extraction-owned consumer and never infers completion from accepted ingress.

**GREEN.** Consume that work with the public library refresh/cleanup lifecycle,
complete it only after the committed result and use the updated library snapshot
for the next extraction. Keep library state separate from run recovery custody.

**REFACTOR.** Reuse current worker supervision and library maintenance/expiration;
no new scheduler, cache lifetime or coupling to execution-row cleanup.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-016–018/041; INV-014; AC-012/033/035/036 | Facade, selector, changed-file and moving-revision proofs assert the entire exact bundle and automatic whole-pipeline invocation, with no authored PR inputs. |
| REQ-042–043; AC-034 | Shared-bundle and request-limit proofs establish one extraction, identical mounted evidence across reviewers/recovery, no reviewer-triggered source fetches, original-source provenance and separate full-request budget. |
| REQ-020; INV-002/010; AC-010/014 contribution | Adapter and maintenance proofs establish tenant/host separation, cold/warm/incremental state and safe offboarding. |
| AC-017 | Dedicated fork proof establishes base-installation authority, head evidence as data, inaccessible-fork failure, no fork credential/source execution path, and repository/integration revocation between accepted evidence and the next protected tool call. |
| REQ-032 contribution | Mirror selector/evidence/denied-identity journey through Python/TS/CLI/MCP, using one shared server owner. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. Task 001 owns loading, acceptance, replay and old-consumer replacement. |

## Implementation Reference

ReviewPipeline and its intended store adapters own library context lifetime;
existing extractor/Workspace/File retention owns immutable evidence. Task 001
supplies accepted run-state descriptor custody and recovery, not root transfer
between databases. Extend these owners without retaining legacy payload rows.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| bohmian-code-review-pipeline::ReviewPipeline::context/refresh/read; pipeline_journey | Existing whole workflow and public fake-provider/consumer-store journeys. Extend the facade for pinned complete evidence; never compose a Wyrd replacement pipeline. |
| Library GitHubClient/GitHubHost; PullRequest/PullRequestFile; tree/blob readers | Existing bounded provider client already owns GitHub. Metadata lacks title/body/URL and patches are optional: extend these exact owners for full acquisition; do not add a second client. |
| Library ObjectStorage, SnapshotStore and ContextStore contracts | Intended consumer adapters are Wyrd work. Preserve database-clock coordination, conditional updates, integrity and read/expiry semantics; source-free snapshots remain library-owned. |
| Action::Extract; hosted blocks/evidence::TenantEvidence; Task 001 publication owner | Current extract is document-only. Add automatic PR selection and typed evidence publication at these existing seams; contracts/SDKs must not gain provider/storage dependencies. |
| Task 002 registry; replacement Task 004 verified installation/manual identity/maintenance; hosted_artifacts and verifier_building_blocks | These provide existing filesystem tools, authority/work and evidence journeys. Reviewers consume the retained bundle through existing mounts; immutable upstream pin includes Tasks 004 then 005 library extensions. |

## Verification and Evidence

Proof numbers identify supporting tests, not additional implementation scenarios.
All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- cargo test --locked -p bohmian-code-review-pipeline --test pipeline_journey --all-features pipeline_returns_pinned_metadata_diff_and_context -- --exact --nocapture
```

### Proof 2

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::pull_request_selector_publishes_one_evidence_directory)"'
```

### Proof 3

```bash
mise exec -- cargo test --locked -p bohmian-code-review-pipeline --test pipeline_journey --all-features changed_file_evidence_is_complete_or_fails -- --exact --nocapture
```

### Proof 4

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::moving_or_mixed_revisions_cannot_commit)"'
```

### Proof 5

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_pull_request_evidence::store_adapters_preserve_library_coordination)"'
```

### Proof 6

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::reviewers_reuse_committed_bundle_without_github)"'
```

### Proof 7

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::fork_source_is_data_without_fork_credentials)"'
```

### Proof 8

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::context_and_request_limits_are_separate)"'
```

### Proof 9

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_pull_request_evidence::refresh_and_offboarding_use_public_library_lifecycle)"'
```

### Lifecycle proof

New selector is **planned**. Use the existing target/setup below, add/list the actual
test before RED, and record a nonzero selection plus the missing-behavior failure and
closure result. Extend the task's existing Python/TS/MCP public-closure case table with
its recovery outcomes rather than adding full suites; Task 006 uses existing
combined-surface cases. No runtime proof is claimed during planning.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::retained_bundle_survives_run_state_recovery)"'
```

### Refresh-to-review journey

Planned public SDK → real server → SDK journey with a local mock GitHub.
Exercise an admitted default-branch update, completed incremental refresh and
a subsequent PR review; assert refreshed context, exact new pins, unchanged old
evidence and no refresh on a warm request alone. Proof 9 supplies narrower
maintenance/offboarding failure coverage. Confirm the selector before RED.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=pull_request_evidence::repository_update_refreshes_next_review_context)"'
```

### Public surface closure

Mirror bound/manual extractor selection, exact evidence readback and absent/denied PR identity; source IO must remain zero on refusal. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_pull_request_evidence.py::test_pull_request_selector_publishes_one_evidence_directory'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/pull-request-evidence.test.ts -t "^pull_request_selector_publishes_one_evidence_directory$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::pull_request_evidence_pull_request_selector_publishes_one_evidence_directory)"'
```

### Upstream delivery

Run from bohmian-code-review after the focused public proofs:

```bash
mise run test:pipeline
mise run fmt:check
mise run lints
```

Record the delivered immutable library revision, lockfile and provider/pipeline evidence. No mutable branch, developer path or uncommitted external dependency is accepted; publishing/pushing is not authorized by this plan.

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No additional Agent-facing repository read/search tools or reviewer-triggered source fetches. No new GitHub client, checkout, symbol index, ranker, pipeline or snapshot cache in Wyrd. Unverifiable complete evidence must fail. Record the delivered upstream immutable revision; do not use a mutable branch or developer path.

## Authority Links

[Approved GitHub amendment revision 24 and existing approved extraction baseline](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
