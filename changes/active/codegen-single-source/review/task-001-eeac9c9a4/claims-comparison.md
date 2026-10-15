# Discovery claim comparison

All six required discovery reports are complete for the pinned candidate.

| Proposed correction boundary | Discovery IDs | Comparison |
| --- | --- | --- |
| Rust metadata -> TS response presence/nullability | BEHAVIOR-001, INVREV-001, SDK-001 | All identify the same deserialization-schema/output-serialization information loss. Always-emitted nullable keys become optional; skip-none fields admit null. Consolidate by cause, not DTO/line. Independent validation must establish reachability, retained obligations, and minimal correction. |
| Standards / maintainer / system resilience | No material proposals | Their scoped PASS results do not contradict a public-type precision defect: each explicitly defers detailed schema semantics to contract/implementation review. |

## Follow-up decision

No focused followup-rev is needed. Discovery agrees on the reachable boundary
and covers the changed owners and sibling consumers. Invariant review records
that schemars 0.8 loses serialization information; the SDK domain report
identifies existing Rust annotation mechanisms to investigate for a bounded
correction. These are compatible observations, not conflicting factual
claims. The required fresh validator must assess that proposed correction
against full source and authority. If that assessment exposes genuinely
unresolved material authority, it must report it rather than assume approval.

No proposed finding is independently validated by this comparison. The final
ledger and correction must come from the fresh ponytail-rev. No intended
verdict is supplied.
