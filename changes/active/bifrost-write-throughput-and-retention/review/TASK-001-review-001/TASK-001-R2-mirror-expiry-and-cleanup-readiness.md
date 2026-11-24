---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 1
original_task: TASK-001
review_base: 66f44aee193077ef3565aa76140a942d8eb593f9
review_candidate: bf438a8d7985aadd29db3beb73ed0269c2b2f7ca
findings: [FIND-TASK-001-1, FIND-TASK-001-2]
---

# Expire cross-pod retry acknowledgements and keep cleanup off readiness

## Outcome and authority

A retry sent to a different running pod cannot receive an acknowledgement from
a receipt that has been deleted. A recovered server can serve retained rows while
initial receipt cleanup is blocked; fresh writes remain busy until cleanup succeeds.

This task corrects only settled REQ-002 and REQ-008–009 in
[spec revision 1](../../spec.md), using the existing owners in
[TASK-001](../../tasks/TASK-001-fence-lifecycle.md). The diagnosed paths and their
independent validation are recorded once in [findings-validation.md](findings-validation.md).
The review covers the cumulative identities above; R1 was maintainer-directed,
not a prior independent acceptance verdict.

The separate R1 visibility/recovery contract decision remains unresolved. This
task does not select missing-row semantics, restore application receipt reads,
or change architecture to match the candidate. Contract clarification and required
owner-document reconciliation must precede acceptance of that affected behavior.

## Corrections and connected scope

### FIND-TASK-001-1: another pod can retain a deleted receipt's acknowledgement

Pod A accepts and durably hands off a batch. Pod B handles a matching retry,
suppresses its rows after SQL arbitration, but still caches the acknowledgement.
A's cleaner deletes the receipt and forgets only A's mirror. B's discovery covers
B-owned streams, so the live B mirror continues acknowledging the deleted identity.

REQ-002 and the Bifrost mirror contract require mirror authority to end with the
receipt. Correct the shared mirror population/lifetime boundary in
`scribe/shards.rs`, `committed_batches.rs`, and the existing ingress/cleaner
consumers as needed. Reuse durable arbitration when the mirror has no valid
authority; do not add ordinary ingress cleanup SQL, a new ledger, or a second
acceptance mechanism. Preserve matching/conflicting retries while the receipt
remains authoritative, tenant separation, and accepted-only crash recovery.

### FIND-TASK-001-2: blocked initial cleanup holds recovered service readiness

After custody recovery, `Scribe::replay_wal_async` awaits `sweep_and_report` before
setting recovery ready. Server boot awaits that call. A row lock on an old,
handed-off receipt can block DELETE or the sweep mutex indefinitely, coupling
maintenance progress to service startup rather than fresh-write admission.

Correct the replay/cleaner lifecycle at its existing Scribe owner. Successful
custody recovery must let the server become available while initial cleanup is
pending; existing local cleanup health must continue refusing fresh writes until
the required whole sweep completes. Preserve recovery-required authority checks,
safe deletion, bounded worker ownership, shutdown behavior, and automatic admission
resumption. Merely handling returned SQL errors does not cover a pending sweep.

## Observable closure scenarios

| Finding / requirement | Scenario and required observation | Narrowest credible proof |
|---|---|---|
| FIND-TASK-001-1 / REQ-002 | Accept/publish on A; retry on B using the same tenant/principal/table/version and prove B acknowledged the prior acceptance. Let A's real cleaner remove the safely handed-off aged receipt, keep B running, and retry on B. Assert authoritative reacceptance and the resulting receipt/rows rather than ACK success alone. No restart may clear B's mirror between deletion and retry. Also preserve matching/conflicting retries while authoritative. | Existing multiprocess Scribe lifecycle journey and focused mirror/arbitration tests. Internal expiry setup stays in the internal harness; introduce no public operational control. |
| FIND-TASK-001-2 / REQ-008–009 | Restart with independently durable accepted rows and eligible receipt debt. Block cleanup DELETE with a real transaction lock. Prove boot/recovery and retained reads finish while fresh writes return retryable busy. Release the lock; a current initial sweep completes and writes resume without restart. Preserve the existing unavailable-recovery-authority case. | Real-Postgres lifecycle/startup regression through the production-shaped server/cluster harness, plus focused cleaner/health checks where needed. |

Use the current Scribe lifecycle journey target under the repository PostgreSQL
wrapper. Confirm each new test's actual nextest name and run its exact selector
before RED and after correction; do not invent names in this packet. The existing
runner shape is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all --test-threads=1 -E "test(/^lifecycle::/)"'
```

Use focused redux mirror/cleaner tests with their actual features and PostgreSQL
setup when applicable. Run the affected owner lane once after focused closure,
`mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, and
`mise exec -- git diff --check`. Run SQL receipt integration only if its owner
changes. Keep Cargo commands serial and follow the existing task's checkpoint
policy; record performance evidence as progress or qualification accurately.

No CPU throughput implementation, extra worker, public test control, migration,
receipt quota, domain contract redesign, full journey aggregate, commit, or push
is authorized by this remediation task. Preserve recorded TDD violations; new
closure tests follow current test-first discipline without claiming to repair history.
