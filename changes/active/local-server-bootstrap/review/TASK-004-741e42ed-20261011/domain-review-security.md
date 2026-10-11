# Domain review: security, RBAC, and tenancy

Subject: TASK-004, `cb5656b2b913dbd88a8f6736bf4a0baaf998960c..741e42ed97ac3ff9638217bbcd4f0c0583e6d464`.

Overall discovery result: **FAIL**. Three proposed findings require independent validation.

## Reviewed boundary and coverage

| Boundary | Authority | Source and proof inspected | Assessment |
|---|---|---|---|
| Local listener selection and startup trust | Spec REQ-005, REQ-008, INV-003; security posture trust boundaries | `server/dev.rs`; ordinary server `app::{run,WyrdServer::bind}`; client platform exchange | HTTP loopback enforcement exists, but readiness does not establish listener ownership. Public gRPC local enforcement is absent. |
| Platform and tenant credential issuance | Spec INV-001; security posture administration-plane separation | Server `main::{setup,root_credential,migrate}`; `Platform::with_client`; `TokenExchange::{new,platform_session,post_form}` | Durable authority remains server-owned. Startup listener substitution can expose the root credential before tenant setup. |
| Setup output and tenant identity | Spec REQ-008, REQ-010 | `ProvisionedTenant::{parse,credential}`; server setup output producer; saved-client verification | Typed tenant ID and key-prefix consistency checks prevent accidental use of another tenant’s default key. Actual authentication remains server-owned. No independent authorization bypass found. |
| Protected local files | Spec REQ-009, INV-004 | `GlobalConfig::{local_api_key,save_local}`; complete `CredentialsFile` owner; existing saved-login and token-cache consumers | Existing ownership, mode, symlink, locking, and atomic-replacement protections are reused. Endpoint persistence leaves a stale gRPC destination reachable. Persistence concurrency is separately assigned. |
| Secret disclosure and errors | Spec REQ-008, REQ-015 | Child invocation/output capture; `disclose_root`; static recovery errors; CLI journey assertions | Tenant secrets are withheld from ordinary output and argv. Root disclosure uses `/dev/tty`. |
| MCP host connection | Spec REQ-011–REQ-014 | `offer_local`; existing installer, host configuration owner, and shared credential-resolution path | Local composition supplies endpoint and host selection, without adding a credential store or secret-bearing host entry. |
| Installed projections and dependencies | Spec REQ-001, INV-001 | Python and Node CLI diffs; CLI manifest and lockfile changes | Signal projection changes introduce no separate credential or permission mechanism. `rustix` is already a workspace dependency; no new package version enters the lockfile. |
| Evidence | Task’s corrected final evidence; repository testing rules | CLI first-use/retry/MCP tests; installed-runtime fixture and SDK consumers | Existing evidence covers ordinary startup and recovery, but not the three proposed paths below. |

## Security Audit

### Critical

None proposed.

### High

- **SEC-001 — INCORRECT: readiness can authorize setup against a substituted local listener.**  
  **Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:198` and `:341`.

  **Violated obligation:** REQ-005 requires startup of the owned ordinary server; REQ-008 requires safe provisioning and credential handling. The security posture states that network location is not identity.

  **Producer-to-consumer evidence:** `DevServer::start` reserves the requested port through migration, drops that reservation, and then spawns the server. `ready` checks that the child has not exited and accepts any successful HTTP response from `/readyz`. These checks do not establish that the responding listener belongs to the child. The ordinary server awaits `build_state` before calling `WyrdServer::serve` (`wyrd-server/src/app/mod.rs:78–88`); HTTP binding happens subsequently in `app/server.rs:287`.

  Once readiness returns, `provision` invokes the installed server’s `setup --server <endpoint>`. Server `main.rs:280–296` initializes or obtains the platform root and builds a platform client for that endpoint. `Platform::with_client` exchanges the credential through `TokenExchange::platform_session`, which sends it as `subject_token` to `/auth/platform/token` (`wyrd-client/src/auth.rs:309–324`, `:478–505`). Loopback HTTP passes the existing transport validation.

  **Plausible exploit:** A different local user claims the released loopback port while the genuine child is still constructing state, answers `/readyz` with 200, and receives the setup process’s platform credential at `/auth/platform/token`. The attacker needs neither configuration-directory access nor database access. The genuine server’s eventual bind failure occurs after the credential can already have crossed the boundary.

  **Impact:** Disclosure of the durable platform administrative credential. An attacker can use it when the genuine deployment becomes available to administer tenants and recover tenant administration.

  **Required correction:** Establish that the owned server actually owns the endpoint before permitting setup to transmit credentials. The correction belongs at startup/listener ownership, rather than in tenant-key parsing or after provisioning. Native listener ownership continuity into the existing server listener owner is a conventional mechanism to assess; delays, another child-liveness check, or stricter parsing of an unauthenticated readiness response do not close the race. If introducing that handoff requires a new resource-ownership decision under the review skill, route that decision explicitly rather than prescribing an unapproved protocol.

  **Focused closure proof:** Interpose another local listener during the reservation-to-server transition while genuine boot is delayed through a normal fixture seam. Prove it cannot receive a platform or tenant credential and that setup fails safely or remains attached to the genuine server.

  **Existing-owner assessment:** `WyrdServer::bind` already owns actual listener binding and bound addresses. Its in-process callers avoid bind/rebind races. The new CLI reservation does not extend that ownership across the process boundary.

### Medium

- **SEC-002 — INCORRECT: changing the saved HTTP deployment leaves the old gRPC destination active.**  
  **Location:** `crates/shared/wyrd-client/src/global_config.rs:141`; incomplete success verification at `crates/wyrd/wyrd-cli/src/server/dev.rs:558–564`.

  **Violated obligation:** TASK-004 requires fresh SDK processes to use the saved local deployment; REQ-009 requires persistence of its effective endpoint while preserving unrelated content. Credential handling must not disclose credentials to another deployment.

  **Producer-to-consumer evidence:** `save_local` changes only `[client].http_url`. An existing `[client].grpc_url` survives. `ClientConfig::from_environment` gives that saved gRPC value precedence over the endpoint derived from the new HTTP URL. `verify_saved_client` performs token acquisition and an HTTP Card list, so it can announce readiness with that split configuration.

  Bifrost’s `connect_with_config` uses `client.grpc_config` and the same authentication middleware as the HTTP client (`wyrd-client/src/bifrost/grpc.rs:98–105`). `add_auth_metadata` sends the locally minted bearer in `x-wyrd-access-token` (`:267–281`). Ingest requests also carry the submitted data.

  **Reachable scenario:** A user has a protected config containing a gRPC endpoint for a previous hosted deployment and no conflicting default API key. Local bootstrap saves a new local tenant key and HTTP URL and succeeds. A subsequent fresh SDK Bifrost writer connects to the retained hosted gRPC endpoint, presenting the local access token and ingest payload there.

  **Impact:** Broken local SDK ingest, plus disclosure of a local bearer and submitted evidence to the old endpoint’s operator. HTTPS protects transit but does not make that operator the intended local recipient.

  **Required correction:** Treat the gRPC destination as part of the deployment connection being established. Reuse `GlobalConfig`/`CredentialsFile` and shared endpoint derivation; do not add another configuration file or transport owner. The smallest preservation-friendly correction is to refuse a conflicting saved gRPC destination before issuing a one-time key, with actionable guidance to use a separate configuration directory or correct the destination. Consistent local gRPC settings must remain supported.

  **Focused closure proof:** Start with a protected config containing a stale gRPC destination. Prove bootstrap either rejects it before provisioning while preserving files, or establishes a consistent saved local destination. A fresh saved-config Bifrost write must never send its local bearer or payload to the stale destination.

  **Sibling consumers:** Rust, Python, and TypeScript Bifrost clients consume the same shared client configuration. Correcting the bootstrap configuration boundary closes the shared path without language-specific guards.

- **SEC-003 — VIOLATION: the local command allows a remote plaintext public gRPC listener.**  
  **Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:151–161` and `:483–514`.

  **Violated obligation:** Spec INV-003 explicitly excludes remote plaintext transport. `architecture/operations/deployment-and-release.md`, “Network and TLS boundaries,” confines plaintext listeners to loopback development or an authenticated local transport boundary.

  **Evidence:** `local_bind` validates only the HTTP `WYRD_SERVER_BIND`. `DevServer::command` supplies that HTTP override and otherwise retains ambient server settings, including `WYRD_GRPC_BIND`. The ordinary server configuration accepts that gRPC socket address, and `WyrdServer::bind` binds it directly. The public gRPC plane is plaintext; the TLS configuration inspected belongs to the separate private peer plane.

  **Reachable scenario:** A developer retains `WYRD_GRPC_BIND=0.0.0.0:50051` from a container/server shell and runs `wyrd server dev`. HTTP is forced to loopback, but public gRPC listens on every interface. The local command does not reject this configuration.

  **Impact:** The purported local deployment exposes its authenticated public gRPC surface over remote plaintext transport. Clients using that listener expose bearer material and evidence to network interception. Authentication remains present; this finding does not claim unauthenticated access.

  **Required correction:** Keep ordinary server boot unchanged. At the CLI-owned local-input boundary, validate/select a loopback public gRPC bind and supply that effective value to the child, analogous to HTTP. Reuse shared endpoint configuration for the saved client destination. A configured file value must not bypass this local override.

  **Focused closure proof:** Run local bootstrap with a non-loopback gRPC override and prove rejection before provisioning, without printing secrets. Retain a successful custom-loopback-port case and ordinary-server remote-binding behavior.

### Low / Defense In Depth

No optional hardening proposed.

### Positive Controls

- Database URLs and credentials remain in the environment or protected files, rather than process arguments.
- Setup stdout is captured; tenant credentials are never replayed to ordinary stdout or stderr.
- One-time platform-root disclosure targets `/dev/tty`; unsuccessful disclosure produces secret-free recovery guidance.
- Server-owned migration, root initialization, authenticated tenant provisioning, authorization, and audit remain authoritative.
- The requested tenant is a validated `TenantSlug`; returned identity is a typed `DataTenantId`.
- Saved default credentials are checked against the returned tenant identity and conflicting keys are preserved.
- Credentials retain owner-only permissions; unsafe ownership, modes, and symlinks fail closed.
- Existing directory locking and atomic replacement are reused. Retry avoids rewriting an equal key and clearing its token cache.
- Authentication exchanges reject remote plaintext HTTP and redirects that could replay secret bodies.
- Production signing-key hardening remains active; local startup does not force the development profile.
- MCP host files contain a proxy command and endpoint, without Wyrd credentials.
- Existing release signature and digest verification remain the installation mechanism.

## Verification limits

This is a source-based discovery audit. I did not rerun the recorded integration lanes or execute the listener-substitution exploit. The task’s green evidence does not cover these proposed paths; independent validation must assess them.

No source or report files were edited. Live release downloads, forced terminal-write failure, and persistence concurrency were not independently tested.
