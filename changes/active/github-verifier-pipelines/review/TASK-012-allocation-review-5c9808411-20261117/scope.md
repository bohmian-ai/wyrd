# Immutable subject and assignments

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004
Read-only candidate checkout: /tmp/wyrd-task012-review-5c9808411
Base: 0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd
Candidate: 5c980841140721cfaa1945fba1057675805ca77a
Diff: git diff 0ae00f429...5c9808411 (64 commits, 87 paths)
Authority: approved spec revision 16, spec-revisions/16-shared-workload-management.md, spec.md, design/shared-workload-management.md; tasks/012-R1-allocation.md is a new behavioral amendment, not a repeat of prior R1 remediation. Original task 012 and historical reviews remain context; revision 16 supersedes speculative precharge policy. Do not reopen superseded precharge findings.

Reports only are written here. Existing dirty skill/workflow files and Oracle implementation edit are excluded. Candidate checkout is detached and must not be mutated. No implementation, benchmarks, broad aggregate lanes, commits or pushes. Tests require coordinating Cargo through the orchestrator; do not start Cargo yourself. Read actual assertions and candidate-bound execution records; no background task result is assumed.

Navigation map (not acceptance):
- ProcessResources -> WorkloadManager, workload operation context, ByteCharge attribution -> CpuLanes; owners resources.rs, resources/{workload,lanes}.rs.
- main/boot -> process allocator observations -> shared manager; worker/sandbox -> child telemetry, idle custody, pressure cancellation.
- gateway invocation/body-limit -> adapter HTTP/stream -> retained response/frame/capture custody and initiating context.
- verification service/runner/hosted -> nested gateway/media -> RunState checkpoint prepare/outbox stage and pressure deferral.
- Bifrost resources/Scribe execution lanes -> ingress/member staging/Parquet; Oracle entry and QuerySpanJoinSetTracer -> physical partitions, spill; Forge managed executor.
- Tests: runtime inline resources; server sandbox_worker + pg_gateway + pg_verification_runtime; gateway adapter; redux integration; wyrd-testing Bifrost oracle capacity and verification_runtime isolated_capacity.

Independent assignments:
- behavior-rev: primary acceptance audit, all six requirements/four ACs and seven scenarios; verification/worker/state/outbox behavior, task evidence, broad changed Rust docs/dependency/portability rules and unchanged seams. Retains unassigned boundaries.
- invariant-rev: shared manager memory/pressure/fairness/CPU scheduling, charge and operation lifetimes, allocator boot observation. Sensitive concurrency/state specialty.
- system-rev: Bifrost Scribe/Oracle/Forge execution integration, task spawning, preserved IO/spill/WAL and failure progress. Separate expertise needed for DataFusion execution boundary.
- domain-rev: gateway retained body/stream/capture ownership, nested attribution, tenant/authorization and participant lifetimes.
- fresh ponytail-rev: reconcile coverage and independently source-validate all proposed findings and mandatory evidence, including empty specialist ledgers. Any new validator finding requires separate focused source validation.

A separate repo/maintainer reviewer is omitted: primary audits broad rules and minimal reuse; concrete owner architecture is covered by the assigned specialist on each affected boundary. UI/SDK/schema/SQL-security specialists are omitted because no public projection, SQL/migration or UI changes occur; primary checks this against diff and dependencies. Specialist selection is subject to independent validation.
