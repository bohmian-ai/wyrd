# TASK-011 r3 security and tenancy domain review

**Result: PASS.** No material security or tenancy finding in the cumulative candidate.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`.
- Immutable range: `338f33235f81c30dfe3a570dc26934fe7bb77048..c5c7a76cc1f45f5bdfad20de35a957b9f2f9ce57`; HEAD matched the candidate during review.
- Approved `spec.md` revision 38, especially REQ-156 and the SYSTEM Drift reader in REQ-145; original TASK-011, prior r1/r2 verdicts and validated ledgers, and R1/R2 remediation tasks.
- `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md` tenant connection and audit rules; `architecture/wyrd-design.md` tenant-local SYSTEM identity; `architecture/bifrost-design.md` Oracle authorization, pinned source cut, and read audit; `architecture/references/languages/spec-driven-development.md` authority order.

## Boundary and source coverage

| Boundary | Source inspected | Result |
|---|---|---|
| Fixed SQL and input validation | `wyrd-server/src/verification/drift.rs` `ObservationWindow::{text,instant,rows_in,incomplete,psi_statement,spc_statement,combined}` and cumulative diff | PASS: SQL structure and table name are server-owned; subject, series names, category labels, and timestamps use SQL-parser quoted literals. Numeric edges must be finite before formatting. Each completeness and feature part retains the subject and half-open window predicate; the configured-series filter is constructed only from escaped names. The r2 duplicate check compares aggregate counts and inserts no caller SQL. |
| Tenant authority, token, and audit | `DriftEngine::try_verify`, `Reader::{caller,fold}`, `wyrd-auth/src/issuance.rs::issue_system_drift_read_token`, `query/{scheduled,service}.rs`, `pg_grpc_ingest_smoke.rs::system_drift_reader_reads_only_the_observation_table` | PASS: table lookup and token mint use the run tenant's `TenantConn`; the verified token has one observation-table read permission and a five-minute maximum TTL. The scheduled consumer enters normal query admission and Oracle, including object authorization and read/denial audit. No direct Oracle or forged principal path was introduced. |
| Stored baseline and result | `DriftEngine::fitted`, `DistributionFold`, `verification/results.rs` | PASS: baseline loading remains tenant-scoped; malformed or legacy fitted state fails visibly, and an unscorable target publishes an inconclusive summary without feature rows or Operator dispatch. No cross-tenant result read or write surface was added. |
| Test-only controls | `wyrd-testing/src/{verification,server,python}.rs`, `wyrd-sdk-ts/native-testing/src/lib.rs`, their manifests | PASS: schedule and legacy-fit mutations are on fixture objects, use bound parameters and fixture-tenant transactions, and add no product HTTP/MCP route. TypeScript native-testing is unpublished. These controls do not mint credentials or expose a new production authorization path. |

## Findings

None. The SYSTEM reader can read the tenant's full observation table, as the approved specification explicitly permits; the server-built fixed SQL supplies subject, series, and window limits. The r2 duplicate-row change does not broaden that authority.

## Verification limits

This was a static, review-only audit; I did not rerun implementation gates. TASK-011 records green Vala, server, Rust/Python/TypeScript journeys, served OpenAPI, codegen, and exact focused tests. No race test injects a row during the combined query; the one-statement shape and Oracle's documented per-query pinned cut are the available consistency proof. No manifest or lockfile, migration, production issuer, token verifier, or query-service implementation changed in this task range.
