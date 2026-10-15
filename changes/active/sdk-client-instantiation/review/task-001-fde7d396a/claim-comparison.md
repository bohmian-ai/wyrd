# Discovery claim comparison

All seven required discovery reports are complete. This is orchestration and
claim grouping, not source validation or a final finding ledger.

| Shared obligation/cause | Discovery source IDs | Comparison / question for validation |
|---|---|---|
| Python local views/binding/metadata prematurely initialize shared lazy connection | BEH-001, INVAR-001, STANDARDS-001, MAINT-001, SYS-001, CONCURRENCY-001 | Corroborated source claims. Cards typed views appear in behavioral/invariant/standards reports; Bifrost appears throughout. Concurrency report includes synchronous insert, whereas invariant report deliberately limits its proposed correction to unambiguous local operations. Validator must resolve that boundary from approved semantics and reject any unapproved enqueue/lifecycle redesign. |
| Required no-client public entry-point journey proof | BEH-002, INVAR-002 | Both report Rust manual resolver + connect_with bypass and Python eager AsyncBifrost success gap. Feasibility of actual Rust ambient proof without runtime child processes/unsafe env mutation is unresolved in discovery. One fresh focused follow-up is required for this specific uncertainty. |
| Delegated server subject attribution proof | BEH-003 | Unique claim: permission-intersection outcomes do not directly assert server-observed subject attribution. Validate against complete existing proof and public evidence seams; no automatic requirement for audit rows. |
| Mandatory changed-item rustdoc/API-doc closure | BEH-004, MAINT-002, STANDARDS-002 | Different documents/sites, one bounded documentation correction category. Validate every listed changed item, distinguish substantive missing mandatory docs from nonblocking wording/preferences, and remove error-doc requests if behavioral correction restores infallibility. |
| Migrated revoked saved-login test failure timing | STANDARDS-003 | Unique source claim that eager Cards connect now fails before intended list refusal assertion; owning identity lane was not reported run. Validate exact producer and real caller before retaining. |

Security review independently returns PASS and proposes no changed security
regression. Discovery reports note pre-existing exchange error mapping and
unrun identity services; neither automatically creates a finding. Pending
TASK-002 prose remains explicitly out of this task.

## Follow-up decision

Required one `followup-rev` because the no-client journey claim lacks a
resolved authorized proof approach. Assigned `followup-review.md` scope:
existing harness/runner feasibility of actual public Rust ambient factories
without runtime child processes, unsafe runtime environment mutation, a new
resolver/public constructor/dependency/harness; also clarify Python eager
AsyncBifrost proof closure. Follow-up is discovery, not validation or a vote.

After the follow-up finishes, a fresh structured Ponytail validator receives
the complete diff, authorities, and every report. Only independently confirmed
or revised claims may enter the final ledger/verdict/remediation.
