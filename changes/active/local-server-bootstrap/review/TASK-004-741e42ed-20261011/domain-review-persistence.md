# TASK-004 independent persistence domain review

Result: **PASS**. Proposed findings: **none**.

## Immutable subject and boundary

Repository: `/Users/stevenforrester/Documents/GitHub/local-server-bootstrap-task-004`.
Complete cumulative subject: `cb5656b2b913dbd88a8f6736bf4a0baaf998960c..741e42ed97ac3ff9638217bbcd4f0c0583e6d464` (both implementation and cleanup). HEAD remained the candidate during inspection. Source was not edited. No other reviewer conclusions were consulted.

This review covers the local durable authority: global client configuration, default tenant key, saved logins, cached tokens, directory locking, atomic replacement, partial commits, and retry. Authentication/RBAC and server database correctness belong to the separate security and runtime reviews; this report traces their interfaces only where they feed the local files. Reviewed the cumulative changed production paths and applicable test/docs consumers, using the shared map only as navigation. There is no `.codegraph` directory.

Authority: approved `changes/active/local-server-bootstrap/spec.md` REQ-008–010, REQ-015, AC-004/007; original `tasks/TASK-004-local-dev-journey.md` scenarios 1–2 and protected-owner constraint; `AGENTS.md` ownership, secrets, cancellation, test and completion rules; `architecture/agent-rules.md`; the design/doctrine client-server boundary; security posture's restrictive secret-file permissions and atomic replacement; deployment/release's local installation owner; reference router and relevant architecture patterns, Rust core, maintainer style, spec-driven development and testing guidance.

## Owner and source coverage

| Surface | Inspected source/callers | Outcome |
| --- | --- | --- |
| Local endpoint/default key writer | `global_config.rs:78–142`, `server/dev.rs:105–126,172–187,443–464,515–533` | Existing `GlobalConfig` owns the endpoint; existing `CredentialsFile` provides both files' protection and replacement. No parallel store or locking mechanism. |
| Durable file and shared lock | Complete `credentials_file.rs`, especially `api_key`, `document`, `replace`, `check_dir`, `lock`, `read_text`, `sync_dir` | All local-save edits derive from protected documents under the same directory lock used by sibling writers. Atomic rename uses a sibling temporary file followed by directory sync. |
| Sibling credential writers | `SavedLogins::{save,remove,renew,read,write}` and `AuthMiddleware::persist` → `CredentialsFile::cache_api_key_token` | Each credential writer locks the same directory before rereading. Saved-login writes retain the default profile; token caching checks the current default key before writing. No second production global-config writer was found. |
| Fresh readers | `Environment::config_dir`, shared `wyrd_utils::config_dir`, `GlobalConfig::{load_in,load_from}`, `ClientConfig::{from_environment,resolve_credential,refuse_selector}`, `CredentialChain::credentials_file` | Same configuration-directory precedence, existing file formats and credential precedence. Verification deliberately constructs a fresh environment with only the selected config directory. |
| Host and installation effects | New `offer_local` caller, existing `HostInstaller::{entry,try_install}`, installer compatibility visibility and installed-server composition | New local flow delegates to existing editors/installer; no new host credential writer. MCP host entry carries selected endpoint and configuration directory, without a key. |
| Runtime projections | Python and Node CLI changes, SDK saved-file cases and direct installed-process fixture | Bindings do not write endpoint/key independently. The shared Rust path remains authoritative. |
| Tests and recovery docs | `global_config::tests::local_setup_preserves_content_and_existing_key`; complete `pg_tests.rs`; installed Python fixture and Rust/TS saved-config cases; current CLI and local-development doc diff | First-use setup is performed by CLI, not the fixtures. Retry, partial persistence failure, safe no-reissuance, protected mode and fresh reads have credible proof. |

The existing alternative is the preexisting credential-file owner, already used for human logins and disk token caching. Extending it with a private filename choice and config protection is sufficient; the candidate does so rather than recreating serialization, locking or atomic IO. `GlobalConfig::save_local` is the new required composition of existing owners, not an alternative durable authority.

## State and failure tracing

**First use.** The CLI checks safe/correct local documents before server issuance. Server-owned setup returns the tenant identity and optional newly issued key. `ProvisionedTenant::credential` validates the candidate against that identity. `save_local_tenant` passes the selected expected key into `GlobalConfig::save_local`. The latter acquires the credential-directory lock, rereads both files, validates their shape and default profile, rejects a different key, then writes the key before the endpoint. Plaintext enters only the intended protected TOML document, not errors or progress messages.

**Concurrent sibling changes.** The preflight/read and later save are separate lock acquisitions, but the final save never trusts the earlier document. A different key inserted before the final acquisition is rejected without changing either file; `credential=None` also rejects an unexpectedly appeared key. An equal key causes no credential replacement, preserving cached tokens and comments. Saved-login or token-cache edits made between the earlier read and save survive because the documents are reread under the shared lock. The expected-key comparison belongs at this final writer boundary; it is not a redundant downstream check.

**Retry.** An active tenant supplies no new key, so the CLI retrieves and validates the saved key. Passing that exact key into the locked save permits a same-key retry without rewriting credentials. New-key installation alone removes old default-profile token cache fields, which prevents reusing tokens associated with the previously absent/empty default. Existing unrelated profiles and saved-login records are not serialized through a narrow DTO and dropped; `toml_edit` edits the full documents.

**Partial commit.** If credential replacement fails before rename, its prior bytes remain. Once rename has occurred, a following directory-sync failure can leave committed bytes with an uncertain durability result; retry rereads the file and remains safe. If the credential succeeds and endpoint replacement fails, the saved key survives and an active-tenant retry can finish saving the endpoint. If setup has committed but no key was saved, automatic retry correctly does not mint again: it fails fresh-client verification with explicit restore/recover-admin guidance. The command does not promise whole-workflow atomicity, erase committed server state, or rotate keys to disguise loss.

**Interruption.** Normal cancellation polls between async stages. Local file save is synchronous and contains no await between the key and endpoint writes; cancellation therefore cannot abandon it halfway through a future suspension. Abrupt process death can occur between the two commits, for which credential-first ordering is the useful recoverable state. Lock lifetime is the OS handle and releases on process death. New shutdown and bindings do not roll back or delete saved files. Server stop remains responsible for settling the child after subsequent verification/host failures.

## Obligation matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Save tenant key safely through existing owner, without replacing another default | `dev.rs:443–464,515–533`; `global_config.rs:115–139`; `credentials_file.rs` protection/replace | First-run CLI test asserts mode 0600 and fresh client read; direct persistence test rejects different expected key | PASS |
| Preserve existing format, unrelated settings/comments/credentials | Whole-document edit and protected config sibling; only `client.http_url` set; only new default insertion clears its old cache | Persistence unit test retains comment and cache settings; retry journey retains credential bytes and config comment | PASS |
| Preserve same-key retry cache and close read/check/write race | `global_config.rs:124–139` reread/comparison under directory lock; no equal-key credential replacement | Unit test asserts exact credential-byte preservation and refuses `None` after a key appeared | PASS |
| Serialize with saved-login/cache writers | Same `CredentialsFile::lock` inode authority in all writer bodies; reread before replacement | Source tracing plus existing owner tests; no separate concurrent stress test claimed | PASS |
| Partial state recoverable without implicit reissuance | Credential-first ordering; no replacement when tenant already active; redacted failure guidance and fresh saved-client verification | `pg_tests::server_dev_repeat_and_partial_failure_preserve_state` drives real provisioning followed by save failure and retry; recovery docs name explicit administrative action | PASS |
| Fresh processes resolve saved endpoint/key | Shared config-directory resolver and normal credential chain; fresh verification with no environment credential | First-run CLI and fresh Rust/Python/TS integration journeys | PASS |

## Proof and limits

Inspected the corrected final task evidence and executable focused lane. The orchestrator's fresh `WYRD_LOG=info mise run test:local-dev:journey` log `/tmp/wyrd-task004-review-741e42ed-journey.log` finishes successfully in 140.92 seconds, including the final packed Node case. No duplicate Cargo/broad verification was run in this domain pass. The preserved-cache unit case is supported by recorded focused evidence and source inspection; it is not claimed as independently rerun here.

Not forced here: power loss between fsync/rename operations, an endpoint-only IO failure after credential commit, or simultaneous OS-process contention. Those paths were reviewed at the existing writer/lock boundary; no new persistence mechanism requires a separate stress harness. No source-backed blocking defect or required scope change was found.
