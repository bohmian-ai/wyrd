# Security and identity domain review

Result: **PASS** for the reviewed security/identity boundary. No material proposed findings.

## Immutable subject and boundary

- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`; HEAD was rechecked at this candidate before writing this report.
- Authority: approved `changes/active/sdk-client-instantiation/spec.md`, revision 9; original TASK-001 read from the base commit; complete cumulative diff, with security-sensitive changes and their consumers inspected directly.
- Boundary reviewed: selection and retention of ambient/explicit/delegated client authority across Rust, Python and TypeScript; eager exchange and refusal; platform/tenant separation; construction/connection secret handling; Bifrost discovery authorization and tenant isolation.
- This is an independent static domain audit, not a whole-task completion verdict. No reviewed source was edited and no build, test, generator or formatter was run.

## Authority and source coverage

| Authority / obligation | Source and reachable path inspected | Assessment |
|---|---|---|
| AGENTS §§2, 3, 4, 7, 9; agent-rules; design Client model; REQ-002, INV-002 | `client.rs:74-109` routes ambient construction through `from_global` and `connected`; `config.rs:229-276` owns credential precedence and refuses contradictory tenant selectors. Python optional client conversion and TS `clientOf` delegate to the same owner. | PASS: no language-specific credential resolver or identity fallback added. |
| REQ-003, REQ-014, REQ-015; D2 | `client.rs:107` calls existing `access_token`; Cards `handle.rs:191,219`, Gateway `gateway.rs:67,84`, Principals `principals/handle.rs:49,66`, OperatorConnections `operator_connections.rs:58,75` retain the resulting client. `auth.rs:840-935` retains bearer/cache/refresh behavior. | PASS: required exchange occurs before return; bearer/fresh cache reuse does not add an authentication probe. |
| REQ-007, REQ-014; INV-003, INV-005 | Python `client.rs:48,85-139` retains an explicit Rust client clone and initializes one successful handle. Bifrost `bifrost/mod.rs:554-625`, Cards `state/mod.rs:1665-1715`, Gateway `gateway.rs:131-184`, Principals `principals.rs:47-99`, Operators `operators.rs:64-116` select `connect_with` when supplied and `connect` only when absent. | PASS: explicit authority is never replaced by ambient identity; subsequent operations reuse their connected owner. |
| INV-003; effective principal and delegation attribution | Shared `client.rs:433-444` derives delegated auth and installs it in HTTP; `auth.rs:771-788` retains subject, audience and actor in a distinct middleware with no disk cache. TS native `client.rs` delegation and four HTTP-handle factories clone that client. Python wrappers retain the same client. | PASS: construction preserves the delegated bearer and its existing renewal path, with no new identity header, actor substitution or authorization implementation. |
| D4; design Runtime identity; security posture Two administration planes | `platform/handle.rs:70-75,94-118,125-137`: ambient Platform calls `from_global` directly, requires the existing presented credential form, exchanges at `platform_session`, and retains the session-bound client. CLI platform credential and server setup migrations only rename the existing call. | PASS: Platform does not pass through `connected` or `/auth/token`; no new tenant-to-platform capability. |
| REQ-014, REQ-017; shared transport authority | `bifrost/facade.rs:123-155,221-243,1033`; `bifrost/query.rs:162-183,316-321`; `bifrost/grpc.rs:85-106`; blocking `bifrost/blocking.rs:213`; Python `bifrost/mod.rs:636-639`; TS native `lib.rs:1169-1183` and public `index.ts:1360`. | PASS: query retains the selected client, ingest shares its auth Arc, and listing uses that client's authenticated HTTP transport. No per-operation ambient resolution, schema fan-out or discovery cache introduced. |
| REQ-018; INV-001; security posture Authorization and Tenant isolation | Existing HTTP list path leads to `wyrd-server/src/bifrost/service.rs:113-130`, authorizing `bifrost_table:read` before catalog work; `components/auth/caller_extractor.rs:44-52` derives tenant/principal/delegation together from verified identity; `vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1379-1387` acquires `TenantConn`; `vala-sql/src/queries/olap_catalog.rs:133-148` executes under that RLS bind. MCP sibling `src/mcp/bifrost.rs:162-186` uses the same service. | PASS: caller supplies no tenant/filter authority; SDK addition projects existing server authorization and audit boundaries unchanged. |
| INV-004; security posture Cryptography and secret handling; errors reference | Existing `ResolvedCredential` Debug (`transport/credential.rs:92-119`), AuthMiddleware Debug (`auth.rs:666-673`), HTTP transport Debug (`transport/http.rs:134-144`), PlatformSession Debug (`platform/handle.rs:37`) redact secret-bearing state. New lazy state adds no Debug/repr dumping retained arguments. `AuthError::into_wyrd` (`auth.rs:95-100`) and native/Python error mappers remain the existing projection. | PASS within changed credential-bearing paths: no raw credential interpolation, logging, or new error channel introduced. |
| Spec-driven development, maintainer-style, errors reference | Read applicable review discipline, existing-owner/reuse requirements, and public error conversion rules. Compared new entrypoints with existing auth middleware, platform session, HTTP transport, catalog and MCP sibling rather than treating the task mechanics as new security authority. | PASS: existing owners remain responsible; no new security machinery needed. |

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

None identified.

### Low / Defense In Depth

No task-scoped recommendation requiring a change.

### Positive Controls

- Explicit/delegated clients share their original authentication state instead of re-resolving ambient credentials.
- Eager initialization reads the existing credential path; it does not introduce token validation probes or skip later server authorization.
- Platform sessions retain their separate exchange and lifetime.
- Listing has no tenant argument and reaches the existing permission check and tenant-scoped SQL owner.
- Secret wrappers and redacted Debug implementations remain in place; foreign-runtime connection errors use existing Wyrd metadata conversion.

## Counterevidence and verification limits

- Source tests inspected: shared `client::tests::server_handles_connect_eagerly_and_share_the_client_auth_path`; Rust `sdk_client_instantiation` journeys; Python unit/integration `test_sdk_client_instantiation.py`; TypeScript unit construction/error tests and integration `sdk-client-instantiation.test.ts`. They cover denied exchanges, invalid bearer refusal on use, narrower delegated authority, cache reuse/renewal, foreign-tenant exclusion and missing table-read permission. Their presence supports the source trace; no execution result was independently reproduced.
- The forged-key journeys expect the existing `WYRD_SPEC_400_VALIDATION` OAuth projection. That code is produced by the existing grant/error path, not changed by this candidate; it is not evidence for a new authentication defect.
- Delegation journeys distinguish subject authority from actor authority using successful/refused operations. They do not independently inspect an audit row's exact subject ID; source inspection shows the retained delegated token still reaches the unchanged verified-caller/audit path. No changed attribution loss was found.
- Rust's ambient journey injects an environment into the shared resolver and then calls `connect_with`; it does not invoke the no-client Rust factories. This is a general acceptance-proof limitation communicated to the orchestrator, not a discovered security regression. Python and TS journeys use their actual no-client factories.
- Runtime results are task-appendix claims without raw transcripts. This report does not certify deployment configuration, live token exchange, actual RLS enforcement, or cryptographic implementation outside the unchanged paths traced here. No dependency or lockfile change entered this range.

No source-local finding IDs are assigned because no material security/identity finding was established. No speculative hardening or new auth machinery is proposed.

Skipped: runtime verification and unrelated repository security surfaces. Risk: PASS is limited to this static domain review and the immutable candidate.
