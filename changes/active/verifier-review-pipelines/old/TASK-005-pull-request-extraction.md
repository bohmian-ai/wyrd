---
id: TASK-005
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
previous_id: TASK-004
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001, TASK-002, TASK-003, TASK-004]
req: [REQ-016, REQ-017, REQ-018, REQ-020, REQ-041, REQ-042, REQ-043, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-059]
inv: [INV-002, INV-010, INV-014, INV-017, INV-018]
ac: [AC-010, AC-012, AC-033, AC-034, AC-035, AC-036, AC-017, AC-047, AC-048, AC-049, AC-051]
---

# Pull-request extraction using bohmian-code-review

## Outcome and Value

Implement extractor: pull_request by extending and calling the library's public ReviewPipeline and existing GitHubClient. The engine automatically persists its result as PR metadata plus one durable directory containing the complete diff/changed source and supplemental context. No new GitHub client or Wyrd extraction pipeline.

## What Changes and What Must Stay True

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

### 1. Acquire the exact bundle

**Behavior.** Through public ReviewPipeline, acquire a PR requiring one changed file and an unchanged supporting declaration. Assert exact metadata/title/body, merge-base diff, verified source bytes, context spans/omissions and matching repository/base/head/tree identities.

**RED.** Library public journey asserts every bundle member and revision; current facade returns supplemental context without all required PR evidence. Expected RED: the current public facade returns context without the complete paired evidence. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Extend the existing public facade and GitHubClient, retaining real library stage orchestration and schemas.

**REFACTOR.** Share provider operations inside the library rather than Wyrd leaf calls. Keep this and earlier scenario tests green.

### 2. Run the automatic extractor

**Behavior.** On bound and manually selected PRs, use the minimal extract step with no root inputs/from. output equals pr.json and evidence mounts as the exact directory. Missing/foreign/disabled/unallowed identity or from refuses before any provider IO; document extraction is unchanged.

**RED.** SDK journey reads all members through Agent filesystem tools and asserts zero requests for denied identity. Expected RED: pull_request is not a supported automatic extractor selector. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Extend extract contract/hosted evidence owner and Task 004 identity resolution; persist extraction evidence atomically via Task 001, without exposing Agent tools to the extractor.

**REFACTOR.** Keep the existing extraction action and evidence reader. Keep this and earlier scenario tests green.

### 3. Preserve changed evidence

**Behavior.** Table: add/delete/modify/rename/copy → correct old/new path and sides; binary → marker plus exact bytes; zero changes → empty diff/inventory, no verdict; missing patch → complete reconstruction or failure. Missing blob, exhausted pagination, unsafe entry or acquisition limit → no accepted bundle/reviewer.

**RED.** Library provider journey verifies inventory/order/hunks/blob IDs and no successful partial result for each negative case. Expected RED: complete changed-file acquisition is not returned by the public facade. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Use existing GitHubClient verified tree/blob acquisition and bounded pagination to produce the exact supplement contract.

**REFACTOR.** Keep reconstruction/completeness logic library-owned. Keep this and earlier scenario tests green.

### 4. Reject revision mismatch

**Behavior.** Move head/base during acquisition or pair context from another head. Extraction exposes moved/superseded or explicit failure; it accepts no mixed bundle and dispatches no reviewer. Metadata-only edits do not mutate the captured snapshot.

**RED.** Public/library negative case asserts accepted identity, zero reviewer calls and absence of a successful publication. Expected RED: the complete paired bundle has no revision-checked acceptance boundary. Run [proof 4](#proof-4); names are planned until added/listed.

**GREEN.** Verify paired identities before commit, retain accepted pins on bounded retries and let a later generation acquire new revisions.

**REFACTOR.** Keep acquisition and accepted bundle identity checks at the extractor owner. Keep this and earlier scenario tests green.

### 5. Persist library state correctly

**Behavior.** Use real Wyrd object/metadata adapters for cold build, warm context, concurrent snapshot update and read-by-ID. Interrupted object/pointer commit converges without a partial pointer; tenant/host scopes stay separate even for equal numeric IDs. Incompatible formats cold-build.

**RED.** Server integration drives the real public pipeline through adapters and asserts pointer/content integrity, database time and cross-scope denial. Expected RED: the intended Wyrd library store adapters do not exist. Run [proof 5](#proof-5); names are planned until added/listed.

**GREEN.** Implement intended store contracts under existing SQL/storage boundaries, honoring library leases/CAS/expiry rather than adding another store lifecycle.

**REFACTOR.** Share the configured storage/Postgres owners and short transaction boundaries. Keep this and earlier scenario tests green.

### 6. Share accepted evidence

**Behavior.** Run three reviewers, crash after extraction commit and recover with GitHub unavailable. All mount identical digests and original-source citation lineage; no sibling/recovery acquisition occurs. Library context readback matches public ReviewPipeline::read. Reviewer filesystem reads/searches outside the mounted bundle are unavailable and cause no GitHub requests.

**RED.** Count library/provider acquisitions and inspect each mounted digest/citation across real recovery. Attempt a filesystem read/search outside the mounted evidence and assert refusal/unavailability with no additional GitHub requests. Expected RED: no accepted PR bundle is restored across reviewer recovery. Run [proof 6](#proof-6); names are planned until added/listed.

**GREEN.** Restore one accepted publication and ContextId/provenance through Task 001; materialize it through the existing workspace owner for filesystem tools.

**REFACTOR.** Share immutable handles with bounded workspace materialization. Keep this and earlier scenario tests green.

### 7. Acquire fork evidence safely

**Behavior.** Review an allowed fork PR using approved base-installation access and pinned head repository identity. Never execute source or expose installation credentials to a fork. Deleted/inaccessible fork data fails explicitly; no source substitution or clean review.
After accepting evidence, revoke repository permission or integration approval
before the next protected hosted-tool call: it refuses and affected work cancels.
This closes Task 002's repository-specific authority seam after Task 004 exists.

**RED.** Mock provider records exact repository/revision requests; assert zero code execution/secret exposure and no reviewer on unavailable evidence.
Also assert the next protected tool is refused after mid-run repository/integration
revocation, with no accepted-authority expansion or new publication acceptance. Expected RED: the automatic pinned fork evidence path does not exist. Run [proof 7](#proof-7); names are planned until added/listed.

**GREEN.** Resolve fork blobs through existing GitHubClient authorization and verified IDs; enforce the complete-bundle failure contract and reuse current per-call
authority checks through the real hosted-tool path supplied by Task 002.

**REFACTOR.** Reuse provider identity/auth logic instead of a fork-specific checkout path. Keep this and earlier scenario tests green.

### 8. Bound extraction and model input

**Behavior.** Context fits context_tokens but the full request exceeds admitted prompt/tool/conversation/response allowance: model call refuses without truncation. Bounded acquisition/staging and concurrent materialization stay within effective limits; another execution progresses during provider backoff.

**RED.** Assert full-request refusal, bounded active IO/staging counters and preserved context omissions; no arbitrary latency threshold. Expected RED: complete PR model input and acquisition accounting are not integrated. Run [proof 8](#proof-8); names are planned until added/listed.

**GREEN.** Apply library acquisition/context limits and existing model admission limits separately, publishing effective ceilings and releasing unrelated capacity during waits.

**REFACTOR.** Reuse existing request accounting and provider retries. Keep this and earlier scenario tests green.

### 9. Refresh and collect safely

**Behavior.** Default-branch push work invokes incremental refresh once logically; warm PR context does not refresh. Retry interrupted maintenance idempotently, honor collection grace/nonoverlap and quiesce active builds before repository removal.

**RED.** Integration/library cases assert refresh request counts, changed snapshot state and no removal under an active build. Expected RED: durable maintenance has no public-library consumer in Wyrd. Run [proof 9](#proof-9); names are planned until added/listed.

**GREEN.** Consume Task 004 durable maintenance work with public refresh/cleanup and complete only after its committed result.

**REFACTOR.** Use library lifecycle owners and current worker supervision. Keep this and earlier scenario tests green.

### 10. Read pinned PR evidence from recovered run state

**Behavior.** Acquire one exact complete PR bundle, share it across queued
recovery and a direct run, then expire live library context and disable GitHub.
Inspect/download the original accepted bundle using Bifrost state and exact File
descriptors, with no PostgreSQL publication payload or provider reacquisition.
Library maintenance/offboarding and evidence retention keep their own lifetimes.

**RED.** Extend the shared-bundle journey with reconstruction and source-failure
cases; assert acquisition counts, pinned digests and bytes. Current descriptor
readback depends on PostgreSQL. Run [lifecycle proof](#lifecycle-proof).

**GREEN.** Retain the intended library workflow/adapters and checkpoint exact
accepted evidence associations through Task 001. Resolve successors from state.

**REFACTOR.** No new PR cache/catalog, branch-tip reconstruction or coupling of
library expiration to execution cleanup.

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
| Task 002 registry; Task 004 integration/manual identity/maintenance; hosted_artifacts and verifier_building_blocks | These provide existing filesystem tools, authority/work and evidence journeys. Reviewers consume the retained bundle through existing mounts; immutable upstream pin includes Tasks 004 then 005 library extensions. |

## Verification and Evidence

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

[Approved spec, revision 18](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
