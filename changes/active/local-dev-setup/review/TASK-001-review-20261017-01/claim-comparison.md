# Discovery claim comparison

This compares claims, not source-validates findings. Independent validation remains mandatory. All seven discovery reports are present.

| Shared behavior/invariant | Discovery source IDs | Comparison |
|---|---|---|
| Resolved noninteractive policy fails to govern implicit MCP host offer | BEH-001, INV-R1 | Same producer/consumer and bounded orchestration correction; one validation group |
| Saved deployment identity conflict is detected after settings mutation/provisioning | BEH-002, INV-R2, SEC-001 | Cross-database restart and preexisting remote client are sibling initial states. Proposed corrections differ in preflight evidence; validator must trace full key selection/save bodies, correct any inaccurate intermediate diagnosis, preserve valid explicit overrides/partial recovery, and select minimum safe source boundary |
| Supplied sealing-key selection is lost on no-export restart | INV-R3 | Unique claim; independent validation required |
| Gateway apply error incorrectly implies noncommit | BEH-003, INV-R4, MNT-002, SYS-001 | Same uncertain-response boundary; consolidate CLI and affected docs, preserve named upsert semantics without added receipt/retry machinery |
| Initial concealed input occurs before signal cancellation supervision | SYS-002 | Unique claim. Security report's RAII cancellation assessment does not itself prove process-signal cancellation; validator must distinguish the two |
| Required SQL-failure rollback proof absent | DATA-001 | Unique proof claim; validate explicit obligation and existing proof before retaining correction |
| Mandatory rustdoc missing on changed test items | REPO-001, MNT-001 | Same explicit rustdoc gate; REPO-001 includes additional constants. Batch rather than style-only remediation |
| New restart proof calls existing Linux-only free_bind helper | Behavior matrix INV-006 FAIL; other reports platform PASS | Focused follow-up required: matrix conflicts over required proof and whether old helper use enters task scope. Fresh followup-rev investigates only this uncertainty |
| Standalone example changes lack immutable subjects | Reports explicitly note external mutable task edits | Review-input blocker, not a behavioral FIND. No findings ID assigned |

Follow-up: requested `followup-review.md` before validation because supported-platform proof has materially different acceptance assessments and no discovery finding fully resolves its changed caller. Matching claims are not treated as proof, and unique claims are not discarded. No prior remediation round exists.
