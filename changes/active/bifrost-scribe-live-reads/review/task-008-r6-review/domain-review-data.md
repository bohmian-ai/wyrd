# Persistent data and durability review

**PASS. No material proposed findings in this domain. FIND-007-13's durable-boundary descriptions are corrected.**

## Subject and limits

Candidate `8955e75b71ded9d39daf7649a0c985be0803e266`; correction parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed cumulative durability changes and the complete R6 correction diff against spec revision 20, original TASK-007/008, prior remediation and R5 verdict, and R6 task/evidence. HEAD remained the named candidate.

Static review only: no cargo, nextest, mise, build, test, benchmark or commit. Historical runtime results in the task packet are supplied evidence, not fresh execution. `git diff --check HEAD~1` passed. Only this report was written. Other discovery reports were not read.

## Boundary and authority coverage

| Boundary | Applicable authority | Source inspected |
|---|---|---|
| ACK, WAL retirement, staged authority | AGENTS.md ownership/durability and testing rules; agent-rules; spec INV-001/004 and REQ-014/015; Bifrost ingest lifecycle | `scribe/mod.rs`, `memtable.rs`, `persistence.rs`, `hot_stage.rs`, `staging_runtime.rs`, original task requirements and cumulative diff |
| Footer identity and rewrite provenance | Spec REQ-015/AC-017; Bifrost physical identity/assembly/promotion; OLAP-serving and Iceberg references | `parquet/footer.rs`, `writer_properties.rs`, `scribe/parquet_writer.rs`, `claim_assembly.rs`, `member_stager.rs`, Forge managed policy/executor |
| Shutdown, restoration, publication clock | Standing no-shutdown-residue-publication decision; spec REQ-016 and INV-001; Bifrost recovery/shutdown; analytical-operations-reliability | `scribe/mod.rs::shutdown/replay_wal_async/flush_staged/publish_due`, persistence restore/resume/drain/publication methods, assembler ready/claim ownership, server lifecycle scanner |
| Documentation and proof boundary | R6 acceptance/constraints; spec-driven-development and maintainer-style references; repository documentation rules | Reliability guide, assembly and staging-runtime rustdoc, lifecycle journey docs/comments/assertions; prior R5/R6 evidence |

The reference router, agent rules, relevant Wyrd design/doctrine ownership statements, Bifrost design sections, and domain reliability/OLAP/Iceberg references were used under the fixed maintainer decisions. Postgres tenant columns and the chosen public error name were not reopened.

## Producer-to-recovery evidence

- The encoder's `prepare_sorted_candidate` checks binding tenant against both seal tenant and frozen seal identity before materialization (`scribe/parquet_writer.rs:388`). `seal_open_artifact` stamps `BifrostFooterIdentity` with `ArtifactPlan.tenant` and inspects the sealed artifact before it is returned (`:605`). Assembly supplies `context.binding.tenant` (`staging_runtime.rs:724`); both Forge rewrite paths supply `self.binding.tenant` to policy configuration, whose writer properties stamp the tenant footer. These changes replace row tenant encoding without changing upload/fenced publication ownership.
- A generation stages and registers its durable member before its manifest advances and before it asks the existing assembler for due publication (`persistence.rs:1900–1908`). Stage recovery validates version, assembly identity, run existence/length/checksum and recorded member validity (`hot_stage.rs:785,862`). The restored encoding context verifies schema fingerprint and re-resolves the same tenant/table/layout (`staging_runtime.rs:620–671`). No new deletion or authority inferred from object listing entered the reviewed path.
- `shutdown` closes admission, flushes/drains shards, closes persistence submission and drains retained workers, then closes execution lanes (`mod.rs:1322–1385`). It has no `publish_staged_residue` call. Already admitted persistence work can finish target/dwell publication; no ready-key residue sweep is initiated by shutdown. Cancellation/deadline preserves existing durable recovery evidence.
- Startup explicitly runs `restore_staging`, `recover_staged_publications`, and `resume_staging_claims` before WAL replay/readiness (`mod.rs:2490–2512`). Ready members retain persisted `ready_at`; `StagingAssembler::restore` registers ready members and rebuilds original outstanding claim identities instead of reallocating membership (`assembly.rs:799–845`). Dwell compares the original oldest ready timestamp (`:679–685`). Restored ready residue therefore remains ordinary due work.
- The production server scanner calls `check_age` and `publish_due` on its one-second lifecycle tick (`app/server.rs:558–577`). Explicit `flush_staged` still drains and calls `publish_residue(ClaimCause::Drain)` (`mod.rs:2450–2467`); that method resumes retryable claims and sweeps ready keys through the same fenced owner (`persistence.rs:2036–2059`). Shutdown retention does not remove explicit flush, claim fences, normal publication or startup reconciliation.

## R6 closure and proof assessment

| Obligation | Source evidence | Result |
|---|---|---|
| Current guide no longer promises forced shutdown residue publication | Reliability guide now distinguishes admitted publication drain, retained staging, startup restore and publication tick | PASS |
| Assembly distinguishes explicit flush from shutdown | `ClaimCause::Drain` documentation and `ready_keys` documentation assign sweep to explicit flush and retained members to restart | PASS |
| Test descriptions match actual durable boundary | Staging-runtime test documentation names explicit flush; lifecycle journey describes durable staging/replay and zero-or-complete publication after restart | PASS |
| R6 changes no executable statement or assertion | All four named source/documentation hunks change only Markdown/rustdoc/line comments; lifecycle assertions, identifiers and attributes remain identical to parent | PASS |
| Existing recovery/readback assertions remain meaningful | Journey still asserts zero-or-full first-claim publication, exact acknowledged-row readback after restart, WAL plus staged recovery after abrupt termination, and exact readback after later explicit publication (`lifecycle.rs:163–239`) | PASS |
| No new persistent-state or durability regression | Cumulative producer/authority/recovery paths retain existing owners; latest source correction introduces no runtime mutation | PASS |

The journey's later publication proof is an explicit flush, while the separate idle-publication journey and actual production tick establish the clock path. This review does not claim a newly executed combined restart/tick test. Recorded module/journey results remain attributable supporting evidence; a documentation-only closure requires no manufactured runtime change. The TASK-006 queue synchronization delta has no staging, WAL, publication or persistent-state effect and is outside this domain's correctness coverage.

## Proposed findings

Empty. No data loss, duplicate replay, weakened publication fence, or altered persistent state was established. FIND-007-13 is closed within this domain; other historical finding closure remains subject to the orchestrator's independent reconciliation.
