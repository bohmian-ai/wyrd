# Independent Ponytail validation of the combined remediation

## Subject

- Combined draft: `TASK-001-R1-close-task-review-findings.md`
- TASK-001 candidate: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5..40a73817d415e9a1626e6ec7a91edda083e344d3`
- TASK-005 candidate: `0af5eef72f83a95167f6ee3bdc873df9f9cc4254..1f6d3b6316bfd87c8a52e36b623b80d608304e19`
- TASK-006 candidate: `32a0aafecbda96a86103cd389e42728ea33e0c54..358cf636eda48ea31a3416e2c5b21b30873f83dd`
- Authority: approved specification revision 7, `AGENTS.md`, routed architecture authorities, complete cumulative diffs, and the three proposed finding ledgers
- Validator: fresh independent Ponytail review, separate from implementation and the primary/standards reviews

The validator traced each finding to its owning code and callers, checked reachability and adjacent behavior, and applied the delete → reuse → native → installed dependency → minimum-code ladder. It supplied no implementation and edited no files.

## Finding dispositions

| Finding | Decision | Validated correction |
|---|---|---|
| `FIND-TASK-001-1` | REVISED | Duplicate of `FIND-TASK-005-1`. Correct each existing receiving authorization owner, not a global service. Preserve Bifrost create's same-transaction append. |
| `FIND-TASK-001-2` | REVISED | Duplicate of `FIND-TASK-005-2`. Remove login, Card lifecycle/reconciliation, and storage-mechanic canonical appends; retain permission audit and operational evidence. |
| `FIND-TASK-001-3` | REJECTED | Oracle relay is intentionally at least once. Existing staging → frozen range → Scribe durability is authoritative; valid relay duplicates do not justify new durability state. |
| `FIND-TASK-001-4` | REVISED | Require exact focused commands, the owning MCP run, and `docs:check`. Do not require `verify:bifrost`, which TASK-001 expressly deferred. |
| `FIND-TASK-001-5` | REVISED | Consolidate with TASK-005/006 raw-owner findings: keep `ValaPostgres`, `OperatorPool`, and `TenantConn`; delete the raw connection helper. |
| `FIND-TASK-001-6` | REVISED | Consolidate all redundant `TenantConn` tenant predicates; retain `wyrd.current_tenant()` only for inserted tenant values. |
| `FIND-TASK-001-7` | REVISED | Limit correction to the publisher module/errors/cancellation, changed Gate test seam/tests, and inaccurate `append_managed_columns` docs. |
| `FIND-TASK-001-8` | REVISED | Import the five identified signature types once and use bare names. |
| `FIND-TASK-001-9` | REVISED | Correct only the enumerated authority, reference, operations, and changed public-doc locations. |
| `FIND-TASK-001-10` | REVISED | Duplicate evidence finding; consolidate with `FIND-TASK-001-4`. |
| `FIND-TASK-001-11` | CONFIRMED | TASK-001 uses unsupported `implemented`; TASK-005/006 remain `ready`. Set all three to `review`. |
| `FIND-TASK-005-1` | REVISED | Same authorization-owner correction as TASK-001-1. |
| `FIND-TASK-005-2` | REVISED | Same non-permission deletion as TASK-001-2. |
| `FIND-TASK-005-3` | REJECTED | Valid against historical revision 6, but closed cumulatively by TASK-006's persisted frozen bound and deterministic range identity. Do not remediate again. |
| `FIND-TASK-005-4` | REJECTED | Same intentionally at-least-once Oracle relay behavior as TASK-001-3. Add no identity, receipt, or relay watermark. |
| `FIND-TASK-005-5` | REVISED | `append_audit_connection` has one caller. Delete it and keep the workflow on `append_audit(&mut TenantConn)`. |
| `FIND-TASK-005-6` | REVISED | Same RLS correction as TASK-001-6. |
| `FIND-TASK-005-7` | REVISED | The cross-crate seam is real, but dynamic dispatch is not. Statically compose Gate over `PostgresGateAudit` in production and `RecordingAudit` in tests. |
| `FIND-TASK-005-8` | REVISED | Same signature-import correction as TASK-001-8. |
| `FIND-TASK-005-9` | REVISED | Same bounded documentation correction as TASK-001-9. |
| `FIND-TASK-005-10` | REVISED | Delete the exemption constant, special-case branch, unused flags, and stale prose—not only the obsolete set member. |
| `FIND-TASK-006-1` | REVISED | Keep only `publish_tenant` callable. Make partial stages private and use the existing transaction fence to test replay. |
| `FIND-TASK-006-2` | CONFIRMED | The sweep is serial. Use installed `futures-util` for fixed bounded unordered concurrency inside the existing publisher. |
| `FIND-TASK-006-3` | REVISED | Replace the incomplete journey with a full-cycle freeze, chain-head fence, retained acceptance, abort, release, replay, uniqueness, and drain sequence. |
| `FIND-TASK-006-4` | REVISED | Same RLS correction as TASK-001-6. |
| `FIND-TASK-006-5` | REVISED | Same owner correction as TASK-001-5, covering both the publisher field and tenant-directory signature. |
| `FIND-TASK-006-6` | REVISED | Same signature-import correction as TASK-001-8. |
| `FIND-TASK-006-7` | REVISED | Same bounded rustdoc correction as TASK-001-7. |
| `FIND-TASK-006-8` | REVISED | Same bounded documentation correction as TASK-001-9. |
| `FIND-TASK-006-9` | REVISED | Require docs, changed MCP, exact focused, and cumulative leaf evidence; exclude the prohibited aggregate. |

## Consolidated validator decisions

The surviving findings reduce to ten correction boundaries:

1. Audit every receiving permission verdict exactly once through its existing owner.
2. Delete canonical audit from transitions that evaluate no permission.
3. Restore `ValaPostgres`, `OperatorPool`, and `TenantConn` ownership and remove redundant tenant predicates.
4. Replace dynamic Gate audit dispatch with static composition.
5. Privatize partial publication stages and prove replay through the existing full-cycle journey.
6. Add fixed bounded cross-tenant concurrency with installed `futures-util` and the existing two-tenant journey.
7. Correct only the enumerated rustdoc and signature sites.
8. Correct only the enumerated authority, reference, operations, and public-doc sites.
9. Delete the complete obsolete tenant-isolation exemption branch.
10. Correct task statuses and exact focused, MCP, docs, and cumulative evidence.

Three proposed findings do not survive: the historical TASK-005 overlapping-range defect is already closed, and the two duplicate Oracle WAL findings describe accepted at-least-once behavior. The revised task must not add Oracle relay durability mechanics or repeat TASK-006's frozen-range implementation.

## Result

The draft required the revisions above. After they were applied, the independent Ponytail validator reviewed the revised combined file again and returned **PASS**.

All 30 original finding IDs are accounted for. The surviving corrections are supported, minimal, and decision-complete for `$wyrd-implement`. No specification revision or further task revision is required.
