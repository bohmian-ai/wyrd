# Discovery claim comparison and follow-up decision

All seven required discovery reports are complete. Immutable candidate remains
3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4. This compares proposed claims, not their
final source validity; the root is orchestrator, not an independent validator.

| Shared boundary | Discovery claims | Reconciliation needed |
|---|---|---|
| Retryable infrastructure versus actual authorization denial | SYS-R1-001 proposes partial FIND-TASK-003a-1 closure: generic 403 also consumes QueryPeerSecurity. Behavior/invariant marked prior closure generally. | Validate complete typed peer producer→gRPC→query→poller path and narrow in-owner correction; do not weaken peer rejection or add recovery machinery. |
| Exact accepted fixed-unit durations | DATA-2 proposes partial FIND-TASK-003a-2 closure: authored day interval uses session calendar days across DST; other reports marked normal UTC/year cases closed. | Validate shared parser fixed seconds, production session settings and SQL arithmetic proof; no new timezone policy or guard. |
| Terminal persisted-shape exception | DATA-1 proposes new explicit narrow-shape constraint violation; invariant report rejects arbitrary privileged SQL as no actual unsafe writer. | Material conflict about task-required durable invariant versus speculative unsupported mutation. Fresh follow-up must resolve exact authority/caller/constraint scope, not assume DB counterexample alone establishes a product exploit. |
| Existing owner-grant journey | SEC-001 proposes a test regression and vacuous trace-only phase; maintainer noted same unclosed sibling without complexity finding; behavior/invariant marked original auth scenarios retained. | Fresh follow-up must trace actual writes, readback, denial consumption and whether new activity occurs. Root exact focused rerun is in progress, through managed PG with tracing, after domain SQL session ended. |
| Changed-item rustdoc | STD-001 and MREV-R1-01 propose FIND-TASK-003a-6 remains open at new InputSlot visitor items; old cited sites repaired. | Validator must inspect all exact new items and inherited-trait docs against mandatory AGENTS rule. No naming-only or unrelated style round. |

FIND-3/4/5 and HUMAN-CLEANUP1–3 have no conflicting retained production proposal
from discovery. New reports' scoped PASS results are corroboration, not proof.
A C13 UTC-midnight setup limitation was noted but not proposed as material; do
not convert it into a root finding or optional remediation advice.

## Focused follow-up

A fresh followup-rev is required for the two explicit closure conflicts: (1)
remaining owner-grant security journey versus approved terminal consumption;
(2) whether the new malformed-terminal constraint states violate an explicitly
required persistence boundary or would only motivate out-of-scope hardening.
It receives both claims/authorities and actual source without an intended verdict.
The unique typed-403 and fixed-duration claims go to independent validation too;
no additional discovery pass is needed merely to obtain agreement on them.

After follow-up, a fresh Ponytail validator must assess every proposal and prior
closure independently, merge common causes, preserve FIND-1..6 IDs and use the
next unused ID only for retained new causes. Rejected claims are omitted, not
softened into optional changes. Accepted product limits remain excluded.

The focused follow-up is complete (RESOLVED). It source-supports the owner-grant
journey regression and narrows DATA-1 strictly to the already-required
NULL-authority terminal constraint exception, excluding broader legacy-input
hardening and any public exploit claim. No new material proposal was added.
All discovery and follow-up proposals now go to a fresh independent validator;
root supplies no intended verdict. Full-trace owner-grant rerun is still in
progress and will be supplied as evidence when complete.

The full-trace owner-grant repeat is now complete: it failed the same selected
journey, with actual scheduler terminal-RBAC-error trace followed by the
completed-execution wait's 90-second assertion. `verification.md` records both
run IDs, counts, timing, capture location and bounded trace excerpts. The validator
receives this evidence without an intended final finding disposition.
