# TASK-002 R4 security and tenancy domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Remediation history: TASK-002 R1 through R3, reviewed cumulatively from the original base
- Candidate identity was checked before inspection and again before this report; it remained exact and the working tree had no source changes.
- Per the caller's explicit ruling, AI co-author trailers are allowed and are outside this review.

## Reviewed boundary

This review traced the security-sensitive observation path from the Card-bound SDK credential through table describe authorization and audit, fixed and dynamic table admission, signed Card-scope subject resolution, server-stamped publisher and tenant identity, schema-fingerprint fencing, and tenant-scoped readback. It also inspected the production audit-publication timeout change and the R2 test-only describe/count/publication controls, including their Rust, Python, and TypeScript test projections.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Identity and tenancy source | `AGENTS.md` §§2, 3, 9; `architecture/wyrd-security-posture.md` security principles and tenant isolation; REQ-076/REQ-118 | `crates/wyrd/wyrd-testing/src/server.rs:2455-2504`; `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:188-203`; real Rust/Python/TypeScript observation journeys | PASS |
| Describe authorization and audit | `architecture/agent-rules.md` audit rules; `architecture/wyrd-security-posture.md` authorization and audit integrity; REQ-127 | `crates/wyrd/wyrd-server/src/bifrost/service.rs:228-268`; `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2632-2670`; fixed-table startup refusals in all three SDK journeys | PASS |
| Test-only describe failure/count hooks | Production-composition rules in `architecture/wyrd-security-posture.md`; AGENTS test-runtime boundaries | `crates/wyrd/wyrd-testing/src/server.rs:1580-1662`; Python and TypeScript testing projections; input interpolation guard and tenant-qualified trigger predicate | PASS |
| Audit publisher production split | Single publisher rules in `AGENTS.md`, `architecture/agent-rules.md`, and `architecture/wyrd-security-posture.md` | `crates/wyrd/wyrd-server/src/state.rs:2075-2082,2120-2123,2150-2159`; `crates/wyrd/wyrd-server/src/app/server.rs:620-633`; `WyrdTestServerBuilder::without_audit_publication_for_test` | PASS |
| Audit publication lock timeout | Single durable audit path and bounded reliability requirements | `crates/vala/vala-sql/src/queries/audit_staging.rs:199-275`; `crates/vala/vala-sql/tests/pg_audit_staging.rs:274-326`; recorded focused Postgres proof and `verify:bifrost` coverage | PASS |
| Writer/publisher versus observed subject | REQ-075, REQ-076, REQ-118, REQ-123, REQ-125; Bifrost identity contract | `crates/shared/wyrd-client/src/observe/mod.rs:64-109`; `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:1136-1183`; managed-column schemas and three SDK readbacks | PASS |
| Tenant and publisher stamping | `architecture/bifrost-design.md` table/row identity; `architecture/wyrd-security-posture.md` tenant isolation | Scribe correlation stamping in `execution_lanes.rs`; authenticated principal and tenant flow through Gate/Scribe; no client-supplied tenant or publisher column accepted | PASS |
| Reserved/unknown/unauthorized table refusal | REQ-127/REQ-128 and Scenario 6 | Local reserved-table refusal, server unknown-table refusal in all SDK journeys, and real denied-describe audit journey | PASS |
| Stale schema fence | Bifrost fail-closed fingerprint contract; AC-025 | `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:1804-1853`; Scribe source-schema fingerprint checks; final readback excludes the stale row | PASS |

## Security and tenancy analysis

### Authorization and audit ordering

`describe_table` builds the audited resource from the received namespace and name, then calls the canonical `audit::authorize` with `bifrost_table:read` before namespace validation or catalog access. Both allow and deny therefore use the verified caller's tenant, an unaudited decision fails closed, and an unauthorized caller cannot use namespace validation differences as an existence oracle. The real denied-describe journey proves a caller lacking table-read receives `WYRD_PERMISSION_403_DENIED_RBAC`, leaves exactly one denied `vala.bifrost.describe` staging row for the requested table, and creates neither a cache entry nor a producer.

### Test-support isolation

The audit-publication disable field and setter exist only under `feature = "test-support"`, default to false, and production compilation unconditionally constructs the normal publisher path. The describe fault lives in `wyrd-testing`, accepts only a restricted ASCII identifier grammar before interpolating a trigger argument, and predicates the injected failure on both the exact FQN and the fixture tenant UUID. Python exposes it only through the testing wrapper, while TypeScript exposes it through the unpublished `native-testing` package. These controls alter only test composition and do not create a production route, configuration field, or authorization bypass.

### Credential and identity split

`credential_registered_service` does not mint a replacement principal: it opens a tenant-scoped connection, resolves the already projected Service principal by Card binding, and adds the test credential and requested fixture roles to that principal in the same tenant transaction. The observation client supplies the selected view's exact CardRef only as correlation. Scribe resolves that reference against the authenticated principal's signed `CardRefScope`, rejects an out-of-scope or UID-less member, ignores any client UID for stamping, stores the matching signed member UID as `card_uid`, and independently stamps the authenticated principal as `principal_id`. The three SDK journeys read Model and Agent subject UIDs back separately while using the registered Service writer credential, directly proving the required writer/subject split.

### Tenancy and schema fencing

Describe catalog access uses `caller.data_tenant_id`; fixture credential lookup uses `TenantConn`; audit probes bind their tenant explicitly; and Bifrost admission continues to derive durable tenant and publisher identity from verified credentials rather than observation payloads. The stale-writer journey submits a batch built from a divergent schema and attempts re-registration; both operations return `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`, shutdown settles the refused batch, and subsequent query results contain only the previously accepted rows.

### Audit publication timeout

`freeze_publication_range` applies a transaction-local three-second `lock_timeout` before its `FOR UPDATE`. A timeout propagates as an error from the aborted tenant transaction rather than being reported as an idle tenant, so the publisher retries the unchanged range on a later cycle. The focused Postgres test holds the chain head, observes the explicit timeout, verifies no range was frozen, releases the holder, and then freezes the original owed range; this preserves the single publisher/watermark contract without an alternate sink or owner token.

## Verification evidence and limits

The task records passing real Rust, Python, and TypeScript SDK journeys, the ignored real-server denied-describe journey, the stale-writer integration test, the focused audit lock-timeout Postgres test, and the full nine-lane `verify:bifrost` run. The final candidate adds only the R3 fixes and evidence after that cumulative security proof; the latest full capability run is recorded at `d6231892`, with the candidate's final `b56560e5` commit changing task evidence only. I did not rerun the expensive suites during this static review.

The describe-count probe intentionally observes transient staging and is valid only with publication disabled; all journeys explicitly select that test-only mode. No test targets the instant a lease renewal completes exactly at Oracle's admission cutoff, but that is unrelated to TASK-002's observation authorization, identity, tenancy, or schema-admission obligations and does not constitute a task finding.

## Prior-finding closure

- `FIND-TASK-002-4`: closed without weakening the trust boundary; TypeScript now refuses hidden, symbol, and named-array data that JSON serialization would silently discard before any native call.
- `FIND-TASK-002-11`: closed; the R3 import-only correction does not alter credential, audit, tenant, or Oracle behavior.
- `FIND-TASK-002-16`: closed; readonly TypeScript aliases preserve the same Eval wire values and validation path.
- `FIND-TASK-002-17`: closed; committed evidence now names the implemented `WYRD_SPEC_400_VALIDATION` refusal.
- Previously closed security-relevant findings remain closed: fixed-table startup fails on audit/describe refusal, a denied describe is canonically audited before admission, signed scope controls subject stamping, and stale schema writes are fenced.

## Findings

No material security, authorization, audit, credential, tenant-isolation, writer/subject-identity, or schema-fencing finding was identified in the cumulative candidate.

## Overall result

**PASS**
