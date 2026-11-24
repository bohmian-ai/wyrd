# TASK-001 verdict: FIX_REQUIRED

The shared encryption capability has credible core/protocol proof, but one
validated blocker remains. Behavioral acceptance is **FIX_REQUIRED**; TDD
compliance is separately **VIOLATED**. No implementation, merge, push, or
deployment was performed by this review.

## Immutable subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `a859dab1d20806e8ad29d9eb064585126419f5c2`.
- Candidate: `584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82` on `revised-encryption`.
- Approved contract: `SPEC-stored-secret-encryption`, revision 1,
  [spec.md](../../spec.md).
- Original task: [TASK-001-common-encryption-capability.md](../../tasks/TASK-001-common-encryption-capability.md).
- Reviewed diff: `git diff a859dab1d...584a5de8b`, 26 files, +3849/-106.
  The four authority changes are in the base commit itself, inspected through
  `git diff a859dab1d^ a859dab1d`; including that prerequisite gives the
  user's 14 commits/30 files. The reviewed source remained immutable.
- Pre-existing TASK-002 and TASK-004 working-tree edits are excluded and
  untouched. Only review artifacts were added. No CodeGraph index exists.

This task supplies the facade, five backends, configuration and lifecycle
primitives. Production owner cutover, init integration, SQL replacement,
public resource actions and whole-store maintenance remain later tasks.
The existing Vault KV consumer is included because its shared implementation
changes in this candidate.

Authority inspected: root and docs AGENTS; spec-driven-development and
implementation-execution; agent_docs index/Rust/errors/maintainer style;
TESTING; security posture Cryptography/Stored secrets/Tenant isolation;
Wyrd design Operator connections; operations Configuration authority/
Stored-secret encryption and Backup contract/Restore ordering; current
maintainer hot-path instructions, as amended below.

## Independent assignments

| Role | Assigned boundary and report |
| --- | --- |
| `behavior-rev` | Assigned requirements/scenarios, implementation and assertions, repository rules, maintainability/reuse, failure paths, CI/docs and four server test fixes: [task-review-behavior.md](task-review-behavior.md) |
| `domain-rev` | Crypto/AAD and tenant-owner trust, envelope/key resolution, file custody, provider auth/refresh, secret lifetime/redaction, refusal and lifecycle security: [task-review-domain.md](task-review-domain.md) |
| Fresh `ponytail-rev` | Independent source validation of every proposal, coverage/proof and specialist sufficiency; reconciliation and retained findings: [findings-validation.md](findings-validation.md) |

No separate invariant/system/repo/maintainer reviewer was selected: the
primary retains those rules, while the domain specialist covers the material
new sensitive boundary. There is no additional SQL, shipped SDK projection,
distributed state machine or runtime ownership cutover to assign separately.
The validator found no expertise gap. The orchestrator only coordinated and
ran verification. The shared Vault KV consumer trace remains relevant to the
wait audit; it does not impose a benchmark requirement.

## Reconciled requirement and scenario matrix

| Requirement / original scenario | Implementation and observable assertions | Evidence and result |
| --- | --- | --- |
| REQ-001, REQ-002, REQ-004 / 1 | `wyrd-kms/src/lib.rs:352-398`; crypto OS-random key/nonce; facade tests `:550-607` compare independent unwrapped DEKs/nonces and reopen file envelope with a new facade | Current library and exact focused reruns PASS for assigned capability slice |
| REQ-003, REQ-004 / 2 | Canonical scope/purpose/owner/authority/version and two domains (`lib.rs:117-138`); trusted-reference check (`:456-490`); refusal assertions `:611-738`; zero backend requests for disallowed key (`remote_tests.rs:341-359`) | PASS for observed binding/tamper/format/DEK-size refusal; no fallback path |
| REQ-001, REQ-004, REQ-005 / 3 | GCP/AWS adapters, signed AWS payload, exact canonical context and injected credential refresh (`remote_tests.rs:222-338`); Transit token reread (`:416-508`); real dev Transit rotation/context/token/refusal test | Protocol and real-Transit proof PASS; overall backend acceptance FAIL due to FIND-TASK-001-1; federation is outside scope |
| REQ-002, REQ-004 / 4 | `lib.rs:407-449`; lifecycle assertions `:821-904` distinguish fresh renewal from preserved-DEK/payload rewrap; provider failure/recovery `remote_tests.rs:514-550` | PASS for lifecycle primitives; remote ownership invariant remains FIND-TASK-001-1 |
| REQ-005 / 5 | Config/env validation (`config.rs:124-168,221-331`), local configured-file loading (`local.rs:38-63`), config tests and facade file/memory refusal assertions | Validation/no-fallback observations PASS; new authentication methods are outside scope |
| Applicable secret-handling invariant / new remote implementation | Key-bearing request/response representations in GCP/AWS/Transit | FAIL — FIND-TASK-001-1 |
| Authority prerequisite and task non-goals | Base commit updates four approved authority owners before implementation; no SQL/runtime cutover; provider orchestration remains server tier, primitives remain cloud-free | PASS |
| CI, documentation and test-fix scope | Real Vault lane selected for KMS/Vault; draft reference; four test-only corrections preserve realized-case assertions and root causes | CI routing rerun PASS; wider task-recorded checks retained with limits below |

Core assertions do not claim that all production owners already consume KMS.
No new public encryption API, old-format compatibility reader, migration or
SDK-owned encryption was introduced. GCP/AWS live-cloud execution is outside
the task's explicitly selected provider-fixture boundary.

## Hot-path waits and qualification

`GatewayEngine::attempt` (`wyrd-gateway/src/engine.rs:862-866`) awaits
`CredentialResolver::resolve` (`credential.rs:143-149`) →
`VaultBackend::fetch` (`vault.rs:70-75`) → changed `VaultKv2::read_field`
(`wyrd-vault/src/lib.rs:145-158`) → extracted `Endpoint::send` (`:367-407`).

| Added or moved external wait | Inventory / authority |
| --- | --- |
| Vault | Existing one KV HTTP request and its response-chunk awaits move to `Endpoint::send`; same screening, deadline, request count and refusal behavior. This preserves the pre-existing ExternalSecret operation explicitly excluded from encryption replacement by spec Persistent and authority changes; no newly sanctioned external wait is claimed. |
| PostgreSQL, Scribe, object storage | None added or moved on production hot paths. Scribe hold and server storage edits are test-only. |
| New KMS provider calls, credential lookup and token-file read | No production facade caller yet; these are not added hot-path waits in this candidate. Consuming-task qualification remains required. |

The extraction changes live hot-path code without adding an external wait.
On 2026-11-19 the maintainer explicitly withdrew the automatic benchmark
requirement and FIND-TASK-001-3. No benchmark is required to close this review.
The wait inventory above remains applicable. No latency regression is asserted.

## Verification, process and handoff

[verification.md](verification.md) records this review's 25/25 library tests,
all 15 KMS tests and new Vault unit test through individual exact selectors,
real dev Vault 1/1, CI routing 64+7 and diff whitespace check, all passing.
The original task records format/lints/dependency/docs and wider family lanes,
including `test:wyrd` 2849/2849 and `test:shared` 898/898. Those broad results
were not independently rerun here. No live GCP/AWS or full Gateway benchmark
was run. Tests establish present GREEN, not historical RED.

TDD remains VIOLATED for scenario 2 and remote adapter work. The directory
test fix also lacked its required prior diagnosis consultation, as recorded;
consultation for the condvar fix is not independently documented. Mutation
tests/current passing tests do not repair process history. Do not create
production churn or retroactive RED evidence to close these deviations.

Retained finding: **FIND-TASK-001-1**. FIND-TASK-001-2 is withdrawn after
maintainer clarification that federation is outside this encryption packet.
No dependency revert is prescribed by that withdrawal.
FIND-TASK-001-3 is withdrawn by maintainer direction; it is not open work. Details and independent validation are recorded once in
[findings-validation.md](findings-validation.md). The cohesive handoff is
[TASK-001-R1-remote-key-handling-and-proof.md](TASK-001-R1-remote-key-handling-and-proof.md).
This is the first review; there is no prior closure or remediation-round
convergence assessment. The next action is bounded remediation and its
maintainer-initiated review; this review launches neither.
