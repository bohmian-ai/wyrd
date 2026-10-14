# Independent repository standards review

Overall result: **FAIL** — one bounded rustdoc finding. No implementation was changed.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/sealing-keyring`.
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`; HEAD still matched at the end of source inspection.
- Approved authority: `changes/active/sealing-keyring/spec.md`, revision 1.
- No original task exists, as explicitly directed by the human. No missing-task finding is asserted.
- Nothing has shipped. No legacy-envelope conversion, compatibility configuration, or rolling upgrade between released formats is demanded.

Reviewed all 29 paths in the cumulative diff. CodeGraph was used before locating implementation (`ManagedSecretBinding`, `CredentialResolver`, `SealedSecretRewrap`, and `local_sealing_key`), followed by direct source, diff, caller, manifest, migration, and test inspection. This is the repository-rule audit, not an additional acceptance or security-design review.

## Authority coverage

Common authority for every row: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/references/README.md`, and `architecture/references/languages/spec-driven-development.md`. Focused authorities were read in addition to the common set.

| Changed surfaces | Focused authority | Source inspected |
|---|---|---|
| `wyrd-spec/src/gateway/credential.rs`; generated credential-write schema | `references/architecture/patterns.md`; `references/doctrine/architecture-constraints.md`; `references/languages/errors.md` | Source/schema descriptions agree; serialized variants and fields remain unchanged; foundational crate gains no IO or runtime dependency. |
| Gateway `managed.rs`, `credential.rs`, `snapshot.rs`, `lib.rs` | `references/languages/rust-core.md`; `references/languages/maintainer-style.md`; security posture, Cryptography and secret handling / Tenant and data isolation | Binding owner and methods, redacted `SealedPayload`, serializer wrappers, resolver callers and failure mapping, opaque snapshot, and changed exports. |
| Server Gateway `service.rs`, `config.rs`, `state.rs`, `boot/mod.rs` | The preceding Rust/structure/error/security authorities; `operations/README.md`; `operations/deployment-and-release.md`; `operations/reliability-and-recovery.md` | `put_credential` through audited transaction and sealing; snapshot reconstruction; resolver construction; state auth ownership; `build_state`, `attach_config_fields`, `rewrap_sealed_secrets`, and existing key loader. |
| Auth `sealing.rs`; SQL `queries/auth/human_connections.rs`, `queries/gateway.rs`; new migration | Agent rules, SQL capability / caller-owned transaction rules; Rust core Postgres section; security posture; deployment migration contract | `SealedSecretTable::ALL`, select/swap statements, `sealed_tenant_secrets`, `swap_sealed_tenant_secret`, `SealedSecretRewrap::run/settle`, Gateway write/snapshot SQL, and original table constraints. |
| CLI `server/dev.rs`, `server/install.rs`; Cargo manifest/lockfile | Rust core; maintainer style; security secret-file rules; operations configuration and local bundle sections | `DevArgs::run`, `DevServer::new/command`, `local_sealing_key`, `ServerInstaller::official/root`, existing install-root lifecycle; workspace base64/rand versions and installed tempfile dependency. |
| CLI `tests/pg_tests.rs`; server Gateway administration/invocation tests | `TESTING.md`; `references/languages/testing-workflows.md`; Rust core and maintainer style | Modified `LocalDev` command/stop/stdin helper and new CLI restart journey; test keyring helper; stored-envelope helpers; deployment/rotation/copy/refusal assertions and existing consumers. |
| `wyrd-testing/src/server.rs` | The preceding testing authorities; Rust ownership; deployment-shaped harness requirement | Shared sealing key creation before state, federation rewrap, Gateway resolver injection, removed per-tenant fixture materialization, and `test_gateway_config`. |
| `mise.toml` | AGENTS §11; testing workflows; `TESTING.md` | Exact new nextest selector in environment-owning local-development lane; existing Gateway, boundary, lint, schema, and docs lanes. |
| Security-posture edit; four docs pages | Active security/operations/design authority; AGENTS §§2, 9, 11, 12 | Deployment key scope, quickstart terminology, installed key location, persisted state and restart guidance, explicit deployment configuration, troubleshooting messages. |
| Approved `changes/active/sealing-keyring/spec.md` | Spec-driven-development reference, subordinate to explicit human direction | Approval/revision present; review uses spec without inventing the absent task. |

No Python, PyO3, TypeScript, frontend, or MCP implementation/declaration files change. Their language-specific rules do not independently require a new binding or export here. Existing all-language Gateway lanes remain consumers of the changed common server/harness. No Bifrost implementation changes, so its engine references are not used as authority for this change.

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Ownership and foundational purity, AGENTS §§2–3 | PASS | Encryption uses existing `wyrd_crypt::SealingKeyring`; durable write stays in server/SQL; contract change is descriptive only. |
| Cohesive retained owner, AGENTS §5 | PASS | `CredentialResolver` retains the shared keyring; `GatewayAdministration` borrows the same state; identity belongs to `ManagedSecretBinding`; existing `SealedSecretRewrap` gains a table variant. `local_sealing_key` retains no cross-call resources and needs no artificial utility struct. |
| Typed identities and secret-bearing values, AGENTS §4 and security posture | PASS, static | Binding uses `DataTenantId`, `ProviderCredentialName`, and `ProviderId`; payload owns `SecretString`, custom redacted Debug, and zeroizing encoded/decrypted buffers. Opaque sealed bytes are the SQL boundary. |
| SQL capability signatures and transaction ownership, agent rules | PASS | Gateway write still accepts `&mut TenantConn<'_>` and does not commit it; administration owns commit. Rewrap scans and swaps accept `&OperatorPool`, include tenant and row identity in cross-tenant swap, and compare exact previous bytes. No production raw pool signature/field is added. |
| RLS as tenant query boundary, agent rules | PASS | Changed tenant snapshot/write paths use the existing TenantConn queries; inserted tenant comes from `conn.data_tenant_id()`. New rewrap SQL is operator work, so its tenant-qualified predicate is appropriate. |
| SQL schema consistency, AGENTS §15 and operations migration authority | PASS, static | Dropped nonce/version columns are removed from writers and snapshots. The earlier migration's independent `CHECK ((source = '"managed_secret"'::jsonb) = (secret_ciphertext IS NOT NULL))` survives; constraints involving dropped columns are obsolete. No previously released data format is assumed. |
| Errors and audit ownership, AGENTS §§4, 9 and agent rules | PASS, static | Local cryptographic errors remain internal and map through existing public errors; permission/audit wrapper is retained; rewrap evaluates no permission and creates no new audit writer. |
| Narrow async boundaries, AGENTS §6 | PASS | Seal/open/decode remain synchronous; resolver and durable orchestration await actual IO. Key setup executes before request serving and adds no runtime. |
| Dependency and feature policy, AGENTS §§2, 4 and agent rules | PASS | Existing workspace base64 promoted from dev-only to normal dependency; rand uses existing workspace 0.9; no new package version, feature, broad-tier dependency, or test-support shipping edge. Supporting `check:deps` passes. |
| Key file confidentiality and atomic persistence, security posture | PASS, static | Existing explicit key env names take precedence; tempfile in install root is persisted without clobbering, after write and fsync; only its path is passed in child env; no key is printed. Server's normal restrictive key-file loader remains authoritative. |
| Generated/public surface consistency, AGENTS §§8–9 and agent rules | PASS, static only | The sole schema difference is the same description as its Rust source; no .pyi, .d.ts, new SDK durable contract, or alternate configuration alias is introduced. Regeneration was not run. |
| Test placement and runtime ownership, AGENTS §11 | PASS | Pure binding checks stay in module; Postgres helpers remain pg tests; new CLI journey uses compiled CLI/server and managed DB, and its exact name is registered in existing local-development lane. No foreign runtime is initialized from Rust. |
| No gate weakening, AGENTS §12 | PASS | No new ignore/allow, weakened assertion, boundary glob, or disabled check occurs; retired tests cover deleted per-tenant keyring mechanics, with deployment-key binding/rotation proof replacing them. |
| Required substantive rustdoc, AGENTS §16 | FAIL | `STD-001` below. Production failure behavior and a changed fallible test helper lack mandatory `# Errors` documentation. |
| Imports at module scope, agent rules | FAIL, non-blocking | New CLI journey imports wiremock inside its function at `crates/wyrd/wyrd-cli/tests/pg_tests.rs:643–644`. This is a placement-only note with no demonstrated behavior/security consequence. |
| Verification completion, AGENTS §§11–12 | Not established | See evidence limits below. Green supporting static checks do not establish full lint, codegen, docs, or journey execution. |

## Material proposed finding

### STD-001 — Changed fallible Rust items omit mandatory error documentation

- Classification: `VIOLATION`.
- Rule: `AGENTS.md` §16 requires `# Errors` on **every** fallible new or materially modified function, including private functions and test helpers; missing substantive rustdoc is explicitly a hard blocker. `architecture/agent-rules.md` repeats that requirement.
- Exact locations: `crates/wyrd/wyrd-server/src/boot/mod.rs:1321–1330` (`attach_config_fields` documentation and signature); `crates/wyrd/wyrd-gateway/src/managed.rs:161–170` (`tests::open`).
- Evidence: the boot helper is materially changed to obtain and inject the deployment keyring. Its docs describe composition and claim “Pure/sync (no I/O)” but omit the required error contract while propagating `OperatorKeys::for_role` failures and returning `ServerBootError::GatewayTransport` for resolver/HTTP dispatcher construction failures. Resolver construction can read a CA bundle, as documented by `GatewayConfig::credential_resolver` itself. The changed test helper returns the error from `ManagedSecretBinding::open`, covering unknown held key, malformed/tampered payload, and binding mismatch, but its rustdoc contains no `# Errors` section.
- Consequence: maintainers cannot infer the composition failure and IO behavior from its documented contract; the explicit touched-item documentation completion gate is unsatisfied even if tests pass. This is the skill's mandatory-rustdoc exception, not a claim that missing prose itself alters runtime behavior.
- Smallest correction: document the actual failure conditions of these two functions in `# Errors`; correct the boot helper's claim that composition performs no IO. Keep its synchronous behavior and existing owners unchanged. No API, new helper, dependency, test harness, or architecture decision is needed.
- Focused closure: inspect both complete rustdoc blocks against their bodies and `GatewayConfig::credential_resolver`; the two contracts must accurately describe failure propagation. Run the normal touched Rust formatting check; no runtime test is necessary for this prose-only correction. Batch with substantive remediation if other review scopes find defects; do not create a separate style-only round.

## Non-blocking note

The `use wiremock::...` lines at CLI pg test lines 643–644 violate the module-scope import rule. Moving them to the existing top-level import group would satisfy the rule. No behavior or security consequence is demonstrated, so this note does not independently require remediation or drive the verdict.

## Verification evidence and limits

The orchestrator's `verification.md` records PASS for `mise run fmt:check`, `mise run check:tenant-isolation`, `mise run check:deps` (including one production inline-secret security test), and candidate diff whitespace checking. I independently ran `git diff --check 526355fb 202e5bab`, which passed.

No implementation execution record was supplied. Full lints, generated-schema regeneration/check, docs build, and runtime journey suites were not run in this immutable review. Source tests were inspected, not reported as executed. Therefore this standards report does not assert AC-003 completion or executed regression proof. No alternate candidate, baseline build, compatibility support, or production rollout exercise was introduced.
