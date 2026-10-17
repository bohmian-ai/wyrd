# Maintainer review

Result: **FAIL**

## Subject and method

Reviewed `bc3bf25e3f78b0c429110350a3de4d58290e632f...5cfb0a3ac61434256fa97a86f3506b124158780c` in `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`. HEAD remained the candidate when checked. Read the complete cumulative source diff by surface, the original task and approved specification, AGENTS.md (especially §§5, 11, 16), agent-rules, the reference router, maintainer-style, spec-driven-development, testing-workflows, and applicable client/doctrine, security and deployment authority. No CodeGraph index exists. No other review reports were read.

This is a source review, not a reproduction of implementation verification claims. No Cargo, build, lint, codegen or integration commands were run. Available task verification claims have no captured command logs. External example working-tree edits have no immutable candidate identities and are outside the reviewed source range; their correctness cannot be established from this candidate.

## Changed-surface coverage

| Surface | Symbols / consumers and proof inspected | Assessment |
|---|---|---|
| Installed local startup | `DevArgs::{run,resolve}`, `DevInputs`, `DevServer::{new,command,run,save_settings,save_tenant,recover,start}`, existing migration/provision/readiness/shutdown stages, `local_data`, `offer_gateway`, `saved_client`; callers from installed CLI; `tests/pg_tests.rs` | New inputs and retained resources remain with the existing startup owner. Public stage reporting has the uncertain-outcome defect MNT-002. |
| Terminal and host offer | New `Terminal::{open,ask,conceal}`, `EchoGuard::drop`; `prompt_local_selection`, `selected`, module exports; installed guided Python test and existing host selection paths | Extracts the existing nonblocking terminal reader rather than adding a parallel wizard. State and RAII restoration have meaningful owners. |
| Local persistence | `DevSection`, `DevDatabaseFile`, `DevDatabase`, `config_dir`, `GlobalConfig::{local_api_key,local_databases,save_local_dev,save_local}`; `CredentialsFile::{at,config_at,lock,document,replace}` and saved-client consumers; config/unit/CLI restart assertions | Existing protected file/lock/replace owner is reused; no second credential/profile store. New fields and methods have substantive role/error docs. |
| CLI gateway authoring | `GatewayCommand::dispatch`, `DeploymentFile::{read}`, `DocumentShape`, `AuthoringApply::resolve`, `AuthoringCredential`, `AuthoringSource`, `is_variable_name`, `apply_deployments_with`, retained `put_deployments`; CLI journeys | Composite syntax extends the existing document path. Existing single/list semantics retain their owner. No new parser framework or transport. New journey/helper rustdoc is incomplete (MNT-001). |
| Shared mutation client and pure contracts | `CredentialWriter::apply_deployments` and `send`, shared `DEPLOYMENTS`, `ProviderDeploymentsApply::validate`, `ProviderDeploymentsApplied`, gateway re-exports, contract tests | Typed request and redacted response match implementation. Restricted mutation owner is retained. Client docs already correctly state that a lost response can follow commit. |
| Server gateway | `GatewayAdministration::{put_credential,put_deployment,apply_deployments,write_credential,write_deployment,audited}`; collection route, MCP constant/catalog/schema/dispatch; Postgres atomicity/race test | Shared transaction-composable helpers retain validation/sealing/reference checks; no duplicate lifecycle or persistence engine. New test decoder lacks required panic docs (MNT-001). |
| HTTP/MCP proof | CLI gateway journey, MCP gateway/connectivity/discovery changes, served OpenAPI assertions, contract title/list offsets | New collection operation is threaded through route/catalog/proof consumers. No unrelated catalog feature introduced. |
| Installed Python journeys | `installed_local_cli`, `stop_local_cli`, provider-key stripping, `converse`, guided setup, gateway restart and standalone-example test | Test names/fixtures identify caller-visible outcomes; fresh consumers and redaction checks are explicit. External checkout-dependent proof remains limited as stated above. |
| Docs/examples/architecture | Setup, quickstart, gateway guide, troubleshooting, MCP reference, gateway README/document, deployment and security additions | Names/commands track actual interfaces and preserve separate credential/deployment setup. Troubleshooting repeats MNT-002's incorrect certainty. |
| Tooling | `test:local-dev:journey` commands and `scripts/check_ts_package.mjs` README allowlist | Existing lane extended rather than a second harness; package allowlist includes the real existing SDK README. No new dependency or Cargo feature. |

## Existing-owner comparison

- Startup already belonged to `DevServer`; adding resolved inputs and retaining them there is sufficient. Installation, migration and tenant setup continue through the installed server owners rather than being recreated in the CLI.
- Terminal IO previously lived in `mcp/hosts.rs::prompt_local_selection`. Extraction into a resource-owning `Terminal` supports both that existing caller and concealed setup input. The old reader is removed, so this is reuse rather than competing orchestration.
- `GlobalConfig::save_local` plus `CredentialsFile` already own secure persistence. `save_local_dev` extends those same mechanisms for server selection and DSNs. A separate settings/credential backend would be unnecessary, and none was added.
- `DeploymentFile` already owns single/list authoring; composite decoding and local environment resolution extend it and feed the same shared mutation client for standalone CLI and startup.
- Gateway atomicity cannot be obtained by repeatedly calling independently committing public puts. The existing `GatewayAdministration` now shares `write_credential` and `write_deployment` between puts and the single collection transaction, preserving the existing SQL reference locking and tenant boundary. This extension is sufficient; no new gateway service/store is justified or introduced.

## Material findings

### MNT-001 — Required panic documentation is missing on new/changed Rust test items

- Classification: **VIOLATION**; blocking under the explicit rustdoc exception.
- Authority: AGENTS.md §16 requires substantive rustdoc on new/materially modified test helpers and functions, and `# Panics` whenever panic remains possible; agent-rules and maintainer-style repeat this gate.
- Locations: `crates/wyrd/wyrd-server/src/components/gateway/pg_administration_tests.rs:1481` (`apply_document`); `crates/wyrd/wyrd-cli/tests/gateway_server_journey.rs:20` (`gateway_env`), `:14` (materially changed `gateway`), and `:463` (`deployment_document_applies_local_environment`).
- Evidence: `apply_document` has only “Decodes a composite apply document from its wire JSON” above an unconditional `.expect(...)`. `gateway_env` performs fallible temp-directory creation, binary discovery and process execution through `.expect(...)`; its doc has no panic contract. The new journey performs fixture/CLI/serialization operations through `.expect(...)` and many assertions without a `# Panics` section. The forwarding `gateway` inherits the helper's panics after its body changes.
- Concrete cost: A maintainer cannot identify the required fixture/process/valid-document preconditions from the helper contracts; these changed items fail the repository's mandatory documentation gate. Nearby `gateway_stdin` and the new `resource_names` show the expected focused panic documentation.
- Smallest correction: Add accurate panic conditions to these touched items, without changing test logic or documenting unrelated untouched items. Review other materially changed items in the same write set for the same explicit rule. Static source review proves closure; no new test or abstraction is needed. Batch this with substantive remediation rather than creating a style-only round.

### MNT-002 — Gateway error reporting falsely guarantees rollback after an uncertain HTTP outcome

- Classification: **INCORRECT**.
- Authority: approved specification INV-004/REQ-007 require honest stage/recovery results; AGENTS.md §16 and maintainer-style require docs and operation behavior to agree.
- Locations: `crates/wyrd/wyrd-cli/src/server/dev.rs:1059-1063`, with user guidance in `docs/src/content/docs/operate/troubleshoot-a-deployment.md:37` and `docs/src/content/docs/get-started/setup.md` gateway failure row.
- Producer-to-consumer trace: `DevInputs::configure_gateway` calls `apply_deployments_with` → `CredentialWriter::apply_deployments` → its `send` → `HttpTransport::request_json_with_headers`. The server's `GatewayAdministration::apply_deployments` commits before responding. The shared HTTP owner can then fail while waiting for/reading/decoding the response (`transport/http.rs:407-420`), and explicitly documents uncertain POST outcomes at `:971-979`. Any such error reaches the same unconditional “Gateway setup failed and nothing was applied” message. The existing `CredentialWriter::apply_deployments` docs already acknowledge a response lost after commit.
- Reachable consequence: If the local server commits credential rotation/deployment replacement and then exits before the response arrives, startup reports no mutation even though gateway state changed durably. An operator following the suggested `--skip-gateway` recovery can resume with a newly replaced configuration while believing the old state remains. SQL atomicity does not establish that an HTTP error means rollback.
- Smallest correction: Reuse the shared transport's existing outcome semantics; make startup's generic failure guidance state that tenant setup remains and gateway resources may already have committed, and direct inspection/reapplication of the same document. Keep deterministic server-refusal rollback claims scoped to actual refusals in documentation. Update the two troubleshooting statements to avoid promising rollback for every gateway failure. Do not introduce receipts, retries, compensation, an idempotency subsystem or a new error hierarchy for this correction.
- Focused closure proof: Exercise the existing apply owner with a committed request whose response is lost, and assert failure guidance does not claim that nothing was applied; verify the committed named resources remain readable after restart. Retain existing refusal/atomicity assertions. The minimal diagnostic wording may also be checked statically, but healthy/refusal-only proof does not establish the uncertainty case.

## Non-blocking calibration

No optional naming, placement, line-count or helper-extraction recommendations are proposed. JSON assembly in the small fixed provider offer is locally understandable and uses installed typed decoding/validation; it does not justify a new provider-discovery abstraction. Public contract/schema additions remain typed and the secret-mutation method stays outside SDK handles.

## Verification limits

No commands were rerun. This review inspected proof bodies but cannot certify recorded green executions. External example edits and their uncommitted/unrelated surroundings were not reviewed as an immutable candidate. Runtime fault reproduction and platform qualification remain unexecuted.
