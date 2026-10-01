# TASK-005 acceptance verdict

**Verdict: FIX_REQUIRED.** Immutable subject: base `05d7d741304af3b0b4e667e7e18f93dec16b897b`, candidate `885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` on `vcc/task-004`. Commit `1f1cbcf5f` is excluded. The candidate commit remained fixed throughout review; no implementation source was changed.

The user-designated original task is `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (SHA-256 `0e62758cd6790451c2fa7497c06f6eba7d57400ed15ea38b50695c30b0177814`). It names spec revision 14; the currently approved `changes/active/bifrost-scribe-live-reads/spec.md` is revision 24. The candidate's shorter task packet does not waive the user's designated task. [The focused follow-up](followup-review.md) found its proof obligations compatible with the current approved spec, so no spec revision is required.

## Reconciled acceptance

| Original obligation | Candidate evidence and limit | Result |
| --- | --- | --- |
| Readable, correlated write, local/remote query, and Forge success/failure traces with real terminal lifetime and one failure reason | Focused captured traces and test assertions cover these paths; no validated contrary finding | PASS |
| Owner-derived, truthful Gate/Scribe/Oracle/storage/Forge measurements without shadow authority | Most focused measurements pass, but Scribe's unlocked backlog publication can leave four gauges behind actual assembler state under concurrent transitions | FAIL: FIND-TASK-005-1 |
| Production abrupt-restart scrape, readback, and backlog settlement, alongside fresh-recorder restore proof | Fresh-recorder and separate restart/readback tests exist; the named combined real-server journey is absent | FAIL: FIND-TASK-005-2 |
| Six dashboard questions backed by rendered before/after production samples, independent facts, traces, and family/series inventory | Candidate table names families, meanings, and tests; numeric transition samples and inventory are absent | FAIL: FIND-TASK-005-3 |
| Standard benchmark validity, whole Scribe/Oracle/Forge journeys, and broad gate after focused tests | Focused scenarios, nearby units, fmt, lints, docs check and diff check are reported passing; benchmark, whole lanes, and gate are explicitly unrun | FAIL: FIND-TASK-005-4 |
| Repository import and typed-signature rules | Added local imports and fully qualified changed signatures violate the module import rule | FAIL: FIND-TASK-005-5 |
| Non-goals: preserve ACK, WAL, Iceberg, admission, terminal, tenancy, Forge settlement, and SDK contracts; no new telemetry platform or compatibility series | Domain and system reviews found no validated violation in the reviewed diff | PASS |

## Independent reviews and claim reconciliation

| Report | Result | Proposed claims |
| --- | --- | --- |
| [Behavior](task-review-behavior.md) | FAIL | B-001, B-002, B-003 |
| [Invariants](task-review-invariants.md) | FAIL | INV-1, INV-2 |
| [Standards](standards-review.md) | FAIL | STD-001, STD-002 |
| [Maintainer](maintainer-review.md) | FAIL | M-1 |
| [System resilience](system-review.md) | PASS | None |
| [Durability](domain-review-durability.md) | PASS | None |
| [Concurrency](domain-review-concurrency.md) | FAIL | CONC-1 |
| [Security and tenancy](domain-review-security.md) | PASS | None |
| [Focused follow-up](followup-review.md) | RESOLVED | Original-task authority and the absent restart selector; no new claim |
| [Structured Ponytail validation](findings-validation.md) | Complete | Five confirmed, deduplicated findings |

The follow-up was required because the user-supplied original task and candidate task packet differ materially. It also corrected the invariant report's mistaken phrase that the absent abrupt-restart selector had passed. The independent validator traced all claims to source and retained only the five ledger entries below.

## Validated ledger

| ID | Class | Closure |
| --- | --- | --- |
| FIND-TASK-005-1 | INCORRECT | Serialize Scribe owner mutation and backlog publication with the existing assembler lock; prove concurrent final gauges match owner state. |
| FIND-TASK-005-2 | MISSING | Add and run the original named abrupt-restart production-scrape journey. |
| FIND-TASK-005-3 | MISSING | Record real before/after samples, independent facts, traces, and inventory for each dashboard question. |
| FIND-TASK-005-4 | VIOLATION | Run and assess the standard benchmark, whole owner journey lanes, and broad gate. |
| FIND-TASK-005-5 | VIOLATION | Conform changed imports and signatures to the repository module import rule. |

Decision-complete diagnoses, source locations, and focused closure proof are in [findings-validation.md](findings-validation.md) and [TASK-005-R1-close-telemetry-proof.md](TASK-005-R1-close-telemetry-proof.md). No prior TASK-005 findings were supplied for closure. The original task's remaining exact focused commands are reported green but were not rerun by this immutable review. Benchmark performance and broad regression acceptance remain undecided until the required runs complete. This review did not implement, merge, push, or deploy.
