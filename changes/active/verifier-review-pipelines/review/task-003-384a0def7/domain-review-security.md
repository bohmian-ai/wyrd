# TASK-003 security and tenancy domain review

Verdict: **PASS**. No material security/tenancy finding proposed.

## Subject and scope

Fresh review of the complete immutable range `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355...384a0def73ed6eac4041dc1e428dc30e981514fe`, including unchanged dependencies consumed by new orchestration paths. No prior acceptance or other reviewer conclusions were used. HEAD matched the candidate, tracked implementation directories had no candidate-relative diff, and all eight pinned authority hashes passed `sha256sum -c`.

Assigned boundary: tenant/permission preservation across dynamic execution, evidence selection, speculative exclusion, inspection, recovery, notification and error details. Revision 22 adds no tenant quota/reservation requirement. Repository-specific features belonging to later tasks are not newly required here.

## Authority and source coverage

Applied root AGENTS.md and `crates/wyrd/wyrd-sql/AGENTS.md`; architecture/README.md; security posture sections on security principles, accepted Verifier authority, access tokens and tenant isolation; `wyrd-design.md:1491-1510,1762-1796`; `bifrost-design.md:455-500`; approved TASK-003/spec r18 plus approved r22; public-contracts.md Execution authorization; output-publication.md collection/speculation contracts; execution-state-and-durability.md recovery; spec-driven-development.md; maintainer-style.md and the task-review skill.

CodeGraph preceded code location. Inspected the complete changed security-relevant production diff, then traced VerificationEngine drive/step/disposition, readiness, Scope, HostedBlocks, AgentExecutor, Workspaces, TenantEvidence, Publications, RunState, dedicated inspection, VerifierRunner, LeaseRenewal and SQL renewal. The Scribe diff changes contradictory retained-payload handling to an isolated refusal and grants no additional authority.

## Acceptance matrix

| Obligation | Source and assertion evidence | Result |
|---|---|---|
| Ordinary consumers receive no pending/discarded/blocked bytes | `engine.rs:877-975` waits for control acknowledgement; `:1013-1037` removes excluded output/member; `:1918-1967` requires decided prerequisites. `scope.rs:356-393` requires successful selected producer for names. `workspace.rs:131-165` mounts no content for withheld collections. `engine/tests.rs:3246-3274` asserts excluded output/named consumers never execute and return no output. | PASS |
| Retained inspection requires requesting caller authority and cannot admit bytes | `inspection.rs:287-375` prechecks permission, opens caller-tenant transaction and authorizes exact recorded Verifier. `:638-676` selects full producer tuple after authorization. Projection reads disposition without mutation. Rust journey `verifier_orchestration.rs:1158-1170,1193-1249` gives only verifier:read, asserts excluded collection entries empty, reads retained excluded JSON, then asserts disposition unchanged. | PASS |
| Exact accepted File association remains tenant bound | Named mount compares full Producer plus name (`workspace.rs:108-127`); collection reader is bound to the execution RunState. `TenantEvidence::published` invokes `published_object`; `publications.rs:412-428` checks manifest digest/length and validates tenant evidence path. Download/local content select the authorized exact manifest association through `inspection.rs:518-673`. No new digest rediscovery or caller-selected tenant/path authority. | PASS |
| Dynamic calls and recovery retain accepted identity/permissions | `HostedBlocks::engine:139-199` passes the same caller to evidence, registry, tools, gateway and traces. `runner.rs:802-838` restores controls into the same engine under Caller::accepted. `RunState::accepted:603-621` errors on missing required accepted state; `durable:900-939` confirms lease before freezing a revision. Existing `pg_durable_verifier_work.rs:3725-3738` asserts revocation fences computation with StaleLease; following cases retain principal_revoked cancellation. No new grant resolver or authority widening. | PASS |
| Retained reads remain protected after queue/Card deletion | `pg_durable_verifier_work.rs:2639-2683` asserts cancelled status/no result after deletion and 403 for an ungranted reader. Accepted historical authority is provenance; requesting caller's current verified token permissions authorize reads. Token snapshots until expiry remain the documented policy, not a new live-role refresh requirement. | PASS |
| Notification cannot select another tenant or manufacture lease authority | Trigger pins empty search_path and emits after running-to-cancelled update commit. `leases.rs:157-171` ignores payload and wakes renewal. `:183-238` derives tenant/token pairs from held leases and renews via tenant_conn. SQL `VerifierExecutionQueue::renew:2973-2989` takes TenantConn and held tokens. Notification grants no read/write/cancel authority; ordinary tick remains fallback. | PASS |
| New cancellation error preserves authorization and sanitized details | Shared client `execution.rs:320-329` maps an authorized terminal status read to cancellation with handle execution_id/status. Status-read errors propagate first; server retained/cancel paths remain exact-Verifier/tenant authorized. No token, private descriptor or foreign row appears in the added details. | PASS |

## Findings and verification limits

Finding ledger: empty. No additional discarded-content permission, reader role, tenant quota, grant-refresh service or evidence resolver is needed.

Static review only: tests/Cargo were not run, as assigned. Cited assertions were inspected; recorded implementation runs are supporting claims, not independently rerun proof. Full crash/concurrency/resource proof remains with the assigned invariant/system/concurrency reviewers. No source edits; only this report was written.
