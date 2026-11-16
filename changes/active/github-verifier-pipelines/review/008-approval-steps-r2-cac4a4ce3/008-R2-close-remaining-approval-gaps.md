---
id: TASK-008-R2
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 8
parent_task: TASK-008
remediates: [FIND-008-3, FIND-008-4, FIND-008-8]
---

# Preserve the original approval ceiling and prove delegated acceptance

An approver must receive a decision receipt only for a still-eligible occurrence under its original execution ceiling, with the actual authenticated actor retained. Complete the remaining deadline and delegated-proof gaps and remove the nested connection acquisition introduced by the first remediation. Use `$wyrd-implement`; no new product or protocol decision is needed.

Approved authority: `changes/active/github-verifier-pipelines/spec.md` revision 8, REQ-010/011/015/017, INV-003/005; `changes/active/github-verifier-pipelines/contracts.md`, Human approval. Original task: `changes/active/github-verifier-pipelines/tasks/008-approval-steps.md`. R1 remediation: `changes/active/github-verifier-pipelines/review/008-approval-steps-r1-798b6b0e8/008-R1-close-approval-acceptance-gaps.md`. Review base / prior candidate: `798b6b0e83595356c0d2328fa541119267e84fb2`. Reviewed cumulative candidate / new remediation baseline: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`. Independent evidence: sibling `findings-validation.md`, domain reports and `verdict.md`. Preserve prior finding IDs; FIND-008-1/2/5/6/7 are closed and need no reimplementation.

## Exact target refusal must not become a longer execution ceiling — FIND-008-3

The production ready-target correction and its stopped-worker test are valid. The remaining INCORRECT branch is `crates/wyrd/wyrd-server/src/components/verification/service/approvals.rs:132–150`: `VerifierCardCache::resolve` returns an outer SQL result and an inner target/readiness result, but `.ok().and_then(...)` erases an inner refusal. A refused/deleted exact target is treated like a successfully resolved Verifier with no authored timeout, selecting the longer runtime duration.

Reachable consequence: park an execution with a short authored timeout under a longer runtime ceiling, stop the worker, then make its exact subject/Verifier unavailable through ordinary Card lifecycle. The accepted occurrence and unfinished queue row remain. Retained identity/permission checks do not restore the unavailable target. After authored expiry, the new lookup refuses, its refusal is erased, SQL OPEN sees the longer duration, and a fresh positive durable receipt can be accepted. This is the original expiry failure under an additional supported lifecycle state, not a cross-store ACK race. Green ready-target proof does not cover it.

Preserve the distinction at the existing resolver consumer. A fresh decision whose exact target cannot resolve must refuse; use the existing `WyrdError::VerificationTargetNotFound` mapping and safe exact-target details as `VerificationControl::resolve` does in `components/verification/service.rs:963–971`. Only a successfully resolved target may supply an optional authored timeout; absent authored configuration is the only case that legitimately uses the runtime ceiling alone. Keep the accepted exact UIDs and the shared `Target::authored_timeout` minimum bound into the existing PostgreSQL clock predicate. Preserve already accepted identical replay before fresh-resolution eligibility. Do not create a stored deadline, second target reader, policy refresh, clock anchor or new public code.

The smallest correction is handling the existing inner result rather than swallowing it. It needs no new lifecycle service and must not claim an unavailable target reveals an exact expiration timestamp. For a ready target the existing typed 409 after actual authored expiry remains; for unavailable target use the existing typed target refusal and prove no new decision was accepted.

## A fresh decision must not hold a connection while acquiring a second one — FIND-008-8

REGRESSION at the same approval admission owner, `service/approvals.rs:122–145`. The handler takes a `TenantConn`, then calls target resolve with `Db::Pool` of that same `WyrdPostgres`. On a normal cold cache or disconnected invalidation listener, `verification/cache.rs:276–307` calls `postgres.conn().await`. Both paths acquire from the same app pool (`wyrd-sql/src/postgres.rs:182–201`). The first connection remains held until the later decision-open commit.

With one serving slot free, a cold decision takes it and waits for another until acquisition timeout. Concurrent cold decisions can occupy all spare connections and wait on one another. This breaks ordinary approval after restart/cache invalidation under supported bounded pool capacity and unnecessarily ties up transactions; warm-cache green journeys miss it. No malformed input or synthetic host load is necessary.

Reuse the existing `Db::Conn(conn.transaction())` resolver variant so the tenant connection already owned by decision admission performs the exact-target lookup and existing open-for-decision predicate. The runner already uses caller-owned resolution; the cache owner supports it. This removes the nested resource acquisition without a second pool, reservation, semaphore, retry, lock, helper owner or changed durable protocol. Keep tenant binding, exact UIDs, timeout minimum, short transaction/commit, ACK-before-receipt and best-effort wake behavior. A smaller change that leaves a second acquisition cannot protect the invariant; additional capacity machinery is unjustified.

Use the existing approval/pool integration owners to force a cold target resolution with only one serving connection available. Show a normal decision succeeds without waiting for another connection, and retain authored-timeout enforcement. Constrain pool capacity/connection availability using existing controls, not host load or an unrelated new harness. Account for existing listeners/fixture connections explicitly so the test exercises this admission owner rather than failing startup.

## Prove the corrected actor through ordinary delegated acceptance — FIND-008-4

The source correction at `service/approvals.rs:163,227` is correct and must stay. The remaining MISSING obligation is R1's required observable closure: ordinary real token exchange, accepted/persisted receipt and inspection naming actor A, exact A retry, and typed conflict for authorized B sharing effective subject S and direct S.

The new unit test at `service/approvals.rs:261–326` manually builds both Caller and expected receipt, then calls only `replay`. Reverting production receipt creation to subject identity would not fail it. It proves neither authentication nor retained receipt/inspection. The earlier explanation that a Service cannot root at a Repository concerns only the selected PR fixture, not all supported queued approvals. Spec REQ-015 permits manual queued approval; the generic enqueue route accepts Service subjects. Existing `pg_verification_routes::enqueue_admits_only_signed_scope_subjects` combines real Service credentials, exchange and queued request scope, and `WyrdTestServer::credential_registered_service`, `bootstrap_service` and `delegate` provide the existing mechanisms.

Use the existing server approval/identity integration owners with a registered Service subject and an approval-only queued Verifier. Obtain valid actors through ordinary real token exchange, with permissions that authorize each request against the effective subject and exact Verifier. Drive a pending occurrence, accept A's decision, read retained inspection, repeat A's exact request, then send the same decision/reason as authorized B sharing S and as direct S. Assert durable receipt and inspected actor A, exact receipt retry, and existing `WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING` conflicts. Keep direct and wrong-scope refusal proof. Do not change auth semantics, invent a credential/actor profile, add a Caller injection hook or weaken scope checks. This is bounded connected test work; the isolated unit test can remain but is not its replacement.

## Preservation and exclusions

Keep the five closed findings' behavior: reached pending identity while independent work runs; accepted-state history and withdrawn refusal; one live state writer and asynchronous background checkpoint recording; same-execution early-decision continuation; mandatory helper docs and accurate approved/rejected owner contract; safe unavailable diagnostics. Keep stable occurrence IDs, accepted source/policy/authority, no budget or deadline replenishment, all-blocked compute release, expiry/rejection as defined data, first-wins Scribe acceptance, durable ACK before receipt, identical replay and conflict, tenant/auth/audit, supersession/cancellation and bounded park recovery.

No UI, SDK/API extension, schema/migration, new pool/workflow owner, approval timer, recent-login requirement, token shortcut, new stored deadline or stronger cross-store atomicity. Do not reopen full task discovery or implement speculative hardening. The two admission findings share the existing resolver call boundary and should be corrected together without parallel machinery.

## Acceptance and focused verification

| Finding | Observable closure |
|---|---|
| FIND-008-3 | Ready exact target: before authored expiry succeeds; after expiry with worker stopped returns existing typed 409 and restart times out. Unavailable exact target: normal lifecycle invalidates the cache, fresh decision returns existing typed target refusal with no accepted receipt, rather than falling back to a longer runtime ceiling. Accepted identical replay remains available under its existing conditions. |
| FIND-008-8 | Cold exact-target admission with one serving connection available resolves/checks and accepts a valid decision without a second pool acquisition; original deadline and tenant binding remain enforced. |
| FIND-008-4 | Real token exchange A for S → accepted receipt and persisted inspection name A → exact A retry replays → authorized B for S and direct S conflict. No synthetic Caller/receipt substitutes for this connected scenario. |

Reuse existing tests/fixtures and record exact selectors for new tests before executing them. Run the narrowest remediation write-set checks, keeping every Cargo command sequential. Existing regression commands:

- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner >/dev/null 2>&1; WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/) | test(=pg_github_pull_request_extraction::an_approval_beside_running_work_is_decided_before_it_parks)"'`; add exact selectors for new proof if outside the approval module.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::verification::service::approvals::tests::only_the_deciding_actor_replays_a_decision) | test(=verification::approvals::tests::an_unavailable_read_hides_its_cause)'`.
- `mise run fmt:check`, `git diff --check`, and owning lint/check lanes for actual changes. Tenant-isolation check applies to the modified database capability path. Do not change generated contracts; if implementation genuinely requires their sources to change, run the owning codegen check and explain why.

Record RED diagnosis, GREEN assertions/results and closure per finding. No full language journey suite, SQL-wide gate or performance experiment is required in this bounded remediation. Full integrated journeys and clean full L1 qualification remain change-review work as in R1.
