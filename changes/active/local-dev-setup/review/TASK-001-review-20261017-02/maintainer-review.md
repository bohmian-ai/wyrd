# Maintainer review — PASS

## Subject and method

Reviewed the cumulative `bc3bf25e3f78b0c429110350a3de4d58290e632f...38dbc023cd71292e1badd3abe90e87c69b76fa96` change, using `5cfb0a3...38dbc02` only to locate remediation. HEAD remained the recorded candidate. Scope is this worktree; sibling repositories are excluded. No CodeGraph directory exists. Read the approved specification, original task, prior validated findings and R1, without consulting other current-round reports.

Applied AGENTS §§2–9, 11 and 16, agent rules, maintainer-style, spec-driven-development, the reference router, and applicable protocol/client, doctrine, security, deployment and testing guidance. This is a source audit, not another implementation or acceptance reviewer. A finding must have a concrete consequence, remove unearned machinery, or meet the explicit rustdoc exception.

## Changed-surface coverage

| Surface | Owning source, callers and proof inspected | Result |
|---|---|---|
| Local orchestration and input selection | Complete `server/dev.rs`: `DevArgs`, `DevInputs`, `DevServer`, `ProvisionedTenant`, resolution, local paths, retained sealing selection, saved client, startup/stop, recovery and gateway offers; CLI module registration and existing installed-server/setup/config owners | PASS: the retained child, effective inputs and sealing path have one lifecycle owner; parsing/preparation stays synchronous except terminal input; no alternate durable engine |
| Terminal and selection | `terminal.rs`, `mcp/{mod,hosts}.rs`, `offer_local`, both host readers, `selected`, concealment and cancellation paths; guided Python PTY helpers | PASS: `Terminal` retains the registered fd; `EchoGuard` retains restoration state; the existing host reader is replaced by shared input rather than duplicated; one selection grammar serves hosts/providers |
| Protected settings | Complete changed `global_config.rs` methods and surrounding load/save behavior, `config.rs` consumer, `CredentialsFile` lock/document/replace/token-cache paths | PASS: settings project into existing config/credential files; locking and replacement reuse their owner; domain data and file operations remain distinct |
| Gateway authoring and shared transport | Complete changed `gateway.rs` types/dispatch and surrounding decoding/list behavior; `AuthoringApply::resolve`, `DeploymentFile::read`, `CredentialWriter`, shared gateway route constant, HTTP transport ownership | PASS: local-only authoring types earn their conversion boundary; existing single/list semantics remain explicit; mutation uses the existing restricted client handle and shared transport |
| Pure contracts and declarations | `gateway/deployment.rs` apply request/response/validation and existing deployment validation; module exports; route and MCP schema sources; SDK gateway boundary | PASS: documented typed wire shapes; no IO or language-specific durable contract; no first-class SDK secret mutation export or authored declaration divergence |
| Server administration | `GatewayAdministration` composite operation, existing individual put/revoke/delete, audited transaction wrapper, extracted write helpers, HTTP router and MCP dispatch/catalog | PASS: meaningful dependency-owning service reused; individual/composite operations share validation/sealing/upsert bodies; caller retains transaction lifecycle; no second persistence or audit owner |
| Tests and fixtures | CLI gateway journey, complete changed `LocalDev` helpers and first-use/repeat/saved-restart/managed-secret cases; server apply test and credential snapshot/open helpers; MCP administration/catalog/count changes; served OpenAPI assertions | PASS: scenarios assert durable/redacted outcomes; SQL abort checks retained envelope and plaintext usability; new fixture port selection uses standard-library binding; changed/new Rust items have substantive rustdoc/panic coverage |
| Python integration | Changed installed CLI fixture, gateway setup and lost-answer proxy, `converse`, hidden-question interruption, unattended tty host test, guided setup and optional standalone-example test | PASS: owning runtime exercises PTY/process behavior; helper names/signatures and docstrings match their roles; readiness-reader correction preserves and strengthens the original deployment assertion |
| Docs, example and build/package integration | In-tree setup/quickstart/gateway/troubleshooting/MCP docs, gateway README/document, security/operations deltas, mise local-dev selectors, TS package README inventory, CLI manifest and lock addition | PASS: public usage projects the same command/document; local versus server environment semantics and uncertainty are explained; existing dependencies and canonical verification lanes are reused |

## Existing alternatives and machinery assessment

- Installer/migration/provisioning remain `ServerInstaller`, installed `migrate` and server `setup`; CLI changes coordinate those owners instead of implementing their work again.
- `GlobalConfig` extends the existing protected `CredentialsFile` mechanism. A second settings store, profile abstraction or filesystem transaction layer is unnecessary and absent.
- Sealing-file validation uses `wyrd_utils::secret::read_secret_file`, the same reader as server configuration. Local byte decoding only supports retaining/comparing the selected effective key; the CLI does not implement encryption/keyring/rewrap behavior. Existing `SecretKey`/`SealingKeyring` own cryptography and do not provide a reusable local configuration decoder/persistence operation.
- `AuthoringApply` adds only the requested local environment form; pure wire validation remains `ProviderDeploymentsApply::validate`, and server-specific assignment, tenant, provider-reference and sealing checks remain the administration service's responsibility.
- The server extracts existing single-resource bodies into `write_credential`/`write_deployment`; extending those methods is sufficient for composite atomicity. No replacement SQL, lock, transport or audit workflow was added.
- `Terminal` replaces the existing nonblocking host input code and adds retained echo restoration; it is a resource owner, not a utility struct. `DevServer` retains the actual child and dependencies throughout setup/foreground settlement. No additional ownership abstraction is warranted.

## Prior-finding closure and remediation consequences

| Prior finding | Current source evidence | Maintainer assessment |
|---|---|---|
| FIND-TASK-001-1 | `DevServer::run` passes `inputs.interactive` to `offer_local`; explicit hosts bypass implicit selection | Closed in this lens; policy has one producer and a documented consumer |
| FIND-TASK-001-2 | Resolution rejects unrelated saved origins; selected-server saved-client preflight precedes `save_settings` and `provision`; final `save_local` conflict protection remains | Correction reuses authentication/config owners; new two-database, remote-selection and listener-override assertions document the boundary |
| FIND-TASK-001-3 | `local_sealing_key` retains validated generated/file/inline selection; every child receives retained file; managed-secret test covers all three sources and bare restart | Missing saved effective selection addressed without a new store; comparison/refusal preserves existing retained material |
| FIND-TASK-001-4 | `configure_gateway`, route 503 description, client/service rustdoc and setup/troubleshooting describe refusal versus uncertain response; proxy proof reads committed state after failure | Prior MNT-002 root cause closed; no compensating write, retry engine or receipt machinery |
| FIND-TASK-001-5 | Biased interruption select now surrounds `resolve`; concealment's guard drops with pending input | Clear lifecycle boundary; PTY INT/TERM test observes restoration and lack of setup effects |
| FIND-TASK-001-6 | Apply test first rotates an existing managed secret, then hits referenced-provider FK rejection; stored row/deployments unchanged and old envelope opens | Actual database-aborted rollback is directly asserted; no failure-injection framework |
| FIND-TASK-001-7 | `gateway`, `gateway_env`, composite CLI journey and `apply_document` now have panic contracts; `VARIABLE`, `SECRET`, `SUBMITTED`, `APPLIED`, and new `ROTATED` explain test roles | Prior MNT-001 rustdoc blocker closed; no further missing rustdoc found on changed items |
| FIND-TASK-001-8 | `LocalDev::free_bind` is loopback `bind(:0)`/`local_addr`; procfs and atomic counter deleted; shared constructors consume it | Platform-specific fixture dependency removed using an existing standard approach; released-port limitation is accurately documented |

The `converse` correction waits for the summary's final line rather than its opening line. Its only caller asserts the later gateway summary; source supports the recorded partial-read diagnosis, and the assertion is retained. The remediation adds no optional style refactor or new architectural owner.

## Findings and limits

**No material maintainer findings. No optional refactoring proposals. Overall: PASS.**

This pass performed no Cargo/build/lint/codegen/integration executions and changed only this report. R1 records focused passing commands on Linux x86_64; this review inspected their scenarios and the documented reader diagnosis, without independently rerunning them. macOS/arm64 execution and sibling repositories were not checked; this PASS is the maintainer lens, not a claim of full task acceptance or a four-target runtime matrix.
