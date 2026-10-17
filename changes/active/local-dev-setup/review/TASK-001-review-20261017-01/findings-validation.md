# Independent findings validation

**Validation status: COMPLETE.** All eight proposal groups are resolved from source. The immutable main-repository candidate has bounded retained findings. **Whole-task acceptance remains BLOCKED** because the required standalone examples have no immutable commit pairs. That input limitation is not a FIND and does not invalidate the bounded source diagnoses below.

## Subject, authority and method

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
- Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`; HEAD matched during validation.
- Approved authority: `changes/active/local-dev-setup/spec.md`, revision 1; original task `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
- Inputs: subject, claim-comparison, both implementation reviews, standards, maintainer, system, security, data and focused follow-up reports. No prior FIND ledger/remediation exists. Review uses the cumulative subject rather than a latest-fix range.

Applied AGENTS §§2, 9, 11, 16, agent rules, spec-driven-development, maintainer style and the approved task/spec. Inspected actual producer/consumer bodies and sibling owners for each proposal. No `.codegraph` exists. This is independent source validation, not a vote across discovery reports. No Cargo, tests, builds, lint, codegen, provider call or implementation edit was performed. Verification claims in the task are not captured execution logs. Only this assigned report was written.

## Proposal disposition

| Discovery claims | Disposition | Final ID |
|---|---|---|
| BEH-001, INV-R1 | CONFIRMED | FIND-TASK-001-1 |
| BEH-002, INV-R2, SEC-001 | REVISED: new key passes its own tenant-prefix check, then protected save rejects replacement; include remote/no-dev and changed-database states in one boundary | FIND-TASK-001-2 |
| INV-R3 | REVISED: lost sealing identity breaks provider decryption; boot need not fail because rewrap errors with a configured key are logged and consumed | FIND-TASK-001-3 |
| BEH-003, INV-R4, MNT-002, SYS-001 | CONFIRMED, consolidated CLI/recovery/collection-route claim | FIND-TASK-001-4 |
| SYS-002 | CONFIRMED | FIND-TASK-001-5 |
| DATA-001 | CONFIRMED as required proof gap, not demonstrated transaction defect | FIND-TASK-001-6 |
| REPO-001, MNT-001 | CONFIRMED, one bounded rustdoc correction | FIND-TASK-001-7 |
| FUP-001; conflicting INV-006 assessments | CONFIRMED as required journey-proof violation, not production OS regression | FIND-TASK-001-8 |

No proposal remains unresolved. No rejected finding is carried as optional advice. No new product, API, architecture, concurrency or persistent-data decision is needed for these bounded corrections.

## Final deduplicated ledger

### FIND-TASK-001-1 — CONFIRMED — VIOLATION: unattended policy is dropped before the host offer

Sources: BEH-001, INV-R1. Obligation: REQ-001 and AC-002 prohibit every prompt under `--non-interactive`, even on a terminal.

Location: `crates/wyrd/wyrd-cli/src/server/dev.rs:527`; consumer `src/mcp/mod.rs:224-245`.

Evidence: `DevArgs::resolve` computes `interactive = ... && !self.non_interactive && stdin.is_terminal()`. `DevServer::run` respects this through gateway selection but invokes `mcp::offer_local` for every fresh deployment. `offer_local` checks stdin and detected hosts; with no explicit hosts and a detected host it awaits `prompt_local_selection`. Thus a complete unattended invocation on a tty reaches a question after provisioning instead of ready. Explicit hosts use the existing installer without prompting. The independent `mcp install` command intentionally has its own selection behavior.

Smallest correction: let the existing dev orchestration invoke an implicit host offer only when resolved interactive policy permits it; retain explicit host installation in unattended runs and configured restarts. The guard belongs at this caller because it owns `--non-interactive`; do not change standalone installer semantics or add another prompt/install owner.

Focused closure: use the existing installed PTY journey with all required inputs, `--non-interactive`, omitted hosts and a detected host config. Assert ready without question or host-file mutation. Preserve explicit `--host` installation. Piped-stdio proof alone cannot detect this failure.

### FIND-TASK-001-2 — REVISED — INCORRECT: deployment conflict is checked after selection mutation and one-time issuance

Sources: BEH-002, INV-R2, SEC-001. Obligations: REQ-002 unrelated-deployment refusal/preservation; REQ-008 deployment identity, preflight and safe one-time credential retention; INV-004.

Locations: `dev.rs:295-302,507-511,551-565,577-600,881`; existing protected conflict owner `crates/shared/wyrd-client/src/global_config.rs:232-249`.

Evidence and exact diagnosis: resolution loads the protected key, but only compares a saved dev slug when one exists and a saved login for the newly requested origin. A remote selected client endpoint with no dev section is not refused; a changed database with the same local slug also passes. `run` starts/migrates the selected database, `save_settings` replaces DSNs/dev fields, and `provision` calls server `main.rs::setup`. An empty database creates a new active tenant and one-time key. `ProvisionedTenant::credential` prefers this NEW key and correctly accepts its own returned tenant UUID. The later `GlobalConfig::save_local` refuses replacement of the unrelated OLD default key. The newly issued key is dropped; selected DSNs/dev state have changed. A bare rerun now uses the wrong key/database pair. In the remote/no-default-key variant, endpoints can be replaced instead of the required unrelated-deployment refusal.

Sibling/correction boundary: `save_local` owns final no-replacement/race protection and must remain. `save_local_dev` is the only dev settings writer. Server setup owns no-reissue of active tenants; `recover` and platform recovery own explicitly authorized rotation. Fresh saved-client authentication already exists in `saved_client` and the shared `WyrdClient` transport; reuse those mechanisms for preflight rather than adding profiles or another identity resolver.

Decision-complete correction: at input resolution, refuse positively unrelated saved client selection (including an existing remote selection without a dev deployment) before writes/startup, with config-home separation guidance. For a saved local deployment/key whose explicit database selection cannot be identified locally, verify the retained credential against the selected ready server through the shared client before replacing local dev/DSN settings or invoking new tenant creation. Bind the check to the expected saved tenant, not merely successful authentication to any tenant. A rejected credential must not cause provisioning of another deployment or mutation of the old selection. Preserve existing recovery guidance and explicitly authorized recovery; absence/revocation of a key for the same saved deployment must not become an automatic reissue or silently acquire another deployment's identity. Preserve first-use protected persistence before issuance, missing endpoints after a partial key save, corresponding listener/DSN/password overrides for the same deployment, and final conflict protection. Raw DSN/URL equality is not a deployment identity rule. Do not reject all overrides or invent a new credential/profile class.

Focused closure: existing CLI/Postgres harness, two fresh managed databases and one config home: configure A, select B with the same slug, assert refusal without overwriting A's files/key or issuing B a tenant-admin key, then resume A. Also seed an unrelated remote endpoint with and without a default key and no dev section: assert early refusal and byte-preserved files. Retain same-database listener/connection override, partially saved configuration and existing explicit recovery tests. This is one producer-boundary correction, not repeated downstream guards.

### FIND-TASK-001-3 — REVISED — MISSING: supplied effective sealing identity is absent from saved restart inputs

Source: INV-R3. Obligations: REQ-003 preserves sealing identity; REQ-008/AC-006 require bare saved-input restart and usable provider credentials.

Locations: `dev.rs:149,941-949`; `DevServer::command:483-485` and `save_settings`; `DevSection` has no effective sealing selection.

Evidence: `local_sealing_key` returns `None` when either supported sealing variable exists. Child commands inherit that supplied key, and gateway `write_credential` seals managed values with it. Neither that selection nor its effective bytes is saved. With exports removed, the next run creates/selects the unrelated default `server/sealing.key`. `config.rs::load_sealing_key` accepts the new default; boot's `rewrap_sealed_secrets` at `boot/mod.rs:1418` logs a rewrap error and returns success when a configured key cannot open old ciphertext. Therefore a boot failure is not required to establish the defect. `wyrd-gateway/src/credential.rs::CredentialResolver::resolve` later opens `ManagedSecretBinding` under the wrong keyring and fails the provider attempt. Saved tenant/readiness can succeed while gateway usability is lost.

Existing/sibling owners: local generated key file, server secret-file validation, configured-key conflict rules, server keyring/rewrap and gateway managed binding remain authoritative. Existing generated-key restart proof does not exercise either supplied source.

Smallest correction: retain the successfully selected supplied effective key through the existing protected local sealing-key persistence, then reuse exactly that identity for a no-export restart. Use the existing local key-file boundary for the saved key rather than a new store; capture the supplied effective key only after its existing configuration/owner-only validation succeeds. Preserve explicit source precedence and conflict errors, validate supplied files through existing secret-file facilities, keep key material out of general config/output, and do not overwrite a valid retained key because of an invalid/failed selection. Preserve intentional explicit override behavior and adjacent retained-key/rewrap semantics. Never manufacture a replacement default for a previously saved effective selection.

Focused closure: extend the installed managed-provider restart journey for supplied file and inline forms. Seal/configure, stop, remove sealing exports, restart from another directory with saved settings, invoke via the local provider fixture, and assert unchanged tenant key/no automatic reapply and no disclosure. This validates decryption, not merely server boot/readiness.

### FIND-TASK-001-4 — CONFIRMED — INCORRECT: generic apply failure asserts noncommit

Sources: BEH-003, INV-R4, MNT-002, SYS-001. Obligations: REQ-006 explicitly recognizes response loss after commit and prohibits failure-as-noncommit; REQ-007/INV-004 require honest recovery.

Locations: `dev.rs:1059-1063`; setup.md gateway failure row at 161; troubleshoot-a-deployment.md gateway row at 34; collection route 503 description `components/gateway/routes.rs:552`. Review operation rustdoc alongside these statements.

Evidence: `configure_gateway` → `apply_deployments_with` → restricted `CredentialWriter::apply_deployments/send` → shared `HttpTransport::request_json_with_headers`. The server `GatewayAdministration::apply_deployments` commits at `service.rs:386` before returning its answer. Shared HTTP subsequently reads/decodes the response at `transport/http.rs:407-420`; failure reaches the same unconditional “nothing was applied” line. A commit acknowledgement/storage disconnect can also be uncertain; categorical 503 noncommit wording is too strong. Standalone deployment put propagates the shared error without this extra assertion. HTTP and MCP route through the same transaction owner.

Smallest correction: remove categorical noncommit from generic CLI/recovery and availability descriptions. State that setup did not complete, tenant setup remains, gateway changes may have committed, and inspection or safe reapply of the same named document is appropriate. Keep known validation/conflict refusals' precommit guarantees and preserve returned error, owned-child settlement, named upserts and unrelated resources. No receipt, compensation, retry framework, new status operation or error hierarchy is justified.

Focused closure: existing local HTTP/installed journey fixture: process a real apply, lose its response after commit, assert failed setup and child settlement, honest uncertainty output, durable redacted resources on restart and safe reapply. Keep existing atomic-refusal assertions. A static wording check can supplement this focused case.

### FIND-TASK-001-5 — CONFIRMED — INCORRECT: first concealed question precedes signal supervision

Source: SYS-002. Obligations: usable concealed input REQ-001; controlled startup interruption REQ-003/AC-007; Terminal's restoration contract.

Locations: `dev.rs:131-135,248-254`; `terminal.rs:28-32,90-97`.

Evidence: native `main` and shared `run_cli_code` install no signal observer. `DevArgs::run` awaits resolution before creating/polling `interruption`. A missing DSN opens `Terminal` and disables ECHO while awaiting input. Only `EchoGuard::drop` restores attributes. SIGINT/TERM at this initial question can terminate the native process with no Rust unwinding, so RAII does not restore the tty. Later recovery/gateway/host questions are under the already-polled interruption owner; their cancellation safety does not prove this earlier path.

Smallest correction: extend the existing single signal/cancellation boundary over input resolution, ensuring it is polled before concealed input and drops pending input on interruption. Reuse `interruption`, `Terminal` and `EchoGuard`; preserve unattended missing-input refusal and later child settlement. No signal subsystem, terminal framework or process-wide unrelated behavior change is needed.

Focused closure: existing real PTY harness, missing DSN, wait for hidden prompt, send INT and TERM, assert controlled exit, restored termios ECHO, no secret disclosure and no install/migrate/provision effects. No database mutation is needed for this prompt-stage proof.

### FIND-TASK-001-6 — CONFIRMED — MISSING: SQL-aborted composite rollback is not proved

Source: DATA-001. Obligations: spec AC-004 and task Scenario 2 expressly require SQL-failure all-or-none proof after an earlier would-be write.

Location: `components/gateway/pg_administration_tests.rs:1495-1706`, especially 1552-1569; production transaction owner `service.rs:378-386`.

Evidence: full P4 body covers later revoked reference, keyless/permission/foreign refusal, success and queued revocation. These errors are application-level validation, not a database-aborted transaction after prior writes. `resource_names` observes names only and the attempted rollback additions are new names, so it also does not establish retained pre-existing credential value on a failed rotation. CLI and MCP composite negative cases use missing references. Existing individual operations do not prove multiple composite writes. This is not a claim that current TenantConn rollback is broken.

Existing mechanism/minimum correction: extend P4 using the actual existing FK at `wyrd-sql/migrations/20260924000000_gateway_administration.sql:76-78` (tenant/name/provider, ON UPDATE RESTRICT). First rotate a seeded managed credential; then attempt changing provider on another already-referenced credential, with submitted deployment references satisfying whole-document validation. The second update reaches existing SQL FK refusal. Assert earlier credential metadata/envelope and usable original value remain unchanged, referenced pair/unrelated resources unchanged, and existing conflict mapping retained. Reuse `stored_credential`, credential binding/resolution and existing SQL fixtures. No injected failure engine, production guard or transaction redesign.

Focused closure command:

`mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped)'`

### FIND-TASK-001-7 — CONFIRMED — VIOLATION: changed test items lack mandatory rustdoc

Sources: REPO-001, MNT-001. Obligation: AGENTS §16 and agent-rules explicitly include private tests/constants and panic sections; task-review expressly retains this blocking exception.

Validated locations: `tests/gateway_server_journey.rs:14` (`gateway`, changed delegation inherits panic), `:20` (`gateway_env`, tempdir/binary/process expects), `:463` (new composite journey, fixture/assertion panics), `:464-465` (VARIABLE/SECRET constants); server `pg_administration_tests.rs:1481` (`apply_document`, decoding expect), `:1496` (SUBMITTED constant); MCP `tests/bifrost/mcp/gateway.rs:97` (new APPLIED constant). All are changed/new in the cumulative range. The relevant full helper and journey bodies exhibit the cited preconditions; existing unrelated undocumented items are not included.

Smallest correction: concise accurate panic contracts on the four changed functions, substantive test-role rustdoc on the four new constants. Preserve behavior, assertions and fixtures; do not add unrelated docs or redesign tests. Batch with substantive remediation. Source review and appropriate formatting prove this documentation correction; no behavioral test is required for rustdoc alone.

### FIND-TASK-001-8 — CONFIRMED — VIOLATION: new mandatory P8 proof reaches Linux procfs

Source: FUP-001 and behavior INV-006 matrix; follow-up resolves conflicting platform assessments. Obligations: spec INV-006/AC-006; task Scenario 4/P8 and explicit prohibition on copying `/proc` assumptions into changed runtime/test code; AGENTS supported platforms and required journeys.

Changed location: CLI `tests/pg_tests.rs:701`; shared fixture source `:310-330`.

Evidence: new `server_dev_saved_settings_survive_working_directory_change` immediately calls `LocalDev::new`. Constructor calls `free_bind` for bind/grpc/peer at 94-96. That full helper unconditionally reads `/proc/sys/net/ipv4/ip_local_port_range` and expects success before selecting a port. macOS has no such file: the mandatory test panics before its restart assertions. Test registration has no platform exclusion; mise newly selects P8. Helper predates the candidate, but the newly mandated proof introduces a reachable dependency on it. No shipped runtime Linux dependency was established. Constructor siblings (first-use, partial failure, selected host, managed-secret journeys) share this one fixture cause.

Smallest correction: remove procfs selection from the existing shared fixture and use standard-library loopback binding/address discovery, already the practice in Python `local_listener` at `test_local_development.py:311-315`. Preserve distinct chosen addresses, saved listener identity, child cleanup and behavioral assertions; remove obsolete Linux-range documentation/imports. The fixture already releases sockets before startup, so this does not promise perpetual port reservation. Do not introduce OS branches, target exclusions, synthetic load, a new dependency/harness or production listener inheritance.

Focused closure: exact P8 on supported macOS/Linux in managed Postgres, retaining all assertions:

`mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'`

Run the existing exact P2/P9 selectors for the shared fixture regression and applicable format/lint checks. If macOS execution is unavailable, record that evidence limit; do not claim a four-target execution matrix.

## Completion and limits

All discovery proposals are source-resolved and deduplicated into eight retained findings. There are no prior FIND IDs to close. Corrections reuse existing owners and preserve security, recovery, partial persistence, explicit valid overrides, transaction/tenant boundaries and sibling-service availability. No rejected optional improvement has been added.

Independent validation is complete; task acceptance cannot PASS while required external example ranges are mutable/unidentified. No executable checks were run in this validation and no new claim of runtime verification is made. Review does not implement, merge, push or deploy.
