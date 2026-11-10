# Human-approved correction revision: authoritative publication readback

This bounded independent recommendation check revises the selected correction for **FIND-TASK-001-3** at the human's direction. It adds no finding and does not change the original candidate, diagnosis or finding IDs. Preserve findings-validation.md as the original review record; this addendum supersedes its proposed inventory-wait/defer correction for publication content already completed by the server.

**Validated recommendation:** retain the existing File owner's verified completion descriptor as private execution-publication metadata, committed with named acceptance. Resolve accepted publication mounts and authorized download/content from that metadata, rather than rediscovering the object through Bifrost inventory. No new wait/defer mechanism is required for this publication readback path.

## Bounded source evidence

Inspected original detached candidate `365bfc94bfef1a153f213462f7ef104afa823741` in `/tmp/wyrd-task-001-review-365bfc94b`; no source changes or execution jobs.

- `wyrd-storage/src/service.rs:600–662`: reconcile_upload uses finish_upload, which verifies the completed backend object, commits completed upload state in PostgreSQL, then returns StoredObjectRef containing the verified tenant storage path, length, digest and optional storage metadata. `persist_completed_upload:1532` already shares completion with Card manifests. This descriptor is an existing verified location, not a new durable Artifact identity.
- `wyrd-server/src/verification/hosted/publications.rs:153–190`: complete receives completed.stored, records the existing inventory fact/ACK and discards the descriptor by returning (). The accepted public manifest retains only relative path/digest/size. The information needed for direct readback is lost at this producer boundary.
- `hosted/evidence.rs::published/file_object` and `components/verification/service/inspection.rs:456–512`: mounts and file/download/content inspection subsequently query inventory by digest/size. That analytical lookup creates the avoidable visibility dependency.
- `wyrd-storage/src/service.rs:842–907`: existing download_plan and download_local_object accept ValidatedPath. Existing verified-content loading can retain its length/digest checks. No replacement storage reader or signing path is needed.
- Card reads' `manifest_completion_rows` is an ownership precedent, not a table to reuse for non-Card publications: Card manifests have an actual owning Card and their cleanup lifecycle.

## Selected correction boundary and preserved behavior

Carry the File completion descriptor through the existing trusted publication backend and persist the required location/completion metadata with the accepted publication's execution/producer/name and manifest-file association. It is private authoritative metadata under the existing tenant SQL/publication owner; public PublicationFile/manifest, producer tuple, call replay identity and wire shapes remain unchanged. Read it by the selected publication, not a caller-supplied object path or a global digest lookup. Validate the recorded evidence path against the caller's tenant with the existing File path validator, and match descriptor digest/length to the accepted public manifest. Digest encodings must use the existing conversions. Keep existing verifier read/run decisions, directory file selection, object integrity checks and effective size ceilings.

Change both sibling consumers: hosted named/collection mounts and inspection download/local-content. Reuse storage download planning/streaming and verified byte loading. Do not change ordinary uploaded input-File discovery, public File dedup, Artifact/Card reads or public authorization as collateral work. Update the owning publication readback description where it implies inventory query visibility is required for an already accepted publication. Keep honest storage absence/failure handling; direct location metadata does not promise unavailable bytes can be read.

Preserve the existing File completion inventory ACK contract and required history/projection intent/delivery. The narrow correction removes the **readback query dependency**, not Scribe durability or inventory recording. Weakening completion ACK or adding a second delivery service is unnecessary. If implementation separately changes that handoff, it must prove the already approved durable delivery guarantee rather than relying on process memory.

## Retention compatibility

The existing upload sweeper selects only expired pending/orphaned initiating rows (`wyrd-sql/.../storage/admin/multipart_uploads.rs:46`); its mutation rechecks open state and leaves completed uploads intact (`wyrd-storage/src/sweeper.rs:288–386`). Completed File objects therefore do not become reclaimable merely because their upload session's expires_at passes. Card cleanup follows Card manifests/owned paths; publications already complete Card-less evidence objects, so this correction must not replace them with a borrowed Card path that Card deletion can remove.

Persisted accepted-publication association is the retention root for this content. Do not persist only an upload capability, require a live resumable session, infer a path from digest, or create a new blob catalog/GC service. Existing File/storage lifecycle remains responsible for unaccepted uploads and eventual reclamation; any touched cleanup path must preserve objects referenced by retained accepted publications and release them only under the existing approved publication/execution retention outcome. No completed-File GC path deleting accepted evidence was established in this bounded source scope, so no additional retention blocker is retained. This is not evidence that a general unreferenced-File collector already exists.

## Focused closure proof

Using existing hosted/server proof seams, complete and accept file/directory publications while keeping their inventory **query projection** invisible; retain actual File completion ACK. Mount exact bytes and obtain/redeem authorized download/local-content immediately, with no inventory query, provider retry, spent attempt or new deferral. Restart over the same durable backing and repeat readback. Check same-call receipt replay returns the original descriptor association after scratch mutation, foreign-tenant/other-Verifier/unsafe directory selection still refuses, and bytes continue to match manifest digest/length. Exercise session expiry/sweeper after acceptance to show completed referenced content survives. Keep refused/stale partial acceptance invisible and preserve inventory/history projection assertions in existing tests.

Skipped runtime and GC fault injection; this validates the selected existing-owner correction, with implementation responsible for the focused durability/retention proofs above.
