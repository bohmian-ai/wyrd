# Security, RBAC, and tenant-boundary review

Result: **PASS**. Proposed material finding IDs: **none**.

## Immutable subject and boundary

- Base: `1b962bb00`.
- Candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
- Approved authority: `changes/active/agent-readiness/spec.md`, revision 2.
- Original tasks: TASK-001 declaration/get/pull, TASK-002 machine-readable local tooling, TASK-003 onboarding. TASK-004 is excluded.
- Boundary reviewed: authenticated callers and tenant identity entering declaration, replay and Card reads; MCP and hosted-tool permissions; authorization audit staging; CLI machine output, readiness and MCP-host credential isolation; onboarding credential instructions.
- HEAD remained the candidate during inspection. Only this report was written. No implementation, commits, or other reviewers' reports were used.

## Authority and source coverage

| Governing authority | Application and inspected sources |
| --- | --- |
| `AGENTS.md` §§2, 7–11; `architecture/agent-rules.md` | Verified Caller owns tenant selection; tenant persistence retains `TenantConn`; authorization precedes protected work; audit uses the process Scribe outbox. Inspected Card routes/service, SQL declaration operations/migrations, auth projection, caller extractor, and audit owner. |
| `architecture/wyrd-security-posture.md`, security principles, principal/credential lifecycle, authorization, tenant isolation, audit/privacy, cryptography/secret handling | Compared changed declaration/read boundaries and machine output against verified-credential tenancy, domain authorization, separate key issuance, secret-free output, and nonblocking audit requirements. |
| `architecture/wyrd-design.md`, runtime identity and registry lifecycle; `architecture/wyrd-doctrine.mdx` | Declaration remains separate from credentials; Card-scoped identity and versioned relationships remain server-owned; the shared client and SDK projections do not acquire durable authority. |
| Reference router; `languages/agent-harness.md`, `languages/errors.md` | MCP read discovery remains available while each domain read authorizes at invocation; write scopes remain caller-dependent; error projection preserves existing codes and safe diagnostics. |
| `languages/spec-driven-development.md`, `languages/maintainer-style.md`; approved spec REQ-001–004/006–009 and INV-001–005 | Reviewed original task scope and source rather than treating implementation evidence as proof. No speculative security expansion or historical-data compatibility requirement was imposed. |

The cumulative diff was the review subject. Domain-focused expansion covered:

- `wyrd-spec/src/{registry,vala/audit_detail.rs,card/agent.rs,mcp.rs}` and their shared-client/SDK consumers;
- `wyrd-client/src/cards/{handle,reads,pull,saga}` and the loader/check dispatch;
- server `components/cards/{routes,service,resolve,mapping}`, `mcp/{cards,guide,mod}`, and `verification/hosted/tools.rs`;
- SQL declaration operation schema/query changes, lifecycle consumers, RLS and tenant-qualified foreign keys;
- CLI `lib.rs`, `main.rs`, `error.rs`, `card.rs`, `server/{dev,install,mod}`, and `mcp/{mod,hosts,proxy}`;
- setup/quickstart, declaration and identity guidance, MCP/tool authorization references, owning examples and corresponding generated-reference content;
- relevant declaration, MCP, CLI/host, identity and SDK journey test sources. Generated documentation and declaration churn were assessed through their owning sources and affected public text rather than as an independent security implementation.

## Boundary findings from source tracing

| Obligation | End-to-end source evidence | Assessment |
| --- | --- | --- |
| A merged selector cannot supply tenant or principal authority | `mcp/cards.rs::GetCardArgs::read` permits exactly one complete selector and calls existing `routes.rs::get_card_for` or `get_card_by_ref_for`. MCP `request_context`/`caller` recover authenticated HTTP extensions through `Caller::from_authenticated`; tool arguments do not construct caller context. Both service reads obtain `registry_tenant_conn(caller.data_tenant_id)`. | PASS |
| Both Card selector forms retain authorization, audit, and identity checks | `routes.rs:89` and `:159` invoke `audit::authorize` with `Permission::card_read()` before service IO. UID reads retain kind matching; exact-reference reads retain optional UID consistency checks. The shared `Cards::get` path delegates to the existing reads owner. | PASS |
| Declaration/replay cannot cross tenants or callers | `routes.rs:427` authorizes Card write, then Operator invocation when applicable, before idempotency parsing or service execution. `service.rs:650`, `:1078`, and `:1177` use the authenticated tenant connection and verified attribution. `declare.rs` matches replay by subject, actor, key under RLS; insertion uses `wyrd.current_tenant()`. Renamed operation tables retain forced RLS and tenant-qualified FKs. | PASS |
| Declaration does not issue credentials or restore revoked authority | `persist_node` reuses `upsert_service_account_from_card` in the same tenant transaction. Existing auth projection grants the default workload role only for a newly inserted principal; replay/existing-node paths do not issue a key or grant a role again. Public receipts still exclude upload plans and internal operation identifiers. | PASS |
| Every evaluated permission retains the established audit path | Shared `audit::authorize` synchronously evaluates the permission and stages allowed/denied events on `state.scribe_outbox`; renamed declaration operation labels do not introduce another writer. Declaration enum/discriminator changes preserve independent audit outcomes. | PASS |
| Merged hosted Card reads preserve both tool and domain grants | `verification/hosted/tools.rs::Invocation::call` still checks `Permission::tool_invoke(tool)` before dispatching `BuiltinTool::CardsGet` through `GetCardArgs::read`, which performs the domain read check. Accepted caller authority is retained; no privileged replacement caller is created. | PASS |
| Guidance/discovery grants no authority | `mcp/mod.rs` still derives every invocation's caller from authenticated context; the guide is static and tenant-independent. `list_tools` retains caller-based write enumeration and owning dispatch checks. The removed get-by-ref dispatch adds no new write path. Start guidance explicitly separates CLI declaration/pull, SDK observation, MCP reads/verification, local files and server evidence. | PASS |
| Machine output does not add a credential channel | CLI error documents project existing boundary metadata; no credential field is added. `DevReady` reports endpoint/tenant/paths and a redacted storage URL. `redacted` removes userinfo, query and fragment. Tenant/setup child output remains captured separately; `disclose_root` sends a new root only to the invoking terminal, never readiness JSON or the diagnostic log. | PASS |
| MCP installation preserves host credential isolation | JSON mode renders the existing `McpInstallReport`. `installed` validates the endpoint through shared client configuration before host writes. Unchanged `HostInstaller` writes a proxy launch and config-directory pin, preserving unrelated entries and conflicts; it does not copy the credential. The proxy uses the shared client and forwards server instructions/catalog/results. | PASS |
| Onboarding uses existing credential boundaries | Setup explains saved protected credentials and foreground ownership. Quickstart provider configuration names `local_env`, carries no actual provider value in tracked YAML/JSON, and uses the existing encrypted credential owner. Local development's saved administrator credential is stated explicitly; deployment key issuance remains a distinct documented operation. | PASS |

## Verification evidence and limits

Inspected test sources corroborate the boundary analysis:

- MCP Card investigation covers ambiguous/partial selectors, not-found results, foreign-tenant reference/list reads, and underprivileged invocation. The binding/enqueue journey retains write discovery/refusal coverage.
- Server declaration tests retain Operator-bearing declaration denial before any Card/principal/binding/operation write, exactly one audit row per evaluated permission, identity/replay behavior, and foreign-tenant Card refusal.
- CLI machine-output coverage asserts structured missing-credential and denied-read results and that configured bearer sentinels never enter stdout/stderr. Installed foreground readiness tests inspect saved protected credentials and diagnostic output; MCP-host tests check unrelated configuration preservation, repeatability, saved-auth proxy use and secret sentinels.
- SDK declaration/pull and support-desk evidence supplies the reported real-server privilege and attribution regressions. TASK-003 records the published quickstart's underprivileged verification denial.

Task Implementation Evidence reports focused commands and relevant checks passing. Those execution results were treated as reported evidence, not independently rerun results. This reviewer performed static source/diff/caller/test inspection only: no broad tests, live provider/agent execution, platform matrix, penetration test, or release availability check. TASK-004 remains excluded. This PASS applies to the changed security/RBAC/tenant boundary, not the overall task acceptance verdict or all repository behavior.

No reachable changed-path permission bypass, cross-tenant authority source, alternate audit writer, credential issuance side effect, or newly introduced configured-secret output was established. No correction or closure test is requested.
