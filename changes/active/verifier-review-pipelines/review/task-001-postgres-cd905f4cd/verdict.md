# TASK-001 PostgreSQL audit verdict

**FIX_REQUIRED.** Remove the rejected per-execution PostgreSQL authority and
correct the independently confirmed canonical terminal/read/reconciliation
violations in the [remediation task](TASK-001-R3-complete-execution-state-remediation.md).
No new SQL ownership/handoff mechanism or additional approval decision is
prescribed. This supersedes the earlier finding20 specification-decision stop
and finding25's relocation-only recommendation under current user authority.

## Single active implementation handoff

At the user's direction on 2026-11-06, the narrower PostgreSQL task was
consolidated with previously validated findings21–23 into
[TASK-001-R3](TASK-001-R3-complete-execution-state-remediation.md). It is the only
active remediation handoff for USER-PG-001 and findings16,17,19,21,22,23,
including deletion closure for24/25. The earlier PostgreSQL-only task file was
retired. Finding18 stays withdrawn and20 stays superseded. Historical review
packets are evidence, not parallel implementation tasks.

The PostgreSQL audit below retains its original scope and evidence. Publication
replay authority, bounded checkpoint preparation and rustdoc corrections come
from the existing independent validation of unchanged source; consolidation
creates no new findings or claims of closure. All diagnosis, correction and
focused proof for the combined scope are in the single task.

## Subject and bounded continuation

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.
Candidate/unchanged HEAD: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
Authority: spec18, durability supplement2, original TASK-001 including Revision1,
applicable repository rules/architecture, and explicit user corrections on
2026-11-06 recorded in [subject.md](subject.md). No source or authority changed.

This audit covers the full TASK-001 verification workflows and reachable SQL
owners, not just new SQL lines and not unrelated platform features. It is a
focused continuation on identical source, not a completed-remediation review
or whole-task PASS. Earlier source-backed evidence carries forward only for
unchanged contracts; the earlier acceptance of the SQL head does not.

## Independent coverage and validation

| Role | Selected independent report / scope | Result |
| --- | --- | --- |
| repo-rev | [PostgreSQL usage](postgres-usage-audit.md): operation/table/field inventory, reads/writes/stored functions/triggers, direct/background placement and owner permissions | FAIL proposals |
| invariant-rev | [Execution storage](execution-storage-audit.md): producer-to-consumer execution authority, queue/Bifrost terminal/result/identity/publication and removal consumers | FAIL proposals |
| domain-data | [Bifrost data](bifrost-data-audit.md): existing generic batch/catalog/source/WAL metadata versus rejected per-execution authority; exact recovery/removal closure | FAIL directed removal, no new FIND |
| ponytail-rev | [Independent validation](findings-validation.md): all inventories/proposals/source/correction burden, prior-finding changes and specialist applicability | FIX_REQUIRED |

All selected reports exist and source/authority hashes remained stable.
Discovery reviewers were fresh and isolated; a fresh validator independently
traced their source rather than counting agreement. No new validator-discovered
blocker or unresolved conflict required a follow-up. Prior unchanged security,
concurrency, system and maintainer evidence carries only current grants/File
custody, short-lock/shared-future/lane mechanics, generic existing metadata and
control lifetimes. A new comprehensive team was unnecessary for this scoped
audit; no required affected specialty was omitted. The orchestrator grouped
claims and wrote the packet from validated evidence, not its own findings.

## What PostgreSQL may still do

The detailed per-operation inventories are incorporated through independent
validation; a module name or JSON field alone is never a blanket exemption.

- Queue admission/idempotency/security/routing, initial input custody, claims,
  attempts/backoff, normal leases/renewal/confirmation/cancel/revocation and
  idempotent completion remain allowed.
- Schedule/group/binding activation, dedup and narrow previous-verdict
  comparison, plus independent Operator delivery payload/control, remain allowed.
- Existing current auth/registry/readiness/baseline and shared File upload/
  session/completion operations remain allowed.
- Pre-existing generic Bifrost catalog definitions, immutable batch/WAL identity
  and fences, file/source publication metadata, read cuts and physical-node
  coordination remain allowed.
- Those roles do not authorize a new per-execution SQL head, SQL-selected
  historical results, SQL-only full terminal errors, or live-Card/queue-parent
  requirements for retained evidence reads.

## Retained violations and directed work

| Identity | Validated issue and selected correction |
| --- | --- |
| USER-PG-001 | Human-directed rejection: remove `vala.scribe_state_heads`, claim/head/advance and all production/recovery/wire/replay/schema/test dependencies; do not relocate or rename the authority. Preserve existing generic SQL metadata and extend the existing Bifrost read/source owners for complete accepted recovery. |
| FIND-TASK-001-17 | Canonical terminal representation/producers omit execution errors, timeouts, refusals and cancellation while SQL retains the full error. Complete existing canonical lifecycle custody and its real terminal producers/readers; preserve legal queue diagnostics/controls without a new finalizer or SQL protocol. |
| FIND-TASK-001-16 | Historical result/inspection depends on SQL selection, live source Cards or queue existence. Record immutable exact identity once and resolve all origins through existing canonical readers with current scoped authorization and exact content custody. |
| FIND-TASK-001-19 | Final-attempt queue exhaustion can bypass an already durable selected result. Reconcile through existing queue/runner/load/deliver/complete before exhausting unfinished computation, with no additional provider attempts or policy extension. |
| FIND-TASK-001-21 | Previously validated active publication replay skips current lease confirmation on the covered fast path. Reuse ordinary confirmation for JSON/file tool success outside the lock; preserve valid replay without another checkpoint. |
| FIND-TASK-001-22 | Previously validated JSON/Variant checkpoint preparation runs before bounded writer admission. Move complete frozen preparation into the existing bounded writer path, preserving coverage, bytes, retries and drain. |
| FIND-TASK-001-23 | Previously validated changed-item rustdoc omissions. Document surviving items while making the corrections; delete obsolete head-only helpers rather than repair them to preserve the rejected owner. |

These are shared correction boundaries, not one issue per SQL caller. The two
new discovery source IDs map to existing16/17;19 is unchanged affected context.
The consolidated findings21–23 retain their original independent source
validation; they were not rediscovered by this PostgreSQL-only audit. No new
FIND ID is assigned, and USER-PG-001 is never misrepresented as an
independently discovered finding from the original audit. Complete diagnosis,
source locations, materiality, correction burden and proof are in
[validation](findings-validation.md) and the self-contained task.

Removal must replace the SQL head's authoritative-position/absence function
through existing Bifrost durable batch/source cuts, file publication coverage,
live/staged reads and WAL replay. An ordinary latest-visible row can miss a
later acknowledged checkpoint on an unavailable owner; return typed
unavailability for missing coverage. This is approved missing implementation,
not a reason to invent a head table/cache or another approval gate.

## Reconciled acceptance and prior dispositions

| Obligation / prior item | Current result |
| --- | --- |
| PostgreSQL restricted to enumerated queue/control/storage infrastructure | FAIL: USER-PG-001 and canonical terminal/result/retained-read field uses above; inventories distinguish legal bookkeeping from authority. |
| Canonical terminal status/full errors and immutable recorded identity | FAIL16/17; normal failed verdict/publication tests do not prove these negative/history flows. |
| Exact recovery/authoritative absence and accepted-source durability | Existing exact head-based path works only while rejected SQL exists; removal must extend Bifrost completeness proof. No deletion-only or latest-visible fallback accepted. |
| Final-attempt durable selected-result completion | FAIL19; ordinary attempt1→2 proof does not cover the ceiling. |
| Allowed queue/auth/File/group/Operator and generic Bifrost SQL | Enumerated source/field uses PASS classification; preserve them and their independent lifetimes. Not whole-platform certification. |
|18 direct final ACK| Withdrawn; direct final input/output/result fast return remains intentional. |
|20 strict new queue-to-Scribe authority handoff| Superseded by explicit acceptance of the specified rare delayed-worker interval; no new protocol or specification-decision stop. |
|24 head-query ordinal decode;25 request-side SQL registration| Subsumed by USER-PG-001 removal, not separate repair/relocation; remain unclosed until deletion proof. |
|21–23| Open and included in the consolidated R3 task: stale publication replay, bounded JSON/Variant preparation and remaining mandatory rustdoc. No closure inferred from consolidation. |

The task identifies exact old spec/supplement/task/architecture mandates to
align with current user authority; their stronger stale-worker promises must
not silently force the rejected design back into implementation. Accepted
rare timing risk does not waive ordinary queue controls, current permissions,
complete accepted-state/source failure, immutable File custody or generic WAL
replay guarantees. User-authorized direct fast return is not a finding.

## Verification, comprehension and limits

Read-only source audit and independently validated operation inventory; no new
runtime tests, migrations, implementation or load generation. The earlier
25 state/outbox units, one engine regression and 68 server integrations plus
fmt/diff checks remain historical positive evidence for their actual
assertions, not proof that SQL authority has been removed. The task specifies
focused RED/GREEN proof at existing server, SQL and Bifrost lifecycle targets;
full language journeys/aggregate gates belong to integrated change review.

A bounded independent comprehension pass of the task and verdict found no
concrete missing explanation, ambiguity or contradiction; it confirmed the
written scope, invariants, proof and implementation handoff without reading
source or author conversation. It is not implementation acceptance. No successful
remediation or whole TASK-001 acceptance is claimed. This is the first focused
PG boundary audit, so remediation-convergence assessment is inapplicable.

No commit, push, merge, deployment or production change occurred. The rare
replacement-worker risk remains accepted; removing heads without complete
Bifrost source coverage would introduce a different, unaccepted recovery gap.
