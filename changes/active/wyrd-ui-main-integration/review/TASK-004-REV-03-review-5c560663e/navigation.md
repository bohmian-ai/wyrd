# Review subject and starting navigation
Repository /home/thorrester/Documents/GitHub/wyrd. First full review, no carried-forward acceptance.
Base e0354f9bdf8608046833029252d8a526e0bd999d; candidate 5c560663e77ae07c33d198862ba6d795b656097a; branch task/wyrd-ui-main-integration/TASK-004. Includes merged Observe Scenario 5. Read git diff e0354f9bd...5c560663e directly. Implementation immutable; reports only.
Spec changes/active/wyrd-ui-main-integration/spec.md revision 19.
Task changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md.
Read all relevant obligations and actual authority independently. Task completion evidence is claims only.

Starting map, expand independently:
- Contracts wyrd-spec/src/auth/{browser,tenant_principals}.rs, error.rs; runtime permission.rs and generated projections.
- Identity wyrd-auth/src/{passwords,browser_sessions,issuance}.rs: existing BrowserSessions, HumanConnections, TenantTokenIssuer and current grants.
- Persistence wyrd-sql migration 20261102000000 and queries/auth/{local_passwords,users,browser_sessions,api_keys,service_accounts}; TenantConn, existing grants and maintenance callers.
- HTTP server/src/auth/browser.rs, components/principals/{accounts,routes}.rs, middleware/authenticate, error, main setup. Configured auth and session provenance.
- MCP server/src/mcp/{principals,mod}.rs. wyrd-client Principals, Python/TS projections, testing bootstrap_user_key and all language principal_roles tests.
- UI Entry.svelte, server/form.ts, browser-sessions.ts, hooks; login/signup/reset/settings/account loaders/actions/pages, identity menu. Trace secret POST to transient response and discard.
- Observe shared Chart, TraceWaterfall, plot; observe host/layout/page, brand palette/generator and projections. Read references/task-03-observe-audit.md and six canvas boards. Geometry/themes/interaction plus recorded-points inspector.
- Dev main setup, scripts/postgres/dev-server.sh, mise dev:full and ui-pages integration lifecycle.
- Proof task V1-V12 exact selectors, TESTING.md and mise tasks; pg_browser_sessions, SDK principal_roles, MCP principals, production-auth and ui-pages integration, library/page and theme tests.

Authorities: root AGENTS; nested SQL/Python/TS/docs AGENTS; changes/spec-driven-development.md; agent_docs index and topic guides; maintainer-style; architecture README then Human sign-in/Client model and security API keys/browser sessions/permissions/Postgres binding. UI review reads UI skill.

Reviewer selection: behavior comprehensive acceptance; invariant shared state/transactions/lifecycle; repo broad cross-tier gates; maintainer ownership/reuse; security domain auth/RBAC/tenancy/secrets; system KDF/replicas/outages/dev failure propagation. Fresh validation follows reports.

Only orchestrator runs Cargo or build/generator/check commands sharing target. Request needed commands; do not run them independently. Pure nonmutating checks permitted. Never print/record secrets; do not capture raw dev:full output. Reports only; no implementation edits, commit or push.
