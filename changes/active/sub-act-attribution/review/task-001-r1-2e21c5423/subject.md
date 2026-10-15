# Immutable cumulative TASK-001 / R1 review

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Original base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `2e21c5423036460d5da2ca22a51efde63635e352`.
- Full acceptance diff: `git diff b22718f2c 2e21c5423` (includes initial implementation commit d917a2751).
- Prior reviewed implementation: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Remediation locator diff: `git diff 69687f648 2e21c5423`; do not substitute it for cumulative acceptance.
- Current approved spec: `changes/active/sub-act-attribution/spec.md`, revision 6; approved r6 commit `785d8293e`.
- Original task: `changes/active/sub-act-attribution/tasks/TASK-001-sub-act-attribution.md`.
- Prior verdict/validated findings: `changes/active/sub-act-attribution/review/task-001-518a5d8b8/verdict.md` and `findings-validation.md`.
- R1: same prior directory, `TASK-001-R1-close-attribution-gaps.md`; its evidence, RED/GREEN, exact commands and diagnoses are reported verification, not acceptance authority.
- Spec r5 settled creator provenance and superseded FIND-TASK-001-3: ordinary missing/malformed/unverifiable bearer refusals remain unaudited; existing exchange-refusal auditing remains. Spec r6 settles local registered Workflow Card correlation under unchanged Card scope. Verify approval and exact behavior from spec/source yourself.
- Preserve stable prior FIND-TASK-001-1 through -9 IDs; new findings begin at -10. Prior closure needs current source evidence. Do not receive another current discovery reviewer's conclusions or any intended verdict.

## Common navigation starting map

This is a starting point, not complete coverage or conclusions. Expand against the full diff and callers. CodeGraph is indexed: use it before source navigation. Write only your exact assigned report in this new directory. Do not edit tracked source, prior reports, spec or task. No Cargo/runtime commands without orchestrator coordination; Cargo-backed work must be sequential across agents.

| Boundary | Owners and likely callers | Relevant proof |
|---|---|---|
| Verified request and authority | runtime AcceptedAuthority/delegated_attribution; server Caller; auth issuance/exchange; verification direct/service/eval; gateway invocation/workflow/ledger | realtime_subject_actor_authority; reloaded_authority_cannot_widen_gateway_access; queue retry/expiry evidence |
| Creation provenance | defining SYSTEM SQL and service_accounts wrapper; TenantProvisioning; BuiltinTables ensure/reconcile; BifrostCatalog ensure_builtin; OLAP upsert; fixture seeds | platform_admin_e2e new_and_existing_tenants_receive_every_builtin; pg_migration system creator test; callerless startup/reserved-owner paths |
| Workflow correlation and scope | shared client cards/hydrate/workflow.rs, workflow/mod.rs/local.rs and PublicWyrdGatewayCaller; server Workflow host/GatewayInvocation Card checks | Rust local/hosted/direct/delegated/refusal tests; Python/TS gateway-inference journeys; r6 |
| Background authority and durable queue | SQL verifier_runs; scheduler, observations, runner, BackgroundAuthority; accepted snapshot/history, replay and locks | observation-owner cases/diagnostic; scheduled_owner_authority_is_retained; queued_subject_authority_survives_restart |
| Audit context | IssueApiKey audit + auth route; CredentialIssuance typed detail; SystemReadAuthority::serving; Drift/Eval readers; Oracle query/audit context | issued_key_audit_keeps_delegation; delegated_input_reads_keep_history; shared read-context unit proof; retained audit |
| Ingest/results/spans/peer | Gate AuthContext, ScribeIngressFrame, managed arrays/table policy, claim sampling; outbox/peer capture; TenantSpanRoute | SDK reserved-field/row proofs, timeline/MCP tests, original V1–V11 evidence |
| Standards and public docs | all cumulative changed contracts/languages/tests; SQL capability types; required rustdoc; design/security/Bifrost docs and examples | r5/r6 authorities; docsite/codegen; served OpenAPI; focused support-desk SDK equivalents |

Required authorities: AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, bifrost-design.md, wyrd-security-posture.md, references/README.md, references/languages/spec-driven-development.md, maintainer-style.md, testing-workflows.md, and applicable slices through the router. Compare implementation-added authority with approved spec; neither task mechanics nor green tests can redefine behavior.

Initial evidence: clean tracked working tree, HEAD matched candidate, cumulative `git diff --check` passed. Review has not independently rerun runtime checks. If HEAD or tracked source changes, notify orchestrator immediately.
