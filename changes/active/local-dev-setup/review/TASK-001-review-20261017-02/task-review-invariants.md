# TASK-001 invariant review — FAIL

Subject: base `bc3bf25e3f78b0c429110350a3de4d58290e632f` → candidate `38dbc023cd71292e1badd3abe90e87c69b76fa96`; HEAD remained that candidate. Reviewed cumulative production/test/document changes, using `5cfb0a3ac..38dbc023c` only to locate remediation. Approved spec revision 1 and original TASK-001 govern. Scope is this worktree; sibling repositories are excluded.

Authority: AGENTS, agent rules, reference router, spec-driven-development, maintainer style, testing workflows, Wyrd design/doctrine, security posture and deployment operations. No CodeGraph index exists. Independent discovery; no current-round reviewer reports consulted. Prior findings/R1 were hypotheses, not acceptance proof.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-001 / AC-001–002: resolve missing inputs, conceal DSN, unattended refusal, explicit hosts | `dev.rs::DevArgs::{run,resolve}`, `terminal.rs`, `mcp/mod.rs::offer_local` | Python guided PTY test includes concealed-question INT/TERM and tty unattended host refusal; CLI missing/conflicting-input assertions | PASS |
| REQ-002: protected DSNs, preserved files, stable absolute data | `GlobalConfig::{local_databases,save_local_dev,save_local}`, `CredentialsFile`; `local_data` uses config-relative defaults and absolute Bifrost paths | config protection unit test; first-use/cross-directory CLI source assertions | PASS except deployment identity below |
| REQ-002/008 / INV-003–004: unrelated deployment refused before replacing settings or issuing another key | `DevArgs::resolve`, `DevServer::run`, `saved_client`, shared `ClientConfig::resolve_credential` | Two-database refusal test has no competing saved login; default-key-only path now covered | FAIL: retained FIND-TASK-001-2 |
| REQ-003 / AC-006–007: ordinary installer/migrate/setup, retained foreground child, interrupted/error cleanup | `DevServer::{command,start,migrate,provision,ready,stop}`; normal installed-server owners retained | first-use, partial failure and interruption journeys; owned listener and saved-key assertions | PASS |
| REQ-003/008: generated and supplied sealing identities survive bare restart | `local_sealing_key`, `supplied_sealing_key`, `command`; existing `read_secret_file`, server keyring and managed binding consumers | managed-secret test runs generated/file/inline cases; original value resolves through mock provider after restart; conflict preserves file | PASS |
| REQ-004 / AC-003/006: gateway optional; no unsolicited repeat offers on saved restart; overrides change corresponding settings only | `configured`/`offer_gateway` producers and gateway/host consumers in `dev.rs` | existing listener-override test is unattended and cannot exercise interactive offer consumers | FAIL: INV-R9 |
| REQ-005 / AC-003/005: local references resolve once; no expanded files; individual/array and existing source forms retained | `DeploymentFile`, `AuthoringApply`, restricted `CredentialWriter`; existing individual `Gateway` methods | composite CLI journey and retained individual/array/source journeys | PASS |
| REQ-006 / AC-004: all-or-none tenant apply, safe references/revocation, redacted order, audit | `GatewayAdministration::apply_deployments` reuses `write_credential`/`write_deployment`, `audited`, tenant SQL/FK/share-lock owner | P4 includes real FK-aborted rotation rollback, original envelope opening, foreign/denied/keyless/revoked cases and waited revoke; CLI/MCP public-boundary cases | PASS |
| REQ-006/007 / INV-004: lost response does not imply noncommit | apply commits before answer; dev diagnostics/docs/503 description explicitly allow uncertainty | Python `LosingProxy` waits for upstream answer then drops it; restart inspects durable result and safely reapplies | PASS |
| REQ-006/009 / AC-008/010: scoped MCP and served HTTP use same owner; secret mutation stays off SDK facades | routes/MCP dispatch → same service; shared restricted writer, existing SDK read/invoke | runtime MCP/served OpenAPI and installed Rust/Python/TS consumer source; prior recorded checks | PASS |
| REQ-009 / AC-009: in-tree examples/docs teach installed startup and local-env document | setup/quickstart/gateway/troubleshooting docs, `examples/gateway-setup`, architecture guidance | shipped document consumed by installed gateway journey; docs/examples checks recorded | PASS within user scope |
| INV-001–006: retained ownership, secrets, production checks, supported platforms, no new recovery/store | existing server durable owners, shared protected persistence, extracted terminal reader; portable fixture | source audit; Linux evidence only, no macOS execution | PASS except the two identity/lifecycle defects above |

## Prior finding closure

| Prior ID | Source and proof assessment | Disposition |
|---|---|---|
| FIND-TASK-001-1 | `offer_local` now consumes caller-resolved interactive policy, while nonempty explicit hosts still use the existing installer. PTY proof exercises tty + `--non-interactive`, then explicit host restart. | CLOSED |
| FIND-TASK-001-2 | Remote/no-dev origin conflict now refuses before setup; saved default key alone is preflighted before local settings/provision. However `saved_client` uses a higher-priority saved human login, so it does not establish the retained default key's identity. See retained finding. | OPEN, narrower remaining path |
| FIND-TASK-001-3 | Supplied file uses `read_secret_file`; inline/file bytes are captured into the existing owner-only retained key. Children all receive that file and remove inline source. Generated/file/inline managed-secret restart proof inspects actual decryption, not merely ready. Different valid supplied bytes refuse without replacement. | CLOSED |
| FIND-TASK-001-4 | CLI, route 503 and support docs acknowledge possible commit after failed acknowledgement. Installed proxy case checks durable result, owned-child settlement, redaction and convergence. | CLOSED |
| FIND-TASK-001-5 | Biased interruption select is polled before resolve; cancelling conceal drops EchoGuard. Both INT and TERM PTY cases inspect restored ECHO and absent persistence. | CLOSED |
| FIND-TASK-001-6 | P4 now rotates an existing managed key before an existing referenced credential's provider update triggers SQL FK failure. It compares both stored rows/deployments and opens the retained envelope to the original value. Same transaction owner remains unchanged. | CLOSED |
| FIND-TASK-001-7 | Changed helpers/journey functions now have panic contracts; new VARIABLE/SECRET/SUBMITTED/ROTATED/APPLIED constants have role documentation. | CLOSED |
| FIND-TASK-001-8 | `free_bind` uses `TcpListener::bind("127.0.0.1:0")` and `local_addr`; procfs/atomic-selection implementation removed. Existing port release/start lifecycle retained. Linux runs recorded; macOS unexecuted is an evidence limit. | CLOSED |

## Proposed findings

### INV-R2 — retain FIND-TASK-001-2 — INCORRECT

**Obligation:** REQ-002/008 and prior FIND-2 require proving the retained local key belongs to the selected deployment before replacing its saved settings. Successful authentication as another credential is insufficient.

**Locations:** `crates/wyrd/wyrd-cli/src/server/dev.rs:537-553` and `:1269-1289`; actual credential authority `crates/shared/wyrd-client/src/config.rs:221-243`; preflight slug-only login check `dev.rs::validate_saved_login`.

**Reachable state/value flow:** A's default tenant-admin key and `[dev]` selection remain saved. The same protected file can also contain a human login for B on the same local origin and same tenant slug: the existing `wyrd auth login`/`SavedLogins::save` writer preserves the default key and stores login records by origin/slug. Select database B. `validate_saved_login` accepts the matching slug. `run` notices A's saved default key, but `saved_client` constructs `ClientConfig` with no explicit credential. Its standard resolver selects the saved login before `[default].api_key`; B's login authenticates and bounded list succeeds. `save_settings` therefore replaces A's DSNs/dev settings. If B's same-slug tenant already exists, setup returns B's identity and `ProvisionedTenant::credential` finally rejects A's key prefix, after selection mutation. The protected final key guard cannot undo those settings. This is the original producer-boundary failure with an untraced sibling credential source, not a new auth profile requirement.

**Consequence:** the refused deployment switch corrupts A's saved selection, despite the new preflight claiming it preserved it. The default-key-only two-database test cannot falsify this path.

**Minimum correction:** only for the preflight, authenticate the exact retained default key through existing `ClientConfig.credential`/`WyrdClient` support, independent of saved human login precedence; clear/avoid a human tenant selector for that exact-key check as the existing client contract requires. Keep normal shared credential selection and the final fresh saved-file client proof unchanged. Preserve protected persistence/race guard, missing-endpoint partial-save recovery, same-deployment overrides and explicit recovery semantics. No new resolver, login/profile framework or global precedence change.

**Focused proof:** extend the existing two-database partial-failure journey with B's valid same-origin/same-slug saved login alongside A's default key. Assert B switch is refused before local settings/provision, A's selected settings and key are preserved, and A resumes. Reuse existing saved-login fixture/writer and installed server; public login behavior need not be reimplemented. No runtime test was run here.

### INV-R9 — INCORRECT — saved deployment is mistaken for fresh setup after HTTP listener change

**Obligation:** REQ-004 says a normal saved restart does not repeat the gateway offer; REQ-008 says explicit inputs override only corresponding settings; AC-006 requires no repeat prompts on restart.

**Locations:** `crates/wyrd/wyrd-cli/src/server/dev.rs:341-345` (`configured` and `offer_gateway`), `:1127` (`configure_gateway` offer consumer), `:567-570` (host consumer). All production consumers of `configured`/`offer_gateway` are in this owner.

**Reachable flow:** first setup saves a key and endpoint A. Stop, then restart the same database/tenant on a usable tty with `--bind` selecting endpoint B (or `WYRD_SERVER_BIND`). The key remains saved, but exact endpoint inequality makes `configured = false`, and `offer_gateway = true`. The corrected preflight accepts the same server/key at B. After setup/persistence, `configure_gateway` asks the provider-selection questions when a known key is present, and the sibling host path also offers detected hosts. Neither the database nor tenant changed. The Rust listener-override journey explicitly passes `--non-interactive`, which makes both offers invisible.

**Consequence:** a valid listener-only restart unexpectedly blocks on setup offers and may republish credentials if approved; its automatic setup policy changed even though only a listener override was requested.

**Minimum correction:** derive resumed-setup/offer policy from the already retained local deployment selection/credential, using the existing identity preflight to refuse an actual deployment switch. Endpoint equality is a transport-location fact and must not stand in for deployment completion. Fix the one producer for both consumers; keep explicit `--gateway-file` and `--host` requests effective, and preserve first-use/partial-save behavior. No new durable completion marker or alternate startup owner is needed.

**Focused proof:** extend the existing guided installed PTY journey: complete setup, stop, change only the HTTP listener, retain a detected host/provider environment variable, restart interactively, and assert ready without gateway/host questions, unchanged key/provider credential state and readable saved endpoint. Retain explicit gateway-file/host behavior.

## Existing-owner and regression assessment

The candidate extends the actual owners rather than replacing workflows: ServerInstaller and server migrate/setup retain installation/provisioning; GlobalConfig delegates protected files to CredentialsFile; shared Terminal extracts the existing MCP nonblocking tty mechanism; composite administration shares individual write helpers, tenant transaction, existing FK/revocation locks, sealing and nonblocking audit. No second durable engine, store, idempotency receipt, SDK secret mutation or production-security relaxation was found.

The remediation's real rollback proof preserves individual put/revoke/delete semantics and unrelated policy resources. Supplied sealing persistence now shares server secret-file validation and keeps the server's managed-secret consumer usable. The documented guided-summary failure diagnosis fits the actual producer: multi-line ready output is consumed in chunks, so `converse` waiting for the final stop-guidance line preserves the original downstream summary assertions. No weaker assertion, new sleep or production timeout was substituted.

## Verification limits and result

Source-only review; no Cargo/build/test/lint/codegen execution or source edits. Inspected the exact R1 command/result evidence and relevant test bodies; this report does not claim independent reruns. macOS/arm64 execution and sibling repositories were not checked and are not independent blockers under the assigned scope. Final integrated checks remain change-review work.

Overall **FAIL**: retained FIND-TASK-001-2 and new source-local INV-R9 require independent validation. The other seven prior findings have source-backed closure.
