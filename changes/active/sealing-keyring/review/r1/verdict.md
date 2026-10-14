# Sealing-keyring review verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/sealing-keyring`.
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`.
- Approved specification: `changes/active/sealing-keyring/spec.md`, revision 1.
- Original task: none, explicitly waived by the human. Acceptance is against the specification and cumulative implemented commits.
- Human clarification: nothing has shipped; compatibility with the removed Gateway keyring and deployed-data conversion are not required.
- Reviewed tracked source and HEAD remained unchanged. Only review artifacts were added. No implementation, commit, merge, push or deployment was performed.

## Reconciled acceptance matrix

Source PASS is not an executed-test claim.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-007: shared deployment seal/open and envelope | Gateway administration and resolver use `state.auth.sealing_key`; `ManagedSecretBinding` delegates cryptography to existing `SealingKeyring`. | PASS at source |
| REQ-007: tenant/name/provider binding | Authenticated payload carries and verifies all three identities; typed tenant admission and RLS remain intact. Negative unit/integration bodies inspected. | PASS at source |
| REQ-007: retained keys and same boot rewrap | Gateway extends `SealedSecretTable::ALL` and existing exact-byte operator CAS; no independent rewrap owner. | PASS at source |
| REQ-007: remove separate Gateway keyring | Config/state/key types, snapshot fields and obsolete SQL columns have corresponding consumer removals; no compatibility path required. | PASS |
| REQ-007: persistent owner-only key before first dev boot | Existing installer root holds no-clobber staged key; explicit env wins; every child receives ordinary key-file input. Installed CLI restart journey exists. | PASS at source |
| REQ-007: quickstart credential step | Real CLI journey submits quickstart-form managed credential and proves upstream use before and after restart in test source. | PASS at source; execution unverified |
| INV-001: tenant isolation | TenantConn write/snapshot paths and bound payload remain; cross-tenant rewrap uses OperatorPool and tenant/row/old-bytes CAS. Tenant-isolation check passes. | PASS at source/supporting check |
| INV-002: no key/provider-secret disclosure | Secret wrappers, redacted views/debug, opaque stored envelope and zeroizing payload buffers; inspected output assertions. | PASS at source; leak-test execution unverified |
| Accurate operational projection of rotation | New optional recipe omits preparation of running readers. | FAIL — FIND-sealing-keyring-2 |
| Required changed Rust documentation | Two changed fallible helpers lack `# Errors`; boot helper falsely claims no IO. | FAIL — FIND-sealing-keyring-1 |
| AC-001: user-journey proof | CLI defect boundary and existing shipped surfaces have source coverage. No historical execution record supplied; candidate journeys not executed here. | Execution not established |
| AC-003: required gates pass | Format, dependency, tenant-isolation and diff checks pass. Full lints, codegen, docs and owning runtime lanes remain unexecuted. | Not established in full |

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: BH-001, consolidated into finding 1 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | BLOCKED on full execution evidence; no source defect proposed |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: STD-001, consolidated into finding 1 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: MAINT-001, consolidated into finding 1 |
| System resilience | [system-review.md](system-review.md) | FAIL: SYS-001, revised into finding 2 |
| Security domain | [domain-review-security.md](domain-review-security.md) | PASS for source boundary |
| Data domain | [domain-review-data.md](domain-review-data.md) | PASS for source boundary |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED: FU-001 corroborates SYS-001; deletion is sufficient |
| Fresh independent validation | [findings-validation.md](findings-validation.md) | Two deduplicated findings; FIX_REQUIRED recommended |

The [claim comparison](claim-comparison.md) grouped findings by cause. A fresh follow-up was necessary because the operational-doc claim was unique while other reports found runtime rotation correct. It established that these conclusions are compatible: runtime supports safe rotation, but the new optional instructions omit its prerequisite. Independent validation then checked all claims, including the unique finding, and selected deletion of the optional sentence. No unresolved disagreement or material decision remains. Placement-only feedback does not contribute to this verdict.

## Validated ledger

Only independently confirmed or revised findings are retained; full diagnoses, caller tracing and closure proof live in [findings-validation.md](findings-validation.md).

| Stable ID | Status / classification | Location and consequence | Selected correction |
|---|---|---|---|
| FIND-sealing-keyring-1 | CONFIRMED / VIOLATION; BH-001, STD-001, MAINT-001 | `boot/mod.rs:1321–1356` and `managed.rs:161–170`: missing required error contracts; boot prose misstates existing IO. Explicit hard rustdoc rule blocks acceptance. | Correct the two existing rustdoc blocks only; preserve code and error behavior. |
| FIND-sealing-keyring-2 | REVISED / INCORRECT; SYS-001, FU-001 | `deploy-wyrd-for-a-team.md:72`: documented rolling key switch can leave running replicas unable to open shared rewrapped secrets. | Delete the optional incomplete rotation sentence; keep setup guidance and runtime unchanged. |

No prior findings or remediation rounds exist. Neither finding requires a specification revision, new product behavior, compatibility support or runtime redesign.

## Verification limits and handoff

[verification.md](verification.md) records PASS for `mise run fmt:check`, `mise run check:tenant-isolation`, `mise run check:deps` (including its one security test), and cumulative `git diff --check`. No full lints, codegen regeneration/check, docs build or runtime journey suites were run. Codegen regeneration was omitted to preserve reviewed source. No older checkout or baseline test was used. Historical Red/Green evidence is absent; that is an evidence limit, not an invented code defect. Full acceptance cannot be declared from source review alone.

All required independent reports are complete and the candidate is stable, so the bounded validated corrections determine FIX_REQUIRED. The incomplete execution obligations remain explicit for later final verification rather than being mislabeled as source failures.

Remediation: [sealing-keyring-R1-documentation.md](sealing-keyring-R1-documentation.md), routed to `$wyrd-implement`. Batch both documentation corrections; run only their narrow checks in remediation and reassess the complete cumulative candidate afterward.

Skipped full runtime/gate execution; its acceptance evidence remains unproven. The selected remediation changes documentation only.
