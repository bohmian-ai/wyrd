# Security, RBAC, and tenancy domain review

Result: **PASS** within the reviewed boundary. Material proposed findings: **none**.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`.
- Candidate: `d6b7c75b88db5eb144e6c282263f51692371bff1`; HEAD matched at inspection.
- Authority: approved `changes/active/agent-readiness/spec.md` revision 2, original TASK-001–003 and the prior review's R1 remediation task. TASK-004 and final integrated acceptance are excluded.
- Boundary: verified caller → declaration/replay/read authorization → tenant SQL/storage; MCP/hosted-tool delegation; authorization audit; local CLI diagnostics/readiness/credential persistence and host isolation.

Reviewed the cumulative base-to-candidate changes in this domain, using the prior-candidate-to-current diff only to locate R1 changes. Read the prior verdict, validation ledger and R1 evidence as claims to reassess. No current peer report or implementation summary supplied a verdict. No `.codegraph/` exists. Only this report was written; no source, Git state or test environment was mutated.

## Authority and source coverage

| Authority | Boundary and source coverage |
| --- | --- |
| `AGENTS.md` §§2–9,11–12; `architecture/agent-rules.md` | Server-owned identity/authorization, pure client projections, TenantConn/RLS, permission-before-action and Scribe outbox. Inspected Card route/service/read owners, declaration SQL/query/migration changes, principal projection and shared audit owner. |
| `architecture/wyrd-design.md`, doctrine/client model, runtime identity, audit, registry lifecycle and spec authoring; `architecture/wyrd-doctrine.mdx` | Declaration does not issue credentials; reference selectors do not choose tenancy; server-derived exact identity and relationships remain authoritative. Checked shared Cards/loader/saga/get/pull consumers and language projection changes. |
| `architecture/wyrd-security-posture.md`, principles, credential lifecycle, authorization, tenant isolation, audit/privacy and secret handling | Verified-credential tenant, receiving-domain authorization, separate privileged credential issuance, secret-free output, protected credential storage. Expanded into Caller/request-context, credential-file/global-config owners and HTTP transport origin/redacted Debug behavior. |
| Reference router, `languages/agent-harness.md`, `languages/spec-driven-development.md`, `languages/maintainer-style.md` | Tool discovery is not authority; static guidance adds no write capability; error/source projections retain the server's contract. Original task/R1 acceptance was assessed under these stronger boundaries. |
| Approved spec REQ-001–004/006–009, INV-001–005 | Declaration/get/pull replacement, machine diagnostics, onboarding and preserved permission/secret/tenant behavior. No migration/compatibility, release or speculative security expansion was imposed. |

Changed owners and consumers inspected include:

- `wyrd-spec/src/{registry,card/agent.rs,mcp.rs,vala/audit_detail.rs}`; shared client Cards, declaration saga, reads and renamed graph/bundle/workspace owners; loader local-check projection.
- Server `components/cards/{routes,service,resolve,mapping}`, `mcp/{cards,guide,mod}`, `verification/hosted/tools.rs`, plus existing `audit/mod.rs` and authenticated caller derivation.
- SQL declaration query/replay/lifecycle sources, operation/artifact/card schemas and tenant-qualified constraints/RLS; existing `auth_projection.rs`.
- CLI `lib.rs`, `main.rs`, `error.rs`, `card.rs`, server dev/install and MCP install/hosts/proxy; existing shared credential/config and HTTP transport owners.
- First-class SDK declaration/get/pull projections and corresponding changed tests; setup/quickstart/identity/MCP authorization guidance, example deployment callers and the source-generated reference consumers.

## End-to-end assessment

| Obligation | Source evidence and consequence | Result |
| --- | --- | --- |
| Merged Card selectors preserve authenticated tenancy | `mcp/cards.rs::GetCardArgs::read` accepts one complete selector and delegates to `get_card_for` or `get_card_by_ref_for`. `mcp/mod.rs::request_context/caller` derives Caller from authenticated HTTP extensions, not arguments. Service reads open `registry_tenant_conn(caller.data_tenant_id)`. | PASS |
| UID and exact-reference reads retain receiving authorization/audit | `routes.rs::get_card_for` and `get_card_by_ref_for` call `audit::authorize(Permission::card_read())` before protected service reads; kind/UID consistency checks remain. Shared client `reads/get.rs::assert_selector_identity` retains optional identity checks. | PASS |
| Declaration and replay preserve tenant/subject/actor scope | `declare_card_http` checks Card write, then Operator invocation for Operator-bearing bindings, before declaration work. `DeclarationWriter::write` and replay use the verified caller's TenantConn and attribution; SQL scopes replay by subject/actor/key under forced RLS and retains tenant-qualified uniqueness/FKs. | PASS |
| Principal projection grants no credential or renewed revoked grant | `persist_node` invokes the existing `upsert_service_account_from_card` owner on the tenant transaction. Its default role is granted only on first principal insertion; existing/replayed declarations do not issue a key or regrant a revoked role. Public receipts carry no secret. | PASS |
| Renamed audit values do not create a second writer | Declaration operation/detail/discriminator changes remain projections. The shared `audit::authorize` owner decides synchronously and stages allowed/denied events on `state.scribe_outbox`, without awaiting persistence. No changed client or service adds an alternate audit write path. | PASS |
| Hosted reads retain both tool and domain permission | `verification/hosted/tools.rs::Invocation::call` checks `Permission::tool_invoke(tool)` using the accepted caller, then merged get invokes the same audited Card read. No privileged replacement caller is introduced. | PASS |
| Guidance/catalog grants no authority | Start is static, tenant-independent guidance behind authenticated MCP context. It adds no declaration/upload/ingest tool; write discovery and owning invocation checks remain. It explicitly separates declarations, key issuance, local files and server evidence. | PASS |
| R1 parent diagnostic destination preserves credential boundaries | `main` installs `install_tracing` before dispatch. JSON-mode requested tracing uses the existing Environment config directory and shared append opener, outside stdout/stderr; inability to open it returns an error before a command runs. Neither the opener nor subscriber reads a credential or bypasses credential-file checks. Shared transport keeps its existing origin/credential ownership. | PASS |
| Dev child/setup and readiness do not acquire a new secret channel | `Diagnostics::attach` redirects the serving child, while `provision`/migration retain captured subprocess output. Provisioning stdout is held in SecretString; `disclose_root` sends the new platform credential only to the invoking terminal, and logs only static recovery notices. `DevReady` contains endpoint/tenant/paths and a storage URL stripped of userinfo/query/fragment. | PASS |
| Credential files and MCP host entries remain isolated | Persistence still uses existing GlobalConfig/CredentialsFile protected writes and checks. Host installation still writes a proxy launch/config-directory pin and preserves unrelated configuration; no credential is copied into host entries or reports. Proxy requests use shared credential resolution and retain server-owned permissions. | PASS |
| Pull/deploy changes preserve the existing trust owner | Renamed bundle code retains alias/artifact path validation and authenticated per-artifact downloads; R1 only hides progress. Example redeployment now retains the old bundle until the existing staged publication owner replaces it, without creating another download/authorization mechanism. | PASS |

The shared append-log helper is a diagnostic writer, not a credential store or audit sink. No new secret-producing input to that writer or reachable permission/tenant bypass was established. Requiring additional log infrastructure or unrelated filesystem hardening would exceed this review's demonstrated change boundary.

## Verification and limits

Inspected proof sources include server declaration denial/replay/cross-tenant tests, MCP exact-selector/ambiguous-selector/foreign-tenant/underprivileged paths, CLI secret sentinels, foreground saved-credential/log checks, and host/proxy isolation tests.

R1's `json_results_and_errors_are_machine_readable` source now exercises `RUST_LOG=debug` for an authenticated success and unreachable operation; whole streams are parsed and `logs/cli.log` is asserted to contain the attempt and neither the live bearer nor synthetic credential. Foreground readiness proof checks owner-only saved credentials and absence of tenant-key/database sentinels from the diagnostic log. These are concrete covered cases, not proof of arbitrary third-party tracing output at every log filter.

Original task and R1 command results are **reported execution evidence**, not fresh runs by this reviewer. This pass performed static cumulative diff/source/caller/test inspection only. No Cargo command, broad suite, live provider/agent run, penetration test, other-host execution or published-artifact acquisition check was performed. Existing supported-host and compatible-published-artifact evidence blockers remain outside this domain PASS and are not waived.

No material finding or correction is proposed. This verdict covers security/RBAC/tenancy of the changed paths, not overall TASK-001–003 acceptance.

