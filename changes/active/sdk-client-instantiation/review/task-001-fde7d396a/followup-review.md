# Focused follow-up: public ambient journey feasibility

Result: **UNRESOLVED**, limited to the literal saved-credentials-only Platform obligation. The feasibility uncertainty for Rust tenant handles is **RESOLVED**: bounded support in the existing fixture owners can prove the public ambient factories without any prohibited mechanism. The Python eager AsyncBifrost omission is independently **RESOLVED** as a bounded test gap.

Subject: base `2f1db2194f5630df850b10987fe0f360946a525c`, candidate `fde7d396a3917964dc344ede2ed09046e179aa66`. HEAD remains the candidate. Scope is BEH-002 / INVAR-002 only. This is static source review; no tests, builds, generators, runtime child processes, or source edits were performed. Only this report was written.

## Evidence inspected

- Approved revision 9, AC-002 / AC-004 / AC-007 and D4; original TASK-001 at the base, Scenarios 4 and 6; both discovery reports and the immutable subject; repository test/runtime rules, Client model, spec-driven development and maintainer references.
- CodeGraph first, followed by the shared configuration, credential and connection owners; Rust journey and support owners; nextest configuration, mise lanes and Postgres wrapper; WyrdTestServer/PgFixture bind, tenant and credential setup; Python ambient journeys, async factory and testing environment publication.
- The cumulative changed-file map and focused cumulative connection-owner diffs were inspected. No audit of unrelated findings was undertaken.

## Current coverage and exact gap

The discovery reports correctly identify a bypass. Rust `ambient(Environment)` at `sdks/wyrd-sdk-rust/tests/integration/sdk_client_instantiation.rs:50` loads config and builds a client itself. `open_every_handle` at :106 calls only `connect_with`; the missing-credential test at :201 fails before reaching any handle. The retention test at :342 likewise resolves an explicit client before calling `connect_with`. These tests do not fail if the public no-client branch is broken. Sharing lower-level resolution code is useful supporting coverage, not equivalent AC-002/AC-004/AC-007 acceptance proof.

Production source has a single ambient owner: `WyrdClient::from_global` (`crates/shared/wyrd-client/src/client.rs:73`) uses `ClientConfig::from_global`; `connect_ambient` at :84 composes that with `connected`. `GlobalConfig::load` (`global_config.rs:156`) and `ClientConfig::from_global_with_overrides` (`config.rs:133`) select `Environment::Process`. `Environment::var` (`environment.rs:28`) directly reads `std::env::var` for that variant. The existing `Fixed` variant is explicit input, not a process-local override that public factories consult. The four HTTP factories, Bifrost async/blocking factories, and Platform all reach that shared path. No second resolver or public SDK test constructor is justified.

## Rust proof is feasible within existing ownership

The orchestrator clarified that narrow private test support inside WyrdTestServer/PgFixture is permitted; “no new harness” does not prohibit extending those owners to provision a startup-supplied test credential. Under that constraint the missing dynamic environment handoff is not a material product decision.

Existing mechanisms provide the necessary pieces:

| Existing owner/mechanism | Source evidence | What it permits |
|---|---|---|
| nextest process-per-test isolation | `.config/nextest.toml:1–17`; journey profile at :106 onward | Ordinary runner startup environment can differ between exact scenario invocations. No test needs to re-execute itself. Isolation does not by itself make runtime environment mutation safe. |
| Existing environment-owning invocation | `mise.toml:686–689`; `scripts/postgres/with-test-postgres.sh` | Keep the managed Postgres lifecycle and nextest runner. Supply ambient settings before the test process starts; the current lane does not already supply Wyrd client settings. |
| Config directory resolution and persistence | `Environment::config_dir` at `environment.rs:40`; `GlobalConfig::save_local` at `global_config.rs:106`; `private_config_home` at `human_login.rs:372` | A startup-selected isolated config home can receive the server's runtime endpoints/key through existing protected-file persistence. Public `connect` then reads those files itself. Changing those files after connection can prove retention without mutating environment. |
| Configured fixture binding | `WyrdTestServerBuilder::with_bind_addrs_for_test` at `server.rs:4348`; `start_bound` at :5161; bind at :3975 | A startup-known HTTP/gRPC address can be the actual real fixture address. The default dynamic port choice is not compulsory. Existing runner isolation/serialization must prevent sibling socket conflicts. |
| Caller-selected test tenant | `PgFixture::seed_additional_tenant_with_uuid` at `crates/shared/wyrd-dev-fixtures/src/pg.rs:275` | Fixture setup already permits the tenant identity carried in a startup-supplied credential. |
| Shared credential parsing and storage | `WyrdApiKey::parse` at `crates/wyrd/wyrd-auth/src/issue_api_key.rs:203`; `WyrdTestServer::mint_machine_principal` at `server.rs:3676`; its existing `hash_secret` / `insert_api_key` calls at :3687/:3702 | Narrow fixture support can provision the startup credential through existing parsing, principal, hash and storage owners rather than implementing a second authenticator or altering production issuance. |

The smallest correction boundary is the existing Rust journey plus only the existing fixture/runner support it needs. Require the public no-client factories to complete the same real server operations under distinct startup environment-only and saved-only cases, exercise no-client Bifrost options and blocking Bifrost, and invoke the public factories themselves in the empty-credential case. Reuse ambient-created handles for the saved-configuration-change retention case. Preserve explicit-client coverage and the existing shared resolution/cache/renewal owners. This states the outcome and owners; it does not prescribe fixture structure or a new test framework.

The environment-only case needs startup-known credentials as well as endpoints. Merely configuring fixed ports would not solve the existing fixture's generated key (`server.rs:3686`). Bounded support to provision a startup-supplied key is ordinary test setup under the clarification above, and is feasible through the existing owners. It does not require unsafe runtime environment mutation, a dependency, runtime child processes, a production auth API, or changing `WyrdClient` construction. The older re-executed TLS test (`crates/shared/wyrd-client/tests/integration/transport/grpc.rs:93`) is an existing alternative, but is excluded by this follow-up's constraints and is unnecessary.

Affected obligations: Rust tenant portion of AC-002 (environment/saved/options), Rust no-client AC-004, and ambient-construction portion of AC-007. No production misresolution is established. A direct public `connect` regression must fail the corrected journey; tests that still call the hand-built resolver do not close it.

## Platform applicability remains a material boundary

Platform's public **environment** success and empty-credential proof can use the same startup approach with fixture-only credential provisioning through existing platform principal/credential storage. Platform credential parsing/hash/storage owners already exist (`crates/wyrd/wyrd-auth/src/platform_credentials.rs`, `wyrd_auth_issue::hash_secret`, and `wyrd-sql`'s `insert_platform_credential_tx`). No production platform-session change is needed.

A literal **saved-credentials-only** Platform success case is different:

- Revision 9 AC-002 says each in-scope handle succeeds in both ambient cases; D4 includes Platform and preserves its current session exchange and shared ambient path.
- `Platform::connect_with` (`crates/shared/wyrd-client/src/platform/handle.rs:94–101`) requires `ResolvedCredential::BearerToken`; other variants are refused before the session exchange.
- The saved file floor (`transport/credential.rs:366–370`) always constructs `CredentialSource::ApiKey`. It does not classify a stored `wyrd_global` string as a bearer. Saved human login resolution produces `ResolvedCredential::Renewable` (`config.rs:241–247`), also unsuitable for Platform.
- `GlobalConfig::ClientSection` carries endpoints, tenant and cache configuration, not a saved platform bearer. Supplying `WYRD_ACCESS_TOKEN` alongside saved endpoints works through the existing contract, but is mixed configuration, not saved-credentials-only success.

Therefore private fixture work cannot make that literal production path succeed. Do not add a persisted platform credential source, reclassify the tenant file floor, or weaken Platform's credential requirement as test remediation. The exact decision is whether AC-002's saved-only success applies to Platform: approve the existing-source applicability qualification (saved-only tenant handles; Platform ambient platform credential/session path), or explicitly approve a new saved platform credential source. The latter changes a production credential contract and is outside bounded proof closure. The independent validator should reconcile this against D4 before requiring an unqualified “every Platform case” correction.

This is the sole unresolved scope boundary. It does not invalidate the feasible tenant proof path or justify treating the current injected-client journeys as equivalent public proof.

## Python eager AsyncBifrost is a separate bounded omission

`sdks/wyrd-sdk-python/tests/integration/test_sdk_client_instantiation.py:41–47` enumerates only synchronous handles. The eager ambient scenario at :91–105 never awaits `AsyncBifrost.connect`. The successful async call at :111–121 is lazy construction only. The public eager factory (`python/wyrd/bifrost/_client.py:1036–1063`) creates its native connection through `asyncio.to_thread`, which is a distinct public entry point despite reusing the native owner.

The existing Python server fixture already publishes real endpoints/key through `src/testing.rs:232–241`, using the established Python environment owner at :87. The journey's `saved_configuration_only` at :63 already selects saved endpoints/credentials with pytest monkeypatch; `asyncio.run` is already used at :121. Extend that same ambient scenario to await the public eager AsyncBifrost factory and make a real server call in both cases, including supported options without a client. No new fixture, dependency, Rust interpreter emulation or auth implementation is needed. This closes the missing AsyncBifrost portion of AC-002; lazy coverage cannot substitute for it.

## Recommended verification and residual risk

No verification was executed. Closure should run only the affected exact Rust scenario selectors through the existing managed Postgres wrapper and `mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=sdk_client_instantiation::<scenario>)'`, with each required ambient environment established at process startup, and the existing focused pytest ambient selector through `mise exec -- uv run python -m pytest ...::test_ambient_connect_opens_every_handle`. Final commands must name the implemented/discovered tests exactly; isolated startup cases must not disappear from the owning mise lane.

Skipped implementation and execution as assigned. Feasibility is source-backed, not a claim of passing runtime proof; literal saved-only Platform success still needs the applicability/credential-source decision above.
