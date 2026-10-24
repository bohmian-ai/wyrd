# Discovery claim comparison

This is routing and deduplication, not source validation or a verdict. All eight discovery reports are complete and read by the orchestrator. Every proposed claim, including unique claims and proof gaps, goes to independent validation.

| Common obligation / proposed cause | Discovery IDs |
| --- | --- |
| Accepted retry resolved under current schema/spec before original acceptance | BEH-001, INV-R1, SYSTEM-001, CONC-01 |
| Native request tuple limit reset per IPC batch | BEH-002, INV-R2, DATA-1 |
| Current Bloom policy versus durable staged fingerprint | BEH-003, INV-R3, MNT-002, DATA-3 |
| Compaction tuple grouping omits retained spec identity | INV-R4, DATA-2 |
| Promotion JSON tuple not compared to durable full partition authority | TENANCY-001 |
| Cross-revision live capture strips revision and may alias restored/new generation | CONC-02; invariant report expressly questions whether global allocation prevents this |
| Cache fill after completed invalidation | CONC-03; validate task-004 scope and reachability carefully because public edit orchestration belongs to TASK-005 |
| Explicit changed-item rustdoc gate | REPO-004-1, MNT-001 |
| Required measured physical/query proof absent | Behavior report acceptance matrix and Required proof gap section; invariant report acknowledges limited measurements but marks preservation policy passing |

## Follow-up decision

A fresh focused `followup-rev` is required for the material conflict over generation allocation and live capture (CONC-02). It receives the conflicting claims and traces the restart/allocation/capture path, without an intended verdict. Other claims have source-local diagnoses sufficient for independent validator investigation; no extra discovery pass is requested to obtain agreement. The differing Bloom correction suggestions and measurement acceptance assessments are explicitly left for authority-based independent validation.

The final validator receives every discovery report and the follow-up report. No claim is accepted solely because multiple reviewers agree. No rejected claim will become optional advice in the final verdict.

The focused follow-up returned RESOLVED: within-process global allocation does not ensure uniqueness against restored staged authority under another revision. Its controlled startup/watermark/capture trace is preserved in `followup-review.md`; the independent validator still adjudicates the claim against source.

## Caller steering

During validation, the caller explicitly required decision-complete remediation for all retained findings and a concrete recommendation whenever a spec revision is necessary. A revision must not leave the correction unspecified. Any approval-required amendment will therefore include the recommended decision and executable follow-on correction, while remaining clearly distinguished from already approved implementation scope.

After the verdict, the caller required preservation of current throughput, no additional PostgreSQL call on the normal ingest hot path, and no speculative guarding/complexity. The caller also explicitly deferred benchmark execution. The existing repository scout and independent validator were reused for a bounded correction-only pass on FIND-1; this does not reopen the candidate or add finding IDs. The unconditional upstream lookup recommendation is superseded by existing-mirror/existing-conflict-read resolution, exceptional validation-failure recovery and canonical WAL authority. Other findings remain unchanged. Performance comparisons are future verification; no tests/builds/benchmarks ran during this revision.
