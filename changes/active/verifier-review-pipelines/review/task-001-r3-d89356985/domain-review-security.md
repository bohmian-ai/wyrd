# Security, RBAC and tenancy review

Verdict: **FAIL**. Finding16 is closed within this domain; FIND-TASK-001-17 remains open for principal-triggered cancellations. No new finding ID is proposed.

## Subject and coverage

Candidate `d89356985566ab3492699682942438c25c8322fd`, prior candidate `cd905f4cd929785bda25696326d58b4a4ae88dee`; HEAD matched candidate during inspection. Read actual remediation diff and affected surrounding owners/callers, original task, R3 remediation, review skill/subject, relevant durability supplement, security posture lines 253–280, workflow/maintainer guidance, and SQL AGENTS. CodeGraph was consulted first. Scope: retained inspection authorization, immutable identity, publication custody, cancellation/revocation siblings, and tenancy. No TASK007 or new approval policy.

Prior findings16/17 and unchanged storage/authority evidence were used as navigation and acceptance context, then affected paths were independently inspected. No other new discovery report or implementation completion prose was read.

| Obligation | Implementation evidence | Verification source inspected | Result |
|---|---|---|---|
| Exact retained identity survives queue/Card deletion | `ExecutionState.identity`; `RunState::at` keeps recorded identity; `inspection.rs:334–380` authorizes recorded Verifier UID without live Cards; `service.rs:232–274` uses recorded references and canonical selected result | New retained cancellation/failure tests in `pg_durable_verifier_work.rs:1776,1839`; existing terminal/publication recovery proofs | PASS |
| Status, pages, JSON publication, download and local content require requesting caller's exact feature grant | `require_verifier` rejects no-grant callers before reads; `retained` audits exact UID permission; `accepted/select/file/download/content` share it; `get_execution` shares `retained` | `status_reads_audit_their_decisions`; `local_publication_downloads_redeem_under_verifier_read` (exact, unrelated Verifier, cards-only grants); retained cancellation negative read | PASS |
| SYSTEM serving does not grant caller authority or cross tenants | `service.rs:1055–1084` grants four registered verification tables only; `SystemReadAuthority::resolve` fixes tenant and table UID; `InputResolver::state` builds fixed typed execution/revision query; queue and batch fences use caller data tenant TenantConn | `verification_state_is_tenant_isolated`; independently traced no-queue branch through batch fences and Oracle context | PASS |
| Cursor is no authorization token | `position/page` bind tenant/execution/stream and fixed cut; each invocation/publication page reenters `accepted` and reauthorizes | Cursor/page source; unchanged page mechanics carried forward | PASS |
| Exact accepted File descriptors retain custody | `inspection::file` selects accepted producer/attempt and objects; `published_object` checks digest/length and `tenant_path::validate_evidence`; caller supplies manifest selector, never storage path | Existing local publication download and exact-descriptor recovery proofs; unchanged validation source rechecked | PASS |
| Explicit cancellation retains canonical error and preserves fencing | `inspection.rs:687–797` authorizes Run, commits existing queue fence, then records failure through RunState/outbox without stale lease | New `cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone`; existing cancelled-work lease proof | PASS for implemented healthy path |
| Principal cancellation retains canonical terminal facts, including no-checkpoint executions | Revocation/admission call SQL-only `cancel_principal`; no canonical writer invocation | Existing revocation SQL tests prove coordination only; new retained cancellation test covers explicit cancel only | FAIL: finding17 |

Current grants follow the existing signed tenant-token snapshot contract, including five-minute expiry; no authorization epoch is required. Existing presigned URL TTL semantics are unchanged. Local content reauthorizes per request.

## SEC-R3-01 / FIND-TASK-001-17 — MISSING: principal-triggered cancellations remain SQL-only

**Obligation.** R3 finding17 explicitly includes user/principal cancellation and outcomes with no prior checkpoint, from the actual lifecycle owner. The durability supplement preserves cancellation decisions; queue history cannot remain sole retained terminal/error custody. Security posture requires suspension and queue cancellation in one commit.

**Locations.** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:3174–3192` (`cancel_principal`) and `:2049–2071` (`fence_principals`); `crates/wyrd/wyrd-auth/src/revoke.rs:50–115`; served caller `crates/wyrd/wyrd-server/src/auth/revoke.rs:108–119`. Contrast new explicit cancellation producer `components/verification/service/inspection.rs:703–730,743–797`. Consumers: `Retained::status` at `inspection.rs:231–244`, `retained` at `:357–368`.

**Producer-to-consumer proof.** Submit/claim a run, accept an ordinary invocation checkpoint, then revoke its subject or accepting principal through the supported revoke endpoint. `revoke_principal_in_conn` suspends the principal and calls `cancel_principal` in its transaction. That method updates only the queue (`cancelled/principal_revoked`) and returns a count. The endpoint commits and returns without staging canonical cancellation. Renewal detects the cancelled lease; `runner.rs:1026–1031` selects `LeaseLost`, and `settle` at `:765–767` returns without a write. While the queue exists, inspection borrows its status/error. Remove its coordination row and Cards: retained checkpoint has neither terminal nor failure, so `Retained::status` answers `running` and loses `principal_revoked`. Before a first checkpoint, removal instead leaves no identity/state and returns not-found.

**Sibling coverage.** Explicit cancel now calls `retain_cancellation`. Principal revoke does not. Delayed admission for an inactive principal calls the same SQL-only `cancel_principal` via `fence_principals`, including submitted/scheduled/group admissions. A later explicit cancel cannot repair it: its SQL update returns false and canonical retention only runs on true. These are one existing finding17 cause. Queue fencing remains effective; this is lost required history, not a claim that unauthorized work continues.

**Materiality.** Supported principal revocation loses a required accountability fact when coordination records disappear, publicly representing terminated work as running or absent. This directly violates the approved historical read/cancellation obligation.

**Minimum correction.** Connect principal-cancellation lifecycle ownership and inactive-principal admission to the existing canonical failure writer, carrying affected execution identities and `principal_revoked`. Keep suspension and queue cancellation in their existing transaction; use the actual owning caller after commit for the canonical write, without restoring cancelled worker authority. Reuse RunState, existing reserved Scribe writer and recorded identity. No journal, finalizer, SQL head, new fencing protocol or weaker revocation is warranted. A read fallback cannot preserve the fact after queue deletion; endpoint-only wiring misses admission's sibling producer. This is bounded completion of the explicitly required producer wiring, not a new security decision.

**Focused proof.** Extend the existing retained cancellation seam to revoke the accepted subject/actor, remove queue and Cards, then read `cancelled/principal_revoked` and no result with a currently authorized reader. Cover no-checkpoint cancellation and the delayed inactive-principal admission seam; retain queue atomicity/lease fencing assertions. Verify absence of the exact read grant still refuses disclosure.

## Limits

No Cargo, database or test command was run, per reviewer instruction. Test references are inspected source, not fresh execution claims. Only this report was written. No optional improvements or unrelated security findings are proposed.

Skipped unrelated security surfaces and approval machinery. Missing principal-cancellation history remains a material risk pending independent validation and root verification.
