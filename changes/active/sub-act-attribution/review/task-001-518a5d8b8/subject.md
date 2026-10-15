# Immutable review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`
- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f` (parent of first implementation commit d917a2751; include that commit).
- Candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Full diff: `git diff b22718f2c 518a5d8b8`; inspect all changed paths, not just the latest fixes.
- Approved specification: `changes/active/sub-act-attribution/spec.md` revision 4.
- Original task: `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md` (original at base, execution evidence appended at candidate).
- This is the first review of this task. Prior verify-attribution reports are prerequisite evidence, not prior findings for this task.
- Verification results are recorded in the task's Implementation Evidence, Exact commands, and Diagnoses sections. Treat results as reported evidence, not acceptance authority. No review runtime checks have yet run.

## Navigation starting map

This map is a starting point, not coverage or conclusions. Expand against the full diff and callers. Use CodeGraph first (repository is indexed); then targeted reads/searches. Do not change implementation or run Cargo-backed commands concurrently; request orchestrator coordination for runtime proof.

| Surface | Owners and paths | Consumers and proof |
|---|---|---|
| Verified request, delegation and authority | runtime/request_context.rs; server components/auth/caller_extractor.rs; wyrd-auth issuance/exchange/card_scope; server verification/authority.rs | verification service/direct/eval/drift; gateway/workflow; workflow/host; realtime_subject_actor_authority |
| Queued/background authority, leases, replay | wyrd-sql queries/verifier_runs.rs and defining migrations; verification scheduler/observations/runner | queued_subject_authority_survives_restart; background_owner_subject_system_actor; pg_verifier_runs |
| Entity creators and operation replay | wyrd-sql queries/cards/register.rs, verification bindings, operator connections, auth/service_accounts; defining migrations | server cards/principals/platform; pg_cards_register; fixture seeds |
| Managed ingestion and queries | Bifrost Gate/AuthContext, ScribeIngressFrame, scribe execution_lanes/claim_sample, table managed_columns; queue batch_builder | shared client Bifrost/observe/state; HTTP/gRPC/peer forwarding; all SDK reserved-column journeys |
| Audit, result and trace attribution | server audit, scribe_outbox, verification/results; shared telemetry export TenantSpanRoute | tenant/platform/unauthenticated decisions; peer transport; Rust/Python/TS/MCP timeline tests |
| Public contracts and projections | wyrd-spec auth/attribution, verification, vala/api, operator; SDKs; architecture; docs; examples | schemas/codegen, authored Python/TS projections, query docs and sample queries |

Required authorities: AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, architecture/bifrost-design.md, architecture/wyrd-security-posture.md, architecture/references/README.md, references/languages/spec-driven-development.md, maintainer-style.md, testing-workflows.md, plus applicable slices through the router. Compare changed authority with approved spec and base authority; implementation cannot silently redefine approval.

Review artifacts only may be written under this new review directory. Candidate tracked source must stay immutable.
