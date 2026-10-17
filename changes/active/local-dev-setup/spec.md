---
id: SPEC-local-dev-setup
revision: 1
status: approved
---

# One local development setup

## Objective and approval

After installing a Wyrd client and making PostgreSQL available, a developer
runs `wyrd server dev` to obtain a working local server and saved client
configuration. An agent supplies inputs without prompts and obtains the same
result. Restarting the command resumes the same development deployment.

The owner accepted the proposal with "Aligned with notes", explicitly
preserving the existing separate credential/deployment method and making
gateway setup optional. The owner requested "create the spec and 1 task plan
for this work" and confirmed "$wyrd-plan ok continue" after clarification
that existing server-side gateway credential bindings remain supported.
This revision records that agreed scope and its concrete interface contract;
implementation is not authorized by planning alone.

This is a separate change from verifier-building-blocks. Nothing has shipped;
no compatibility aliases or legacy migration framework are introduced.
Existing supported credential sources and commands are real retained features.

## Existing owners and authority

The inspected development baseline is the current repository, including the
existing installed-server local startup and gateway administration surfaces.
`DevArgs` / `DevServer` already download a verified server, run its migrations,
provision through server-owned setup, save a tenant key and HTTP/gRPC endpoints,
authenticate a fresh saved client and supervise the foreground process.
`GlobalConfig` / `CredentialsFile` already own protected local persistence.
`GatewayAdministration` owns validation, sealing, tenant SQL and audit;
`CredentialWriter` is the shared CLI/scoped-MCP secret-mutation client.

Current gaps are input collection, saved server settings, optional gateway
discovery, and an atomic credential-plus-deployment operation. Current
deployment-array input sends separate requests, and relative storage defaults
can select different data from another directory. The Python example also
duplicates bootstrap with Docker scripts.

[AGENTS.md](../../../AGENTS.md), [agent rules](../../../architecture/agent-rules.md),
[design](../../../architecture/wyrd-design.md),
[doctrine](../../../architecture/wyrd-doctrine.mdx),
[security posture](../../../architecture/wyrd-security-posture.md),
[deployment operations](../../../architecture/operations/deployment-and-release.md)
and [Bifrost design](../../../architecture/bifrost-design.md) govern this change.
Update their affected local-setup and gateway-administration descriptions with
implementation; no production readiness/security/storage invariant is relaxed.

## Scope and non-goals

In scope: installed Rust CLI and existing Python/TypeScript CLI projections;
interactive and unattended startup; protected persisted development settings;
optional provider discovery; a local environment-reference deployment document;
one atomic server gateway-configuration operation; HTTP/MCP contracts;
documentation, server-backed examples and the three standalone example repos.

Out of scope: installing PostgreSQL or creating database roles; embedded DB;
new production deployment modes; UI wizard; daemon/service-manager behavior;
new login/profile framework; automatic tenant credential rotation; new secret
vault/encryption system; discovery of arbitrary third-party endpoints; paid
provider calls during setup; SDK provider-secret mutation APIs; application
Card registration or application startup hidden inside server startup.

## Required behavior

### REQ-001 One input resolution and execution path

`wyrd server dev` works without a required `--tenant` on the parser. Inputs
resolve as explicit command argument, canonical environment, saved development
configuration, then a prompt on a usable terminal. Invalid supplied values
are reported, never silently replaced by defaults or another source.

Retain `--tenant` and `--host`. Add `--non-interactive`, `--bind`, `--grpc-bind`,
`--storage-url`, `--gateway-file` and `--skip-gateway`. Listener arguments use
the current loopback address grammar; credential-bearing storage URLs are
refused as arguments. Canonical server environment variables remain supported,
including `WYRD_DATABASE_URL`, optional `WYRD_PLATFORM_DATABASE_URL`,
`WYRD_SERVER_BIND`, `WYRD_GRPC_BIND`, `WYRD_STORAGE_URL`, and
`WYRD_BIFROST_DATA_DIR`. Secrets are supplied through environment, protected
files or concealed interactive input, never command arguments.

Without a terminal, or with `--non-interactive`, no prompt is attempted.
Report all unresolved required inputs together and exit unsuccessfully before
download, migration or tenant provisioning. A complete invocation runs the
same workflow as the interactive command. Conflicting gateway-file/skip
arguments fail before mutation. Existing optional MCP-host behavior remains.

### REQ-002 PostgreSQL, storage and saved configuration

When no database URL resolves, ask for the PostgreSQL connection URL with
concealed input, validate it through the current settings owner and use it in
all server children. This does not modify the parent shell's environment.
Persist credential-bearing connection settings through the existing protected
credential-file boundary; do not put passwords in general configuration,
output, trace fields or process arguments. Preserve the optional separate
platform DSN and existing database-role security checks.

Persist non-secret development selection in global configuration: tenant,
HTTP/gRPC binds/endpoints, effective storage selection and Bifrost data root.
The shared client continues to find its HTTP/gRPC destinations through the
existing `[client]` fields and tenant API key through the existing credentials
store; do not invent parallel `WYRD_SERVER_URL` aliases in configuration.
Describe the corresponding environment variable and saved setting to users.

On first local-storage selection, warn that `WYRD_STORAGE_URL` is unset and
local storage will be used, and show its absolute location. Use stable
user-config-relative defaults beneath the existing local server directory,
with separate storage and Bifrost roots. Save absolute effective paths;
subsequent working directories do not change data identity. Explicit existing
paths remain usable. Never silently move, erase or merge existing `.wyrd`
data: explain and require an explicit path selection when adopting it.

Protected-file validation and replacement reuse `CredentialsFile` locking,
permissions and atomic writes. Preserve unrelated client settings, logins and
keys. An unrelated saved deployment/tenant is refused with guidance to the
existing `WYRD_CONFIG_HOME` separation mechanism, not overwritten or adopted.

### REQ-003 Install, migrate, provision and supervise

Reuse `ServerInstaller`, normal server `migrate` and server-owned `setup`.
Download only when a compatible installed release is absent, retaining signed
verification and interrupted-download behavior. Show progress, await migration
success, start the ordinary serving process and await readiness before tenant
or gateway calls. "Background migration" means no separate user command,
not serving against an unfinished migration.

If there is no saved tenant selection, ask its slug and provision through the
existing platform boundary. Save its tenant-admin development API key and
client endpoints without displaying secrets; do not invent another role/key
class. A normal rerun reuses the exact tenant and key and never issues another
credential merely because it is a restart. Preserve the local sealing key so
saved managed provider credentials remain decryptable.

The CLI retains the foreground server and settles its children on interrupt,
startup-stage error or unexpected exit. Committed migrations/configuration
remain usable on rerun; failure does not reset the deployment.

### REQ-004 Optional gateway discovery and selection

Gateway setup is optional. On a fresh interactive setup, ask whether to
configure it. Declining, choosing no providers, or having no keys does not
block readiness. Noninteractive invocations without `--gateway-file` skip
gateway setup; `--skip-gateway` explicitly skips it. A normal saved restart
does not repeat this offer or republish keys. Passing a gateway document is
an explicit request to apply it, including on restart.

If accepted, discover only known nonempty provider environment variables:

| Provider / adapter | Local variables | Default deployment / credential |
|---|---|---|
| OpenAI / `openai` | `OPENAI_API_KEY` | `openai` / `openai-key` |
| Anthropic / `anthropic` | `ANTHROPIC_API_KEY` | `anthropic` / `anthropic-key` |
| Gemini / `gemini` | `GOOGLE_API_KEY`, else `GEMINI_API_KEY` | `gemini` / `gemini-key` |
| TypeSafe Jev / `jev` | `TYPESAFE_API_KEY` | `jev` / `jev-key` |

Show provider, variable name and proposed deployment name, never the value.
The user approves a subset and can change deployment names. Do not publish
unapproved variables or configure an endpoint merely from a generic API key.
Use current typed adapter/auth/capability defaults; do not discover models or
invent aliases between provider and deployment identity. Vertex/custom
providers remain usable through explicit existing configuration with their
required non-secret coordinates; automatic guessing is outside scope.

Interactive selection and `--gateway-file` produce the same local document
and call the same apply workflow. Existing selected names are explicit
replacement targets, shown during confirmation; unrelated deployments stay.
An explicitly requested gateway apply failure reports failure and settles
the owned server without claiming complete setup. Previously committed tenant
setup remains; rerun with corrected inputs or `--skip-gateway`.

### REQ-005 Deployment documents may reference local environment variables

Preserve existing `wyrd gateway credential put` followed by
`wyrd gateway deployment put`, including current single-deployment and array
document behavior. Extend the existing `deployment put --file` to also accept
a composite object with `credentials` and `deployments` arrays; `server dev
--gateway-file` consumes that same composite document.

Each credential has existing `name`, `provider` and `source`. A local-only
source form is `{"local_env":"OPENAI_API_KEY"}`. Its value names one
environment variable, not shell code, interpolation, a file or server binding.
Resolve all selected names locally before the request; missing/empty values,
invalid provider/configuration, duplicate resource names, unused submitted
credentials or invalid document fields fail without network mutation.
Every submitted credential must be referenced by a submitted deployment;
deployments may also reference existing tenant credentials.

For example, an authoring document can contain:

```json
{
  "credentials": [
    {"name":"openai-key","provider":"openai","source":{"local_env":"OPENAI_API_KEY"}}
  ],
  "deployments": [
    {"name":"openai","adapter":"openai","auth":{"bearer":{"credential":"openai-key"}}}
  ]
}
```

The CLI converts local references to existing write-only `managed_secret`
values in memory and submits one request. The document contains variable
names, not secrets; no expanded temporary JSON file or `.env` file is created.
Local references are never persisted or sent for the server to resolve.
Existing `environment { binding }` and `external_secret` sources retain their
operator-assigned server semantics. No existing source is reinterpreted.

### REQ-006 One atomic gateway administration operation

Add `POST /v1/admin/gateway/provider-deployments` for the composite wire body
`{credentials: [ProviderCredentialWrite], deployments: [ProviderDeployment]}`.
`deployments` is nonempty; omitted `credentials` means none. Existing names
are upserted, names absent from the body are untouched, duplicate/unused
credential names are invalid, and current deployment/credential validation
and terminal-revocation rules remain authoritative.

The server derives tenant and principal from authentication, requires existing
`gateway:write`, validates the entire proposed result including references to
new and existing credentials, seals managed secrets with the existing keyring,
and commits all named credentials/deployments in one tenant transaction.
Failure before commit changes none of those resources. Concurrent applies and
credential revoke/delete use the existing locking/integrity boundary: no
deployment may commit with an invalid/revoked/foreign credential, and unrelated
resource families remain unaffected. Readers observe a committed configuration,
never an intermediate credential/deployment combination.

Return 200 with submitted credentials as existing redacted views and submitted
deployments in request order, never secret material. Existing Wyrd gateway,
permission, validation and availability errors apply. Reapplying converges
through named upserts; no new receipt/job/idempotency store is added. A lost
response may follow a successful commit: safely reapply, without claiming a
failure proves nothing committed or that secret rotation timestamps are fixed.

Expose the same typed operation as scoped MCP write tool
`gateway.apply_provider_deployments`, with existing discovery/scope/domain
checks. Server-side code calls the same `GatewayAdministration` operation.
The shared CLI/MCP credential-mutation handle gains the operation; first-class
SDK `Gateway` handles do not gain managed-secret mutation. First-class SDKs
consume/read/invoke the resulting deployments through existing contracts.
All submitted values use redacted secret types and existing bounded ingress.
Audit each evaluated authorization decision through the nonblocking Scribe
outbox; rollback and audit-write failures never change that audit contract.

### REQ-007 Ready means usable saved configuration

Before the final ready message, prove `/readyz` and a fresh client assembled
only from saved files can authenticate and perform the existing bounded read.
If gateway configuration was requested, also confirm the selected deployments
were committed and are readable through the existing redacted administration
surface. No paid upstream inference or claim that provider keys were validated
by the provider is part of readiness.

Print a concise summary: tenant, HTTP/gRPC endpoints, data paths, files used,
gateway configured/skipped and deployment names, and foreground stop guidance.
Explain that fresh SDK/CLI processes use saved settings without exporting a
key or endpoint. Errors name the failed stage and a safe next action.

### REQ-008 Restart and partial setup recovery

Bare `wyrd server dev` reuses saved database/storage/listener/tenant settings,
API key and sealing key even from another working directory. Explicit inputs
override only their corresponding saved settings after validation; switching
deployment identity never silently binds an existing key to another tenant.

Do not promise transactional atomicity between Postgres provisioning and local
filesystem writes. Preflight protected persistence before issuing credentials;
retain a newly issued key safely whenever possible and resume partially saved
configuration without issuing a replacement automatically.

If the one-time key was lost or the platform root exists without usable local
authority, report that exact boundary and guide the existing platform/root or
tenant-admin recovery in the command's setup flow. Recovery requires existing
authorized platform mechanisms and explicit confirmation for rotation; an
unattended process must supply that authority and explicit recovery intent,
otherwise fail with actionable guidance. No implicit reissue, auth bypass,
new recovery protocol or automatic database reset. An ordinary stop/restart
with a saved key requires none of this recovery work.

### REQ-009 Examples, docs and machine-readable surfaces

Every server-backed repository example uses `wyrd server dev` for startup,
migration, tenant and optional gateway setup. Its deployment document can
reference local environment variables, while application Card registration
and app launch remain explicit. Update setup/quickstart and gateway guides,
canonical support-desk and related examples; retain useful documentation for
the existing two-step method. Truly offline examples need no server.

Update standalone `wyrd-example-py`, `wyrd-example-ts` and `wyrd-example-rs`
setup instructions/scripts and secret-free gateway documents. Remove their
duplicated Wyrd bootstrap/key-generation recipes; an optional PostgreSQL-only
Compose recipe may remain. Do not claim the SDKs/releases are published:
document current package acquisition separately, without requiring a Wyrd
server-image build or a Wyrd checkout just to start an installed server.

Update design, operations/security configuration guidance and generated schema
sources. Served OpenAPI and runtime MCP catalog must describe the composite
operation and its permissions/redaction. Tests use actual installed CLI
behavior, fresh SDK processes, managed PostgreSQL and local provider fixtures;
no provider credentials or live model calls are required.

## Invariants and boundaries

- **INV-001 Single owners.** CLI collects/resolves local inputs and supervises;
  server owns durable provisioning, encryption, authz, SQL and atomic apply.
  Pure contracts stay IO/async/PyO3-free; no alternate startup engine.
- **INV-002 Secrets.** No secrets in argv, examples, general config, progress,
  logs, errors, audit, responses or expanded files. Protected DSNs/keys reuse
  existing secure persistence; values only reach their intended server/provider.
- **INV-003 Identity and authorization.** Existing platform/tenant separation,
  RLS, delegation, operator assignments, provider matching and permission scopes
  remain. Loopback and a supplied slug grant no existing tenant authority.
- **INV-004 Honest persistence and availability.** Saved paths and keys retain
  identity across restart. No implicit data deletion, key rotation, insecure
  fallback or readiness claim after an unfinished required stage. A local
  component error does not justify crashing an unrelated external server.
- **INV-005 Existing features retained.** Two-step gateway setup, individual/
  array deployment documents, server bindings, external secrets, managed
  secrets and existing SDK consumption remain supported. Gateway is optional.
- **INV-006 Platform and proof.** The workflow works on macOS/Linux x86_64 and
  arm64; do not copy platform-specific fixture/process assumptions. Journeys
  prove user/agent success, refusal, restart and recovery in owning runtimes.

## Acceptance criteria and evidence classes

| ID | Required proof |
|---|---|
| AC-001 | Fresh interactive run asks missing database/tenant values, explains local storage, installs/migrates/provisions/saves, can decline gateway and reaches fresh-client readiness. Real terminal/CLI journey. |
| AC-002 | Complete unattended inputs reach the same outcome; incomplete input reports all missing requirements without prompts or setup effects; invalid/conflicting inputs are refused. CLI journey and pure resolution tests. |
| AC-003 | Optional discovery approves only selected known providers; no-key/decline works; explicit composite files resolve local variables without secret files, shell expansion or server-env dependency. Installed CLI with local provider fixtures. |
| AC-004 | One request commits all submitted gateway resources or none on validation/sealing/SQL/auth failure; concurrent revoke/replacement preserves references; unrelated tenant/config state remains. Public CLI/MCP journeys plus focused Postgres failure/concurrency proof. |
| AC-005 | Existing individual credential/deployment and array behavior, server-binding/external/managed sources and permissions keep working. Existing exact regressions with retained assertions. |
| AC-006 | Restart from another directory retains tenant/key, storage/Bifrost paths, endpoints, provider decryption and data; no environment re-export, repeat prompts or unsolicited credential issuance. Installed CLI journey. |
| AC-007 | Interrupt, failed save, gateway rejection, occupied port, migration/readiness failure and unusable saved key give honest stage/recovery results; owned children settle, committed state survives and recovery never bypasses platform authority. Existing and expanded interruption/partial-setup journeys. |
| AC-008 | Fresh Rust/Python/TypeScript SDK and MCP consumers use saved setup and gateway after restart. Secret mutation remains off SDK exports. Existing installed-CLI language projections, real server and mock provider. |
| AC-009 | Server-backed examples and all three standalone apps use the shared command and local-env document; docs match optional gateway/two-step support and current package availability. Example smoke proof, docs and source review. |
| AC-010 | Codegen, served OpenAPI, MCP schema/runtime checks, dependency/tenant boundaries, relevant typing, formatting and lint proof pass. Report actual commands; broad integrated journeys run once at change review. |

## Material decisions and open questions

The accepted outcome is one command, optional gateway, retained old setup and
one composite apply. This spec selects their concrete CLI/document/wire
contracts above, existing protected-file persistence and existing admin-key
semantics. No new authentication role, secret resolver, queue, daemon or SDK
secret-mutation surface is introduced. Recovery remains an explicitly
authorized existing operation, never silent key replacement.

No open product decision remains within that agreed scope. Private module
layout, helper methods, fixture design and non-contract file organization are
implementation-owned. If implementation needs a new durable recovery or
authentication semantic, return for a spec revision rather than inventing it.

## Revision history and sources

| Revision | Status | Change |
|---|---|---|
| 1 | approved | Records the owner's accepted one-command proposal, optional gateway and retained separate credential/deployment method; owner requested this spec and exactly one implementation task and confirmed continuation after the server-binding clarification. |

Source anchors: `wyrd-cli/src/server/dev.rs::{DevArgs,DevServer}`;
`server/install.rs::ServerInstaller`; `wyrd-client/src/global_config.rs` and
`credentials_file.rs`; `wyrd-server/src/main.rs::setup`;
`components/gateway/service.rs::GatewayAdministration`;
`wyrd-client/src/gateway_credential.rs::CredentialWriter`;
`wyrd-cli/src/gateway.rs::DeploymentFile`; `wyrd-server/src/mcp/gateway.rs`;
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py`;
`examples/gateway-setup/README.md`; standalone example READMEs.
