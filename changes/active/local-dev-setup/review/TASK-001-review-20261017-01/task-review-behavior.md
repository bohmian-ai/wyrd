# Independent behavior acceptance audit

## Subject and result

Role: `behavior-rev`, independent of implementation and other discovery reviewers.

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c` (HEAD confirmed unchanged at completion).
Approved authority: `changes/active/local-dev-setup/spec.md`, revision 1.
Original task: `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
Actual cumulative range inspected with `git diff bc3bf25e...5cfb0a3`; no implementation edits or executable verification performed.

**Overall: FAIL for the immutable main-repository candidate.** Three bounded behavioral findings below prevent acceptance. Separately, the supplied subject has no immutable base/candidate for the mandatory standalone example repositories, so complete task acceptance also has a global review-input blocker. That input gap is not a behavioral finding and receives no finding ID here.

## Authority and navigation coverage

Applied AGENTS.md ownership, portable platform, secret handling, API/SDK boundary and journey requirements; `architecture/agent-rules.md`; reference router; spec-driven-development, maintainer-style and testing-workflows; active design/doctrine; deployment configuration/database authority and security-posture local persistence/credential boundaries. Bifrost engine behavior is unchanged: this change selects and persists its existing data-root input rather than changing ingest/query semantics.

Expanded paths reviewed:

- `DevArgs::run/resolve` → `DevInputs` → existing `ServerInstaller` → `DevServer::start/migrate/ready` → installed `wyrd-server setup` → `save_settings/save_tenant` → saved-only `WyrdClient`/`Cards` read → optional gateway and MCP offer → foreground shutdown.
- Protected persistence: `GlobalConfig::{local_api_key,local_databases,save_local_dev,save_local}` → existing `CredentialsFile` lock/document/replace mechanism. Compared original startup's `validate_local_config` and `save_local_tenant` responsibilities with the new composition.
- Interactive gateway selection and file input → `DeploymentFile::read`, `AuthoringApply::resolve`, pure `ProviderDeploymentsApply::validate` → restricted `CredentialWriter` → HTTP collection route or scoped MCP → `GatewayAdministration::apply_deployments` → shared transaction-composable credential/deployment writers. Compared sibling individual put/revoke/delete and existing snapshot reader.
- Existing source types/operator assignments, sealing, credential share locks and SQL integrity remain owners. No second durable gateway owner, transport, secret store, startup engine, or receipt state was introduced.
- MCP host input owner: `mcp::offer_local` → `hosts::prompt_local_selection` → shared `Terminal`; standalone `mcp install` remains a sibling consumer. This expansion identifies BEH-001.
- CLI compiled-binary journeys, Python installed setup/gateway/guided/standalone journeys, server apply test, MCP journey and served OpenAPI assertion. Main setup/quickstart/gateway/troubleshooting guidance, gateway example, mise lane and package inventory change were inspected. Offline examples retain their existing runtime boundaries.

New machinery is largely earned: `DevSection` adds the missing persisted selection on the existing config owner, `DevInputs` holds the selected per-run values without a second bootstrap owner, composite authoring adds only local source resolution before existing transport, `Terminal` extracts the existing nonblocking host reader and adds concealed input, and shared server writers permit the required single transaction rather than nesting independently committed public methods. No speculative abstraction finding is proposed.

## Acceptance matrix

`PASS` means source and inspected proof bodies support the obligation in this source audit; it does not claim rerun verification. `FAIL` includes unestablished mandatory external evidence as explicitly identified.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: optional tenant parser, one explicit/env/saved/prompt resolution path, listener and argument validation, complete missing-input refusal before server setup | `DevArgs` and `resolve`, `first`, `loopback`, concealed `Terminal` input | P1 guided input test; P2/P8 missing/conflicting arguments and saved settings | PASS |
| REQ-001: `--non-interactive` never prompts even on a terminal; retain optional MCP host behavior | `DevServer::run:527-529` omits resolved `interactive` when calling `mcp::offer_local`; that owner uses terminal detection alone | Existing P1 covers interactive host decline; P8 uses nonterminal stdio and cannot detect this defect | FAIL — BEH-001 |
| REQ-002: protected DSNs, separate platform DSN, unchanged shell environment, stable absolute default roots, existing `.wyrd` not adopted, unrelated settings retained | `local_databases`, `save_local_dev`, `command`, `local_data` | Global-config roundtrip; P2/P8; storage owner refuses credential-bearing or relative URLs before readiness | PASS |
| REQ-002/008: unrelated saved deployment not overwritten/adopted, existing key not stranded when selecting another deployment | Only saved slug/human login is checked at `resolve:295-302`; `run:508-511` saves selected database before provisioning/validating existing key | No cross-database saved-key journey; P8 keeps original DB | FAIL — BEH-002 |
| REQ-003: reuse verified installer, migrate before serving, readiness before setup, ordinary server process and tenant key class | Existing installer retained; `start`, `migrate`, `ready`, server `main.rs::setup` | P2/P9 and existing installed bootstrap journeys | PASS |
| REQ-003/008: normal restart reuses exact key/sealing key; interrupts and startup errors settle owned process; committed state survives | `local_sealing_key`, `provision`, `save_tenant`, `stop`, `kill_on_drop` | P8/P9, installed bootstrap interrupt/exit, existing managed-secret restart case | PASS for same deployment; BEH-002 covers changed deployment |
| REQ-004: gateway optional, only selected known providers, deterministic Gemini precedence, rename/confirmation, unattended omission skips gateway and explicit document reapplies | `PROVIDERS`, `offer_gateway`, `configure_gateway`, `DevArgs::resolve` | P1 selects renamed Gemini only, P3/P7 explicit document; existing no-key installed startup | PASS |
| REQ-005: retain single/array/two-step and operator/external/managed sources; local-env literal names resolve in memory, malformed/blank/duplicate/unused input refuses without partial mutation | `DeploymentFile`, `AuthoringApply`, existing `put_deployments`; pure apply validation and server source validation | P3, P5a/b, P6a/b, P7 retained array/single assertions | PASS |
| REQ-006: nonempty typed composite POST, named upsert, untouched unrelated resources, one tenant transaction, shared source/sealing/reference checks, redacted request-order answer | `ProviderDeploymentsApply/Applied`; HTTP handler; `GatewayAdministration::apply_deployments`, shared write helpers and existing SQL locks/integrity | P4 later-invalid rollback/keyless/denied/foreign/reference-lock test; P3/P10 public paths | PASS within source-inspection limits |
| REQ-006: lost response may follow successful commit; never claim error proves nothing committed | `configure_gateway:1059-1063` unconditionally prints “nothing was applied” for transport/response failure as well as server refusal; docs repeat this guarantee | Existing tests cover server refusal, not response loss after commit | FAIL — BEH-003 |
| REQ-006: scoped MCP same typed service/authz; restricted mutation handle; no SDK secret mutation; nonblocking audit | MCP tool catalog/gating/dispatch, `CredentialWriter`, `GatewayAdministration::audited`; SDK Gateway surface unchanged | P10 runtime discovery/apply/read/refusal; P11 schema refs; P4 staged authorization decisions | PASS |
| REQ-007: `/readyz`, saved-only authentication/bounded read before final ready; readback requested names; summary and foreground guidance; no paid upstream readiness claim | `ready`, `saved_client`, `configure_gateway`, final summary | P1/P2/P7/P8; local provider fixtures only | PASS except uncertain apply error guidance in BEH-003 |
| REQ-008: no implicit recovery/reissue, preflight protected persistence, explicit authorized recovery, actionable lost-key boundary | `save_settings`, `save_tenant`, `recover`, existing platform endpoint and `recover_tenant_admin` | P9 proves missing/fake unconfirmed authority does not reissue; successful confirmed recovery is source-inspected only | PASS for implemented boundary; successful recovery proof limit retained |
| REQ-009: main server-backed examples/docs use installed setup, app registration explicit, two-step path retained, source-driven served contracts | Gateway example and docs diffs; unchanged offline examples; runtime OpenAPI and MCP sources | P7/P10/P11 and reported docs/examples checks | PASS |
| REQ-009: standalone Python/TS/Rust applications and honest acquisition instructions | Mandatory external trees are identified but uncommitted; no immutable range is supplied | P12 reads mutable checkout and skips when absent; task records local smoke claims | FAIL — acceptance unestablished; global review-input blocker, no behavioral finding |
| INV-001: single owners, pure contracts, server durable logic, no duplicate startup | Existing startup/config/transport/service owners extended as above | Source comparison and package-boundary proof claims | PASS |
| INV-002/003: no plaintext disclosure or inferred authority; existing tenant separation/provider matching/scopes retained | Redacted wire secret type, decode errors omit document text; protected credentials; same TenantConn and authz owner; storage server parser rejects URL credentials | P1/P3/P4/P7/P10 plus source trace | PASS |
| INV-004: honest saved identity, no implicit reset/key rotation; settled owned server | Same-deployment lifecycle retained; selected database saved before identity verification | P8/P9 happy recovery boundary; missing cross-deployment case | FAIL — BEH-002/003 |
| INV-005: optional gateway and retained source/individual/array/SDK features | Existing methods kept, composite is additional shape/operation | P5/P6/P7 retained regressions | PASS |
| INV-006: supported macOS/Linux architectures, no changed engine OS prerequisite | Terminal uses Unix APIs shared by macOS/Linux, compatible release machinery unchanged; newly added P8 reuses existing Linux-only `LocalDev::free_bind` fixture | No macOS execution evidence; Rust journey fixture reads `/proc/sys/net/ipv4/ip_local_port_range` at `pg_tests.rs:312` and cannot run on macOS | FAIL for required portable journey proof; governing standards/domain review owns classification |
| AC-001 | Guided concealed input/local storage/provision/selection/ready path | P1 real terminal, installed bootstrap no-key path; no dedicated decline-with-keys terminal proof | PASS for source outcome; runtime rerun absent |
| AC-002 | Full unattended source path and missing-input refusal; terminal host leak | P1/P8 lack tty plus noninteractive combination | FAIL — BEH-001 |
| AC-003 | Local-env and provider selection behavior | P1/P3/P7 | PASS |
| AC-004/005 | Atomic/security/reference and preserved legacy feature boundaries | P3/P4/P5/P6/P10; no new SQL-failure injection observed in P4 | PASS for inspected implementation; narrower failure-proof limit retained |
| AC-006/007 | Same-deployment restart and failure recovery; changed database/ambiguous gateway error gaps | P8/P9 and previous installed fixtures | FAIL — BEH-002/003 |
| AC-008 | Existing saved-only Rust/Python/TS/MCP consumers kept on shared facade | Installed bootstrap/gateway runtime selectors; local provider fixture consumers | PASS |
| AC-009 | Main example update present; immutable standalone subject absent | P12 mutable external checkout | FAIL — global input blocker |
| AC-010 | Typed source/OpenAPI/MCP changes and scoped verification claims | Task lists focused P1–P12 and lint/type/codegen/doc checks; no captured logs supplied; no checks rerun | PASS as available claimed evidence, subject to independent validation and noted portability/external limits |
| Non-goals: no PostgreSQL installer/roles, UI wizard, daemon, new profile/vault/recovery protocol, provider calls/discovery, SDK secret mutation, hidden app registration, migration/store/receipt machinery | Cumulative diff remains within existing owners and permitted source/docs/test changes; package inventory README acceptance matches npm packaging | Full changed-path inventory and implementation source comparison | PASS |

## Proposed findings

### BEH-001 — VIOLATION — noninteractive setup still asks an MCP host question on a tty

Obligation: REQ-001 unconditionally forbids prompting with `--non-interactive`, including complete invocations; AC-002.

Exact changed location: `crates/wyrd/wyrd-cli/src/server/dev.rs:527-529`; reachable downstream owner `crates/wyrd/wyrd-cli/src/mcp/mod.rs:224-245`.

Producer-to-consumer evidence: `resolve` correctly sets `DevInputs.interactive = false` with `--non-interactive`; gateway offer respects it. After provisioning and saved-client verification, fresh setup nevertheless calls `mcp::offer_local(hosts, endpoint)` whenever `!configured`. With omitted hosts, that owner skips only when stdin is not a terminal or no host is detected; otherwise it calls `prompt_local_selection`. A developer/agent using a tty, a detected Codex/Claude config, complete DB/tenant inputs and `--non-interactive` reaches an interactive MCP prompt after mutation and cannot reach ready without answering.

Sibling callers: standalone `mcp install` retains intentional terminal behavior; explicit repeated `server dev --host` must still install those hosts, including unattended use. The defect is the startup composition dropping its resolved prompt policy, not the host reader.

Smallest correction: use resolved `inputs.interactive` when deciding whether startup may invoke the implicit MCP offer; retain explicit nonempty host selections. Reuse `mcp::offer_local`; do not add another host installer or prompt framework.

Focused closure: extend the existing installed tty journey with a fresh complete `--non-interactive` setup and a detected host. It must reach ready without any question and without touching host config; separately preserve existing explicit `--host` behavior. P8 with piped stdio cannot prove this scenario.

### BEH-002 — INCORRECT — selecting another database overwrites saved deployment settings before recognizing the unrelated saved key

Obligations: REQ-002 requires refusal of an unrelated saved deployment/tenant without overwriting/adopting it; REQ-008 preserves deployment identity and safely retains newly issued credentials; INV-004; AC-006/007.

Exact changed locations: `crates/wyrd/wyrd-cli/src/server/dev.rs:295-302`, `507-511` and `551-565`; downstream `GlobalConfig::save_local` at `crates/shared/wyrd-client/src/global_config.rs:232-249`.

Reachable path: complete local setup for tenant slug `local` in database A. Stop it. In the same `WYRD_CONFIG_HOME`, explicitly supply `WYRD_DATABASE_URL` for an empty database B (with its platform DSN where required), keeping slug/listeners the same. The saved key exists and saved slug matches, so resolution succeeds. `run` starts/migrates B, then `save_settings` replaces protected saved DSN A with B before verifying that B recognizes the saved credential. `provision` calls existing server `main.rs::setup:261-307`: B has no matching active tenant, so setup creates a new tenant and returns a new one-time key. `ProvisionedTenant::credential` prefers that new key and validates only its own returned UUID. `GlobalConfig::save_local` then refuses replacement of A's different protected default key. B's just-issued key is dropped, B's tenant is active, and the prior saved DSN has already been replaced. A bare rerun now targets B using A's key and fails instead of resuming A.

This is not a request to compare raw DSN strings: password rotation, equivalent endpoints, and legitimate listener changes can refer to the same deployment. The missing authority is recognition of the existing protected credential by the selected server, before durable local identity selection and new issuance.

Existing owners/siblings: ordinary first use without a key needs the current protected-file preflight before issuing a key. Existing same-deployment restart, `GlobalConfig::save_local` nonreplacement, server setup's active-tenant/no-reissue behavior and platform recovery remain authoritative. Explicit endpoint/database changes must not bypass those boundaries.

Smallest correction: when a protected default key already exists, authenticate it against the selected ready server through the existing `WyrdClient` credential path before replacing saved development settings or invoking tenant creation. Refuse an unrecognized/mismatched key with `WYRD_CONFIG_HOME` separation guidance while preserving old local files. Keep first-use no-key persistence preflight and existing authorized recovery intact; do not introduce a profile store or new credential class.

Focused closure: installed CLI journey using two managed empty databases, one config home and the same tenant slug. After A is configured, selecting B must fail without changing A's saved DSNs/key/client/dev settings or issuing B a tenant-admin credential. Restart with unchanged saved files must still reach A. Preserve a same-database explicit listener/DSN override regression.

### BEH-003 — INCORRECT — a transport failure is falsely reported as proof the atomic gateway apply did not commit

Obligation: REQ-006 explicitly says a lost response may follow a successful commit and forbids claiming failure proves nothing committed; REQ-007 stage errors need honest safe next actions; INV-004; AC-007.

Exact changed location: `crates/wyrd/wyrd-cli/src/server/dev.rs:1059-1063`; adjacent user guidance `docs/src/content/docs/get-started/setup.md` gateway failure row; new collection-route availability description in `components/gateway/routes.rs` likewise says availability means nothing applied.

Evidence: `configure_gateway` catches every `apply_deployments_with` error and prints `Gateway setup failed and nothing was applied`. The restricted handle calls shared `http.request_json`, whose result includes transport and response-decode failures. The existing shared handle's new rustdoc correctly acknowledges response loss after commit. On the server, `apply_deployments` commits before producing/transporting the response, so losing the response or failing to decode it can return an error with all named resources committed. The CLI still makes the categorical no-commit claim and shuts its owned server down. The same uncertainty applies to a commit-result connection failure mapped to service unavailable.

Sibling consumers: standalone composite `deployment put` propagates the structured error without the startup-specific categorical extra line; MCP/HTTP use the same committed operation. No extra receipt/idempotency state is required or permitted.

Smallest correction: remove the unconditional no-commit claim from startup failure guidance and affected collection-operation documentation. State that setup did not complete, the operation may have committed, and inspecting/reapplying the same named document safely converges. Preserve stronger no-mutation guarantees only for a known precommit validation/refusal boundary where warranted; do not add recovery stores, jobs, or retries.

Focused closure: use an existing local transport fixture/proxy to let the apply commit and then lose its response; verify resources are committed, startup reports uncertainty rather than `nothing was applied`, and reapplying succeeds. At minimum the changed guidance must have a direct regression assertion for the shared transport-error branch. Keep explicit server validation rollback checks.

## Rejected hypotheses and limits

- Storage URL secret leakage through `inputs.storage_url` initially looked possible because only the argv form is locally checked. The production `BackendConfig::from_url` rejects credentials/query/fragment for every source before server readiness and before `save_settings` or the final summary. No reachable leakage finding is proposed.
- Server writer extraction reuses the complete prior source validation, sealing, provider assignment, credential share lock and deployment upsert logic. The new composite owns the transaction once; no independently committed loop was found.
- Existing array semantics deliberately remain per-entry writes with ordered completed/failed/unattempted output. The approved spec explicitly retains this feature, so it is not a failure of composite atomicity.
- Unix terminal APIs serve both mandatory OS families; `cfg(unix)` alone is not a forbidden Linux-vs-macOS feature gate. However, the materially expanded Rust installed journey reuses `free_bind` reading Linux `/proc`. That portable-proof issue is recorded in the matrix for governing standards/domain classification, rather than an additional speculative runtime finding.
- P9 confirms refusal of unattended missing/unconfirmed recovery authority but does not drive valid platform-authorized terminal-confirmed recovery to completion. P4 covers one locked-row revoke race, not all combinations of concurrent replacement or an injected SQL failure. Source supports the shared-owner behavior; those are explicit evidence limits, not claims that green checks establish unexecuted scenarios.
- No Cargo, mise, provider, database, codegen or lint commands were run by this reviewer, per the coordinated immutable source-review instruction. Available command results are task-authored claims without captured logs. External standalone uncommitted sources are not silently adopted as reviewed immutable content.
- No prior findings/remediation exist. Only this report was written; implementation remained unchanged.
