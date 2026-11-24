# TASK-002 independent findings validation

Subject: base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a` to candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`, approved
SPEC-stored-secret-encryption revision 1, original TASK-002.
The validator is separate from all discovery reviewers and inspected the immutable
source, affected callers, actual diff, assertions and authority. No production files
were changed; no Cargo command was executed by this validator.

## Proposal dispositions

| Discovery ID | Disposition | Retained ID / reason |
| --- | --- | --- |
| DOMAIN-1 | CONFIRMED | FIND-TASK-002-1: distinct human connection rows share identical authenticated context. |
| DOMAIN-2 | CONFIRMED | FIND-TASK-002-2: whole-tenant prepare-before-commit makes healthy aggregate work lose all progress at the tenant deadline. |
| SYSTEM-001 | CONFIRMED | FIND-TASK-002-3: installed unprepared credentials lose retryability and return 502. |
| Response-semantics discrepancy; focused independent domain follow-up | CONFIRMED | FIND-TASK-002-4: a losing existing credential replacement retries over the winner instead of returning the required conflict. |

These are implementation defects established by source paths, rather than assertions
that missing regression tests alone prove incorrect behavior. The existing GREEN
assertions cover adjacent cases, not the failing starting states described below.

## Retained findings

### FIND-TASK-002-1 — INCORRECT: auth envelopes omit the human connection owner

Authority: REQ-003; architecture/wyrd-security-posture.md Stored secrets requires
refusal when an envelope is copied to another owner's row. TASK-002 scenarios 3–4
exercise actual auth consumers and owner substitution, not only cross-tenant refusal.

Location: `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:534–542,629–636`;
writer `connections.rs:237–249`; maintenance projection
`crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:467–469`.

Starting state/trigger: a tenant replaces an OIDC client credential using its normal
Active/Candidate connection flow. Both connections name the same issuer, OAuth
client and method, but have different connection IDs. Substitute the Active row's
ciphertext into the Candidate row, then complete candidate sign-in.

Expected: the concrete Candidate owner refuses another connection's ciphertext.
Actual: `ClientSecretOwner::context` uses tenant, purpose, issuer URL, client ID and
method, with `version: None`. The writer, human row decoder and maintenance reader
all omit `connection_id`; both rows therefore supply byte-identical context.
`tested_candidate` validates the selected Candidate ID/revision but passes its row
through that decoder, so this check does not authenticate ciphertext ownership.
If the prior provider secret remains valid during normal overlap, it is used at the
token endpoint rather than refused. Database modification is the explicitly approved
substitution threat; no public plaintext-decryption endpoint is alleged.

Proof: schema permits one Active and one Candidate; insert assigns a separate ID,
and `put_candidate` preserves that identity across candidate updates. Consumer
`connections.rs:417–446` selects the exact Candidate but decoder context ignores its
identity. Both encryption layers receive that identical context for both rows.
Existing cross-tenant resolver proof changes tenant, and Operator substitution proof
uses a different owner with correct context; neither closes this same-tenant gap.

Correction boundary: carry concrete trusted human connection identity through the
existing candidate preparation/commit, open and maintenance paths under the common
facade. Preserve outside-transaction remote calls and candidate/revision/session
semantics. No new table, cipher, authentication capability or key owner is required.
Closure: actual same-tenant Active/Candidate consumer use must refuse copied envelopes
while intact connection credentials still authenticate; retain cross-tenant refusal.

### FIND-TASK-002-2 — REGRESSION: Operator rewrap can repeat forever without progress

Authority: TASK-002 Existing owners and scope expressly retains maintenance safety
before TASK-004; REQ-009/012 retain the owning lifecycle. The Operator architecture
requires bounded, cancellable rewrap. This finding preserves existing maintenance,
and does not move future whole-store administration into TASK-002.

Location: `crates/wyrd/wyrd-server/src/components/operators/secrets.rs:194–208,230–262`;
30-second tenant budget in `src/verification/mod.rs:234–235`.

Starting state/trigger: a tenant has enough old-key connections that sequential,
individually healthy remote unwrap/wrap work exceeds the tenant budget in aggregate.
A configured new active key and retained old key trigger periodic maintenance.
Expected: bounded committed progress permits subsequent passes to finish. Actual:
`rewrap_tenant` lists the entire tenant, prepares every noncurrent envelope into one
Vec, and only then starts any fenced swap transaction. Timeout drops every prepared
result before any commit. Every subsequent pass reads the same old-key rows and
repeats the same work, blocking convergence despite a healthy key provider.

Proof: base `keys.rs` limits stale selection to `REWRAP_BATCH = 100` and commits that
bounded batch. The candidate removes that selection/limit. The current timeout still
wraps the entire read/prepare/all-swaps future. This is not merely hypothetical
unbounded memory: the retained deadline makes zero progress deterministic whenever
aggregate preparation exceeds it. Existing slow-provider tests cover timeout/isolation
and recovery for a small row count, not successful work spanning deadlines.

Correction boundary: reuse OperatorSecrets and SQL exact-envelope/version swaps to
commit bounded completed work without requiring all tenant remote calls to complete
inside one deadline. Retain cancellation, delivery independence, and no remote IO
inside transactions; no scheduler, job table or new policy is needed.
Closure: a real facade/local remote-service fixture with individually successful delays
and a small budget must commit a subset in one pass and converge over repeated passes,
while concurrent replacement/disable remains fenced and preserved.

### FIND-TASK-002-3 — INCORRECT: completed key-service failure returns Gateway 502

Authority: explicit current user requirement that key-service outage causes retryable
503 on the affected credential, recorded also in TASK-002 scenario 5; REQ-009 retains
background preparation and resource-local failure handling.

Location: server `components/gateway/service.rs:1057–1068`,
`snapshots.rs:253–266,360–368`; gateway `credential.rs:138–139`,
`engine.rs:868–881`; server `components/gateway/invocation.rs:962–965`.

Starting state/trigger: background loading of a managed Gateway credential completes
with a retryable KMS failure, such as a prompt HTTP 500. Invoke its deployment while
that key service remains down. Expected: exact retryable service-unavailable 503 and
no dispatch, with unrelated usable credentials available. Actual: `credential_snapshot`
returns secret None and transient aggregate retry=true; reload installs that snapshot
then requeues it. Cached calls bypass the cold-miss 503 branch. The runtime credential
has lost the transient reason; resolver returns Unavailable, engine folds it into
BeforeDispatch, and invocation returns GatewayUpstreamUnavailable, catalog HTTP 502.

Proof: all branches above are explicit. No fallback is needed for the supported
single-deployment route. The existing cold journey stalls unwrap for one minute,
makes two early calls, then restarts on a working endpoint; it never requires a
failed load to finish and install, nor observes in-process recovery. Thus its 503
assertions cannot falsify this failure path. No stale secret reuse or plaintext leak
is alleged.

Correction boundary: preserve the affected credential's transient preparation state
through the existing snapshot/resolution/execution boundary and project the existing
503 classification. Keep unrelated credentials/fallback usable, background retries,
generation fencing, immutable admitted snapshots and memory-only warm calls.
Closure: complete a retryable remote failure, assert repeated exact 503 and no
provider dispatch, verify an unrelated healthy credential, restore that same key
service in the same running process and observe background recovery without editing
the resource or restarting. No new cache, TTL or invocation-time KMS wait is needed.

### FIND-TASK-002-4 — INCORRECT: losing Operator replacement retries over the winner

Authority: REQ-008 explicitly requires conflict when a replacement/revocation wins
concurrently, and commits only against unchanged observed secret/authority state.
TASK-002 assigns that requirement to existing writes and scenario 4 expressly races
an existing replacement with revoke/update. REQ-006 includes existing replacement
operations. TASK-003 adds/extends administration; it does not exempt the owner being
changed here. Finalized admin journey corroborates CAS-and-conflict behavior.

Location: `crates/wyrd/wyrd-server/src/components/operators/service.rs:259–313`;
contradictory assertion `tests/integration/pg_operator_connection_routes.rs:190–237`;
incorrect implementation explanation TASK-002 scenario 4 evidence at lines 162–167.

Starting state/trigger: replacement A reads the current row and waits on remote
sealing; a disable or credential replacement B commits before A's conditional update.
Expected: A returns existing OperatorConnectionConflict and leaves B's committed state
unchanged. Actual: SQL's config/status/version/envelope fence returns None correctly,
but CHANGE_ATTEMPTS rereads B's row, reapplies A's original request and can return 200.
For concurrent replacements this overwrites B's newly committed credential with stale
A's requested value; for disable the secret is replaced after the disable, although
Disabled itself is correctly preserved. The explicit conflict contract is lost.

Proof/validation: the initial domain report identified the discrepancy, and fresh
validator traced the loop, SQL fence and exact test assertions. A focused independent
domain follow-up (`focused-response-semantics.md`) then separately established the
requirement's existing-operation scope, concrete competing-replacement path, and base
history. Validator independently rechecked these sources before retention. Base has
no retry loop; the new source's correct SQL fence is bypassed by reread/reprepare
orchestration. The positive test demands 200 plus a third encrypt and changed secret
version, proving the contrary implementation rather than approved behavior.

Correction boundary: when a prepared replacement loses the existing exact-row fence,
return the already-defined conflict instead of automatically applying that request
to the winner. Preserve outside-transaction remote IO, disable, redaction and SQL
fences. Correct the race assertion and the evidence claim that retry is the contract.
Closure: delayed remote sealing followed by disable must leave status, secret envelope
and version as the disable committed, with exact conflict on replacement; a competing
replacement must retain the winner's envelope/value and return conflict to the loser.
Use the existing real server/Postgres/remote-facade fixtures and focused owner runner.
No new endpoint, schema, encryption owner or coordination machinery is required.

## Response-semantics disposition

This is FIX_REQUIRED under settled REQ-008, not BLOCKED or SPEC_REVISION_REQUIRED.
The original scenario asks to preserve the winner, and permits the specified conflict.
Only implementation evidence asserts that a 409 was wrong because the new loop is
“the contract.” That ledger and its test cannot override the spec. Both the task's
actual scenario mechanics and approved requirement can be satisfied together by the
bounded owner correction. The current user summary promises preserved Disabled and
does not explicitly approve success-after-reprepare or concurrent replacement loss.
No new product semantics or retroactive requirement is introduced.

## Coverage reconciliation

| Existing requirement / scenarios | Source, assertions and execution | Result |
| --- | --- | --- |
| REQ-001; 1–3,6 | build_kms constructs one facade; server composition passes its Arc to auth/Gateway/Operator. Real init rows and consuming auth/platform/delivery/Gateway paths route through that facade; legacy owners are removed. | PASS |
| REQ-002; 1,3 | Accepted shared facade retains independent random DEK/nonce behavior; current root KMS/crypt 21/21 and common-envelope init assertions. No owner adds another cipher. | PASS |
| REQ-003; 3–4,7 | Operator tenant/connection/provider/name/version context, Gateway tenant/name/provider context, platform scope, backend context assertions and consumer refusal remain. Human connection owner is omitted. | FAIL: FIND-TASK-002-1 |
| REQ-004; 3–4,7 | Common envelope/trusted configured key selection retained from TASK-001; provider refusal/tamper tests and fresh KMS/Vault results support it. | PASS |
| REQ-005; 1–3,6–7 | Actual init persists protected file key and reuses it; restart journey uses one stored connection in Python/TypeScript/Rust; cloud tests construct production facades/config/credentials. Live cloud is an accepted CI limit, not claimed GREEN. | PASS within accepted limit |
| REQ-008; 3–4 | Remote key calls outside locks, SQL exact-row fences, tenant capabilities and redacted views/buffers traced. The service retries a losing replacement over its winner. | FAIL: FIND-TASK-002-4 |
| REQ-009; 2,5 | Restart delivery, warm request counts, generation/listener fences and immutable snapshots have credible assertions/source. Completed failed preparation has wrong caller classification. | FAIL: FIND-TASK-002-3 |
| REQ-012; 1–2,6 | Competing readers/config/crypto ownership removed, initial schema replacement is explicitly authorized, provider/setup docs and runnable guide examples exist. Existing Operator maintenance progress regresses. | FAIL: FIND-TASK-002-2 |

Primary plus domain and system assignments cover the materially affected boundaries.
Security/persistence/concurrency and Gateway recovery each received separate expertise;
primary covers broad repository rules, composition, provider/CI/docs and owner simplicity.
Their scoped maintainability checks make a separate maintainer/repo reviewer unnecessary;
complex state/lifecycle is explicitly covered by domain/system, so no additional invariant
reviewer is needed. No selected report is missing. The primary report's description of
the Operator reprepare loop as pre-existing is inaccurate: `CHANGE_ATTEMPTS` is added
in this diff. It must not be used as evidence of unchanged response semantics.

## Verification and TDD

Root verification.md binds current KMS/crypt 21/21, exact init 1/1, managed Vault1/1,
exact-selector Vault1/1 with the task-scoped tool supplied, and diff check to the
candidate. The first direct exact Vault invocation lacked that scoped executable;
its setup failure is not a product failure, RED, or contradiction of managed GREEN.
Historical task proof supplies the additional owner journeys/checks; no fresh aggregate
execution is claimed by validation. Live AWS/GCP and Linux startup remain explicitly
accepted CI limits. Current root AGENTS/spec require explicit benchmark request, so
no benchmark is imposed. System report's wait inventory traces no new warm PG/KMS
wait, cold single-catalog reload, background unwrap and post-commit administration
warming; no provider IO was moved into invocation.

TDD is VIOLATED for scenarios 2 and 3 as recorded. Current GREEN and future regression
tests do not repair that history. No production churn or manufactured RED is required.

## Recommendation

**FIX_REQUIRED** with FIND-TASK-002-1 through FIND-TASK-002-4. All four have
independent discovery and source validation. There is no unavailable selected reviewer,
unresolved approved-contract ambiguity or required external access preventing this
verdict. One cohesive remediation task can correct these existing owner boundaries
and add narrowly focused consumer/progress/failure/race proof. Do not implement future
TASK-003/004 administration, broaden provider authentication, add benchmark gates or
manufacture historical TDD RED to close these findings.
