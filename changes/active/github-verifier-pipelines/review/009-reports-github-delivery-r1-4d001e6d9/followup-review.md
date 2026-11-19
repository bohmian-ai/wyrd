# Focused delivery lifecycle follow-up

Result: UNRESOLVED — source resolves reachability and rejects the proposed transaction-lock correction; a decision-complete correction for the complete required behavior needs specification reconciliation.

Subject: d07c55158...4d001e6d9ed3daf88bf81995510319373bd8c353. HEAD was the candidate during inspection. Scope: DATA-009-1 and the separately requested INV-009-1 cancellation authority seam only. No unrelated discovery findings were reviewed. No source edits, Cargo commands or commits; this report is the only write.

## Authority and inspected paths

Read scope.md, the task-review skill's focused role and correction-burden rules, domain-review-data.md, only the requested INV-009-1 finding from the invariant report, approved spec revision 10 (including revision 9's decision), original task 009, applicable root/SQL rules and maintainer guidance, and the relevant architecture sections. CodeGraph preceded source search/read. Source tracing covered:

- `components/github/delivery.rs`: Write::send, spawn, recover, deliver_retried, deliver, recorded, render, send, attempt, allowed, record and key.
- `components/github/app.rs`: check lookup and unconditional create/update adapters; `components/github/presentation.rs`: render and CheckRun request body.
- `verification/runner.rs`: claim custody, terminal/park settlement, LeaseLost handling and delivery handoff; `verification/leases.rs`: loss notification; `verification/state.rs`: accepted, recover, at, fail/decide and Writer::current.
- `components/verification/service/inspection.rs`: Retained::status and cancel; `service.rs`: delivery outcome inspection.
- `wyrd-sql/src/queries/verifier_executions.rs`: claim, release, terminate, cancel and principal cancellation; its `pull_requests.rs`: PR locking/current generation and current delivery enumeration; `app/server.rs`: startup recovery composition.

## Overlap: reachable, with no provider fault needed

`delivery.rs:191–199` launches detached tasks. `runner.rs:1530–1538` calls it after both park and settlement, after dropping the renewing claim registration at line 1516. `app/server.rs:661–670` also starts recovery; `delivery.rs:215–232` enumerates the current executions and invokes the same worker on each server. Recovery does not claim or reserve the execution. `current_pull_request_executions` is a read, not a delivery admission primitive (`pull_requests.rs:908–922`).

A parked delivery can retain an in_progress GET response (`delivery.rs:147–151`), then a newly resumed execution can settle and its separate task can PATCH that same Check to completed. The older task then PATCHes its stale in_progress body (`154–155`). Its guard observed the old GET, not the Check when PATCH takes effect. `app.rs:735–745` has no conditional update. Both deliveries pass current-generation validation because the execution and PR generation are unchanged. Their outcomes have different accepted revision keys (`delivery.rs:595–606`), so both outcomes can be final. Terminal recovery subsequently sees its recorded outcome and returns at `295–300`, leaving the stale remote status indefinitely. This violates the explicit never-return-to-in_progress rule in original task RENDER and `wyrd-design.md:2540` independently of any exactly-once interpretation.

Two startup/live workers can also both see no outcome, both GET no Check/review, then successfully POST duplicates. The recorded outcome fence is acquired after provider IO (`delivery.rs:303–326, 548–590`); it cannot serialize prior remote effects. This is a healthy overlap, not a lost-answer retry.

Revision 9 deliberately selected lookup-before-create and accepted rare duplicates after a lost answer (`spec.md:264–273, 458`; original task Delivery). Original task Proof also says exactly-once remote HTTP effects are not promised. That prevents demanding absolute remote exactly-once delivery, but does not make the completed-status regression permissible. Nor does the narrower approved lost-answer exception explicitly settle normal healthy multi-replica overlap. Do not use the one-Check wording to silently reintroduce the dropped delivery ledger or absolute exactly-once semantics; keep that distinction in final validation.

## Why the proposed shared transaction lock is not an authorized correction

The proposed execution-specific transaction advisory lock would order the normal non-crash interleavings only while the transaction and its connection remain alive through lookup, write and outcome publication. It is not a short coordination transaction. REQ-017 expressly requires: “Release connections after commit; never hold them over compute/provider, Bifrost, file or encoding waits” (`spec.md:327–333`). Taking a different advisory identity avoids blocking PR admission, but does not cure this violation. Existing locks (`pull_requests.rs:40–45, 498–518`; `verifier_executions.rs:156`) are transaction-scoped. Committing before the HTTP wait releases their ordering.

Existing mechanisms do not supply the missing owner as currently contracted:

- `allowed` commits its generation transaction at `delivery.rs:528`. Its lock owns PR observation/admission, not an entire external side effect; extending its lifetime is specifically disallowed and affects sibling bindings.
- The execution claim is a computation/recovery/settlement owner. CLAIM_SQL selects pending/retrying or expired running rows only (`verifier_executions.rs:411–445`), not completed/cancelled rows. Park releases it before delivery; settlement retires it before delivery. Retaining or reacquiring a lease for terminal delivery changes claim/resource ownership and effectively introduces a delivery obligation into queue coordination. That is not reuse of an existing delivery contract.
- Scribe's first-wins fence owns the final outcome. Moving it before provider IO would falsely record final delivery before the effect, or turn it into the explicitly dropped progress ledger. A crash between such a record and the provider effect would cause recovery to skip required delivery.
- A process Mutex cannot order replicas. Another accepted-state/status read cannot close the subsequent remote-write window. A distributed lease without a provider-side fence also needs an explicit late-IO/takeover contract; the architecture already admits that submitted IO can finish after lease loss (`wyrd-design.md:2108–2109, 2131–2135`). Merely naming an existing lease does not solve that boundary.

A smaller local mitigation deserves distinction from the proposed global serialization: skipping every nonterminal PATCH when a Check already exists removes the demonstrated stale-status PATCH on that ID without holding SQL connections. It does not prevent simultaneous absent-object creates, including a late waiting create beside an already completed Check, and it stops refreshing the waiting explanation when an execution re-parks. It therefore is not a demonstrated complete correction for the combined finding as written. The current finding must not prescribe global serialization while treating this narrower behavior choice as already decided.

Required specification decision: reconcile the exact supported overlap/late-provider-write guarantee with lookup-before-create, released SQL connections, no delivery ledger and the current post-settlement handoff. Either define a bounded delivery ownership mechanism and its crash/takeover behavior, or explicitly select a weaker overlap guarantee and any narrower status-update policy. This report selects neither and does not authorize new schema, a generic lock service or a replacement lifecycle owner. If ordinary duplicate creation alone is rejected as outside the approved exactly-once promise, the explicit completed-status regression still needs a bounded correction or a reconciled policy.

## Cancellation: same accepted revision, different authoritative status

The requested INV-009-1 path is also reachable:

1. A PR parks at accepted revision N. Delivery records an in_progress Check and the immutable delivered outcome for N.
2. Authorized cancel (`inspection.rs:1015–1041`) commits queue cancellation and returns. CANCEL_SQL (`verifier_executions.rs:607–613`) sets cancelled, clears the lease and retains checkpoint state. `cancel_principal` is the sibling writer with the same lifetime behavior (`615–623, 3200 onward`).
3. A parked row has no running state writer. Cancelled rows are not claim candidates. A running holder instead loses its token (`leases.rs:175–185`), follows LeaseLost, returns stale_lease (`runner.rs:1174–1175`) and is excluded from delivery handoff (`1530–1537`). The cancellation endpoint itself schedules no delivery.
4. `Retained::status` (`inspection.rs:248–275`) now projects cancelled from SQL while accepted revision remains N. Even explicitly invoking delivery or restarting does not help: `deliver:295–300` finds N's prior outcome. `service.rs:301–310` exposes that same delivered outcome for the cancelled execution. The existing Check can remain in_progress forever.

This violates REQ-013's explicit cancelled conclusion mapping and task RENDER on an ordinary current-generation manual cancellation. PR supersession/deactivation is different: it intentionally removes authority to publish the obsolete execution.

The suggested “record cancellation through RunState” is not an already-authorized mechanical fix. The architecture explicitly says: “A cancellation or revocation settles only the queue row, which owns that status; a running holder learns of it through its lost token and records nothing more” (`wyrd-design.md:2127–2130`). `RunState` is the running execution's only writer (`state.rs:565–577`); `fail` calls `decide`, which refuses a lost writer (`785–794`, `Writer::current:330–331`). Reconstituting a fresh non-lost writer in cancellation would grant post-cancellation write authority, with races against an old frozen revision and principal-revocation paths. It is not reuse of the current cancellation protocol.

Conversely, adding cancelled to the delivery key or overwriting N's waiting outcome violates `bifrost-design.md:487–495`: one row per accepted state revision, first outcome final, replay records nothing. Adding only a spawn fixes neither that suppression nor inspection/restart semantics. Delivering the cancelled Check without a corresponding new canonical outcome leaves the current outcome describing the earlier waiting presentation.

Required specification decision: choose how SQL-only terminal transitions participate in the canonical delivery version/identity, or expressly authorize a cancellation checkpoint owner and its post-revocation write/fencing protocol. That authority must cover manual/principal cancellation and recovery; a detached best-effort callback cannot supply it. The existing cancellation response latency, committed lease fence, and no-post-loss-writes behavior must remain preserved unless explicitly amended. No correction to this seam is currently decision-complete within the per-accepted-revision immutable-outcome contract.

## Disposition and proof boundary

DATA-009-1's proposed transaction-lock remediation must not be passed directly to implementation. The stale-status interleaving is source-proven and material, while the complete concurrency correction remains a specification decision. INV-009-1's cancellation diagnosis is source-proven and material; its proposed state-owner reuse conflicts with the explicit SQL-only cancellation authority. Route these authority conflicts as SPEC_REVISION_REQUIRED rather than inventing the missing concurrency/persistent-data decisions in a task remediation.

After a decision, focused proof should force the selected overlap semantics with existing GitHub transport barriers and separately park → record waiting outcome → public cancel → provider/inspection readback → restart. Tests must assert final provider status and canonical outcome consistency, not just successful API responses or sequential waiting/completion. No additional harness or broad benchmark is justified by this follow-up.

Skipped runtime tests and unrelated findings. Remaining risk: required terminal presentation can stay in_progress, while the current approved ownership and outcome rules do not provide a complete permitted correction.
