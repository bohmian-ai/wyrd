# TASK-011 r2 security and tenancy domain review

**Result: PASS**. No material security or tenancy finding in the cumulative candidate.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`.
- Immutable range: `338f33235f81c30dfe3a570dc26934fe7bb77048..3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`; HEAD matched the candidate during this review.
- Approved contract: `changes/active/verified-change-contract/spec.md` revision 38, especially REQ-156, AC-034, REQ-145 and the SYSTEM Drift-reader scope. Original `TASK-011-conventional-psi-spc.md`, prior r1 verdict/findings, and `TASK-011-R1-production-drift-closure.md` were considered.
- Governing repository rules: `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md` tenant-connection, audit, and permission rules; `architecture/wyrd-design.md` tenant-local SYSTEM identity; `architecture/bifrost-design.md` tenant source binding, Oracle immutable query cut, and read audit; `architecture/references/languages/spec-driven-development.md` authority order.

## Boundary and source coverage

| Boundary | Source inspected | Result |
|---|---|---|
| Fixed SQL input and selection | `crates/wyrd/wyrd-server/src/verification/drift.rs` `ObservationWindow` and `DistributionFold`; cumulative diff and SQL fixtures | PASS: table name and SQL shape are server constants; subject, series, category labels, and timestamp values use SQL-parser quoted literals; numeric edges are finite-checked and formatted as numbers. Every combined part retains the same subject and half-open window filters. |
| Tenant read authority | `DriftEngine::try_verify`, `Reader::{caller,fold}`, `wyrd-auth/src/issuance.rs::issue_system_drift_read_token`, `query/scheduled.rs`, `query/service.rs` | PASS: a tenant-scoped connection looks up the registered observation table; a five-minute-or-shorter SYSTEM token contains only that table's read permission; its tenant is verified; the authenticated scheduled consumer calls the ordinary query service and Gate/Oracle audit path. No direct Oracle or caller-authored SQL path was added. |
| Stored baseline and query results | `DriftEngine::fitted`, `DistributionFold`, `pg_grpc_ingest_smoke.rs::system_drift_reader_reads_only_the_observation_table` | PASS: baseline lookup uses `TenantConn`; the token test proves cross-tenant verification refusal, result-write refusal, other-table query denial, and audited read decision. Malformed aggregates terminate rather than exposing a scored result. |
| New test controls | `wyrd-testing/src/{verification,server,python}.rs`, `sdks/wyrd-sdk-ts/native-testing/src/lib.rs`, relevant manifests | PASS: due-binding and legacy-fit mutations are on test-server fixture objects, use tenant transactions, and add no product HTTP/MCP route. The `wyrd-testing` Python feature is optional, and TypeScript controls are in the native-testing package. |

No manifest, lockfile, migration, or production auth implementation changed in this range. The candidate did not add a command, template, path, webhook, redirect, or secret-handling surface.

## Findings

None. The SYSTEM read token can read the tenant's whole registered observation table; the approved spec explicitly permits this and requires the fixed server SQL to enforce subject, series, and window scope. The inspected query builder does so. The test fixture's direct SQL mutations are reachable through the test harness, not the product API, and run under its fixture tenant's RLS connection.

## Verification limits

This was a static, review-only audit; I did not rerun the implementation's lanes. The task records green Rust/Python/TypeScript Drift journeys, server integration, `test:principals:integration`, format/lints, codegen, and exact focused tests. The one-query consistency proof is structural plus SQL execution tests; no mid-query ingest race test exists, and Oracle's documented cut is per statement. I found no basis for a material security finding from that limit.
