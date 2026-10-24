# Immutable review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-002
Base: 036fc48fe26fb87648a277b10099bc475d293b92
Candidate and branch initial HEAD: b1ff72dc34c344b596c85cf408e6a9217685be35
Branch: task/service-identity-and-telemetry/TASK-002
Scope: cumulative TASK-002 plus supplemental TASK-009 acceptance, one integrated review. Read complete `git diff 036fc48fe...b1ff72dc3`; TASK-009 starts at 43cdb7405, but latest-only review is insufficient. No previous review of these tasks exists. TASK-001 and TASK-004 reviews are unrelated historical baselines.

Approved authority is current working-tree spec revision 15 and supplements/tasks/architecture, including uncommitted approved plan revisions. Their initial SHA256 hashes below freeze the authority independently of source commits. TASK-002 historical wording is superseded only where TASK-009/spec15 explicitly change it. Task metadata proposed/review does not override caller's explicit implementation review request. Table schema/projection/storage/readback and Task003 work are deferred; context/accepted authority/client APIs and transport are in scope. Do not require Task003 implementation here.

Source is clean at candidate; uncommitted changes are authority/plans and unrelated new verifier packet. Review agents may write ONLY their assigned report; do not mutate source, tasks, authority, git state, or generate build outputs into tracked files. Use git show for committed source if necessary. Notify orchestrator if HEAD/branch changes. No fixes, commits or broad aggregate suites. Verification evidence is in the tasks' appended evidence sections; independently assess it, do not treat PASS claims as proof. No implementation-summary narrative is supplied.

## Shared starting navigation map

Expand this map independently; it is not coverage or a conclusion. Use CodeGraph first (`mise exec -- codegraph explore ...`), then targeted rg/source reads. Commands through mise; read applicable nested AGENTS and agent_docs.

- Contracts: wyrd-spec auth/workload.rs, auth/token.rs, auth/tenant_principals.rs, error.rs, verification/execution.rs, card/verifier/mod.rs. Claims and restrictions, public errors, environment selectors. Consumers every SDK and server ingress.
- Auth owners: wyrd-auth issuance.rs, exchange_api_key.rs, card_scope.rs, jwt_bearer.rs, refresh.rs, service_accounts.rs; auth-verify lib.rs, auth-oidc registry.rs; server auth/token_extract.rs, caller/principal extractors, oauth.rs/routes.rs; gateway credential cache/request admission.
- Tenant persistence: wyrd-sql queries/auth service_accounts/principal_directory/workload_bindings, queries/cards/auth_projection deleted, verification.rs, verification_target.rs, verifier_executions.rs; two new migrations stable accounts and env. Account unique scope, authentication activity, RLS and atomicity.
- Runtime context: shared runtime principal.rs/request_context.rs; client auth.rs/client.rs/state.rs/transport/http.rs/bifrost/scope.rs/grpc.rs/observe/verify; immutable state config, root selection, caching, renewal, delegation, exporters.
- Accepted verification: server components/verification/service.rs; verification authority/cache/runner/scheduler/submissions; Bifrost scribe execution_lanes/ingress/replay. Binding occurrence/environment and originating authority vs fresh scheduled resolution.
- Public clients: Python src/state/observe/client/verify/principals and python/wyrd/client/_http_auth.py, otel.py, authored stubs; TS native cards/client/principals and wyrd/src/index.ts/otel.ts plus generated declarations; Rust SDK otel.rs. Run context ownership, provider SDK adapters, telemetry lifecycle.
- CLI/MCP account/restriction/federation surface, old issue-key deletion, fixture changes.
- Tests: task evidence exact selectors; principal roles SDK Rust/Python/TS journeys, observe_a_run configured journeys; gateway HTTP auth provider tests + unit adapter tests; server pg_principal_roles and pg_verification_runtime; auth issuance/delegation; otlp negative; shared WyrdTestServer::exchange_rooted fixtures; migration and SQL tests. TESTING.md gives runners.

## Authority checksums

- `AGENTS.md`: `a269bd5842658a17258d9514e5eb292627d5abd6806b4449bb1f340e21d4e75f`
- `changes/spec-driven-development.md`: `a6a8843b9e192832739141da88d2dbdbc6c9a4b114f308fff3391aaad8db3a27`
- `agent_docs/maintainer-style.md`: `3a180f870ef7a03edc4f4af52fd4fe90c7e0a51f532b0e244fca9a280fb859ba`
- `changes/active/service-identity-and-telemetry/spec.md`: `507e89ed11e7c09a5e4faaff9e2679032c9de9158bfe874ac417e1886b2a00ef`
- `changes/active/service-identity-and-telemetry/tasks/TASK-002-stable-accounts-and-auth.md`: `4912540517ff37355803b2e7ed6311ba213e8f2cef83a5478f8c3cf75240d73e`
- `changes/active/service-identity-and-telemetry/tasks/TASK-009-configured-service-context.md`: `bd94961998951c76b1e05039066df504579247ce44f845131466cd561b46caa5`
- `changes/active/service-identity-and-telemetry/supplements/auth-system.md`: `68ba1370c40fc8c003dafdd662842a2a4960ffc16d9f91dc189fb1e483998140`
- `changes/active/service-identity-and-telemetry/supplements/table-storage-and-correlation.md`: `619423f583b7bfa1b6ce9e2b5d37b39dd6bab01b1fd985938588c5f61c25bb30`
- `changes/active/service-identity-and-telemetry/supplements/telemetry-schemas.md`: `537f25e99e87c25e1f938c94212441a4c50e6c3d3ebbedfe3bbaf8ca6fd2b16c`
- `architecture/wyrd-design.md`: `1629b393004229b52123de6cc815b0646c3bef421a0e642a499034e5a4e89724`
- `architecture/wyrd-security-posture.md`: `dc03d8092a0069b82084aac829589d3c7acaafbd44af74488f6cf794fec192a4`
- `architecture/bifrost-design.md`: `23b1c295c8efb15211e1b2d951675349275c5fc5a9960a6117920d6344027435`
