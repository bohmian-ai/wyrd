# Discovery claim comparison

All six required discovery reports are complete. Behavior proposes BEH-1 (successful all-null timeseries has no drawable domain but brush release attempts an invalid Date conversion). System proposes SYS-1 (lazy Node SDK handle initialization permanently caches rejection). Invariant, standards, maintainer and security/tenancy reports propose no material finding. All source-backed prior finding dispositions support closing FIND-003-1..15, including R7.

The proposals describe different violated invariants and correction owners: Chart geometry-to-action conversion versus Node handle initialization lifetime. They are not duplicate range-admission defects and do not reopen a prior finding by caller. They remain proposals until independent validation; green checks and reviewer agreement are not proof.

A fresh focused follow-up was required because discovery exposed previously unexamined reachable failure paths despite the broad PASS coverage of other reports. It investigated only reachability, authority and correction boundaries for these two claims. followup-review.md is RESOLVED: the Chart path reaches WyrdView before host intent validation; retained public custom-app Node executors can keep rejected promises, while shipped BFF/example instances are request-local. The follow-up introduced no additional findings and did not expand TASK-003 into TASK-027 or PR #205 content.

Next: fresh structured Ponytail validation independently checks every proposal, full source/callers, prior closure and minimum correction. Its final ledger, including rejection or revision, governs the verdict.
