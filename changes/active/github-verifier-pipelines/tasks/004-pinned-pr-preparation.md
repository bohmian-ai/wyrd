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
| L4 | ¶2 | Active comparison is optional previous-reviewed head → head, beside the complete comparison | incremental journey `active.base == baseline`; first review without a baseline |
| L5 | ¶2 | Rewritten or unavailable baseline is reported, never silently replaced | journey asserts `BaselineUnusable` reasons |
| L6 | ¶3 | Changed files carry status, old path, hunks; renames and deletions | rename/delete file assertions |
| L7 | ¶3 | Before/after source text or explicit binary/unavailable/excluded identity | binary, oversize and excluded assertions |
| L8 | ¶2, ¶3 | Pinned metadata, including fork head repository identity and merge base | fork journey metadata assertion |
| L9 | ¶3 | Unchanged `ReviewContext` selected for the active comparison, spans carry source | context spans text and active-change anchoring |
| L10 | ¶3 | Truncation/pagination verified; reconstruct or fail explicitly | truncated, paged and rebuilt comparisons |
| L11 | ¶3 | Context-selection omissions stay coverage data | omission journey succeeds with omissions |
| L12 | ¶3 | Empty active comparison is valid | identical baseline yields empty active files |
| L13 | Proof | Cold then warm; warm reuses snapshot without rebuilding an index | warm call archive count 0 |
| L14 | ¶2, Dependency ¶1 | Library takes no policy/baseline/finding/verdict/tenant/delivery decision; App signing/callback/connection policy stays in Wyrd | source review of facade signature and library diff |
| L15 | ¶2 | Existing context/refresh workflows and cache rules unchanged | existing library suites green |
| W1 | Storage bullet 1 | `ObjectStorage` over object_store: create-only `AlreadyExists`, exact reads, `NotFound`, idempotent delete, backend failure | adapter tests on local and emulator backends |
| W2 | bullet 2 | Three metadata tables in `wyrd-sql` with forced tenant RLS | migration/RLS test |
| W3 | bullet 3 | Tree compare-and-swap and insert-if-absent pointer publication under competition | competing publication test |
| W4 | bullet 3 | Collection claims against upserts/publication, grace, interrupted cleanup retry | claim/upsert/retry tests |
| W5 | bullet 3 | Context upsert keeps stamps; absent/expired/live; backend failure distinct | expiry and lookup tests |
| W6 | bullet 3 | Scoped by tenant/installation/numeric repo ID; isolation; rename continuity | isolation and rename tests |
| W7 | bullet 4 | `TenantConn`, `OperatorPool` cross-tenant expiry, `statement_timestamp()`, no transaction across IO | source + expiry sweep test |
| W8 | Task 002 ¶ | PostgreSQL holds no object bytes or execution payloads; cache expiry never removes retained evidence | schema review; path isolation |
| W9 | Proof | Actual pipeline over real Wyrd adapters: cold/warm, competing publication, collection and retry, non-extending expiry, isolation, rename | Wyrd pg pipeline tests |
| N1 | Task 002 ¶ | Metadata traits not over Bifrost/Scribe; no invented execution IDs; no cache-coordination service | diff review |
| D1 | ¶ last | Design docs state adapter custody without making the cache canonical | doc diff |
| D2 | Dependency delivery | Revalidated `wyrd-production` branch; reviewed, committed, pushed library revision; Wyrd pins that normal git revision | Cargo.toml/Cargo.lock rev; PR review |

## Evidence

Library: `bohmian-code-review` branch `wyrd-production` at
`d380f57fa53429eaa4f3e7272ce651e1825a8b38`, PR #6 (open, not yet reviewed).
Paths below are relative to `crates/bohmian-code-review-pipeline/` there (`J` =
`tests/pipeline_journey.rs`). Wyrd: `S` =
`crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs`, `Q` =
`crates/wyrd/wyrd-sql/tests/integration/pg_code_review_cache.rs`, `A` =
`crates/wyrd/wyrd-server/src/components/github/review_cache.rs`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| L1: facade takes PR number, base, head, head repository and optional baseline | `src/review.rs:147` `ReviewPipeline::pinned(client, identity, &PullRequestPins, Option<&ObjectSha>)` | J:955 cold call, J:1061 warm call; J:966 fork head repository; J:1185 a mismatched head repository fails | library `mise run test:rust` | PASS |
| L2: comparisons obey pins when the live PR moved | the comparison reads pins, never the live head | J:975 `(full.base, full.head) == (c, a)` with live head `f`; J:1071-1077 no request names `f` | `test:rust` | PASS |
| L3: full is merge base → head | `src/review.rs` full uses `source.merge_base` | J:1275 base `5`, merge base `c`; J:1285 files equal the merge-base diff (`pinned_full_review_compares_from_the_merge_base`) | `test:rust`; a mutation comparing from `pins.base` fails it | PASS |
| L4: active is previous-reviewed head → head, beside the complete comparison | `PinnedReview.active` | J:978 `active == full`; J:1107 `active.base == baseline` | `test:rust` | PASS |
| L5: rewritten/unavailable baseline reported | `ComparisonError::BaselineUnusable` | J:1175 asserts `BaselineUnusable` for each of `Rewritten` and `Unavailable` (J:1166-1167) | `test:rust` | PASS |
| L6: status, old path, hunks; renames and deletions | `ChangedFile` | J:988 deleted and kinds table; J:1007 renamed old path; J:1001, J:1004 hunks; `src/comparison.rs:691` hunk split | `test:rust` | PASS |
| L7: exact before/after source or explicit binary/unavailable/excluded | `Source`, blobs read by verified tree id | J:997 before/after text; J:1023 `Binary`; J:1032 `VendoredSegment`; J:1039 `Oversize`; `src/comparison.rs:721`, `:741` | `test:rust` | PASS |
| L8: pinned metadata, including fork head repository and merge base | `PullRequestSource` | J:962-978 number, base, head, fork, merge base, title, body, draft, URL; J:973 `state == Open` | `test:rust` | PASS |
| L9: unchanged `ReviewContext` for the active comparison; spans carry source | `PrContext::run_files(active_files)` | J:1051 span text and blob; J:1116-1137: the full review's context names files outside the active change, the incremental one names none of them | `test:rust` | PASS |
| L10: truncation verified; reconstruct or fail | `GitHubClient::tree` splits truncated listings; `Truncated`, `Unverified`, `PatchUnavailable` | J:1205 compare truncated; J:1242 unrebuildable tree truncated; J:1336 truncated head listing rebuilt equals the untruncated review; J:1370 a full commit page is accepted on its verified files; J:1376 a short list ending elsewhere is refused; J:1228 `Unverified`; J:1196 patch unavailable | `test:rust`; mutations disabling the rebuild, the page rule or the head check each fail | PASS |
| L11: omissions stay coverage data | context omissions | J:1220 | `test:rust` | PASS |
| L12: empty active comparison valid | `ChangeSet` empty | J:1144 | `test:rust` | PASS |
| L13: warm reuses snapshot, no rebuild | `ensure_snapshot` | J:958, J:1064 archive count stays 1 | `test:rust` | PASS |
| L14: no policy/baseline/finding/verdict/tenant/delivery decision; App policy stays in Wyrd | signature takes caller pins and baseline only and returns evidence; the library diff adds no signing, callback or connection code | `src/review.rs:147`; `git diff main..d380f57f` | source review | PASS |
| L15: existing workflows unchanged | no change to `context`/refresh/cache rules | all prior library suites | `mise run test:rust`, `lints`, `fmt:check` exit 0 | PASS |
| W1: object adapter create-only, `AlreadyExists`, `NotFound`, idempotent delete, backend failure | `A:95-97`, `A:112-113`; `StorageHandle::create_object` | S:371 `AlreadyExists`; S:373 exact read; S:375 second delete succeeds; S:378 `NotFound`; S:402, S:406 failing store is `Backend` on create and read; refused foreign key is `Backend`; `handle_crud.rs` `run_create_only` (all backends but the GCS emulator) | server `pg_code_review_cache`; `test:storage:matrix` (local, RustFS, Azurite; GCS emulator runs CRUD only); `gcs_create_only` added to `test:storage:gcs:cloud` | PASS |
| W2: three tables with forced tenant RLS | migration lines 63, 71, 79 | `pg_migration.rs` `assert_rls_metadata("code_review_%", …)` | `pg_migration::pg_tests::migrations_apply_and_are_idempotent`; `mise run test:sql` | PASS |
| W3: tree CAS and insert-if-absent under competition | `publish_code_review_snapshot_pointer` | Q:115 exactly one first publication; Q:118-122 swap on tree; S:447 one of two joined publishes wins; S:455 stale swap refused; S:665-685 racing cold pipeline reviews agree, keep one snapshot, and a later review reuses it | `test:sql`; server `pg_code_review_cache` | PASS |
| W4: claims vs upserts and publication, grace, interrupted retry | one `UPDATE … RETURNING` claim; touch `WHERE NOT claimed` | Q:418 aged record touched or claimed, never both, over 24 joined rounds; Q:166-170 referenced record never claimed; Q:161-165 grace; Q:175-178 SQL retry; S:480 grace; S:490 `Collecting`; S:497 retried collect deletes the object | `test:sql`; a mutation dropping the claim guard fails Q:418 | PASS |
| W5: context upsert keeps stamps; absent/expired/live; backend distinct | no-op `DO UPDATE` | Q:257, Q:263-272; S:547 stamps; S:566 Absent; S:578 Backend; S:592 Expired; S:608 no extension | same | PASS |
| W6: scope by tenant, installation and numeric repository; isolation; rename continuity | primary key columns; `A` `repository(identity)` uses numeric IDs | Q:475-489 sibling repository and installation see and claim nothing; Q:517 a sibling's pointer does not protect the record; Q:337-381 tenant isolation; S:337-347 rename keeps snapshot and context | same; a mutation dropping repository correlation fails Q:517 | PASS |
| W7: `TenantConn`, `OperatorPool` expiry, `statement_timestamp()` deadlines, no transaction across IO | `A:214-226` one conn per statement, committed before object IO; `A:441` operator pool; `A:450` tenant mismatch refused; SQL binds durations into `statement_timestamp()` + `make_interval` | Q:229-232 stamps from the bound lifetime; Q:288 sweep; S:619 `collect_expired` deletes record and object | `check:tenant-isolation`, `check:deps`, `test:sql` | PASS |
| W8: no object bytes or payloads in Postgres; expiry never touches retained evidence | migration holds keys, JSON metadata and stamps only (no `BYTEA`); objects live only under `<tenant>/code-review/` | `tenant_path.rs:294-301` code-review paths stay under their own prefix with no Card UID; S:316 every object under that prefix | schema review; storage unit test | PASS |
| W9: actual pipeline over real adapters | `Custody` uses Postgres plus a local `StorageHandle` | cold/warm S:295, S:324, S:329-330; competing publication S:665-685; collection and retry through `Snapshots::collect` S:497-504; non-extending expiry through `pipeline.read` and `Contexts::collect_expired` S:608-629; rename S:337-347 | server `pg_code_review_cache` 5/5 | PASS |
| N1: no Bifrost/Scribe metadata, no execution IDs, no coordination service | the metadata owner is `A` over `wyrd-sql` tables only; the migration has no execution ID column | `A:200-230`; migration source | diff review | PASS |
| D1: design docs state custody, cache not canonical | `wyrd-design.md:2098`; `bifrost-design.md:654` | doc diff | `git diff --check` | PASS |
| D2: revalidated branch; reviewed, committed, pushed library revision; Wyrd pins it | branch `wyrd-production` cut from the library's current `main`, no deleted App branch revived; `Cargo.toml:257` rev `d380f57f…`; `Cargo.lock` | lockfile source | `check:deps`; build | FAIL: library PR #6 is pushed but not yet reviewed or merged |

### Commands

- Library: `mise run test:rust`, `mise run lints`, `mise run fmt:check` (exit 0).
  Deviation: the task text names `cargo nextest`, but the library pins no
  nextest version. `test:rust` is its managed aggregate and runs
  `pipeline_journey` with `testing`.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/^pg_code_review_cache::/)'`: 6/6.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_code_review_cache::/)'`: 5/5.
- `mise run test:sql`: 222, 7, 88 and 3 passed (before the two new SQL cases,
  which pass in the selector above).
- `mise run test:storage:matrix`: every shard passed.
- `mise run lints`, `mise run fmt`, `check:tenant-isolation`, `check:deps`,
  `git diff --check`: clean.
- Not run: `test:storage:gcs:cloud` (merge-to-main, real credentials).

### Process exceptions and diagnoses

- RED: the library facade and the Wyrd adapters were written before their
  assertions. This is a recorded process exception, not a RED pass. Mutation
  checks stand in:
  - comparing the full review from `pins.base`: fails J:1285;
  - disabling the truncated-listing rebuild: fails J:1336;
  - a touch that ignores claims: fails Q:418;
  - pointer protection without repository correlation: fails Q:517;
  - earlier, reverting pin handling and baseline selection failed the
    journeys.
- `collection_claims_race_touches_and_pointers_and_retries`.
  - Symptom: the retry claim returned `[]`.
  - Cause: the retry used a 3600s grace, unlike the original collection. The
    library's reference `MemorySnapshotStore::unreferenced` applies the grace
    to claimed records too, so the SQL is right.
  - Fix site: the test now retries with the original grace.
- `handle_crud::gcs_handle_crud`.
  - Symptom: `handle_crud.rs:124` failed on fake-gcs-server.
  - Evidence: curl showed that `x-goog-if-generation-match: 0` returned 200
    twice and the object was overwritten.
  - Cause: the emulator ignores GCS preconditions. object_store 0.14
    `gcp/client.rs:406` sends the header and maps 412 to `AlreadyExists`.
  - Fix site: lane routing, following the `gcs_multipart_*` precedent.
    `run_create_only` runs on local, S3 and Azure; `gcs_create_only` runs on
    real GCS only.
  - A fresh diagnostician agreed.
- `storage_e2e::local_client_server_round_trip`.
  - Symptom: HTTP 401 on `GET /v1/cards/download/local`.
  - Cause: the test replaced the server's root-relative plan with an absolute
    URL. Since `e36474189`, `authenticated()` sends credentials only for
    root-relative plans.
  - Fix site: the test now uses `download.plan`.
  - A fresh diagnostician agreed; no other caller hand-builds that URL.

Non-goals stayed excluded: no metadata traits over Bifrost/Scribe, no
execution IDs, no coordination service, no Task 005 execution evidence. The
only change outside the task's write set is the stale storage journey fix
above.
