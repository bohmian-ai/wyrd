# Tenancy domain review

Overall result: **PASS**.

## Immutable subject and reviewed boundary

- Base: `e0a926031155f263a8721c47d4662dfed8c125ea`.
- Candidate: `5478b2605acae3cecd2522941c30ee7f1674f0be`.
- Approved authority: `changes/active/gateway-api-key-auth/spec.md`, revision 1, and original `tasks/TASK-001-gateway-api-key-auth.md`.
- Scope: the complete cumulative change's effect on credential-derived tenant identity, tenant SQL capabilities, cached authority, and gateway consumers. No production, task, or specification edits were made.

The reviewed trust transition is credential header → existing extraction → API-key classification → parsed-key tenant transaction → verified credential and tenant-local principal/grants → signed access token → normal token verification → authenticated extension → Caller → gateway resources, accounting, audit and capture. Cache hits enter at the same token-verification step and retain no separately asserted tenant or principal.

## Authority and source coverage

The review used CodeGraph before source navigation and inspected the complete cumulative diff, including the task's recorded verification claims and all changed test and documentation files. Governing sources were AGENTS.md, architecture/agent-rules.md, the reference router, spec-driven-development and maintainer-style references, the applicable runtime-identity/delegation sections of wyrd-design.md, doctrine authority mapping, wyrd-security-posture.md's identity, API-key, access-token and tenant-isolation rules, and the v1 security and principal-extraction foundations.

| Boundary or obligation | Source evidence | Assessment |
|---|---|---|
| Tenant selection comes only from the parsed key, INV-001 | `components/auth/routes.rs:237-258,297-309`; `wyrd-auth/src/issue_api_key.rs:203-220` | PASS. The new caller reuses TokenGrants::api_key. Parsing supplies the tenant for the existing transaction; neither ingress, body, model, nor another header selects it. |
| Parsing alone grants no identity | `wyrd-auth/src/exchange_api_key.rs:159-177,222-236`; `wyrd-sql/src/queries/auth/service_accounts.rs:352-382` | PASS. Verification checks the parsed tenant against TenantConn, looks up a live key within that tenant, verifies the complete presented secret against the stored hash, and obtains the principal from the matched row. Altering tenant or suffix cannot reuse a different key's authority. |
| Tenant-bound SQL and transaction lifecycle | `components/auth/routes.rs:237-258,297-309`; `wyrd-auth/src/issuance.rs:566-628`; `wyrd-sql/src/tenant_conn.rs:25-53`; `wyrd-server/src/postgres.rs:55-64` | PASS. The existing TenantConn capability establishes transaction-local RLS. The grant boundary owns commit; lower exchange and issuance operations borrow the connection. No new SQL, raw pool, OperatorPool, tenant override, or parallel lookup mechanism was introduced. |
| Principal and grants stay in the selected tenant | `wyrd-auth/src/issuance.rs:424-481,566-628` | PASS. The issuer checks tenant admission, resolves the active stored principal and roles through the same TenantConn, uses that connection's tenant in TokenPrincipalRef, and resolves the permission snapshot there. No independently supplied principal or permission cache was added. |
| Cached authority cannot cross keys or tenants, INV-001 | `components/gateway/api_keys.rs:82-115,118-167`; `state.rs:2198-2200,2273` | PASS. The sole cache key is SHA-256 over the complete key. Only a successful committed exchange whose token verifies is inserted. State clones share the same bounded process cache; an entry contains the token and fixed deadline, with no independent tenant/principal fields that could disagree with the token. Both hits and misses verify for the Wyrd audience. |
| Signed identity reaches every handler coherently | `components/gateway/ingress.rs:122-157,200-230`; `components/auth/token_extract.rs:143-156`; `wyrd-auth-verify/src/lib.rs:339-360`; `components/auth/principal_extractor.rs:34-49,70-99`; `components/auth/caller_extractor.rs:43-54,70-80` | PASS. The extension is populated only after verification. Caller copies data_tenant_id from the verified principal; request tenant metadata cannot override it. The access-token branch remains the original verifier path. |
| Invocation, listing, authorization and retained evidence use that tenant | `components/gateway/invocation.rs:426-483,614-685,726-791,848-859,888-910`; `components/gateway/service.rs:656-675` | PASS. Permission checks use caller.principal; deployment snapshots use caller.data_tenant_id; admitted state retains both. Admission/accounting transactions use call.tenant, and audit/capture stage under the same tenant. Observation-target lookup also acquires that tenant's connection. No raw credential is propagated as resource authority. |
| Sibling ingress consumers retain tenant scoping | `components/gateway/routes.rs` handler and shared invocation call sites; `components/gateway/batches.rs:382-420,480-570,620-694,771-791` | PASS. Native/compatible invocation, models, Files and Batches receive the same Caller. File/batch lookup and persistence use caller.data_tenant_id through the existing conn owner; subsequent provider work uses the shared governed invocation. No alternate tenant construction was added. |
| Gateway cache does not broaden other surfaces | `http/router.rs:60-118`; `http/middleware/authenticate.rs:35-55,71-126`; `grpc/otlp.rs`; unchanged token grant principal-only sibling | PASS. Management, MCP and Bifrost retain their own verifier layers and do not consult GatewayApiKeys. OTLP's pre-existing static-key path remains independent and unchanged, using the shared grant's principal-only capability rather than this cache. |
| Test fixtures do not bypass the tenant boundary under review | `wyrd-testing/tests/gateway/harness.rs:177-275`; `native.rs:949-1023`; changed Python and TypeScript tests | PASS. deploy_as sends the supplied administrator token through real management HTTP. The second tenant is provisioned by the existing fixture, its key is independently exchanged, and the ledger assertion uses the fixture's TenantConn for that tenant. Stock clients receive a URL and static key while setup remains fixture-owned. |

Existing alternatives were checked: VerifierCache stores tenant/UID-qualified immutable Card values, while the client auth cache owns one client credential and transport renewal. Neither is a substitute for the explicitly required server-side multi-key, fixed-age token cache. The implementation reuses the complete existing grant and verification workflow rather than creating another tenant parser, SQL authority resolver, or exchange path.

## Verification assessment and limits

Source-inspected proof includes the cache unit test at `api_keys.rs:203-251` (distinct complete keys, fixed expiry and eviction) and `native::gateway_api_key_exchange_is_scoped_cached_and_revocable` at `native.rs:671-1108`. The real-server journey first warms an invoking principal, refuses another principal's underprivileged key twice without dispatch, then presents a second tenant's key to the first tenant's configured model and expects 404. That refused call has already exchanged and cached the second key; after creating its own deployment, the same cached key succeeds. Separate ledger reads assert six calls in tenant A and one in tenant B. The cold/hit audit assertions independently pin the original caller's principal identity. These checks exercise the changed boundary rather than an in-process principal substitute.

`native::gateway_api_keys_remain_rejected_outside_ingress` at `native.rs:1118-1205` warms a key, then proves authentication refusal on MCP, management and Bifrost, with access-token controls. The Python OpenAI, Anthropic and Google clients and TypeScript OpenAI client changes provide additional real-client closure; tenant-negative proof is owned by the Rust journey, not duplicated across client libraries.

The task records successful exact U1/U2/R1, J1/J2/R2/R3 and stock-client runs, adjacent regressions, tenant-isolation and dependency checks. Those are supplied execution evidence, not commands independently rerun by this reviewer. I did not run Cargo lanes or external services. I checked the corresponding test bodies and production paths; a green static tenant-isolation lane was not used as a substitute for capability inspection. No new persistent schema or RLS policy is part of this candidate.

HEAD still matched the immutable candidate at review completion, and the worktree showed only review artifacts outside the committed subject.

## Material proposed findings

None. No reachable cross-key, cross-principal or cross-tenant authority reuse, newly widened SQL capability, or tenant-confused consumer was found in the cumulative candidate.

Skipped: independent runtime reruns and unrelated pre-existing tenancy mechanisms. Remaining limit: concurrency and revocation timing are separately reviewed domains; this PASS concerns tenant identity and isolation.
