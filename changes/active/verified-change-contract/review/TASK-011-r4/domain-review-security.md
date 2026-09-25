# TASK-011 r4 security and tenancy domain review

**Result: PASS.** No material security or tenancy finding.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`.
- Immutable cumulative range: `338f33235f81c30dfe3a570dc26934fe7bb77048..1268bbe3ef820ba50e1c6dbb065d4c01b415f5f3`.
- Approved `spec.md` revision 38, original TASK-011, r1/r2/r3 verdicts and remediation tasks; `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md` tenancy and audit rules; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md` query authority, pinned cut and audit contract; `architecture/references/languages/spec-driven-development.md`.

## Boundary and source coverage

| Boundary | Source inspected | Result |
|---|---|---|
| Server-built SQL and scope | `wyrd-server/src/verification/drift.rs` `ObservationWindow::{text,instant,rows_in,incomplete,psi_statement,spc_statement,combined}` and cumulative diff | PASS: the table, predicates and SQL structure are server-owned. Subject, configured series, categorical labels and timestamps are emitted as escaped SQL string literals; fitted numeric inner edges must be finite. Each aggregate part retains the subject and half-open window filter. The r2 repeated-series check narrows incomplete records without accepting caller SQL. |
| Tenant identity and token | `DriftEngine::{try_verify,fitted}`, `Reader::{caller,fold}`, `wyrd-auth/src/issuance.rs::issue_system_drift_read_token` | PASS: baseline and registered-table lookup use the run tenant's `TenantConn`. The verified SYSTEM token is scoped to this tenant, one Verifier and `bifrost_query:read` on the registered observations table UID, with a five-minute maximum lifetime. No hand-built principal or broader result-write authority appears. |
| Query authorization and audit | `query/scheduled.rs::{authenticated,dispatch,run_with}`, `query/service.rs::stream_query`, `pg_grpc_ingest_smoke.rs::system_drift_reader_reads_only_the_observation_table`, Bifrost design | PASS: the reader enters public capability admission, Gate/Oracle object authorization and the normal audited read or audited object denial. The scoped token cannot read a second table or write results in the integration proof. One combined PSI/SPC statement uses one Oracle pinned cut. |
| Test-only mutations | `wyrd-testing/src/{verification,server,python}.rs`, `wyrd-sdk-ts/native-testing/src/lib.rs`, package manifests | PASS: schedule and legacy-fit fixture writes are parameterized SQL through a fixture-tenant connection. Python exposure is behind the SDK's optional `testing` feature; TypeScript exposure lives in the separate native-testing crate. No production HTTP/MCP route, credential mint or cross-tenant capability was added. |
| r3 closure | `vala-drift/src/{psi,spc}/mod.rs`, r3 remediation and candidate diff | PASS within this boundary: treating an Arrow `Null`-typed target as incomplete affects scoring only and does not widen authentication, SQL, or persistence authority. R1/R2 status corrections are task metadata only. |

## Findings

None. The SYSTEM reader may read the whole tenant observation table by approved design; the fixed server SQL restricts the actual read to the selected subject, series and window.

## Verification limits

This was a static review, without rerunning gates. TASK-011 records green Vala, server, SDK journey, OpenAPI and focused tests. The single-cut claim is structural: one statement through Oracle's documented pinned query cut; there is no injected concurrent-ingest race test. SDK JSON cannot represent NaN or infinity, so the server SQL and Vala tests cover those inputs. No production token issuer, verifier, query service, migration, or manifest changed in the reviewed range.
