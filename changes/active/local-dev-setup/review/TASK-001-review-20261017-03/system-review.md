# System resilience review

Result: **PASS**. Proposed material finding ledger: **empty**.

## Subject and authority

Reviewed the cumulative worktree change from `bc3bf25e3f78b0c429110350a3de4d58290e632f`
through immutable candidate `6542f50252d12ba1fb80dda24e7ed2dd8b90111a`.
HEAD still matches that candidate. The `38dbc023c...6542f5025` range located
the latest corrections; it did not replace the cumulative review. Scope is
this worktree; sibling repositories are excluded. No current-round peer
report was read.

Applied approved spec revision 1, original TASK-001, prior validated ledgers,
verdicts and R1/R2; AGENTS, agent rules, reference router, spec-driven
development and maintainer guidance; governing client/runtime identity,
deployment configuration/migration, security sealing/audit, testing and
Bifrost durability/failure boundaries. No CodeGraph directory exists.
Inspected changed runtime owners, their existing callers and sibling writers,
and focused proof bodies. No source change, Cargo/build/test/lint/codegen run,
deployment or sibling-repository operation was performed. Only this report
was written. Recorded implementation results are execution evidence supplied
with the subject, not independent executions by this reviewer.

## Deployed topology and failure/recovery coverage

Paths below are relative to the repository. `dev.rs` means
`crates/wyrd/wyrd-cli/src/server/dev.rs`; gateway service means
`crates/wyrd/wyrd-server/src/components/gateway/service.rs`.

| Deployed path / affected capability | Source evidence and credible failure | Recovery and assessment |
|---|---|---|
| Native and language CLI projections → verified installed CLI/server bundle | `DevArgs::run`, existing `ServerInstaller::{official,installed,install}`, `DevServer::new/command`. No second startup engine. Every ordinary migrate/setup/serve child uses the same resolved environment; `kill_on_drop(true)` covers cancelled child-command futures. | Invalid or incomplete required input fails before install/migrate/provision. Download cancellation drops the existing installer future; staged installation is not selected as runnable. Compatible installed release remains reusable. No automatic restart loop amplifies an outage. PASS. |
| Initial concealed input → cancellation; later gateway/recovery/host prompts | `dev.rs:137-178`; `terminal.rs::Terminal::{open,ask,conceal}` and `EchoGuard::drop`; gateway/host `select!` in `DevServer::run`. The biased interruption future registers signal observers before resolution can disable ECHO. | INT/TERM drops the pending concealed future and restores terminal attributes; after startup it settles the owned child. Server exit cancels an unanswered gateway/host question. Unattended policy reaches `mcp::offer_local`; explicit hosts still work. PASS. |
| PostgreSQL migration → serve → readiness → setup | `DevServer::{start,migrate,ready,provision}`. HTTP reservation refuses a currently occupied listener before migration. Migration must succeed before serving; readiness watches child exit with two-second HTTP requests/two-minute total budget. | DB/role/security/migration/readiness refusal is a startup failure and does not claim ready. Committed migrations remain for the normal migration owner on rerun. Cancellation owns the reservation and serving child; no external already-running server is adopted or stopped. PASS. |
| Ordinary server → hosted Monty workers and Bifrost lifecycle | Existing `boot/mod.rs::attach_verification_blocks`, `verification/hosted/blocks.rs::HostedBlocks`, `sandbox.rs::Sandbox`, `app/supervise.rs`. Hosted capacity stays process-owned; checkout drop kills an operation's worker. Server supervision retains readiness removal, cancellation and drain. | This change neither replaces worker-failure classification nor adds component errors that deliberately crash a shared external service. Local setup boot errors fail that owned startup; request-side gateway failures stay request-scoped. Existing Bifrost WAL/stage/publication recovery remains unchanged. PASS for the changed boundary. |
| Saved identity → selected ready server → local selection write → server provisioning | `DevArgs::resolve:307-352`; `DevServer::run:548-562`; `saved_key_config/proven`; shared `ClientConfig::from_environment/resolve_credential`. No-dev retained key and remote selection are refused early. Saved-dev resume uses explicit exact protected key, empty fixed environment/default config, no tenant selector, default in-memory token cache. | A saved login, ambient bearer/API key or disk cache cannot mask an invalid retained key. Exact-key refusal occurs before `save_settings` and `provision`; old files remain, selected child settles, no new tenant-admin key is issued. Normal post-setup `saved_client` still uses ordinary login precedence. PASS. |
| Protected files ↔ PostgreSQL one-time issuance | `GlobalConfig::{save_local_dev,save_local}` and existing `CredentialsFile` lock/atomic replacement; `DevServer::{save_settings,save_tenant,recover}`. Settings save precedes provisioning; key save precedes endpoints. | There is deliberately no filesystem/Postgres atomicity. DSNs can survive a later general-config failure, before issuance. After key save/endpoint failure, `[dev].tenant` already exists, exact key can prove the selected server, and rerun can finish endpoint save without issuing a replacement. Active tenant with no key needs existing platform recovery; no implicit reissue. Final conflicting-key guard remains. PASS. |
| Saved deployment with listener override → automatic setup offers | `configured = key.is_some() && global.client.http_url.is_some()` in `DevArgs::resolve`; shared consumers are `offer_gateway` and the `mcp::offer_local` gate. Server exact-key proof must succeed before either consumer is reached. | Changing only HTTP bind resumes the saved deployment without provider/host questions. No saved endpoint means partial setup remains unconfigured; first-use/partial offers and explicit gateway-file/host requests remain enabled. No new completion record is introduced. PASS. |
| Stable local storage and provider sealing across restarts | `local_data`, saved absolute storage/Bifrost fields, ordinary storage settings validation; `local_sealing_key/supplied_sealing_key`. File/inline supplied key is validated then retained owner-only with staging and no-clobber persistence. Every child uses retained key file. | Working-directory changes do not select new default data. A different supplied sealing key refuses before migration/setup, retaining ciphertext authority. Interrupted first key creation cannot replace an existing valid key with partial bytes. Existing server keyring/rewrap and provider resolver stay owners; merely booting would not prove decryption, so actual provider proof was inspected. PASS. |
| Local authoring document → restricted credential writer → HTTP/scoped MCP → gateway service | CLI `DeploymentFile::read/AuthoringApply::resolve/apply_deployments_with`; shared `CredentialWriter::apply_deployments`; route and MCP handler → same `GatewayAdministration::apply_deployments`. | Local references resolve and validate before network mutation; no expanded file or new server environment interpretation. Authz stages its audit decision nonblockingly before tenant work. Provider/assignment/sealing/SQL errors drop the one tenant transaction. Existing individual puts reuse the same private write helpers; source and reference checks survive extraction. PASS. |
| Composite apply ↔ existing puts/revoke/delete/readers | Service `write_credential/write_deployment`; `wyrd-sql/src/queries/gateway.rs` active upsert, `FOR SHARE`, revoke/delete; migration composite tenant/name/provider FK with RESTRICT. `TenantConn` owns SQLx transaction drop rollback. | Cancellation/precommit failure rolls back named writes. Reference locks serialize revoke/delete; FK prevents orphaning provider changes. Readers cannot see an intermediate committed credential/deployment combination. Competing request failure does not stop server or unrelated resource families. PASS. |
| Commit → lost answer / ambiguous acknowledgement → retry | Gateway service commits before returning; HTTP transport response read follows commit. `configure_gateway` and collection-route 503 text explicitly preserve uncertainty. | Setup failure settles its owned foreground child while retaining prior tenant setup. Changes may have committed; inspect or safely reapply named document after restart. No compensation, automatic secret rotation, receipt store or false noncommit guarantee is used by this workflow. PASS. |
| Foreground unexpected exit / stop | `DevServer::{exited,stop}` and outer `DevArgs::run`. TERM then reap; 45-second deadline exceeds ordinary server drain budget; kill/reap if exceeded. | Unsolicited child exit fails command; controlled interruption settles child. Persisted files/committed server state survive and rerun resumes. No successful readiness claim follows unfinished gateway/host stage. PASS. |

## Prior findings: current source and proof bodies

| Stable ID | Current closure evidence | Assessment |
|---|---|---|
| FIND-TASK-001-1 | `mcp::offer_local(hosts, endpoint, inputs.interactive)` checks resolved policy. Installed PTY `unattended_terminal_offers_no_hosts` compares detected host bytes and then proves explicit host installation. | Closed. |
| FIND-TASK-001-2 | No-dev key refusal in resolution; exact-key preflight before writes/issuance. Current `pg_tests::server_dev_repeat_and_partial_failure_preserve_state` seeds same-origin key without `[dev]`, tries another tenant with/without real recovered platform authority, asserts byte-preserved files/listener availability and absent requested slug using operator-session `tenant_slugs`. It also saves the other database's real same-origin/same-slug login and demonstrates exact-key rejection with preserved files and settled listener. Existing changed-database, remote-selection, lost-key and A-resume branches remain. | Closed. This directly addresses the login substitution missed by the prior system pass. |
| FIND-TASK-001-3 | Retained supplied sealing identity is passed to all children. `server_dev_keeps_the_gateway_managed_secret_key` covers generated/file/inline source, clean saved-input restart from another directory, local upstream invocation before/after restart, unchanged key and conflicting-key refusal. | Closed; proof establishes decryption, not just boot. |
| FIND-TASK-001-4 | `configure_gateway` reports possible commit and safe named reapply. Python `LosingProxy` reads the complete successful upstream response before dropping it; installed gateway journey asserts unsuccessful setup/uncertainty, released listener, durable resource on restart and convergent reapply. | Closed. |
| FIND-TASK-001-5 | Signal registration now surrounds input resolution. PTY `interrupt_hidden_question` waits for hidden prompt/ECHO-off, types unsent input, sends INT and TERM, asserts 130, ECHO restored, no disclosure and absent config/credential/sealing files. | Closed. |
| FIND-TASK-001-6 | Real Postgres `gateway_apply_is_atomic_and_tenant_scoped:1667-1733` rotates an existing managed secret, then changes an already-referenced provider and reaches SQL FK refusal. It compares original row/envelope/metadata, referenced credential and deployments and opens retained ciphertext to the original secret. Later lock-waiter/revoke branch retains concurrency proof. | Closed. |
| FIND-TASK-001-7 | Current cumulative CLI helpers/journey/constants, server apply helper/constants and MCP proof constants have role/panic contracts; latest catalog test `gateway_catalog_annotates_every_administration_operation:633` now states concrete missing/mismatched catalog/schema/annotation panic conditions. No runtime assertion is removed. | Closed for prior required set. |
| FIND-TASK-001-8 | Shared CLI `free_bind` uses standard `TcpListener::bind("127.0.0.1:0")`/`local_addr`, no procfs read or OS-specific exclusion. Existing first-use/partial/restart journeys share it. | Closed in source; macOS execution not supplied. |
| FIND-TASK-001-9 | One corrected `configured` producer feeds gateway and host consumers; no endpoint equality remains. Installed `restart_on_another_listener` runs interactive PTY with provider variables/detected host present, waits for full ready summary without answering questions, reads gateway through fresh CLI at saved new endpoint, compares tenant key, checks saved binds and released new listener. | Closed. |

R1/R2 contain exact focused Linux execution commands/results for these bodies.
The current saved-settings journey separately retains card data/key across
working-directory change and updates usable endpoint for listener override.
First-use proof checks saved HTTP/gRPC behavior and a real Bifrost write/read;
gateway journey drives Rust/Python/TypeScript consumers through local fixtures;
MCP and served OpenAPI proof cover the shared operation's published surfaces.

The key-saved/endpoint-missing partial transition is established here from the
actual save ordering and resume guards. The inspected focused journey does not
inject a failure at that exact filesystem boundary; no additional execution of
that branch is claimed. This is a proof limit, not a demonstrated recovery
defect or a new requirement to build fault-injection machinery.

## Decision and limits

REQ-001/002/003/007/008 and INV-004 pass for changed input, startup, identity,
persistence, cancellation and recovery. REQ-004/005/006 pass for optional
gateway, source resolution, transactional refusal, concurrent reference safety
and uncertain completion. Current source closes all nine prior findings within
this role's evidence boundary. No new source-local correction is warranted.

Skipped: independent executable reruns, macOS/arm64 execution, forced-kill and
broad external-dependency outage qualification. These are explicit evidence
limits; unchanged server/worker/Bifrost internals were inspected to establish
lifecycle ownership, not requalified or redesigned.
