# TASK-001 invariant review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
- Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.
- Approved specification: `changes/active/local-dev-setup/spec.md`, revision 1.
- Original task: `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
- Overall: **BLOCKED**, because the required standalone-example changes do not have an immutable review subject. The main-repository audit also proposes four bounded implementation findings below; those findings independently prevent acceptance of the reviewed main-repository behavior.

The cumulative `git diff <base>...<candidate>` was inspected. HEAD remained the candidate; the only working-tree additions were review reports. No implementation edits or Cargo/build/check commands were performed. No other reviewer report was read.

## Authority and navigation coverage

Read the original task and approved spec, AGENTS.md, agent-rules, reference router, spec-driven-development, maintainer-style, testing-workflows, doctrine, and applicable protocol/security/deployment sections. The review follows the specification's existing startup/credential/service owners; it does not turn task-private mechanics into stronger authority.

Expanded navigation:

| Producer / authority | Consumers, sibling writers and invariant checked |
|---|---|
| `DevArgs::resolve`, `first`, `secret_var`, `local_data` | Every `DevServer::command` child, `save_settings`, `save_tenant`, gateway/host offers, ready summary; supplied/saved selection, secrets, data identity and unattended behavior |
| `GlobalConfig::{local_api_key,local_databases,save_local_dev,save_local}`, `CredentialsFile`, `SavedLogins::select` | Existing default API keys, human logins, normal CLI/SDK configuration; protected-file locking/replacement and refusal before issuing a one-time key |
| `local_sealing_key` and `DevServer::command` | Server `config::load_sealing_key`, boot `build_sealing_keyring`/`rewrap_sealed_secrets`, gateway managed-secret sealing/resolution, retained restart test |
| `Terminal::{open,ask,conceal}`, `EchoGuard`, `mcp::{offer_local,InstallArgs::run}`, `hosts::{prompt_local_selection,selected}` | Startup/recovery/provider/host questions; one resolved interactive policy must reach all prompt consumers |
| `DeploymentFile::read`, `AuthoringApply::resolve`, `put_deployments`, `apply_deployments_with` | Standalone gateway command, dev explicit-file path and interactive provider offer; single/array semantics retained, local names resolved without files |
| `ProviderDeploymentsApply::validate`, `CredentialWriter::apply_deployments` | HTTP collection route, MCP catalog/dispatch; same typed request, restricted secret mutation, redacted results |
| `GatewayAdministration::{apply_deployments,write_credential,write_deployment,audited}` | Existing put/revoke/delete, tenant SQL credential/deployment queries and schema; permission audit, one transaction, FK/provider integrity and reference share locks |
| Installed CLI, CLI gateway, server administration, MCP and OpenAPI tests | Read the added/changed proof bodies, not their completion-summary conclusions |
| Setup, gateway, troubleshoot, MCP docs and shipped gateway JSON | Saved-only consumption, recovery, atomicity claims and existing two-step documentation |
| `mise.toml`, package inventory script | Local-dev lane includes new proof; README inventory addition corresponds to the package README already present before base |

The new `DevInputs` is retained effective configuration for the existing `DevServer`, not a second bootstrap engine. `AuthoringApply` adds the required local-only source grammar to the existing `DeploymentFile` owner; resolution ends in the existing wire type. Atomic apply extends `GatewayAdministration` and extracts its existing per-resource validation/sealing/reference bodies into transaction-composable methods. These are sufficient reuse boundaries; no alternative store, issuance owner, transport, or encryption owner is needed. The existing host offer already owns host mutation, but its independent prompt decision is not correctly composed with the new CLI interactive policy (INV-R1).

## Acceptance matrix

`PASS` here means source inspection supports the obligation with the cited proof body; it does not claim this reviewer reran the test. `FAIL` identifies a concrete gap or an unavailable required immutable input.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001/002: tenant optional at parse time; one resolution/execution path; complete unattended invocation never prompts | `DevArgs` and `resolve` converge into `DevInputs`; host call at `dev.rs:527` loses `interactive` policy | Guided PTY proof and missing-input proof exist, but flag-on-terminal is absent; INV-R1 | FAIL |
| REQ-001: argument/environment/saved precedence, required missing inputs collected before install/migrate/provision, loopback and file/skip conflicts | `resolve`, `first`, clap conflict annotation, `loopback` | `server_dev_saved_settings_survive_working_directory_change`, guided unattended initial case | PASS |
| REQ-002 / AC-001: protected DSNs, separate platform DSN, ordinary production role checks | SecretString database inputs; `GlobalConfig::{local_databases,save_local_dev}`; ordinary installed server children | Protected-DSN round-trip and installed first-run proof | PASS |
| REQ-002 / AC-006: stable absolute storage/Bifrost defaults; existing paths and `.wyrd` not moved/erased/adopted | `local_data`, server storage `BackendConfig::from_url` rejects relative file roots | Working-directory restart proof and first-run saved-path assertions | PASS |
| REQ-002 / REQ-008: unrelated saved deployment/tenant refused before mutation, with separate-config guidance | Guard at `dev.rs:295` only handles an existing `[dev].tenant`; late credential identity check follows provisioning | Existing preservation tests do not seed a non-dev remote saved client/key; INV-R2 | FAIL |
| REQ-003 / AC-001/002: existing verified installer, migrate before serving, readiness, existing tenant provisioning | `DevArgs::run`, `DevServer::{start,migrate,provision,ready}` | Installed first-use and existing failure/interrupt proof | PASS |
| REQ-003 / REQ-008 / AC-006: exact tenant key reused, no automatic reissue, default sealing key retained | `save_tenant`, existing `GlobalConfig::save_local`, local default sealing-key owner | Repeat/partial failure, retained managed-secret key proof | PASS |
| REQ-003 / REQ-008 / AC-006: effective sealing identity survives a bare restart with no repeated exports | Supplied sealing-key env returns `None`; no persisted effective selection/key; later run generates default | Existing restart proof covers only generated local key; INV-R3 | FAIL |
| REQ-003 / INV-004: owned serving child settled on interruption/stage failure/exit, no external server adopted | Listener reservation, `kill_on_drop`, top-level interruption select, `stop`, `exited` | Retained interrupt/exit/partial-stage cases | PASS |
| REQ-004 / AC-003: optional known-provider offer, approved subset only, Gemini precedence, name changes, no-key/decline allowed, no upstream inference | `PROVIDERS`, `offer_gateway`, `configure_gateway`, `configured`/skip handling | Guided test selects only renamed Gemini from GOOGLE over GEMINI; explicit-file installed test; retained no-key startup | PASS |
| REQ-005 / AC-003/005: literal local-env references become managed secrets in memory, invalid names/missing/blank/duplicates/unused entries refused, no expanded files | `AuthoringApply::resolve`, `is_variable_name`, `ProviderDeploymentsApply::validate` | `deployment_document_applies_local_environment`, contract test, shipped document test | PASS |
| REQ-005 / INV-005 / AC-005: existing single/list/two-step and server environment/external/managed sources retained | Existing command branches and `AuthoringSource::Wire`; shared service methods preserve old bodies | Retained single/list and credential-source regressions; new installed test reapplies list and single | PASS |
| REQ-006 / AC-004: one scoped operation, all named writes in one transaction, redacted response ordered by request | `GatewayAdministration::apply_deployments`, `audited`, `write_*` and `conn.commit` | `gateway_apply_is_atomic_and_tenant_scoped`, CLI rollback and MCP rollback cases | PASS |
| REQ-006 / INV-003: tenant-derived authority, gateway-write permission, existing FK/provider/revocation/assignment rules and reference locking | Authenticated Caller → permission/audit → `tenant_conn`; SQL `FOR SHARE`, composite FK, active-only upsert; existing revoke/delete share authority | Server denial/foreign/revoke-wait proof and MCP deny/discovery tests | PASS |
| REQ-006: transport error cannot establish noncommit; safe reapply guidance | Client docs recognize ambiguity, but dev stderr and troubleshooting claims erase it | No post-commit lost-response proof; INV-R4 | FAIL |
| REQ-006 / AC-008/010: scoped runtime MCP + served OpenAPI; mutation stays on restricted handle rather than first-class Gateway SDK | HTTP route and MCP catalog/dispatch use typed apply/service owner; `CredentialWriter` only | MCP gateway/catalog/connectivity and served OpenAPI tests updated | PASS |
| REQ-007 / AC-008: saved-only fresh client authenticates and bounded-reads; selected gateway deployments read back; summary truthful about provider key validity | `saved_client`, `configure_gateway`, final summary | Installed first-use, Python/TypeScript/Rust consumers and gateway installed test | PASS |
| REQ-008 / AC-007: protected persistence preflight, no claimed filesystem/Postgres atomicity, missing key needs existing platform authority and explicit confirmation | Settings save before provision; `recover` uses existing `PlatformEndpoint`/`recover_tenant_admin`, interactive affirmative answer; no implicit reissue | Negative lost-key unattended cases exist; successful terminal rotation not newly exercised | PASS |
| REQ-009 / AC-009: main gateway/setup/quickstart docs and shipped local-env example use shared startup and retain two-step | Actual changed documents and example JSON | Installed shipped-document proof; docs/examples checks claimed in task | PASS |
| REQ-009 / AC-009: all three standalone app changes included in immutable task candidate | Main candidate contains a consumer test, but external repo task edits are uncommitted and no immutable pairs supplied | P12 is claimed locally, but reads mutable siblings and skips when absent | FAIL |
| INV-001: single durable owners; pure contracts remain IO/async/PyO3-free | Shared service refactor and pure DTO validation; restricted transport retained | Source and dependency checks claimed | PASS |
| INV-002: DSNs/API keys protected; local provider secrets never in document expansion/output/redacted result | Secret-bearing inputs use secret types; protected config owner; local env resolution not persisted | Secret sentinels in installed/server/CLI/MCP proof bodies | PASS |
| INV-003: no loopback/slug privilege, platform/tenant plane retained | Existing server setup and platform recovery APIs; no new auth bypass | Source, server foreign/denial tests | PASS |
| INV-004: no reset/deletion/availability widening; retain truthful persistence/recovery boundaries | Ordinary lifecycle preserved, except unrelated configuration and sealing/lost-response cases | INV-R2/R3/R4 | FAIL |
| INV-005: optional gateway and existing APIs/sources preserved | Existing source branches and old command/service behavior retained | Existing regression cases remain; new paths compose same owners | PASS |
| INV-006 / AC-010: supported platform/runtime proof, no new OS-specific process fixture assumptions | New terminal path is Unix and covers both supported OSes; Python lifetimes remain Python; normal process supervision retained | No four-target execution evidence rerun here; source review only | PASS |
| Non-goals: no PostgreSQL installer, UI wizard, daemon, login framework, new vault/store, arbitrary discovery, live provider calls, auto rotation/reset or implicit app deployment | Complete cumulative implementation contains none of these additions | Source inspection; no new services/dependencies/migrations | PASS |

## Proposed findings

### INV-R1 — INCORRECT — `--non-interactive` does not govern the MCP host offer

**Obligation:** REQ-001 and AC-002: with the flag, no prompt is attempted, even when stdin is a usable terminal.

**Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:527` (call at 529), with consumer `src/mcp/mod.rs:224–242`.

**Reachable producer-to-consumer path:** Fresh `wyrd server dev --non-interactive --tenant local` with a complete database env and stdin attached to a PTY computes `DevInputs.interactive = false` at line 241. Provider/recovery questions respect it. After settings/key are saved and saved-client authentication succeeds, `!inputs.configured` is true and unconditionally calls `offer_local`. That function sees stdin is a terminal plus an existing supported host directory and enters `prompt_local_selection`. The resolved false interactive value never reaches that sibling prompt consumer.

**Consequence:** A complete unattended invocation blocks waiting for host input after provisioning, contradicting its promised convergence and no-prompt contract.

**Smallest correction:** At the existing dev orchestration call, offer hosts automatically only when `inputs.interactive` permits it; explicitly selected `--host` values must still install without questions. Keep standalone `mcp install` behavior and configured-restart suppression unchanged. Reuse existing `offer_local`/installer; no new prompt framework.

**Focused proof:** Extend the existing PTY installed-CLI proof with complete inputs, a detected host, and `--non-interactive`; send no answer and require readiness without a host prompt or implicit host edit. Preserve the explicit `--host` journey.

### INV-R2 — INCORRECT — unrelated saved API-key deployments are rejected only after new provisioning

**Obligation:** REQ-002 requires unrelated saved deployment/tenant refusal and preserved settings; REQ-008 requires preflight before issuing one-time credentials and no silent identity adoption.

**Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:295–302` and lifecycle ordering at 508–511; `GlobalConfig::save_local_dev` at `global_config.rs:175–198` is the early writer.

**Reachable path:** A normal saved client may have `[client].http_url = "https://another-deployment.example"` and a protected `[default].api_key`, without `[dev].tenant` (existing `GlobalConfig::save_local`/ordinary client persistence supports that state). Running fresh local dev with a new database and tenant does not trigger line 295 because `saved_tenant` is absent. `validate_saved_login` checks human saved-login entries for the new origin, not the existing default key/client deployment. The process migrates/starts, writes new `[dev]` settings and protected DSNs, provisions the new tenant, then `ProvisionedTenant::credential` finally rejects the remote tenant key by UUID prefix. The new one-time key is dropped on return. `GlobalConfig::save_local` correctly refuses replacing a different default key, but it is downstream of the point that needed refusal.

**Sibling authorities checked:** Default-key persistence, saved human login selection, configured endpoint calculation, `save_settings`, provisioning, `save_tenant`, `ProvisionedTenant::credential` and `GlobalConfig::save_local`. The latter checks remain valuable trust/identity guards and should not be weakened to force startup through.

**Consequence:** An invocation the spec says to refuse first mutates a different database and local development settings and issues an unusable one-time key. The existing saved key is retained, but the newly provisioned tenant now requires recovery, and bare reruns select new `[dev]` state inconsistent with that key.

**Smallest correction:** Extend the existing `DevArgs` preflight, using already loaded saved client/dev/key/login state, to refuse an unrelated pre-existing default-key deployment with `WYRD_CONFIG_HOME` guidance before data creation/install/migration/settings/provisioning. Preserve legitimate explicit listener overrides and partial local saves; keep `save_local`'s late safety check. Do not authenticate by guessing a tenant from a slug and do not add a profile/store/issuance mechanism.

**Focused proof:** Seed a remote saved endpoint/default key with no `[dev]`, invoke local dev against an empty managed database, and assert failure with separation guidance, unchanged config/credentials, no new local development data/settings, and no tenant issuance. Existing partial-save/restart cases must continue to pass.

### INV-R3 — MISSING — supplied sealing-key identity is not retained for bare restart

**Obligation:** REQ-003 preserves the local sealing key; REQ-008 and AC-006 explicitly require bare restart without repeated exports and continued provider decryption.

**Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:149`, `local_sealing_key` at 941–949, and `DevSection`/`save_settings` effective-setting persistence.

**Reachable path:** On first setup, a supported `WYRD_SEALING_KEY_FILE` or `WYRD_SEALING_KEY_BASE64` is present. `local_sealing_key` returns `None`, so the child uses the inherited configured key. A managed provider credential is successfully sealed under that key. Neither protected dev database persistence nor the non-secret `DevSection` records the effective sealing selection/key. Stop the process, remove the export and run bare dev: `local_sealing_key` now generates an unrelated default `server/sealing.key` (or selects a previously existing different default), then supplies it to all children. Server boot builds that keyring and runs existing `rewrap_sealed_secrets`, which cannot open the prior envelope; the deployment cannot recover its provider configuration from saved state.

**Sibling consumers checked:** Server `config::load_sealing_key` uses either file or inline env and refuses both; boot keyring/rewrap and gateway seal/unseal use those actual bytes. The retained managed-secret restart test exercises only the generated default file, not either supplied source.

**Consequence:** A valid first setup cannot resume as promised without re-exporting a sealing input; bare restart changes cryptographic identity and fails boot/decryption despite retained tenant/database/data settings.

**Smallest correction:** Extend the existing local protected persistence/sealing-key owner to retain the effective local sealing identity supplied on successful setup and restore that exact identity on later no-export runs. Preserve explicit override precedence, owner-only validation, and existing envelopes; never silently generate a replacement for a previously selected key. No new key store/encryption system is needed. The precise representation remains private but must not place key material in general configuration.

**Focused proof:** Extend the existing installed managed-secret restart test for the supported supplied file and inline sources: configure a deployment, stop, remove sealing exports, restart from another directory using saved inputs only and invoke against the local provider fixture. Confirm no implicit reapply/rotation, unchanged tenant key and no secret disclosure.

### INV-R4 — INCORRECT — gateway error diagnostics falsely promise that no commit happened

**Obligation:** REQ-006 explicitly says a lost response may follow successful commit, reapply is safe, and a failure must not be represented as proof of noncommit; INV-004 requires honest persistence.

**Location:** `crates/wyrd/wyrd-cli/src/server/dev.rs:1059–1063`; the same unconditional guidance appears in `docs/src/content/docs/get-started/setup.md` and `docs/src/content/docs/operate/troubleshoot-a-deployment.md` gateway failure rows.

**Reachable producer-to-consumer path:** `GatewayAdministration::apply_deployments` commits before constructing/delivering its response. A connection failure after that commit makes `CredentialWriter::send`/`apply_deployments_with` return an error to `configure_gateway`. Its catch-all stderr says "nothing was applied" regardless of whether the error came from validation/authorization, precommit failure, or loss after successful commit. The client operation's own rustdoc already acknowledges the latter ambiguity. No transaction or receipt evidence is available to the CLI at that point.

**Consequence:** Users/agents are told the gateway configuration is unchanged even though named credentials and deployments may already have been replaced. This is a required recovery contract, not wording preference.

**Smallest correction:** Remove the unconditional noncommit assertion from the existing CLI and matching recovery docs. Explain that the outcome may be unknown and that inspecting/reapplying the same named document safely converges; preserve tenant-state guidance and owned-child cleanup. Definite server validation refusals can retain their precise no-commit description. No receipt, status job, retry system or new store is necessary.

**Focused proof:** Use an existing local transport fixture/proxy to drop an apply response after the service commits. Require a failed CLI result and child cleanup, committed redacted resources readable on restart, no false "nothing was applied" claim, and successful safe reapply. A narrow output assertion plus shared transport/service proof can supplement the existing installed gateway test; do not invent a general failure harness.

## Review boundary and verification limits

The original task includes mutable sibling repositories with unrelated edits. No immutable base/candidate pair identifies which external changes belong to this task. P12's source reads sibling working trees and skips when absent; it cannot identify an immutable reviewed standalone candidate. This blocks the complete task audit until those task-owned changes have an immutable subject (and a defined separation from unrelated external edits). It is a review-input blocker, not an implementation finding or authorization to commit those repositories.

The task records P1–P12 and checks as passing but supplies no captured run logs. Their bodies were inspected; this reviewer did not repeat commands, run a new managed PostgreSQL instance, or execute four-target proof. Test gaps named above are tied to reachable required behavior, not a demand for broad new suites. No unrelated debt or style-only refactor is proposed. No prior findings/remediations exist to close.
