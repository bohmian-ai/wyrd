# Focused follow-up: snapshot publication versus collection

Result: **RESOLVED**. No new finding proposed. This report resolves the specific protocol uncertainty; it does not decide the task verdict or specification approval.

## Immutable subject and scope

Wyrd base `81aca5479`, candidate `359a19f41597b8cfdc673373409b0d96e8298447`; library base `d8c772de4248ce0130f79ec3b5fefda518b1431a`, candidate `d380f57fa53429eaa4f3e7272ce651e1825a8b38` in `/home/thorrester/Documents/GitHub/bohmian-code-review`. Both checked-out HEADs equal the recorded candidates. Read-only source inspection; only this report was written. CodeGraph preceded Wyrd code navigation; the library has no root `.codegraph` directory.

Question: does the SQL pointer/claim implementation violate the required library protocol at a supported Task 004 boundary, despite the library's touch-before-publication and collection-grace preconditions? Reports were used to locate the conflict, not as source proof.

## Authority and meaning of the boundary

The original task at base, `tasks/004-pinned-pr-preparation.md:60-64`, requires preservation of “atomic collection claims against upserts/pointer publication, grace periods and retryable cleanup.” Its adjacent metadata bullet explicitly asks for the library protocols equivalent to its README three-table schema. The task's proof requires actual adapters and collection versus upsert/publication. No task text introduces a new arbitrary-delay publication guarantee or removes the library's collection preconditions.

Library `src/persistence.rs:151-163` defines the workflow being implemented: the library refreshes the record before creating its object and moves the pointer after validation; claiming and refusal of a claimed refresh prevent a build from refreshing and then publishing a key already being deleted. `SnapshotStore::unreferenced` at `:209-225` requires atomic selection/claim against record upserts and pointer swaps and gives one `UPDATE ... RETURNING` as its example. `publish_snapshot_pointer` at `:174-193` promises tree CAS; its documented outcomes are moved versus competing publication and backend error. It does not promise a claimed-record guard.

Crucially, the library's supplied PostgreSQL implementation in `README.md:289-321,350-374` prescribes the same relevant design as the candidate: pointer INSERT/tree-conditioned UPDATE has no record lock or claimed guard; a single record UPDATE selects old unreferenced rows using NOT EXISTS over pointers and returns them. The README also explicitly documents grace longer than the slowest build at `:536`. These source examples cannot override a stronger Wyrd contract, but here they establish what the task's instruction to preserve these particular library protocols means. Treating the atomicity wording as a requirement to reject all direct post-claim CAS calls would add an outcome absent from the protocol and inconsistent with its supplied implementation.

The separate library `MemorySnapshotStore` reinforces that distinction: `src/testing.rs:292-307` performs pointer CAS under the pointer mutex without looking at record state. `:344-369` takes pointer and record locks to select and claim together. Even that implementation permits a subsequent direct CAS to a claimed key. Mutual ordering of claim and pointer swap is therefore not equivalent to the stronger proposed rule that a pointer publication can never acknowledge a claimed target. B-004-1's sequential claim-then-publish demonstration does not, by itself, falsify atomic selection/claim: those two operations are already ordered, and that demonstration is possible in the reference store too.

## Actual producers and consumers

All non-test library `publish_snapshot_pointer` calls converge on `ColdBuild::publish`, `src/snapshot.rs:887-925`. It serializes the snapshot, refreshes the deterministic record at `:898`, creates/adopts identical object bytes at `:899-909`, and only then tree-CAS publishes at `:917-923`. The sole other occurrences are the trait declaration, store implementations/delegation, and tests. `Update::run`, `src/update.rs:88-151`, ends through the existing cold build/complete owner, rather than publishing independently. Pinned preparation likewise reuses the existing snapshot owner; there is no second library writer bypassing the refresh.

The consumer deleting snapshot bytes is `Snapshots::collect`, `src/snapshot.rs:403-438`: it claims, validates namespace, deletes the object, then deletes its record. Its explicit caller obligation at `:413-417` is grace longer than the longest build, with no overlapping collections for one identity. `remove_repository`, `:441-455`, permits zero grace only with no running build. These are lifecycle contracts, not merely deployment advice.

Wyrd `CodeReviewMetadata`, `src/components/github/review_cache.rs:249-370`, implements those trait methods. Its `tenant` owner at `:214-227` opens/commits each protocol call before returning to object IO. SQL `src/queries/code_review_cache.rs:28-61` implements first/tree CAS, guarded record refresh, and one atomic claim statement. The migration supplies no claimed-target publication trigger, and none is inferred here.

For an aged key the supported orderings are:

1. Claim wins before the build's refresh: refresh's same-row conflict update sees `claimed=true`, returns no row, and the adapter returns `Collecting` (`review_cache.rs:310-319`). The build exits before object creation and pointer CAS.
2. Refresh wins: claim's conflicting-row recheck sees the refreshed timestamp. Grace exceeds the remaining build duration, so the record cannot be eligible before the pointer is published. A later claim sees the pointer and excludes the key.
3. Publication moves away from an older pointer: a collector whose statement snapshot still sees that older reference conservatively leaves the old key for a later collection. This cannot delete the new target, whose refresh is still protected by grace.

The supplied library regression `snapshot.rs:2996-3070` deliberately exercises both claim-first and refresh-first publication orderings using held deletes/swaps and a 60-second grace without advancing the test clock during the build. Its assertions check `Collecting`, retry producing a readable object, and exclusion of a refreshed/published key. The candidate SQL claims/touches tests cover the same-row decision, and existing actual-adapter tests cover claim retry; this review did not execute them or claim they prove an arbitrary pointer/claim schedule.

## Resolution and correction burden

No concrete supported Task 004 failure was found. Publishing directly to an already-claimed record bypasses the required producer refresh. Refreshing and then waiting past grace before CAS, or overlapping an active build with zero-grace removal, violates the collection contract. Those scenarios establish the absence of a stronger unconditional guarantee, not loss of a guarantee shipped by this task.

The SQL does not provide a shared record lock for arbitrary concurrent pointer writes, and this report does not label it globally serializable. The prescribed one-statement selection/claim, same-row refresh arbitration, pointer exclusion, and required grace together preserve the supported publication/collection protocol. The literal words “against ... pointer swaps” should not be detached from that protocol to require a new claimed-target CAS outcome: the trait, its PostgreSQL example, its reference memory implementation and its documented caller preconditions must be read together. There is no stronger architecture requirement: `architecture/wyrd-design.md:2098-2112` establishes cache custody/isolation and separation from retained evidence, rather than redefining collection concurrency.

Accordingly, the proposed direct post-claim pointer-publication consequence is not a supported failing path, and the required correction in B-004-1 exceeds the demonstrated defect. No SQL/adapters remediation is warranted on this evidence. Additional common-lock/target-existence/claimed-target refusal machinery would strengthen library semantics and change the correction boundary; it should not be required as a local fix absent an explicit new concurrency requirement. This conclusion does not waive grace, claim/touch arbitration, CAS, tenant isolation, retry, or any actual-adapter proof requirement.

No tests, mutations, service outages, or cloud checks were run. No source changes were made; production cleanup scheduling and enforcement of future callers' grace choices remain outside this focused review.
