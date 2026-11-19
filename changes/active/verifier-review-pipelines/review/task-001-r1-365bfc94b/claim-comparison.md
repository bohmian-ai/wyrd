# Discovery claim routing

This groups proposals for independent source validation. Agreement is corroboration, not confirmation. No finding is retained by the orchestrator at this stage.

| Claimed invariant / consequence | Discovery proposals | Validation routing |
|---|---|---|
| Original path receipt recovery independent of mutable scratch/storage | BEH-001, INV-R1, SYS-002, CONC-001 | One receipt-replay cause; evaluate concurrent same-call CONC-002 at same SQL owner without hiding its distinct interleaving |
| Concurrent identical replay falsely conflicts | CONC-002 | Independently inspect READ COMMITTED snapshot and uniqueness behavior |
| Revocation ordered with admissions still in transit | SECURITY-1, CONC-003 | One shared admission/principal authority cause, all sibling admission paths |
| Current lease authority after expiry before reclaim | CONC-004 | Check exact requirement and SQL/renewal reachability; preserve terminal replay semantics |
| Durable deadline conservation through target loading | INV-R2 | Reconcile behavior matrix's broad deadline PASS with the narrower claim-time handoff failure |
| Stable first-page committed cut under parallel writers | BEH-004, DATA-001 | One sequence/commit-order cause across invocation and publication pages |
| Required backlog-sensitive indexes | DATA-002 | Check actual indexes/query shapes and explicit Task1 index obligation; do not infer cost from LIMIT alone |
| Effective publication and cumulative retention budgets | BEH-002 | Distinguish explicit Task1 backend ownership from future Agent tooling and Task2 budgets; check accepted-file consumer mismatch |
| Content visibility lag consumes model/action attempts | BEH-003 | Trace producer and pre-model admission failure; use existing deferral owner if retained |
| Final completed result survives crash before Scribe ACK | SYS-001 | Focused follow-up resolves authority conflict and precise failure boundary; then independent validation |
| Deleted exact capability and internal definition resolution | BEH-005, SYS-003 | Assess shared resolution source; internal reads and unavailable-capability terminal semantics may need separate outcomes at one owner |
| Python/TS required success-path journeys absent | BEH-006 | Check explicit per-surface proof obligation and smallest existing seeding seam; no requirement to implement Task3 tools early |
| Replay framing after supported request-ceiling change | DATA-003 | Assess task-introduced durable replay, exact physical ID/digest behavior and correction burden; reject speculative/pre-existing expansion |
| Blocking snapshot IO in async backend | MNT-001 | Check mandatory rule, backend task acceptance and current caller limitation; preserve exclusion lock if retained |
| Python DownloadPlan expiry field parity | REPO-002, MNT-002 | One authored-stub omission; shared wire contract remains owner |
| Mandatory changed-item rustdoc | REPO-001 | Validate listed items/rule applicability; batch bounded corrections with behavioral work |

Focused follow-up selected for the material authority conflict in SYS-001: approved spec strengthens final durability while owner architecture text retains prior best-effort result semantics. The reviewer must resolve whether the approved architecture-amendment scope authorizes correction. No extra discovery pass is needed merely for unique findings or agreement counts. Other proposals go directly to the fresh independent validator.

Human CLI naming concern remains separate from discovery findings and is not a FIND ID. No rename is approved. Cross-platform CI absence is a stated verification limit, not independently converted into new implementation scope.
