# TASK-002-R1 review: FIX_REQUIRED for requested GCP follow-on

**All four original findings are CLOSED.** The R1 correction is behaviorally
accepted. No new independently discovered blocking finding remains.
TASK-002 stays **FIX_REQUIRED** solely for the maintainer's explicit addition:
shared Google credential resolution for storage and KMS.

Next task: [TASK-002-R2-shared-google-credentials.md](TASK-002-R2-shared-google-credentials.md).
This task is created by this invoked `wyrd-task-review`, not by an advice or
implementation turn. It packages user-requested scope; it is not FIND-TASK-002-5.

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`.
Cumulative base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
Prior reviewed candidate `c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`.
Reviewed candidate `52f69ed2da3846733be0d314c6627c95d308e8dd`.
Direct ancestry verified; latest review diff is `git diff c8cbfeeb0 52f69ed2d`.
HEAD matched before and after execution; tracked source remained clean. Only this
new review directory was written. No source archives or patches are stored here.

Contract: [approved spec revision 1](../../spec.md), original
[TASK-002](../../tasks/TASK-002-standardize-secret-owners.md), prior
[verdict](../task-002-c8cbfeeb0/verdict.md), validated findings and
[R1 remediation/evidence](../task-002-c8cbfeeb0/TASK-002-R1-secret-owner-binding-and-runtime-safety.md).
The current user explicitly requests the GCP credential follow-on, corroborated
by the [pending request note](../task-002-c8cbfeeb0/gcp-credential-parity-addendum.md).
This is existing credential-source integration under REQ-005/012, not a new
identity flow or a specification-boundary change.

Governing authorities: root and nested SQL AGENTS; changes/spec-driven-development.md;
TESTING.md; applicable agent_docs guides; architecture/wyrd-security-posture.md
stored-secret and tenant rules; architecture/wyrd-design.md Operator/Gateway;
operations deployment configuration and backup/restore authority. No migration or
architecture change is introduced by R1. No CodeGraph index exists.

## Independent coverage

Fresh `r1_ponytail_rev` independently checked prior closure, changed auth/SQL/Operator
paths, assertion strength, scope, evidence and applicable rules, and validated the
Gateway specialist. Its [findings-validation.md](findings-validation.md) retains the
stable original IDs and separately records the human addition.

Fresh `r1_system_rev` covered materially changed Gateway failure propagation,
accounting/breaker/fallback behavior, same-process recovery, immutable snapshot
lifecycle and external waits: [task-review-system.md](task-review-system.md).
No further discovery specialist was needed: the bounded security/persistence/
concurrency corrections are traced by the fresh validator using prior independent
coverage, while the material new engine error propagation received separate system
expertise. Maintainability and cross-tier rules are included in those assignments.
No selected report or required boundary coverage is missing. The root orchestrated,
executed focused checks and compiled the result; it did not review its own findings.

## Original finding closure

| Stable finding | Corrected owner and consumer proof | Disposition |
| --- | --- | --- |
| FIND-TASK-002-1 | Auth ClientSecretOwner includes connection ID at stage/open and SQL maintenance reconstruction; candidate identity/revision fences protect commit. Exact real-auth substitution and rotation/maintenance consumers pass 2/2. | CLOSED |
| FIND-TASK-002-2 | OperatorSecrets commits each exact-envelope/version swap before preparing the next row; timeout retains durable progress/count. Exact progress test passes, plus owning Operator lane 20/20 after final JSON assertion correction. | CLOSED |
| FIND-TASK-002-3 | Gateway per-credential pending state survives resolver/engine/HTTP mapping; no dispatch/billing/breaker penalty, fallback preserved. Exact completed-failure/503/isolation/same-process recovery journey passes 1/1. | CLOSED |
| FIND-TASK-002-4 | Operator change returns existing conflict immediately when its observed-row fence loses; disable and winning replacement survive. Exact conflict/winner-delivery/progress group passes 3/3 and full owning lane passes. | CLOSED |

The validator independently confirmed the specialist report rather than relying
on agreement. No new discovered blocker required a further source-validation pass.

## Reconciled requirement matrix

Unchanged prior source-backed acceptance is carried forward; this remediation
review does not restart unrelated backend, setup or SDK discovery.

| Requirement / original scenarios | Implementation and strongest applicable proof | Result |
| --- | --- | --- |
| REQ-001 / 1–3,6 | Prior boot build_kms/common Arc composition and real owner consumption unchanged; corrected auth/Gateway/Operator consumers still use the shared facade. | PASS |
| REQ-002 / 1,3 | Shared KMS random-DEK/nonce/envelope implementation unchanged; prior accepted crypto/init proof remains applicable. | PASS |
| REQ-003 / 3–4,7 | connections.rs stage and pg_resolvers.rs human decoder authenticate connection ID; human_connections.rs maintenance builds matching context. Exact real OIDC substitution/refusal and rotation consumers pass; other ownership binding carries forward. | PASS |
| REQ-004 / 3–4,7 | Configured key selection/refusal unchanged; pending Gateway classification neither tries unrelated keys nor carries old opened material forward. | PASS |
| REQ-005 / 1–3,6–7; explicit current addition | Prior backend/init/restart proof remains accepted. Storage factory service_account_json inputs are still omitted by KMS gcp.rs production constructor. Current user requires one shared resolution path. | FAIL only for user-requested addition |
| REQ-008 / 3–4 | Auth candidate CAS and Operator immediate conflict preserve observed state; SQL capabilities/RLS, redaction and outside-transaction crypto retained. Exact race and actual winner delivery tests pass. | PASS |
| REQ-009 / 2,5 | Gateway snapshot/credential/engine/invocation pending propagation and actual recovery pass. Prior listener/generation/admitted snapshot proof remains valid; per-row maintenance preserves progress. | PASS |
| REQ-012 / 1–2,6 | Prior competing-system deletion/config/schema/docs cutover retained; maintenance regression fixed and owning tests pass. Credential-source setup documentation will accompany the explicit GCP addition. | PASS for original assignment; addition remains in R2 |

## User-requested follow-on and limits

The next task preserves both existing Google JSON inputs, their established
precedence and malformed-input behavior, supported credential-file/default fallback,
provider refresh and redaction. Storage and KMS must use one small shared owner,
not duplicate resolution or introduce a KMS dependency on the storage data plane.
Proof must exercise production construction paths; an injected credential provider
alone cannot demonstrate environment selection. Setup guidance must explain shared
identity with separately required KMS permissions. No new auth flow, provider SDK,
new schema, benchmark or broad journey aggregate is added.

This is the first remediation review. R1 converged on all four original findings;
the remaining work is explicitly requested scope, not an incomplete fix, fix-induced
regression, missed independently retained defect or unsuccessful second round.
Only the GCP addition belongs in R2. Review does not launch its implementation.

## External waits

R1 adds or moves **zero hot-path waits** on PostgreSQL, Scribe, object storage,
Vault or KMS. Managed Gateway resolution reads the admitted snapshot; cold misses
retain the sanctioned catalog reload and queued background preparation. Snapshot
failure classification/accounting adds no provider or persistence IO. Existing
background reload and post-commit administrative warming retain their owners.

Outside defined hot paths, auth stage adds a short tenant read/commit before
remote sealing; Operator change removes retrying administrative read/seal/write
cycles; Operator rewrap replaces whole-tenant persistence with one short fenced
transaction per completed row. No remote crypto runs within those transactions.
The system/validation reports contain the full reconciled path inventory. No
unsanctioned caller-waited IO or new runtime/client construction was found.

## Execution and process

[verification.md](verification.md) binds fresh executions to this candidate:
Operator owning lane **20/20**, exact Operator closure **3/3**, exact Gateway
recovery **1/1**, exact auth substitution/rotation with real Keycloak/Dex **2/2**;
setup migration-idempotency checks and diff whitespace check also pass.
The full Operator rerun closes the handoff's stated post-assertion-correction gap.

The JSON assertion correction matches the persisted Operator secret representation;
it preserves the progress/convergence/concurrency assertions and is not a production
redesign. R1's four recorded expected-behavior RED/GREEN cycles support **TDD PASS**.
These GREEN reruns do not reconstruct historical RED. Original TASK-002 scenarios
2/3 remain **TDD VIOLATED**; remediation acceptance does not erase that history.

Format, lints, dependency/tenant checks and codegen are recorded implementation
results, not freshly rerun here. No full multi-language aggregate is claimed.
Live AWS/GCP and Linux-only startup remain the user-accepted CI limits. Current
AGENTS/spec require an explicit current benchmark request; none was made, so no
benchmark is imposed. No implementation, commit, push, merge or deployment occurred.
