# Discovery claim comparison

First full review of the subject in scope.md. All seven selected discovery reports are present. No carried-forward findings.

| Candidate cause / obligation | Discovery source IDs | Route to independent validation |
|---|---|---|
| Reached occurrence enters state only after independent actions finish | BEH-008-1, INVREV-01, SYS-001 | Consolidate one lifecycle production/consumption issue; verify whether acceptance requires immediate live decision consumption as well as timely accepted occurrence visibility. No proposed new polling/wait service. |
| Historical discarded speculative occurrence still accepts a first decision | BEH-008-2 | Unique finding. Verify reachability and explicit stale/non-pending refusal. Invariant review proved discarded output cannot release work; behavior review explicitly does not claim release. Those are different promises, not contradictory findings. |
| Decision expiry omits shorter authored execution ceiling | INVREV-02, SYS-002, DATA-001 | Consolidate one effective execution deadline issue. Verify stopped-worker, request-start-after-expiry scenario rather than imposing a new atomic cross-store protocol. |
| Delegated receipt/replay uses effective subject instead of actual actor | SEC-008-1 | Validate supported delegation path, normal authorization, actor contract and smallest shared attribution correction. |
| New helper missing mandatory Panics docs | REPO-001 | Validate full changed helper and mandatory gate; batch with material corrections. |
| New architecture request decision values conflict with approved contract | REPO-002 | Validate owner amendment, schema and contract; spelling correction only if approved behavior is unambiguous. |
| Raw SQL diagnostics exposed in public approval unavailable error | REPO-003 | Validate propagated public path and existing safe-error owner; bounded local redaction. |

Maintainer report proposes no findings and supplies source-backed owner comparison: shared Scribe fence/ACK extended, separate approval row/domain projection justified, no replacement runtime. All reports distinguish source inspection from executed checks. Verification.md records orchestrator-run checks and limits.

## Follow-up decision

No focused followup-rev is selected at claim comparison. There is no material unresolved conflict: stale decision acceptance and stale work release are explicitly distinguished; duplicate deadline and pending-visibility proposals agree on their traced causes. Each unique claim has concrete paths for the mandatory fresh validator to inspect. Agreement is not treated as proof. If validation exposes a new blocker or unresolvable common cause, obtain a separate focused source review before retaining it.

Performance: system review inventories new/moved waits and identifies shared direct-engine impact. No clean full L1 record is claimed. Validator must preserve this PASS qualification and avoid converting it into unsupported product scope or a broad remediation suite.
