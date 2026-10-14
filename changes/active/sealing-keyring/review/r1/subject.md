# Sealing-keyring review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/sealing-keyring`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`
- Approved specification: `changes/active/sealing-keyring/spec.md`, revision 1.
- Original task: none. The human explicitly requests review against the spec and implemented commits only; absence of a task is not a blocker.
- Human scope clarification: nothing has shipped. Do not demand compatibility or migration of previously deployed Gateway secrets.
- Review the complete `git diff 526355fb8c58dae6d67d516f98dddcd5c02b0e79 202e5bab768cb998a01f04555c39f43b53c09929` and current source, not commit summaries.
- Source starts clean. Keep source and commits immutable. Write only your assigned review report using apply_patch. Other reviewers are working concurrently; do not revert their files or read their conclusions during discovery.
- No recorded execution evidence accompanies the spec. Source tests are available; report runtime verification limits accurately. Review is not implementation or final change review; do not run broad journey suites or mutate source to clear checks.

## Shared starting navigation map

This map is a starting point, not complete coverage or a conclusion. Follow CodeGraph first and independently expand callers, sibling consumers, and tests.

| Surface | Owners and paths | Consumers and proof starting points |
|---|---|---|
| Deployment key configuration | `crates/wyrd/wyrd-server/src/config.rs`; `src/boot/mod.rs::build_sealing_keyring` | `AppState.auth.sealing_key`; Gateway boot wiring; key file validation tests |
| Gateway sealing and identity binding | `crates/wyrd/wyrd-gateway/src/managed.rs::ManagedSecretBinding`; `src/credential.rs::CredentialResolver`; server `components/gateway/service.rs::GatewayAdministration` | Credential writes, admission snapshots, invocation, redacted views; `pg_administration_tests.rs`, `pg_invocation_tests.rs`; managed module unit tests |
| Shared cryptography | `crates/shared/wyrd-crypt/src/lib.rs::SealingKeyring` (existing owner) | Existing stored identity secrets, Gateway seal/open, rewrap |
| Rotation and persistence | `crates/wyrd/wyrd-auth/src/sealing.rs::SealedSecretRewrap`; `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs::SealedSecretTable`; gateway SQL queries and new migration | Boot rewrap, compare-and-swap updates, keyless boot, concurrent replicas |
| Local CLI lifecycle | `crates/wyrd/wyrd-cli/src/server/dev.rs::local_sealing_key`, `DevArgs`, `DevServer`; `server/install.rs` | Installation root, migrate/serve/setup children, explicit env overrides, restarts; `tests/pg_tests.rs::server_dev_keeps_the_gateway_managed_secret_key` |
| Harness and all user surfaces | `crates/wyrd/wyrd-testing/src/server.rs`; `mise.toml`; contract source/schema and modified docs | Rust/Python/TypeScript/CLI/HTTP Gateway journeys; actual task wiring; no SDK write capability should be invented |

## Authority

Read `.agents/skills/wyrd-task-review/SKILL.md` for your assigned role, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, and applicable references selected via `architecture/references/README.md`. Security and runtime boundaries also use `architecture/wyrd-security-posture.md`, relevant operations authority, and `TESTING.md`. Current user direction outranks workflow defaults.

All discovery reports need scope/source coverage, evidence, verification limits, material source-local findings with testable corrections, and PASS/FAIL/BLOCKED. Behavior and invariant reports need explicit obligation matrices. Standards needs an authority coverage table. Placement/style findings without consequence are non-blocking; missing substantive rustdoc on changed Rust items remains blocking. Exclude optional improvements and unrelated debt.
