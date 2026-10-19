# Discovery claim comparison

All seven required discovery reports are present. The orchestrator groups claims here; this document does not validate them or select a verdict.

| Claimed violated invariant / correction boundary | Discovery source IDs |
|---|---|
| Parent tracing bypasses JSON stream policy when RUST_LOG is configured | BHV-001, INV-001 (part), MAINT-001, SYS-001 |
| Shared complete-pull terminal display writes progress to JSON stdout | BHV-002, INV-001 (part), SYS-002 |
| Published Observe continuation uses get for download, undeclared tables/bindings/slots, and lacks visible multi-language telemetry setup | BHV-003, INV-003 (part) |
| Derive-owned bundle errors prescribe get rather than pull | BHV-004, INV-003 (part), REPO-002 |
| New/materially changed Rust item documentation lacks required contracts | MAINT-002, REPO-001 |
| Published deployment callers remove the previous bundle before the shared safe replacement workflow | INV-002 |
| Supported-target runtime acceptance has only Linux x86_64 execution evidence | Behavior acceptance matrix and proof-limit discussion; invariant and standards reports also disclose missing target execution |

The data and security reviewers report PASS for their bounded domains. The data report assesses safe replacement at the shared pull owner; INV-002 instead challenges inherited example callers before that owner is invoked. These are different paths, not conflicting descriptions of the same implementation. Whether that inherited caller behavior is in task scope still requires independent validation.

The behavior report marks target execution acceptance incomplete; the other reports distinguish source conformance from their explicit runtime-proof limits. There is no disagreement about what was executed. The validator must reconcile the acceptance consequence against the original task and approved specification.

## Follow-up decision

No additional focused discovery reviewer is required: there are no conflicting source claims or newly identified reachable paths left without discovery tracing. Unique proposals (example pre-deletion, telemetry visibility, documentation contracts) have source/caller evidence in their originating report and still require independent validation. Repeated stream and recovery claims share the owners above. No prior remediation round exists for this agent-readiness review, so there is no repeated remediation uncertainty to investigate.

The fresh Ponytail validator receives the complete report union, cumulative immutable diff and authorities. It must independently trace every proposal, reject out-of-scope/inherited-debt claims where appropriate, determine the smallest safe correction, and explicitly assess missing supported-target execution proof. Agreement counts only as corroboration.
