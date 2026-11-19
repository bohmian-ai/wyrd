# Security domain review

Result: **FAIL** — one material proposed finding, SECURITY-1.

## Immutable subject and boundary

Reviewed base `f4e79ea00523af5cd9212fdacde64f16c97ad554` to candidate
`365bfc94bfef1a153f213462f7ef104afa823741`, using the actual cumulative Git
diff and current source. HEAD still matched the candidate at the final source
check. First review; no carried-forward evidence.

Scope: exact-Verifier execution read/run authorization, principal revocation,
publication and download trust boundaries, untrusted selectors and paths,
tenant isolation and fixed internal Bifrost reads. This is an independent
security discovery review, not final finding validation. No source edits or
Cargo jobs were performed.

Authorities inspected: root AGENTS.md, SQL AGENTS.md, architecture authority
map, security posture (accepted authority, token snapshots, SYSTEM reads,
revocation and tenant binding), Wyrd design's verification/public-surface
sections, approved revision-16 spec (REQ-014/015, INV-002/003, AC-025), Task 001,
public-contracts.md, output-publication.md, spec-driven workflow and applicable
maintainer/agent-surface/error guidance. CodeGraph exploration preceded source
navigation. The report covers the security boundary, not unrelated SDK style
or broad repository gate compliance.

## Coverage and acceptance

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Read requires exact `verifier:read`; cancellation requires exact `verifier:run`; no Cards fallback | `service/inspection.rs::require_verifier`, `authorized`, `select`, `cancel`; `service.rs::get_execution`, `direct_execution`; runtime `Permission::validate`, matching and built-in grants | Runtime permission tests; Rust `execution_pages_are_stable_and_tenant_scoped`; route authorization and local-content tests | PASS |
| Tenant identity comes from verified caller; foreign executions are not found; selectors cannot substitute another execution/producer | Inspection transactions bind `caller.data_tenant_id`; queue reads use `TenantConn`; migration RLS; SQL inspection queries bind execution; cursor verifies tenant/execution/listing; publication selection compares attempt/item/iteration/name within execution and step | Rust page journey's foreign tenant, other-Verifier grant and cursor refusals; SQL page tests | PASS |
| Dedicated reads do not require Cards or arbitrary Bifrost table grants and do not grant arbitrary SYSTEM queries | `service.rs::records` grants exactly the three verification tables through existing `SystemReadAuthority`; `InputResolver::{record,direct,result}` builds fixed typed-ID predicates; `ObjectInventory::file_object/rows` uses tenant-bound, storage-table-only authority and validated File paths | Exact read-only grant in Rust journey; local publication route test; inspected public Bifrost query owner remains unchanged | PASS |
| Retained publication read/download is authorized as requesting caller; manifest selection cannot become arbitrary storage-path access | `inspection.rs::{download,content,file}` authorize through `select`; directory path must equal a listed regular file; object inventory resolves digest and size within caller tenant; local bytes repeat the feature decision; cloud plans use existing short-lived presigning | `local_publication_downloads_redeem_under_verifier_read` proves bytes and refuses other-Verifier/Cards-only callers; Rust journey tests invalid selectors, unlisted paths and JSON refusal | PASS |
| Acceptance retains execution-owned work with no extra external-write authority; frozen content, producer identity and lease fence constrain publication | `hosted/publications.rs::{json,path,complete,accept}` reuses File lifecycle and records exact producer under SQL lease; workspace freeze uses writer lock and strict tree traversal; TenantEvidence binds publication reads to its execution and verifies stored length/digest | `pg_durable_verifier_work` immutable file/JSON, directory, replay and mount tests; workspace path/tree source inspected | PASS |
| Revocation stops affected queued/running work and prevents later dispatch/recovery under revoked authority | `revoke_principal_in_conn` suspends and cancels existing rows atomically, but admission is a later independent handoff without principal-state synchronization | `cancel_and_revocation_fence_unfinished_work` covers already admitted rows; no delayed-admission interleaving proof | FAIL — SECURITY-1 |
| Ordinary grant edits do not silently replace frozen accepted authority | Queue stores `AcceptedAuthority`; runner uses `Caller::accepted`; downstream checks retain frozen permissions | `queued_subject_authority_survives_restart`, plus accepted-authority implementation | PASS; preserve this during correction |

Publication list queries select metadata without publication JSON/content.
Authorization checks are made before content reaches a caller. Publication
names and step IDs deserialize through existing validated types, and local
content URLs encode selector values. The base64 cursor contains its tenant
binding; this does not give a caller authority over that tenant, and no
source-proven cross-tenant access or material identity exposure was found.
No new arbitrary-SQL or arbitrary-object-path route was found.

## SECURITY-1 — revocation misses admissions still in the committed handoff

Classification: **INCORRECT**. Severity: high security/lifecycle boundary.

**Violated obligation.** REQ-014 and AC-025 require revoked execution authority
to stop work and prevent recovery from dispatching it; INV-003 permits
revocation to reduce accepted authority. The owning security posture says
principal revocation cancels unfinished executions so queued work cannot
outlive its accepted authority. Task 001 scenario 7 expressly requires this.

**Locations.** `crates/wyrd/wyrd-server/src/verification/submissions.rs:260–302`
(`SubmissionSink::write` admission transaction),
`crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:629–636`
(`CANCEL_PRINCIPAL_SQL`), with the producer at
`crates/wyrd/wyrd-server/src/components/verification/service.rs:718–790`.

**Source-proven failing path.**

1. An authorized enqueue captures the effective authority in `Submission`
   and stages its input through Scribe before its SQL admission. The request
   waits for the admission reply. A slow/retrying Scribe or submission writer
   is an ordinary supported state, not malformed input.
2. An administrator revokes the principal while this handoff is pending.
   `wyrd-auth/src/revoke.rs::revoke_principal_in_conn` suspends it and calls
   `cancel_principal` on the revocation transaction. The cancellation UPDATE
   only sees existing visible unfinished execution rows.
3. `SubmissionSink::write` subsequently admits the captured submission. Its
   only per-submission resolution is the Verifier/subject Card pair; then
   `accept_submissions` checks existing IDs/idempotency and inserts the saved
   authority. `INSERT_SUBMISSIONS_SQL` joins Cards, not active principals.
   There is no principal check or lock shared with revocation. The same
   failure also occurs when admission has inserted its row but has not
   committed when revocation's UPDATE takes its snapshot.
4. `CLAIM_SQL` (`verifier_executions.rs:419–480`) selects pending/retrying
   rows using status, due time and worker format only. The runner constructs
   `Caller::accepted(&run.authority, system)` (`runner.rs:496`) and runs under
   the saved permissions. No second revocation scan occurs. The work can
   therefore execute, checkpoint and publish after principal revocation,
   including after the original token expires.

**Sibling writers and existing owner.** The same SubmissionSink also commits
acknowledged scheduled/grouped candidates. `accept_scheduled` (2264 onward)
and `accept_grouped` (2815 onward) insert previously prepared authority and do
not serialize with principal suspension. Scheduled preparation's current-state
resolution happens before this later handoff, so it cannot close this gap.
Claim and recovery share the same queue owner; adding a guard to one model
caller would leave these sibling dispatch paths exposed. Existing
TenantConn/principal state, the auth revocation owner and the queue admission
owner already contain the required state; there is no need for a token epoch,
new queue or general reauthorization system.

**Observable consequence and materiality.** Administrative revocation reports
success while work already accepted into the handoff can become newly runnable
under exactly the retired principal's frozen authority. This defeats the new
active-work revocation guarantee on ordinary response/projection delays and
concurrent admission; it is not merely the documented five-minute bearer-token
window. The stored authority has no token expiry to bound that queued work.

**Smallest testable correction.** At the existing durable admission/revocation
boundary, order principal-state validation and queue admission against the
revocation transaction, for every admission kind using the retired subject or
accepting actor. If admission wins, revocation must see and cancel its committed
row; if revocation wins, delayed admission must never leave runnable work.
Check current suspended/revoked state without rebuilding or widening accepted
grants. Keep completed records, idempotent replay, tenant isolation and frozen
grant semantics intact. A state check without ordering still leaves the
concurrent insert/UPDATE race; a claim-only check still permits the later
revocation race for running work. Reuse the existing principal/queue owners
rather than adding repeated guards throughout tools or storage.

**Focused closure proof.** Hold an already authorized submission between Scribe
staging and durable queue admission, revoke its principal through the public
route, then release the handoff and start/restart the runtime. Assert that no
invocation is dispatched or publication accepted and that the refusal retains
the typed revoked reason. Exercise the reverse ordering with an uncommitted
admission and concurrent revocation so admission either serializes first and
is cancelled, or is refused after revocation. Cover the shared
scheduled/grouped handoff at its existing seam rather than adding a separate
test harness per caller. Existing test
`cancel_and_revocation_fence_unfinished_work` waits for both active and pending
rows before revoking and therefore cannot prove either interleaving.

## Verification limits

Static source review only; no Cargo, database or external service checks were
rerun, as assigned. Recorded green checks were treated as evidence claims,
and the relevant test bodies were inspected. No speculative filesystem attack
requiring an unsupported symlink writer or unrelated authorization redesign
was retained. Independent validation must confirm SECURITY-1 before the final
ledger includes it.
