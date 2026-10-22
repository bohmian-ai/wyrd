# Immutable review subject

Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry
Base: f38dff803 (resolve full SHA via git rev-parse).
Candidate: 0a29bae4f4ff20e98d2c7937f3943877ac31bc62.
Range: git diff f38dff803...0a29bae4f4ff20e98d2c7937f3943877ac31bc62 (435 files).
Spec: changes/active/service-identity-and-telemetry/spec.md, approved revision 12 and normative supplements.
Original task: changes/active/service-identity-and-telemetry/tasks/TASK-001-flat-card-identity.md.
First task review; no prior findings/remediation.
No .codegraph directory exists. Source was clean at start.

# Common navigation map (starting points, not coverage limits)

- Contracts: crates/wyrd-spec/src/{reference.rs,metadata.rs,envelope.rs,ids.rs,registry/,graph/}, CardRef/identity/scope, metadata, canonical hashing, graph and query DTOs. Consumers include all authoring wrappers and runtimes.
- Authoring/config/loader: crates/wyrd/wyrd-config/src/{config.rs,merge.rs}, crates/shared/wyrd-loader/src/{parse.rs,config.rs,resolve.rs,validate.rs}, crates/wyrd/wyrd-cards/src/{card_ref.rs,identity.rs,data.rs,model.rs,prompt.rs,agent.rs}. Follow declaration, defaults, path/inline/external references.
- Persistence: crates/wyrd/wyrd-sql/migrations/20261024000000_flat_card_identity.sql; src/queries/cards/{declare,get,list,delete,relationships,version_sql,version_query,version_resolve,auth_projection,field_resolver}.rs, row_types/cards.rs, queries/auth/service_accounts.rs and verifier_executions.rs. Trace unique keys, hashes, locks, lifecycle, RLS and tenant transactions; fresh-only baseline and immutable applied migrations.
- Server: crates/wyrd/wyrd-server/src/components/cards/{routes,service,resolve,mapping}.rs; mcp/{cards,guide}.rs; auth/card_scope.rs; verification/hosted/registry.rs. Consumers: HTTP, MCP reads, OpenAPI and hosted verification.
- Client/state/pull: crates/shared/wyrd-client/src/cards/{handle.rs,reads/,saga/,pull/}, state.rs. Follow Cards, CardSelector and GraphPuller into exact reads, latest/version queries, bundles and offline states.
- Auth/runtime: crates/shared/wyrd-auth-{issue,verify}/src/lib.rs; wyrd-runtime/src/{principal,request_context}.rs; crates/wyrd/wyrd-auth/src/; server auth/boot; SQL service account projection. Identity closure is in scope, account-behavior redesign belongs to TASK-002.
- Public surfaces: crates/wyrd/wyrd-cli/src/{card.rs,auth/issue_key.rs,auth/workload_binding.rs,init.rs}; sdks/wyrd-sdk-python/{src/state/mod.rs,python/wyrd/**/*.pyi}; sdks/wyrd-sdk-ts/{native/src/cards.rs,wyrd/src/index.ts,wyrd/src/card-types.ts,generated declarations}.
- Tests: reference::tests::flat_card_reference_contract, config::parse::removed_space_defaults_are_rejected, parse::tests::flat_card_graph_loads_without_space, pg_cards_declare::flat_registry_identity_is_tenant_and_kind_scoped, declare_and_pull::flat_card_identity_round_trip (Rust), Python/TS declare-and-pull journeys, CLI card_lifecycle flat journey and mcp_journey, server pg_card_declaration_route::flat_card_http_contract, identity_e2e. Check actual coverage rather than names.
- Consumer closure: fixtures/cards/, examples/, SDK fixtures, Skald/Vala/Bifrost consumers, UI shared imports. TASK-008 owns final docs prose; runtime guides and executable snippets remain TASK-001.
- Authority: root and nested AGENTS.md (docs, SQL, Python, TS), architecture/README.md; wyrd-design.md Card/reference/declaration/config sections; wyrd-security-posture.md security/tenant binding; operations references for migration/boot/recovery; agent_docs/index.md and applicable topics, maintainer-style.md; TESTING.md and changes/spec-driven-development.md.

# Verification and review discipline

Available recorded results are in original task Implementation Evidence and commit history, not independently reproduced yet. Assess their credibility and failure diagnoses. Do not assume completion from those assertions. Do not read user completion prose as evidence. No overlapping Cargo-backed commands; ask orchestrator before any build/test command. Read-only inspection requires no coordination. Never edit source, task, authority or another reviewer's reports. Write only your exact assigned report. Every reviewer reads actual full cumulative diff independently and expands navigation map. Check candidate SHA has not changed. Follow complete role requirements in .agents/skills/wyrd-task-review/SKILL.md. Findings need exact source evidence, realistic consequence, violated obligation and smallest testable root correction; no optional improvements or unrelated debt. Missing rustdoc on changed Rust items remains blocking. Do not browse unless a technical standard claim needs verification.
