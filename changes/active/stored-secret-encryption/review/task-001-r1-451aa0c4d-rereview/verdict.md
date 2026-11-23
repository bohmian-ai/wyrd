# TASK-001-R1 verdict: PASS

FIND-TASK-001-1 is closed against the remediated candidate. The focused review
found no new blocking defect or evidence gap. FIND-2 and FIND-3 remain withdrawn
under the maintainer's explicit scope decisions. No further remediation task
is created. Behavioral acceptance does not erase recorded TDD deviations.

## Immutable identities and scope

| Subject | Identity |
| --- | --- |
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd` |
| Prior reviewed candidate / remediation base | `584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82` |
| Reviewed candidate | `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`, `revised-encryption` |
| Original cumulative base | `a859dab1d20806e8ad29d9eb064585126419f5c2` |
| Contract | SPEC-stored-secret-encryption revision 1, [spec.md](../../spec.md) |
| Original task | [TASK-001](../../tasks/TASK-001-common-encryption-capability.md) |
| Remediation | [TASK-001-R1](../task-001-584a5de8b-r1/TASK-001-R1-remote-key-handling-and-proof.md) |
| Prior verdict and findings | [verdict](../task-001-584a5de8b-r1/verdict.md), [validation](../task-001-584a5de8b-r1/findings-validation.md) |

Reviewed `git diff 584a5de8b 451aa0c4d`: 14 files, +649/-150.
The inclusive sequence begins with fix `17d0e3e6b`; its parent is the prior
candidate. No rebase correspondence is required. Production source matches
the immutable candidate; the working-tree packet/scope edits are excluded
from the implementation diff and preserved. Current maintainer instructions
govern interpretation of the withdrawn findings. No CodeGraph index exists.

Authority: root AGENTS; task-review skill and spec-driven scope discipline;
TESTING; Rust/error/maintainer guides; security posture Cryptography and secret
handling/Stored secrets; approved requirements and original/remediation tasks.
This packet protects stored secrets, including auth-owned secrets; it does not
require federation or another authentication stack. No benchmark is assigned.

## Independent review assignment

One fresh `ponytail-rev` performed the remediation review: prior FIND-1 closure,
changed key-buffer ownership, serialization/parsing, growth/drop/redaction,
provider and shared KV consumers, assertion strength and applicable repository
invariants. Its source validation is [findings-validation.md](findings-validation.md).
The orchestrator coordinated and ran verification, without validating its own
findings. No new blocker required separate source validation.

Additional discovery/specialists were omitted because this is the first focused
remediation review of one known secret-handling boundary. There is no new
SQL, distributed lifecycle, runtime cutover or public SDK workflow, and the
single fresh reviewer covered the affected security ownership boundary. Prior
unaffected acceptance evidence is carried forward, not rediscovered.

## Reconciled coverage and closure

| Existing requirement / scenario | Implementation and assertions | Result |
| --- | --- | --- |
| FIND-1; REQ-008; remediation protected remote key representations | `SecretBytes` owner in `wyrd-crypt`; borrowed request/plaintext representations, protected wire bodies and response growth in KMS/Vault; three new behavioral tests and current-candidate adapter/integration runs | PASS — CLOSED |
| REQ-001/002/004; original scenario 1 | Facade/envelope orchestration unchanged; changed unwrap return owner preserves bytes and 32-byte validation; combined suite retains fresh key/nonce, file restart and common-envelope assertions | PASS — prior evidence carried forward and current library suite passes |
| REQ-003/004; original scenario 2 | Context/key trust remains unchanged; new borrowed parser refuses unprotected-copy input; current suite retains context/tamper/disallowed-key/format/DEK-length refusal | PASS |
| REQ-001/004/005; original scenario 3 | Existing direct provider API requests retain wire context and authentication/signing; GCP credential loader restored; real Transit and shared Gateway KV journeys rerun after revert | PASS — protocol/real-service boundaries preserved; no federation requirement |
| REQ-002/004; original scenario 4 | Lifecycle primitives unchanged except protected adapter return type; current suite retains renewal/rewrap distinction and provider failure/recovery proof | PASS |
| REQ-005; original scenario 5 | Configuration/file/memory logic unchanged; current library suite retains validation/no-fallback proof | PASS — prior unaffected evidence remains applicable |
| FIND-2 and FIND-3 | Explicit maintainer withdrawals; Google authentication addition reverted; wait inventory retained without benchmark qualification | WITHDRAWN — not unresolved obligations |

No new external package is introduced by the net remediation. Existing
`bytes`, `serde` and crypto ownership are reused. No cipher, envelope format,
database schema, resource authorization, authentication flow or durable runtime
consumer was added. The earlier four server-test corrections are unchanged;
their prior inspected evidence is carried forward without reopening them.

## External-wait audit

The affected shared path is Gateway attempt → CredentialResolver ExternalSecret
→ VaultBackend → `VaultKv2::read_field` → `Endpoint::send`. The existing one
Vault KV HTTP request and response-chunk awaits remain, with protected response
buffering. No additional Vault request or PostgreSQL, Scribe or object-storage
wait is added or moved by this remediation. It preserves the existing external
secret consumer excluded from encryption replacement. New KMS operations still
have no production consumer in TASK-001. No benchmark is required or claimed.

## Execution, process and limits

Current-candidate verification passed: crypto/KMS/Vault libraries 28/28;
all three new tests via individual exact selectors; real Transit 1/1; and real
Gateway ExternalSecret journeys 2/2. Commands, run IDs and assertions are
recorded in [verification.md](verification.md); source validation and assertions
are in [findings-validation.md](findings-validation.md).

Post-revert Clippy, `check:deps` and `check:workspace-hack` success is reported
by the task, not independently rerun here. Live GCP/AWS accounts were not used;
their local protocol boundary remains the approved proof scope. Broad original
family lanes are carried-forward evidence, not fresh execution claims.

TDD compliance remains **VIOLATED**: the helper's test was written after the
type, and the original task's scenario-2/remote-adapter deviations and diagnosis
limits remain recorded. The remediation records expected RED for the two new
adapter tests and GREEN afterward; this review reproduces present GREEN only.
No historical evidence is manufactured or production churn requested.

This is the first remediation review; no repeated-round convergence assessment
is needed. PASS completes TASK-001-R1's review gate and closes the remaining
original task finding. It does not complete the whole encryption change or
review TASK-002–004, and launches no implementation, further review, merge,
push or deployment.
