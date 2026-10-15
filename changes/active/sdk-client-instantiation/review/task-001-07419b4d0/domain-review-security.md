# Security and identity domain re-review

Result: **PASS**. No material proposed findings in the reviewed security/identity boundary.

## Subject and authority

- Immutable cumulative subject: `2f1db2194f5630df850b10987fe0f360946a525c` → `07419b4d060303959384b682ca88e8673c722e63`; latest closure diff starts at `fde7d396a3917964dc344ede2ed09046e179aa66`. HEAD was rechecked at the candidate before reporting.
- Read the original TASK-001 from the base, approved specification revision 11, assigned subject/map, task-review skill, prior security report and validated finding ledger. No current peer reports or implementation completion conclusions were used.
- Governing authorities: user-supplied AGENTS rules; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model, Runtime identity and delegation; `architecture/wyrd-doctrine.mdx` Public surfaces; `architecture/bifrost-design.md` Public surface; `architecture/wyrd-security-posture.md` identity, delegation, authorization, tenant isolation, audit and secret handling; spec-driven development and maintainer-style references.
- Scope: credential resolution and retention; explicit/delegated authority across Rust, Python and TypeScript; eager/refusal behavior; separate Platform session path; retained audit subject/actor proof; Bifrost discovery tenant/RBAC boundary; errors/redaction and changed consumers. This is a domain result, not a whole-task verdict.

## Authority and source coverage

| Obligation / boundary | Exact source evidence and assessment |
|---|---|
| REQ-002, INV-002; one shared ambient resolver | `crates/shared/wyrd-client/src/client.rs:75–101`: `from_global` delegates to `from_environment(Process)`; that resolver loads GlobalConfig and invokes the existing ClientConfig credential owner. `config.rs:221–249` preserves explicit/environment, saved human login and credentials-file precedence and refusal behavior. The old `ClientConfig::from_global` resolver is removed. Rust journeys use this actual resolver rather than reconstructing it (`sdk_client_instantiation.rs:131,143,156,186,390`). PASS under spec11; no new source, runner mechanism or auth profile required. |
| REQ-003/014/015; tenant handles retain the supplied authority | `client.rs:111–132` uses existing `access_token()` then clones the client. Cards `cards/handle.rs:189–225`, Gateway `gateway.rs:67–84`, Principals `principals/handle.rs:49–66`, Operators `operator_connections.rs:58–75`, Bifrost `bifrost/facade.rs:123–155,221–243` retain that client. Existing `auth.rs:840–935` remains the bearer/cache/renewal owner. PASS: no fallback to ambient after explicit-client refusal; bearer/fresh tokens add no validation probe. |
| REQ-007/014; Python authority retention through lazy first use | `sdks/wyrd-sdk-python/src/client.rs:84–145` retains the supplied native client and publishes one successful native handle; `state/mod.rs:1665–1738` shares that owner with typed Cards views. Bifrost `bifrost/mod.rs:541–664` separates local binding from native connection and chooses `connect_with` only for the retained explicit client. Gateway, Principals and Operators handle methods use the same LazyConnection choice. PASS: local access does not select a new authority; first successful connection owns later operations. |
| INV-003; Rust and TypeScript identity projection | Shared delegation `auth.rs:771–788` retains subject token, audience and actor middleware, uses memory-only delegated caching, and shares existing exchange resources. TS `native/src/client.rs` delegates to that owner; `native/src/{cards,gateway,principals,operators}.rs` clone and connect that client. Public `wyrd/src/index.ts:1458–1459` uses the supplied client before ambient construction; each handle factory retains its native result. PASS: no new identity headers, subject substitution or language-local auth implementation. |
| D4, spec10; Platform/tenant separation | `platform/handle.rs:70–118` resolves ambient configuration but bypasses tenant `connected`; only the existing presented bearer variant proceeds to `platform_session`. `:125–135` retains the session-bound client. Server setup and CLI platform credential changes only migrate the constructor name. Rust ambient journey `sdk_client_instantiation.rs:150–170` supplies the existing platform credential through Environment and performs a platform operation. Saved tenant credentials remain inapplicable to Platform; spec10 explicitly approves that limitation. PASS: no new saved source or tenant-to-platform exchange. |
| AC-003; retained server subject and actor attribution | Rust `sdk_client_instantiation.rs:276–367` preserves subject permission denial, performs a delegated Bifrost query, waits for retained audit and filters by the known subject ID, then requires the actor ID. Python `test_sdk_client_instantiation.py:180–211` checks eager and lazy delegated queries and requires two retained subject-filtered rows with the actor. TS `sdk-client-instantiation.test.ts:123–149` checks the same server-produced subject/actor evidence for its delegated query. These are nonempty exact assertions, not decoded client claims. PASS source proof. |
| Existing producer of attribution | `caller_extractor.rs:44–52` obtains tenant, principal and chain together from verified identity; `wyrd-server/src/query/service.rs:181` carries the chain into the query context. `vala-bifrost-redux/src/oracle/mod.rs:650–664,692–710,3364` stages the read decision under the effective principal with delegation detail. Rust retained-audit helper `wyrd-testing/src/server.rs:1954–1968,2003–2059` reads canonical retained Bifrost records. PASS: the new assertions observe the existing authoritative sink; audit semantics are unchanged. |
| REQ-017–019, INV-001; discovery RBAC and tenant boundary | Shared `bifrost/query.rs:316–321` sends the fixed authenticated list route; async/blocking/Python/TS projections return the existing compact entries. Server `bifrost/service.rs:113–130` authorizes `bifrost_table:read` before catalog access. Catalog `bifrost_catalog.rs:1379–1387` obtains TenantConn; SQL `olap_catalog.rs:133–148` runs under that bind. MCP sibling `mcp/bifrost.rs:162–186` reaches the same service. PASS: no caller-selected tenant, raw pool or bypass added. |
| AC-008/009; negative discovery proof | Rust journey `sdk_client_instantiation.rs:432–495`, Python `test_sdk_client_instantiation.py:236–260`, TS `sdk-client-instantiation.test.ts:174–196` create a foreign-tenant table, exclude it from the own listing, assert existing RBAC denial, and separately describe the own schema. PASS source proof; no execution claim. |
| INV-004; errors and credential redaction | Existing `transport/credential.rs:92–119`, `auth.rs:666–673`, `transport/http.rs:134–144` redact credential/auth/transport Debug state; Environment Debug prints names only. Lazy owners introduce no secret-bearing Debug/repr. `auth.rs:95–100` and SDK boundary mappers preserve the existing Wyrd error projection. PASS within changed paths: no raw credential logging/interpolation or new error channel observed. |
| Dependency, deployment and consumer closure | Cumulative manifests/lockfiles are unchanged. CLI eager factories, server setup, local-loader/workflow migrations, executable examples and native Bifrost construction changes preserve the existing client/auth owner; no new endpoint, credential storage, CORS/callback, crypto or SQL mechanism enters this change. PASS for reviewed security consequences. |

## Prior finding closure

- **FIND-TASK-001-3: closed in source.** Each language now filters server-retained audit by the known subject and checks the actor chain while preserving meaningful permission denial. The existing server attribution producer remains unchanged.
- **FIND-TASK-001-5: closed in source.** `sdks/wyrd-sdk-rust/tests/integration/saved_user_auth.rs:207–224` still revokes the real saved refresh chain and expires its access token, then checks `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` on eager connection refusal. It no longer expects successful connection before the unreachable protected call. Sibling successful login/renewal behavior was retained.
- **FIND-TASK-001-6: resolved by approved spec10.** Saved-only acceptance is limited to tenant handles with an existing saved credential source. Platform keeps its environment credential/session and missing-credential behavior. No production source was added or weakened to satisfy the former conflict.
- Related **FIND-TASK-001-2**: spec11 expressly permits injected Environment through the sole shared resolver; the manual test resolver is deleted and production `from_global` delegates to that resolver. Python additionally performs eager AsyncBifrost connection and real listing in both ambient modes (`test_sdk_client_instantiation.py:95–108`). No superseded process-environment/runner requirement is imposed.
- Related **FIND-TASK-001-1**: revised local Cards view/Bifrost binding paths retain the same identity owner and defer connection. No security consequence remains in those paths. Whole concurrency/behavior closure and FIND-4 documentation coverage belong to their independent reviewers.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

None identified.

### Low / Defense In Depth

No task-scoped change recommended.

### Positive Controls

- Explicit and delegated clients keep their original middleware, transport and renewable cache.
- Eager exchange preserves existing refusal and bearer semantics; it adds no health/auth probe.
- Platform sessions remain distinct from tenant token exchange and tenant saved credentials.
- Discovery permission is enforced before tenant-scoped catalog access.
- Retained attribution assertions now cover subject and actor across all three languages.

## Verification limits

Static source review only; no test, build, generator, formatter, runtime child process or external vulnerability scan was run. Task evidence reports focused checks, but raw transcripts were not supplied and those pass claims were not independently reproduced. No current peer report was read. Only this assigned report was written; source and earlier artifacts stayed immutable.

Skipped execution and unrelated hardening as assigned. Residual risk: runtime journey and managed identity results depend on the recorded verification evidence; this PASS establishes no additional runtime result.
