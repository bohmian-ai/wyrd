# TASK-011 r4 domain review: query consistency and persistence

**Subject:** `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; base `338f33235f81c30dfe3a570dc26934fe7bb77048`; candidate `1268bbe3ef820ba50e1c6dbb065d4c01b415f5f3`. Reviewed the cumulative diff against approved spec revision 38, original TASK-011, r1–r3 verdicts and ledgers, and R1–R3 remediation tasks. **Result: PASS.**

## Boundary, authority, and source coverage

| Boundary | Authority | Source and determination |
|---|---|---|
| One consistent observation cut | Spec REQ-153–156, AC-034, INV-012; R1 FIND-TASK-011-3; `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `architecture/bifrost-design.md` query-cut and audit rules | `wyrd-server/src/verification/drift.rs:110–360,670–755,803–905`; `query/scheduled.rs`. PSI/SPC combine completeness and every fitted-feature aggregate in one `UNION ALL` statement. `Reader::fold` executes that statement once via the authenticated scheduled query consumer. Oracle binds that query to one immutable cut and audits its authorization. Each arm filters subject, selected series, and `[start,end)`. PASS. |
| Incomplete, duplicated, and invalid selected records | Spec REQ-156 and AC-034; R2 FIND-TASK-011-8; `architecture/logic/drift.md` | `drift.rs:161–201,461–509,1240–1294,1375–1510`; `vala-bifrost-redux/src/tables/drift/observations.rs:15–34`. The table requires non-null `record_id`; completeness groups selected feature rows by that identity. Missing/repeated configured series or invalid values make the whole fold unscorable. The focused 99-record-plus-duplicate SQL/fold test exercises the actual statement. PASS. |
| Publication and dispatch | Spec REQ-156, AC-034; `architecture/wyrd-design.md` Verifier result semantics | `drift.rs:489–509,670–755`; `verification/results.rs:205–263,919–929`; `verification/runner.rs:375–412,485–532`. The unscorable fold returns `Drift(None)`. Result publication emits one inconclusive summary with null details and no feature rows. Operator dispatch follows failed settled results, so an inconclusive run does not dispatch. PASS. |
| Fit version and historical results | Spec REQ-157; `AGENTS.md` tenant/persistence boundaries | `vala-drift/src/baseline/mod.rs:18–33`; `drift.rs:757–800`; `wyrd-sql/src/queries/drift_baselines.rs:361–374`; Rust/Python/TypeScript Drift journeys. The server reads the exact tenant and Verifier UID, refuses missing or older fitted format before decoding, and does not rewrite historical results. The journeys read historical results after legacy refusal. PASS. |
| Test controls | Spec AC-034 and INV-012; `AGENTS.md` server ownership | `wyrd-testing/src/{verification,python,server}.rs`, `wyrd-sdk-ts/native-testing/src/lib.rs`, and three SDK journeys. Test controls adjust due bindings or fit metadata; the ordinary runtime still claims, queries, settles, publishes and dispatches. They add no product route. PASS. |

## Findings

No material findings in this domain. R3 changes only direct `vala-drift` target-column handling and task evidence/status; this candidate adds only the R3 task status update. The query, persistence, audit, and publication boundaries previously reviewed at r3 remain unchanged in the cumulative candidate.

## Verification limits

This is source and recorded-evidence review; I did not rerun lanes. The one-query invariant follows from statement construction and Oracle's documented pinned-cut contract; the SQL test uses fixed input rather than injecting a concurrent write during an Oracle query. The duplicate proof is at SQL/fold level; SDK sparse journeys and the result-builder test prove the inconclusive publication shape. SDK JSON cannot carry NaN or infinity, so unit and server SQL tests own those values.
