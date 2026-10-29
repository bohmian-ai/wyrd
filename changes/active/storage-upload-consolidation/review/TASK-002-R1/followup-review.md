# Focused follow-up: BEH-001 correction boundary

Result: **RESOLVED**. The failing path is source-proven. Revise the original correction recommendation to require distinct final object paths for independently initialized File sessions, using the existing upload identity under the existing digest namespace. This is bounded implementation plumbing under the approved specification, not a requirement for a new concurrency service or specification revision. The independent validator must validate this recommendation; this report is discovery, not final validation.

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
Candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
HEAD remained the candidate. No source or tests were modified. No `.codegraph/` exists.

Assigned uncertainty: whether BEH-001 can be corrected without an expensive new concurrency, public-contract, resource-ownership, or persistent-data decision. I inspected the relevant cumulative diff and traced File planning, shared initialization, SQL identity/indexes, persisted paths, local and cloud transfer, completion, abort, inventory selection, and existing sweeper consumers. This is not a repeat of comprehensive TASK-002 discovery.

Authorities inspected: root AGENTS and SQL AGENTS; approved specification revision 2; original TASK-002 requirements independently of implementation evidence; `changes/spec-driven-development.md` (particularly private persistence/concurrency mechanics); maintainer guide; Wyrd design's evidence routes and heavy-artifact coordination; Bifrost/security authority for inventory and verified tenant prefixes. No new proposed finding arose.

## Established cause and observable failure

`VerificationControl::evidence` (`crates/wyrd/wyrd-server/src/components/verification/service.rs:375–399`) checks completed File inventory, then opens a new upload when no completed match exists. Two independent calls can both take that branch before either finishes.

`file_upload_init` (`crates/wyrd/wyrd-storage/src/service.rs:202–226`) supplies no Card and no idempotency key, and builds the same digest-derived path. `init_upload` (`:269–287`) invokes `find_and_mark_prior_pending`, which finds a NULL-Card pending row with that relative path and declared digest and unconditionally marks it aborted (`:812–834`; SQL `queries/storage/multipart_uploads.rs:392–436`). The initialization lock serializes initialization; it does not make the second request a retry of the first invocation.

Thus A-plan, B-plan, A-transfer is sufficient: B has revoked A's returned session. A local transfer loads only pending state (`service.rs:650`), and A completion accepts pending/completed but refuses aborted (`:484–492`). Shared-client `RunEvidence::pin` (`crates/shared/wyrd-client/src/verify/mod.rs:522–549`) propagates the storage failure, so ordinary parallel verification of a common File fails. No execution race or unusual forged input is needed.

## Smaller alternatives tested against source

| Alternative | Source consequence | Assessment |
|---|---|---|
| Delete only File's prior-abort call | The unique `storage_multipart_uploads_storage_path_active` index covers initiating and pending rows at the same path (`20260601000002_storage.sql:48–50`). B's insert conflicts. | Insufficient. Keep the index. |
| Remove/widen that index to allow independent sessions at the same object key | All protocols still write/commit one mutable key. Azure stages and commits fixed block IDs at that key, independent of upload identity (`cloud.rs:298–315,373–392,680–685`). Distinct SQL rows do not isolate its block sets. | Insufficient; also discards an existing safety property. |
| Replay A's live session to B | Both callers receive one mutable backend upload. S3/GCS completion and abort target the same persisted backend ID (`cloud.rs:316–324,368–392,403–418`); completing twice is not the existing completed-row replay when both requests first observe pending. One caller's explicit abort changes their shared row. Azure's block IDs and commit are also shared. | Requires new shared-session ownership/commit semantics. Do not prescribe it. |
| Refuse B with readiness/conflict, or add caller retries | A survives but the independent supported File operation now fails or depends on new retry/wait behavior. REQ-013's inventory visibility failure does not establish tenant-wide admission exclusivity for live File transfers. | Does not meet the retained regression outcome by itself. |
| One final key per existing upload identity beneath the digest namespace | Both initiating/pending rows pass the unchanged active-path index; every protocol uses a separate location; transfer/completion replay of one upload retains its persisted location. Inventory already selects a matching completed File and returns the recorded location. | Smallest sufficient correction. |

Azure precision: ordinary upload rows currently persist a backend ID only for `MultipartV1` (`service.rs:1156–1162`). Therefore ordinary Azure explicit abort/sweeper paths with no handle do not call cloud blob deletion. The shared-session objection does not depend on claiming otherwise: shared SQL terminal state, shared fixed block IDs, and shared completion are enough. `cloud.rs:403–418` does delete by location if its Azure abort branch is called, so a proposed generalization must not turn it into a cross-session deletion mechanism.

## Recommended correction, within existing authority

Give every independently initialized File session its own final object location beneath `<tenant>/evidence/sha256/<digest>/`, distinguished by its already existing `UploadId`. Use the same identity for the SQL session and its path. Persist that path once and keep it through transfer, backend commit, inventory recording, and completion replay. Apply destructive pending replacement only to Card reinitialization. Retain the existing planner, dispatcher, unique active-path index, ownership authorization, limits, local hash validation, and backend lifecycle.

This requires a bounded change to the existing path builder/strict validator and initialization path, plus affected fixtures and consumers. The File validator must continue to validate the verified tenant, canonical digest, and strict server-generated session segment, and reject Card paths/traversal/foreign tenants. Service `row_path`, inventory `file_fold` (`inventory.rs:360–398`), and the sweeper must agree on that File shape; BEH-002 already covers the sweeper omission, so do not invent another finding for it. Do not add a second upload owner, generalized path resolver, lock protocol, queue, reference count, new ID system, or physical-object promotion. No provider implementation change is needed to isolate A from B once their locations differ.

The authority supports that correction:

- REQ-004 explicitly allows the existing upload identity to associate retries and does not require a new object-ID or global digest deduplication service. SQL already stores both upload ID and full storage path.
- REQ-010 requires digest/size selection from completed File inventory and reads the recorded location. It does not mandate deriving that location from the declaration. `ObjectInventory::file_object` queries by origin/digest and `file_fold` can select one undeleted matching path from multiple completed objects. It does not require every matching digest to have one physical object.
- Current Wyrd design `:1690` says plans live **under** the tenant's `evidence/sha256/<digest>` namespace. A session leaf inside it preserves that namespace. The exact no-suffix key is stated in implementation rustdoc/validation (`tenant_path.rs:114–163`, `service.rs:192–197`), which cannot elevate this particular implementation into an approved material constraint. Keep the owner documentation accurate and make the session leaf explicit if clarification is useful.
- The base design's removed direct-write text specified the exact digest key and idempotent repeated writes. The approved change deliberately replaces that direct evidence path and location inference with shared sessions and inventory-based resolution. Its fresh-install scope and REQ-017 exclude upgrade/data-preservation obligations. There is no deployed data migration or compatibility alias to design.
- This is a final location from initialization onward: no temporary-to-final promotion, immutability guarantee, checksum protocol, or new hashing pass (REQ-003). Retrying the same upload still refers to the same logical stored object. Independently opening two uploads can retain two File objects, which is not prohibited global physical deduplication or cross-owner sharing. Existing evidence retention remains unchanged; do not add garbage collection to remove the extra object.

Changing the public File declaration, sharing a live session between independent callers, retaining a deterministic final key through promotion, or imposing tenant-wide waiting would create the material decisions the review must avoid. None is necessary for the recommendation above. The present report therefore does **not** recommend `SPEC_REVISION_REQUIRED`.

## Focused closure proof and limits

Use existing Postgres/local and backend fixtures. Obtain A and B plans for identical same-tenant bytes before either completes; both complete successfully; verification resolves a completed File from the inventory; replay completion for one upload and confirm its location/logical inventory identity remains stable. Separately abort A and finish B to establish independent lifecycle state, and retain Card re-init replacement proof. Keep the existing shared backend lanes: S3/GCS multipart handles must target separate locations, and Azure is the important fixed-block-ID case. Existing malformed/foreign-path tests must cover the new strict shape. No load test, new harness, broader retry service, or broad cargo run is warranted for this correction.

This was static source review, not implementation or runtime proof. I did not run tests or credentialed GCS multipart. Same-upload completion/abort races are existing lifecycle behavior and are not strengthened here; the correction prevents introducing cross-invocation coupling through a shared File key/session. Independent final validation remains required.
