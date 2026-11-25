# TASK-003A first acceptance review

Repository: /home/thorrester/Documents/GitHub/wyrd
Base: 43c77a219fd10554e8c8663e980788b70f1a6296 (43821f9d6^)
Candidate: c233b75456fad4f45c772ec242a9d115b4d35c6b
Branch supplied: task/wyrd-ui-main-integration/TASK-005
Range: git diff 43c77a219fd10554e8c8663e980788b70f1a6296 c233b75456fad4f45c772ec242a9d115b4d35c6b (base is ancestor; equivalent triple-dot first-review range).
Task: changes/active/verification-workflow-ui/tasks/TASK-003A-execution-status-events.md
Approved authority: changes/active/verification-workflow-ui/spec.md revision 4, REQ-015 / AC-010; design/verify-status-events.md subordinate to architecture.
Scope: complete cumulative range, no prior TASK-003A acceptance review or carried-forward evidence. Source immutable; reports only here; no commits.

## Navigation map

- Queue producer: wyrd-sql queries/verifier_executions.rs (CancelledExecution, Admitted, claim/release/retry/complete/cancel), pull_requests.rs (deactivate/supersede/repository cancellation). Consumers: runner, submissions, auth revocation, GitHub PR admission and connection services.
- Lifecycle: wyrd-server verification/runner.rs (claim timestamp and settle/apply/settle_alone), completions.rs (complete_many group commit and fallback), mod.rs (CapabilityCrash test seam). Claim owner and engine/state/checkpoint consumers must be traced beyond the diff.
- Commit owners: components/verification/service/inspection.rs, auth/revoke.rs, verification/submissions.rs, components/github/{pull_requests,service}.rs; transactional auth result in wyrd-auth/src/revoke.rs.
- Projection/sink: scribe_outbox.rs ScribeWrite::Status/status, stage_cancelled, rows/lane/weight and existing shared frames/retry/loss accounting. Managed attribution and checkpoint-schema owner are unchanged dependencies requiring inspection.
- Proof: wyrd-testing tests/bifrost/server/verification_runtime.rs public-query journeys; server integration pg_verification_runtime, pg_deferred_evidence, pg_github_pull_requests, pg_card_declaration_route, pg_verification_groups; SQL pg_verifier_executions; outbox inline unit test. Test-support wrapper changes in wyrd-testing/src/verification.rs and server test_support/boot.

## Contract coverage and reviewer selection

| Contract/invariant | Producers, owners, consumers | Selected roles and justification |
|---|---|---|
| Exact status lifecycle, row shape and newest-row public query | Queue mutation -> runner/commit owners -> Scribe -> public Bifrost reader | behavior-rev comprehensive task matrix; invariant-rev lifecycle, stale lease, cancellation and concurrency across owners |
| Atomic commit and asynchronous delivery/retry/loss/group fallback | TenantConn commits, Completions/Flusher, runner, shared Scribe outbox and checkpoint reader | system-rev: failure/recovery behavior, test stall resequencing, group fallback; domain-data-rev: SQL RETURNING, attribution and transaction identity; invariant-rev: sibling writers |
| Permission/revocation/admission fencing and tenant separation | auth revoke, principal fence, PR authority, TenantConn, managed Scribe attribution, query reader | domain-security-rev: materially changed revocation results and admission/cancellation event propagation, unchanged permission boundaries must remain intact |
| Rust/cross-crate ownership, rustdoc, test support, SQL capability rules and gates | auth, server, sql, testing crates; all Rust and task evidence | repo-rev: broad cross-tier diff and changed-item rustdoc, SQL nested AGENTS, gate applicability |

maintainer-rev omitted: no substantial new lifecycle owner or abstraction replaces existing ownership. ScribeWrite adds a narrow payload variant and SQL adds concrete result records at existing owners, as the design requires; behavior-rev and repo-rev still inspect reuse, shape, documentation and maintainability. No new frontend, wire/API/schema, SDK, migration or deployment surface; corresponding independent specialists omitted. Data/concurrency specialist is one coherent transaction/persistent-state boundary; security specialist is separate.

Every reviewer receives this same map as a starting point, not exhaustive coverage. Each independently expands against actual source, original task and authority. Reviewer reports must not consume another discovery report.

## Verification handling

Task's acceptance records and commands are claims to inspect. TDD compliance is separately reported, with historical violations retained. User notes owner lane 126/126 passed before final test-only commits a53a91094 and c233b7545; only affected tests rerun afterwards. Review will attempt owner lane on exact candidate. Cargo-backed commands run sequentially, orchestrator only; discovery agents do static review and request focused checks. Hot-path applicability independently assessed against repository definitions; if touched, wait inventory and clean VALID benchmark comparison are mandatory.
