# TASK-002 first review scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7` (parent of the first implementation commit `fa0870703`).
Candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
Read the complete `git diff 15f939fc9...5ef2981e6`; the user named implementation commits inclusively. HEAD must remain candidate throughout review. Initial working tree contains only unrelated untracked TASK-001-R2 review reports. No `.codegraph/` directory exists.

Authority: root AGENTS; SQL, Python, TypeScript and docs nested AGENTS; `changes/spec-driven-development.md`; approved `changes/active/storage-upload-consolidation/spec.md` revision 2; original `tasks/TASK-002-shared-file-uploads.md` (read its original obligations separately from Implementation Evidence); applicable architecture sections from Wyrd design, Bifrost design and security posture; agent guides and TESTING.md.

This is the first TASK-002 review, with no prior TASK-002 acceptance carried forward. TASK-001-R2 reports establish dependency context, not acceptance of this task. TASK-003 broader canonical readback remains excluded.

## Common navigation map

| Changed area | Owners and paths to trace | Relevant proof |
|---|---|---|
| File admission and reuse | VerificationControl::evidence; ObjectInventory::file_object; wyrd-spec verification/execution EvidenceUpload | pg_verification_routes V1–V3 |
| Shared lifecycle and ownership | wyrd-storage service upload_init/upload_owner/upload_local_blob/upload_complete/upload_abort; tenant_path; SQL multipart uploads and new nullable-card migration; storage routes authorize_upload and body limit | storage_e2e backend journeys; V3; SQL migration |
| File consumption | TenantEvidence::read/file_object/stored and existing Artifact siblings | pg_hosted_files, pg_hosted_artifacts |
| Client orchestration | wyrd-client verify File pin/projection; storage upload/dispatch; removed upload_evidence | Rust/Python/TS verifier_building_blocks; observe tests |
| Public contracts | HTTP verification routes/router/OpenAPI, MCP verification/guide, spec contracts, SDK exports/generated bindings; CLI Artifact caller | pg_openapi_contract; MCP V7; CLI V8; codegen |
| Authority and docs | changed Wyrd design evidence section, MCP docs and llms-full generator | docs/codegen evidence |

This map is a starting point; each reviewer must expand and verify callers, siblings, producers and consumers against source.

## Independent roles and exact reports

- behavior-rev: full task adversarial acceptance, negative paths, failure/recovery and invariants; `task-review-behavior.md`.
- repo-rev: broad cross-tier mandatory rules, changed-item rustdoc, SQL capability types, nested rules and contract parity; `standards-review.md`.
- maintainer-rev: substantial ownership consolidation, shared-client orchestration and evidence/inventory owner reuse; `maintainer-review.md`.
- domain-security-rev: operation authority, tenant isolation, persisted ownership, local body admission and audit boundaries; `domain-review-security.md`.
- domain-data-rev: new File upload coordination, inventory matching/readiness/deletion semantics and completion/retry lifecycle; `domain-review-data.md`.
- Fresh ponytail-rev after discovery: independent source/materiality/correction validation and coverage reconciliation; `findings-validation.md`.

During validation, one fresh followup-rev was selected to resolve BEH-001's correction boundary: the existing active-path index and backend ownership prevent naive coexistence or shared pending-session replay. Its `followup-review.md` establishes a bounded existing-UploadId final-location correction under approved authority. The independent validator reviews that report before finalizing the ledger. The standards reviewer also independently checked one additional occurrence of its existing rustdoc finding in File admission; the source-backed addendum is in `standards-review.md`.

Separate invariant/system reviewers are not selected: lifecycle and persistent state are covered by data specialist; behavior reviewer covers credible failure propagation, with unchanged TASK-001 ACK machinery as context. Security and data represent materially changed sensitive boundaries. Reports stay in this directory; no patches or source archives.

## Available verification evidence

Original task Implementation Evidence records V1–V8, backend e2e (local/S3 multipart/GCS single-put/Azure multipart), targeted hosted/route/OpenAPI modules, unit tests and format/lint/language/codegen/dependency/tenant/storage/docs checks. Treat these as claims to inspect, not acceptance conclusions. The storage e2e diagnosis is recorded there and must be falsified against fixture and ingest source; distinguish any required independent diagnosis report. Real GCS multipart has no credentialed runtime proof. Review-time verification will be recorded separately.
