# TASK-003-R1 remediation review subject

- Original base: `74034459d468fd0adf277e3b51c32b1a676e8e55`.
- Prior reviewed candidate / remediation base: `c0900bff523e83d3666658440daa81355067cb34`.
- Remediation candidate: `0d8c3fed690d51fac5b9c4eb51af951025927f5f`.
- Review delta: `git diff c0900bff5 0d8c3fed6`.
- Cumulative range remains available: `git diff 74034459d...0d8c3fed6`.
- Direct ancestry confirmed; no rebase correspondence is required.
- Spec revision 4 SHA-256: `fca219efc01e8300dfb36641e603c7bfb0bec49f5e83f7068643b79a89e6da6f`.
- Original TASK-003 SHA-256: `0b139b2b21ad3d9d414568e6d1e51e117f58fb645638a6e64b76ea89e2439a91`.
- TASK-003-R1 with evidence SHA-256: `a8e9a40e585d9a094ad429df7aafb09b7bb32254c5f7b48bdab5f9b6ab0ceced`.

The packet is intentionally uncommitted, supplied by the maintainer and fixed by
these hashes for review. Unrelated working-tree benchmark/capacity changes are
excluded. The review writes reports only and leaves source and original packet
unchanged. The maintainer-run benchmark remains deferred until code acceptance;
full TASK-003 completion still requires REQ-020 qualification.

## Independent assignment and navigation

One fresh `r1_ponytail_rev` checks prior FIND-TASK-003-1 through -5 closure,
the remediation delta and affected dependencies, using the prior validated
behavior, invariant and collateral reports. It owns `findings-validation.md`.
Any new proposed blocker requires another independent focused source check.
The orchestrator handles identity, execution and report packaging, not review.

Changed production owners are `ShardOwner::run`/`drain_mailbox`,
`GroupPipeline::ready_stage`, `TenantTableRoundRobin::eligible` and collection
dispatch; consumers include the retained stage futures and control barriers.
New proof covers preparation priority, eligible collection and a real same-shard
multi-tenant/table stream plus failed COMMIT reconciliation. Architecture changes
describe the existing soft target. Unchanged acceptance, recovery, custody and
collateral boundaries carry forward only where their prior proof remains valid.
