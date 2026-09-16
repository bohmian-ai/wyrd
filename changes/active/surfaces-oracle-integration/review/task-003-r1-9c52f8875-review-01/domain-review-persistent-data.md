# Persistent Data, Tenancy, Durability, and Concurrency Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Reviewed tasks: original `TASK-003`, original `TASK-004`, prior cumulative verdict/findings, and remediation `TASK-003-R1`

The repository has no `.codegraph/` directory, so caller and lifecycle tracing
used immutable Git objects and repository search. The candidate commit and tree
matched the supplied identities throughout the review. The shared checkout's
unrelated changes were not used as candidate source evidence.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- **`PERSIST-R1-01` — DRIFT / VIOLATION — [`crates/wyrd/wyrd-storage/src/settings.rs:21`](../../../../../../crates/wyrd/wyrd-storage/src/settings.rs), [`settings.rs:76`](../../../../../../crates/wyrd/wyrd-storage/src/settings.rs), [`settings.rs:177`](../../../../../../crates/wyrd/wyrd-storage/src/settings.rs): storage deployment configuration was replaced outside the approved remediation boundary.** `TASK-003-R1` lines 30-47 limits remediation to five retained findings, says no correction needs a configuration value, and lines 187-190 prohibit a new runtime knob. Candidate commit `9c52f8875` nevertheless deletes the established `WYRD_STORAGE_BACKEND` plus backend-specific location variables and makes new `WYRD_STORAGE_URL` and `WYRD_STORAGE_ENDPOINT_URL` values mandatory across server boot, storage factories, Iceberg warehouse derivation, documentation, emulator lanes, and live-cloud workflows. None of the five retained findings concerns storage configuration; the nightly finding explicitly says to reuse existing tasks and not duplicate storage. A deployment upgraded with the previously accepted variables now fails startup at `from_env`, while an operator must also absorb changed S3 region and path-style semantics. This is a reachable availability and deployment-migration regression, not a test-only change. **Correction:** remove the configuration consolidation and restore the candidate's pre-remediation `StorageSettings`/factory/deployment contract. If a storage lane could previously skip after selection, make that existing lane fail on missing existing variables and use exact nonzero test selection; do not redefine the production configuration contract to prove the lane. A one-URL storage contract requires its own approved specification decision rather than this remediation.

### Low / Defense In Depth

- None retained. The task expressly excludes hostile-filesystem/symlink hardening beyond the implemented ordinary write probe.

### Positive Controls

- `BifrostDataRoot::prepare` retains one exclusive root lock and now create/write/sync/remove-probes the WAL, Scribe stage, Scribe output scratch, and Oracle spill directories before role construction. The focused read-only-child test exercises the previously missing failure.
- `OracleQueryAudit` takes a pool-sized pending permit before spawning, retains the distinct quarter-pool connection semaphore, emits no task on saturation, and uses the existing scrubbed failure log and counter. The locked-chain-head journey drives twice the pool size, proves the pending ceiling, verifies spare pool capacity, and drains admitted commits after release.
- Oracle read and tripwire events still enter only canonical tenant-scoped `vala.audit_staging` through `TenantConn`; no alternate WAL, relay, table, or cross-tenant executor was introduced. Oracle/Scribe shutdown closes the shared tracker and treats retained audit tasks as an incomplete shutdown.
- Multipart completion remains tenant-RLS scoped and preserves terminal safety: the storage route may accept a row Card activation already completed, preserves the original `completed_at`, and still refuses initiating, aborted, failed, missing, or cross-tenant rows. The route verifies backend completion and object metadata before its transaction persists artifact metadata and completion.
- Storage URL parsing itself rejects embedded credentials, queries, fragments, invalid schemes, extra object-location path segments, and malformed endpoint URLs without echoing the supplied value. These controls are sound but do not authorize the unrequested configuration replacement.
- The system-owner UUIDv7 migration, audit staging RLS/hash-chain authority, per-tenant frozen publication range, and Forge orphan-cleanup uniqueness migration remain coherent in the cumulative candidate.

## Boundary coverage

| Boundary | Evidence traced | Result |
|---|---|---|
| `FIND-TASK-004-1`: data-root preflight, lock lifetime, role activation, replica ownership, and write probe | `boot/data_root.rs`; boot composition; Scribe stage/output mutation; Oracle spill; focused root tests | **PASS** |
| `FIND-TASK-003-1`: Oracle audit admission, pool share, total pending bound, canonical append, tenant isolation, and shutdown | `oracle/query_audit.rs`; both `OracleAudit` entry points; Oracle/Scribe ownership and shutdown; locked-chain journey; staging SQL/RLS | **PASS** |
| Audit publication durability and tenant staging | canonical append; hash-chain migration/query owner; frozen range/watermark settlement; publisher and multi-tenant journeys | **PASS** |
| Storage configuration and backend binding | `settings.rs`; S3/GCS/Azure factories; `StorageHandle`; Bifrost catalog properties/warehouse URI; docs, mise, and cloud workflow | **FAIL**: `PERSIST-R1-01` |
| Multipart completion concurrency | storage completion route; Card activation; `mark_completed`/`mark_completed_if_pending`; RLS migration tests | **PASS** |
| Relevant persistent migrations and recovery tests | system-owner sentinel; Oracle reader authority; Forge orphan-cleanup active uniqueness; Postgres migration tests | **PASS** |

## Prior-finding closure

- Prior `PERSIST-DATA-01` / stable `FIND-TASK-004-1` is closed by the shared-owner write probe and focused failure proof.
- Prior `PERSIST-DATA-02` / stable `FIND-TASK-003-1` is closed by the pre-spawn pending semaphore and saturation/drain journey.
- No regression was found in the one audit staging path, retained-publication recovery, Scribe WAL/restart authority, Oracle spill cleanup, Forge durable cleanup fencing, or tenant RLS boundaries.

## Verification limits

- This was a review-only source, migration, configuration, caller, and test-evidence audit; no candidate source or test was changed or rerun.
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf 9c52f887595d4e8a04764d3d2836926e5df70950` passed.
- Recorded local verification covers the root tests, bounded-audit journey, migration tests, emulator storage lanes, broad gate, and language journeys. Credentialed cloud evidence has no candidate-SHA workflow run; that is an acceptance-evidence limit, not the basis of `PERSIST-R1-01`.
- `architecture/wyrd-security-posture.md` and parts of `architecture/operations/` still describe a retired Oracle local acceptance WAL, while revision 8, `AGENTS.md`, and `architecture/bifrost-design.md` require the tracked non-blocking staging task. `TASK-003-R1` explicitly excludes a stale-authority documentation sweep, so this review records the conflict as a final-change-review limit rather than a new remediation finding.

## Overall result

**FAIL** — the two retained persistent-data findings are correctly closed and no exploitable tenancy, injection, secret, or authorization regression was found, but the candidate adds an unapproved, deployment-breaking storage configuration contract outside the bounded remediation task. Proposed finding: `PERSIST-R1-01`.
