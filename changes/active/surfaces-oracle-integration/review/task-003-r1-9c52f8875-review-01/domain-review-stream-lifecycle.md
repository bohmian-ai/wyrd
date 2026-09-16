# Domain Review: Bifrost Stream Lifecycle and Resource Ownership

## Review Findings

### Critical

No critical findings.

### Important

- **`STREAM-R1-001` — DRIFT — [`crates/wyrd/wyrd-storage/src/settings.rs:21`](../../../../../../crates/wyrd/wyrd-storage/src/settings.rs#L21): the candidate replaces the repository's storage configuration contract even though TASK-003-R1 explicitly excludes new public APIs and runtime knobs.**
  - **Violated obligation:** TASK-003-R1 is limited to five retained findings and says that no correction needs a configuration value (`TASK-003-R1-close-cumulative-review-findings.md:30-47`), requires preservation of already-passing integration behavior (`:63-69`), and makes new public APIs and runtime knobs explicit non-goals (`:187-195`). The approved integration spec also keeps Surfaces authoritative outside Bifrost (`REQ-003`, `INV-001`).
  - **Exact evidence:** commit `9c52f8875` changes 47 files and replaces `WYRD_STORAGE_BACKEND` plus the existing backend-specific bucket, root, account, container, region, endpoint, and path-style settings with required `WYRD_STORAGE_URL`, optional `WYRD_STORAGE_ENDPOINT_URL`, and `AWS_REGION`. `BackendConfig::from_url` at `settings.rs:76-142` is a new public parser and configuration contract; `from_env` at `settings.rs:177-235` refuses startup without the new variable. The public self-hosting contract is rewritten around that new surface at `docs/src/content/docs/self-hosting/storage.svx:11-92`. None of the five issue diagnoses or acceptance criteria at TASK-003-R1 lines 73-205 requests or authorizes this redesign. The task's own implementation record acknowledges the added consolidation at lines 326-348 while incorrectly claiming at lines 385-390 that no public API or runtime knob was added.
  - **Observable consequence:** an otherwise valid deployment using the pre-candidate documented storage variables now fails boot because `WYRD_STORAGE_URL` is mandatory. The candidate also changes supported endpoint behavior and removes public test/harness constructors. This is a cross-cutting deployment and API decision bundled into a bounded lifecycle/CI remediation, not the smallest correction for a skipped storage lane.
  - **Required testable correction:** remove the storage-configuration consolidation from this candidate and restore the pre-`9c52f8875` storage settings, factories, public constructors, docs, and consumers. Keep storage lanes non-skipping by supplying and asserting their existing backend configuration in the owning `mise` tasks/tests; that proof fix does not require changing production configuration. If one-URL storage configuration is desired, route it through a separately approved specification because it changes public deployment behavior and the shared artifact/Bifrost storage owner.

### Suggestions

No optional improvements.

## Open Questions

None. The storage rewrite is not needed to satisfy any retained TASK-003-R1 finding and is expressly outside its approved correction boundary.

## Prior Finding Closure and Adjacent Lifecycle Results

| Obligation | Result | Evidence |
|---|---|---|
| `FIND-TASK-004-1`: prove every managed Bifrost data-root child usable before role activation | **PASS** | `BifrostDataRoot::prepare` retains the one root owner and lock, then creates/writes/syncs/removes one fixed probe in each managed directory (`crates/wyrd/wyrd-server/src/boot/data_root.rs:66-171`). All four focused tests passed independently. |
| `FIND-TASK-003-1`: bound non-blocking Oracle audit ownership without starving the Vala pool | **PASS** | `OracleQueryAudit` keeps the quarter-pool connection semaphore and adds a pool-sized pending semaphore acquired with `try_acquire_owned` before spawn (`crates/wyrd/wyrd-server/src/oracle/query_audit.rs:40-109`). Saturation follows the existing logged/counted failure path and never blocks the read. The locked-chain-head journey passed independently. |
| SDK ambiguous-write retry proof | **PASS** | The production producer/retry owner is unchanged. The test replaces a guessed 350 ms sleep with a bounded wait for the retained retry entry to settle, then still proves stable batch identity, stable byte ownership, exact one-row durability, and zero terminal ownership (`crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:1285-1449`). The exact focused journey passed independently. |
| Forge uncertain-publication recovery proof | **PASS** | Production Forge code at the final candidate is unchanged from `bbfdf35e2` because the attempted production change was reverted. The test now admits the two stop-originated terminal strings emitted by the existing authority-check/checkpoint paths while retaining the durable operation, task-state, and exact recovery assertions (`crates/vala/vala-bifrost-redux/tests/integration/forge/compaction_admission.rs:2499-2663`). The exact focused test passed independently. |
| Prior direct-write shutdown, producer terminal-refusal, query terminal/settlement, Scribe admission, and Forge recovery corrections | **PASS / not reopened** | Complete cumulative source inspection found no post-`bbfdf35e2` production change to these owners. The final candidate keeps the reviewed `WriterPool`, `Producer`, `QueryResultStream`, Scribe reservation, and Forge settlement behavior. |

## Reviewed Boundary and Authority Coverage

- Immutable subject: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `9c52f887595d4e8a04764d3d2836926e5df70950`; remediation delta `bbfdf35e26212b2a831bda5e31e1ef4433e41900..9c52f887595d4e8a04764d3d2836926e5df70950`.
- Authorities read completely for this boundary: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/bifrost-design.md`, `architecture/references/domain/analytical-operations-reliability.md`, approved specification revision 8, TASK-002/TASK-002-R3, TASK-003, TASK-004, the prior cumulative Wave-1/Wave-2 reports, and TASK-003-R1.
- Source and caller coverage: Oracle audit admission/tracking/shutdown and its locked-chain journey; the Bifrost data-root owner and boot tests; SDK producer retry/settlement proof; Forge worker cancellation/publication refusal and recovery test; the complete remediation diff affecting storage settings, factories, handles, server/test composition, workflows, docs, and `mise` lanes. CodeGraph was unavailable because the repository has no `.codegraph/` index.

## Verification Notes

- Independently passed: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^boot::data_root::tests::/)'` — 4/4.
- Independently passed through repository-managed Postgres: `wyrd-testing::server audit_publication::reads_are_served_while_audit_commits_wait_on_the_chain_head` — 1/1.
- Independently passed through repository-managed Postgres: `wyrd-client::pg_bifrost_e2e pg_tests::public_sdk_owned_batch_timeout_retry_deduplicates_and_settles` — 1/1.
- Independently passed through repository-managed Postgres: `vala-bifrost-redux::integration forge::compaction_admission::acceptance_unknown_recovers_from_durable_state` — 1/1.
- `git diff --check` passed for both the cumulative base-to-candidate range and the remediation delta.
- The recorded broad `gate` and storage-emulator results were reviewed but not rerun in this domain pass. Passing checks do not authorize the unrelated production configuration change.

## Overall Result

**FAIL.** The bounded Oracle audit owner, managed-directory probes, SDK retry proof, and test-only Forge assertion preserve the required lifecycle behavior. The candidate nevertheless adds a large public storage configuration redesign outside every retained finding and in direct conflict with TASK-003-R1's non-goals; remove that drift or obtain a separately approved specification before acceptance.
