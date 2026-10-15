# TASK-001 re-review verdict

**FIX_REQUIRED — FIND-TASK-001-4 remains partially open.** The other five prior findings are closed. No new finding ID, specification revision, authentication source or production runtime redesign is required.

## Immutable subject

- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Prior candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Reviewed candidate: `07419b4d060303959384b682ca88e8673c722e63`.
- Approved authority: [specification revision 11](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-unify-sdk-handle-instantiation.md), original content at the base, interpreted under approved revision 11.
- Complete subject, navigation and verification limits: [subject.md](subject.md).

The complete base-to-candidate change was reviewed; the latest remediation diff served as navigation. HEAD and tracked source remained unchanged. Only this new review directory was written.

## Independent review and validation

| Required independent role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: residual FIND-4 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: residual FIND-4 |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: consolidated residual FIND-4 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: three fallible helper paths |
| System resilience | [system-review.md](system-review.md) | PASS |
| Security domain | [domain-review-security.md](domain-review-security.md) | PASS |
| Concurrency domain | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL: residual native insertion contract |
| Fresh structured Ponytail validator | [findings-validation.md](findings-validation.md) | FIX_REQUIRED: one REVISED stable finding |

[Claim comparison](claim-comparison.md) records why no focused discovery follow-up was needed: the claims trace the same documentation obligation, and no conflicting runtime path or unresolved material decision remained. Independent validation adjudicated every proposed site, including standards-only sites. Agreement alone did not establish a finding.

## Reconciled acceptance

PASS here means source and inspected test assertions support acceptance; execution was not repeated.

| Original task obligations, under approved revision 11 | Result |
|---|---|
| REQ-001–011: common optional-client construction, eager/lazy semantics and removed forms | PASS |
| REQ-012: task-owned API documentation | FAIL, bounded native operation contract in FIND-4 |
| REQ-013–019: governing references, retained client/cache, auth module and compact discovery | PASS |
| INV-001–005: server ownership, ambient resolver, identity, secrets and language boundaries | PASS |
| AC-001–005: public surface, ambient/negative/delegated proof and IO-free Python construction | PASS under approved revision 11 |
| AC-006: task-owned API documentation, typing and declarations | FAIL, bounded mandatory Rust documentation in FIND-4; typing/declarations otherwise supported |
| AC-007–009: retained configuration/renewal and typed compact discovery | PASS under approved revision 11 |

The validator's compact reconciliation and both full implementation matrices contain the source/test evidence for every ID. TASK-002 owns docsite/README work; it is not a TASK-001 omission. Approved revisions 10/11 settle Platform saved-config applicability and injected Rust Environment proof. Neither decision is reopened.

## Validated ledger and prior closure

| Stable ID | Result |
|---|---|
| FIND-TASK-001-1 | CLOSED: local Python Bifrost access and Cards view selection no longer initialize transport. |
| FIND-TASK-001-2 | CLOSED: one production ambient resolver, used by approved Rust injected-environment journeys; Python eager AsyncBifrost ambient proof added. |
| FIND-TASK-001-3 | CLOSED: all three languages inspect retained audit subject and actor attribution. |
| FIND-TASK-001-4 | **REVISED / OPEN**: native insertion timing/errors, three fallible helper connection panics, and the specifically listed changed-item error/panic documentation remain incomplete. |
| FIND-TASK-001-5 | CLOSED: revoked saved login is asserted to fail during eager connection itself. |
| FIND-TASK-001-6 | CLOSED by approved authority: saved-only success applies to tenant handles; Platform preserves its existing credential/session path. |

The sole retained finding is a VIOLATION of mandatory changed-item rustdoc (AGENTS §16 and agent rules), REQ-012 and AC-006. Native insert still promises unconditional nonblocking behavior although its first call can synchronously connect and fail. Three already-fallible ApiKey test helpers unwrap the new eager connection result, bypassing their existing error channels. The validated changed tests and writer helper lack their required actual failure contracts. This is the explicit mandatory documentation exception, not optional style cleanup. No new production runtime defect is asserted.

The [validated ledger](findings-validation.md) contains exact sites, per-proposal dispositions, counterevidence and the smallest correction. Bearer-only and prefilled-cache tests must not be documented as necessarily performing an exchange. Previously corrected sites remain closed; untouched and name-only migrations are excluded.

## Remediation and verification limits

Implement [TASK-001-R1-complete-connection-contracts.md](TASK-001-R1-complete-connection-contracts.md) directly with `$wyrd-implement`. It batches the documentation corrections and three existing-Result propagation changes. It requires no spec change, new error type, new harness or new authentication behavior.

Reported focused Rust/Python/TypeScript, CLI and managed identity checks were examined through their source assertions and recorded evidence. No raw transcripts were supplied; no tests, builds, formatters or generators were run during this static review. Green lint and broken-link results cannot prove semantic rustdoc accuracy. The remediation specifies only the three affected existing journeys and Rust checks; broad integrated suites belong to change review.
