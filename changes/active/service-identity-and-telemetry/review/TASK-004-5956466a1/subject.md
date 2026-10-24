# Immutable remediation review

Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004
Candidate: 5956466a1299a99521ed9264b2a711d3f1957e3e
Prior candidate: f75ada758827266ac0b25a05d6a7b530f86d845a
Original base: 3e7c59db7a0f5e9506273eab826875cc8b59f226
Remediation range: git diff f75ada758 5956466a1; cumulative original base to candidate, ancestry retained. Source must remain immutable.

Authority: changes/active/service-identity-and-telemetry/spec.md revision13, supplements/table-storage-and-correlation.md, tasks/TASK-004-shared-table-storage.md; architecture/bifrost-design.md; root/nested AGENTS.md, applicable agent_docs, TESTING.md, changes/spec-driven-development.md. Prior verdict/ledger and R3: review/TASK-004-f75ada758/. Earlier R1/R2: review/TASK-004-c4dd61029/; superseded where spec13/R2 says so.

Scope: prior findings1 and8–15 closure plus latest diff and affected consumers/callers. Carry forward unaffected acceptance and findings2–7 closure; reopen only on evidence of material impact. Do not restart complete discovery or guard hypothetical producers. No benchmarks now; throughput unmeasured. No additional successful-fresh-ingest SQL/copy/hash/global lock; no speculative hardening. Explicit native changed-version claims remain strict.

Shared navigation map (starting points, not complete coverage or conclusions):
- Gate canonical no-version producer -> scribe ingress prepare_and_dispatch -> execution_lanes/preprocess -> shards grouped fence/completion -> vala-sql queries/scribe_batch_commits classify -> committed_batches mirror. OTLP negative and scribe write_read journeys; grouped rollback/counts, WAL/replay/cancellation.
- Catalog bifrost_catalog native definition/default target -> Scribe conformance/description and Forge managed policy; server boot/service constructors.
- SealKey -> file_list_writer -> promotion/PublishedHotFileIdentity -> parquet/promoted_object -> Forge scribe_promotion/compact; persistent accepting version and independent footer/tuple validation.
- Forge managed policy/identity/executor -> layout forge_data_location -> publication/worker rewrite_authority -> committed lineage/gc/cleanup. Precise conflict test, mixed promotion tests, Oracle Bloom journey.
- Fork catalog publication/version no-op: iceberg-rust850d308a8c50cdd7890ecfdf5bfd9825efebb241, compactionc213af00eac40377d800e24f3ed68945034e7de4; Cargo manifest/lock. Read exact pinned source; preserve data-only commits and atomic CAS/reference.
- SDK Python _client description projection and native bifrost/mod handle -> same-name binding threaded regression. TS index facade/result types and unit/type/journey tests; API int/bigint, wire strings.
- wyrd-spec vala ids parser/test rustdoc; mandatory documentation on changed Rust items. Generated card-types parity.

Selection: fresh remediation Ponytail closure/standards discovery covering all nine and affected tiers; scoped system-recovery specialist for changed no-claim retry/grouped fence; persistent-data specialist for promotion/Forge/native fork publication. Fresh independent validator after all reports. No separate security specialist: established tenant/auth boundaries used; selected reviewers still trace changed capability preservation. Follow-up only for material uncertainty.

Verification evidence: appended R3 implementation table at55cab2a19, commands/counts/diagnoses are claims to inspect, not independently run here. Two redux timeouts unexplained, reruns reportedly pass. No benchmarks. Reviewers must not independently start Cargo/build/DB commands; root coordinates any needed runtime commands sequentially. Reports only, no source edits/commits/push.

Current candidate includes a post-evidence merge that only updates task-review skill/config and spec-driven-development docs; inherited workflow changes, not new Bifrost functionality. Unrelated dirty task002/003/005/006/007/008 edits are excluded and must be preserved. No .codegraph directory exists.
