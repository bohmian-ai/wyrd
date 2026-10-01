# Claim comparison (orchestrator; groups proposals, does not validate them)

All eight discovery reports are complete: task-review-behavior.md (FAIL), task-review-invariants.md (FAIL),
standards-review.md (FAIL), maintainer-review.md (FAIL), system-review.md (FAIL), domain-review-security.md (FAIL),
domain-review-durability.md (FAIL), domain-review-concurrency.md (FAIL).

| Group (violated behavior/invariant) | Proposals |
|---|---|
| G1 Scribe-only target must be unready after a WAL fault | B-2, SR-1 |
| G2 WAL replay failure on restart must leave only Scribe unready | DUR-1 |
| G3 Shutdown withdraws readiness/advertisements before draining accepted work; tracked work drains | SR-2 (sibling commit 83ccbc634), SR-3 (plausible), MR-6 |
| G4 Every held governed byte charged (transport / Eval body) | CONC-1 (HTTP unknown-length body), INV-1 (Eval ACK body after release) — conflicts with system-rev residual note calling the Eval case acceptable |
| G5 R13-D proof selector and cancel-during-wait proof | B-1, INV-3, CONC-2 |
| G6 Deleted byte-wait API survives | B-7, INV-2 |
| G7 Ticket protocol/doc remnants (stop condition: no ticket + mTLS) | MR-1, F-SEC-1 |
| G8 Proof gaps for named obligations | B-4 (Py/TS resource terminal), B-5 (Forge unit), B-6 (R13-B pressure), F-SEC-2 (wrong tenant), S2 (D19 PG test silently passes) |
| G9 Replay ledger / undocumented scope | B-3, B-8 |
| G10 Check exemption D19 | S1 — conflicts with domain-security accepting the exemption |
| G11 Repo rules / docs / declarations | S3, S4, S5, S6, MR-2, MR-3, MR-4, MR-5 |

## Follow-up decision
One focused follow-up is needed for G3: SR-3 is self-rated plausible/timing-dependent, SR-2 is attributed to a
sibling-task commit, and no other reviewer traced the signal-shutdown order end to end, so it is an unreviewed
reachable path. G4 and G10 conflicts are resolvable directly from task text and source and go to ponytail-rev
validation without a follow-up.
