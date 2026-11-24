# TASK-002-R1 independent remediation validation

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; cumulative base
`451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`; prior candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`; candidate
`52f69ed2da3846733be0d314c6627c95d308e8dd`.
Reviewed the direct remediation diff and affected consuming paths against approved
SPEC-stored-secret-encryption revision 1, original TASK-002, prior verdict/findings,
and TASK-002-R1. Source remained immutable. This validator is separate from prior
discovery reviewers, current implementation and the current Gateway specialist.

## Prior finding closure

| Stable finding | Independent source/assertion validation | Disposition |
| --- | --- | --- |
| FIND-TASK-002-1 | HumanConnections::stage reads the candidate ID in a short tenant transaction, commits, then seals to that ID or a fresh insertion ID. put_candidate checks the observed candidate ID and supplied expected revision under the existing slot lock. ClientSecretOwner authority adds connection ID for human connections, while issuer/platform contexts remain unchanged; decoder and SQL maintenance projection reconstruct the same ordered authority. The real identity consumer test copies Active ciphertext onto a same-issuer/client/method Candidate, asserts exact 503, leaves Candidate untested, proves Active still authenticates, then tests an intact restaged Candidate. Existing rotation journey opens the corrected context after maintenance. | **CLOSED**; source, assertions and fresh execution reconciled below. |
| FIND-TASK-002-2 | OperatorSecrets::rewrap_tenant commits its read before any remote call, then commits each exact-version/envelope fenced swap before preparing the next row. Previously committed swaps and the caller-owned moved count survive tenant timeout/cancellation. The real remote-facade/Postgres test uses individually successful delayed operations, observes partial progress, converges across passes, and preserves racing replacement and disable. | **CLOSED**; source, assertions and fresh execution reconciled below. |
| FIND-TASK-002-3 | Validated the selected system report against credential_snapshot/resolver/engine/respond and ledger source: per-credential Pending survives a completed failed reload; it returns the existing 503 without dispatch, remains nonbillable, skips breaker failure and permits fallback. The real journey asserts repeated exact 503, unrelated availability and same-endpoint recovery without restart/edit. | **CLOSED**; source, assertions and fresh execution reconciled below. |
| FIND-TASK-002-4 | OperatorConnectionControl::change no longer retries a lost conditional update. It returns the existing conflict after committing the no-op losing transaction; it never applies the original request over the winning row. Exact config/status/envelope/version SQL fences remain. Route test asserts 409 exact code, unchanged winning envelope/version and Disabled; competing-replacement delivery test asserts first success, second conflict and actual provider delivery using the winner. | **CLOSED**; source, assertions and fresh execution reconciled below. |

Exact owners: wyrd-auth/src/connections.rs:237–352 and pg_resolvers.rs:542–554,
626–663; wyrd-sql/src/queries/auth/human_connections.rs:230–249,469–474;
server components/operators/secrets.rs:172–270 and service.rs:248–313;
server identity_e2e.rs:4809 and its existing rotation consumer;
pg_operator_delivery.rs:967,2265; pg_operator_connection_routes.rs:189.

The auth owner keeps the same candidate identity through replacement; fresh ID
selection is guarded against the candidate slot changing between preparation and
commit. Remote sealing remains outside locks. Actual production SQL paths accept
TenantConn and named OperatorPool capabilities; RLS/tenant authority is unchanged.
The per-row rewrap change preserves the existing conditional swap owner rather
than adding another persistence workflow, scheduler or progress table. Disable
state is not changed by rewrap, and replacement races are protected by envelope
and version comparisons. No migration, dependency, new public action or crypto
implementation appears in this remediation diff.

## User-requested GCP credential addition

This is an explicit maintainer addition for the next remediation, **not a newly
independently discovered FIND-TASK-002-5**. The pending input and current user request
are its authority. REQ-005 existing supported credential sources and REQ-012 setup
consistency provide the existing packet context; no authentication capability or
material contract change is being invented by review.

Source check at this candidate confirms the gap remains: storage
factory/mod.rs:90–99 resolves GOOGLE_ACCOUNT_JSON_BASE64 or
GOOGLE_APPLICATION_CREDENTIALS_JSON, and cloud.rs:90–97 applies the result through
with_service_account_key. KMS gcp.rs:47–59 constructs from the underlying provider
environment builder without this Wyrd resolution. A deployment using one of these
inputs alone for storage need not supply that identity to KMS; it can fail there
or select ambient credentials instead. Neither R1 nor the accepted cloud CI limit
implements this requested parity.

The smallest correction is one authoritative existing-input resolver shared by
both construction paths at a suitable dependency boundary. Keep the existing
provider parser, refresh and clients; do not make KMS depend on storage's SQL/data
plane, introduce a provider SDK, or add new credential configuration/auth modes.
Preserve current behavior exactly: present UTF-8 base64 takes precedence; decoding
or decoded UTF-8 failure returns None without falling through to raw JSON; raw JSON
is considered when that base64 input is absent/unreadable. A None result leaves the
existing provider chain in charge. This task must not silently replace that policy
with new fail-fast or fallback semantics.

Required focused proof should drive production credential construction for both
consumers under each existing input, demonstrate the selected service-account
identity, pin both-input precedence and current malformed behavior, and retain
credential-file/default-chain selection when neither is provided. KMS proof needs
actual credential acquisition/use through its normal constructor against local
fixtures; injecting a prebuilt credential provider cannot prove environment loading.
Update the stored-secret setup reference/affected runner descriptions so a storage
user knows the same input is accepted while key permissions remain separate. No
live cloud account, new lane, benchmark, schema or full journey aggregate is needed
for this input-selection regression.

## Process and evidence assessment

R1 records expected behavioral RED for all four closure scenarios before its
production changes: auth copied ciphertext incorrectly completes sign-in; rewrap
commits zero rows; completed Gateway preparation returns 502; and losing Operator
replacement returns 200. These are behavioral failures, not setup/compilation RED.
Recorded TDD for R1 is PASS; this review does not claim to recreate historical RED
from current GREEN or Git commit order. Original TASK-002 scenarios 2 and 3 remain
TDD VIOLATED; remediation acceptance does not erase their history.

The rewrap test's first GREEN reached its final assertion only after progress,
convergence and winning writes were exercised. The production secret representation
is JSON: OperatorSecrets::seal consumes serialized secret data, and open returns
that payload. Correcting the final assertion to parse JSON and compare its value
is consistent with the owner and does not weaken progress/race assertions. This
is a test representation correction, not a further production redesign or a
manufactured RED. Fresh execution is bound to the candidate in verification.md;
final results are reconciled below.

No Cargo command was run by this validator. Root coordinates execution. Live
AWS/GCP and Linux-only production startup retain the maintainer's explicit CI
limits; no successful live provider/image execution is claimed. No benchmark is
required by current root AGENTS/spec without a maintainer request for this change.
Only review reports were written; no source, existing task or authority was edited.

## Gateway specialist validation and wait reconciliation

The selected task-review-system.md is present and independently source-backed.
The snapshot pending bit is local to the affected credential and set only for
KmsError::Unavailable. Generic unusable/tampered credentials retain their existing
classification. Resolver failure occurs before dispatched is set or provider work
begins. Engine after_failure sends CredentialPending to Continue without recording
a breaker failure; successful fallback returns ordinary success, while provider
refusal/interruption precedence remains intact. The final pending marker applies
only after failed fallthrough with at least one pending credential and no billable
attempt. The HTTP mapper emits the existing ServiceUnavailable code. Accounted::new
prices only billable attempts and preserves existing unused reservation handling;
no settlement query is added. This corroborates specialist coverage rather than
accepting its conclusion by agreement. No new proposal needs validation.

The closure journey observes a completed failed reload, not only an unfinished
remote request: it toggles the same remote fixture to HTTP 500, triggers reload,
waits for installed refusal, asserts two exact 503s and unchanged upstream calls,
proves unrelated no-credential use, then restores that endpoint and observes
credential-authenticated success in the same process. Billing/breaker behavior is
established by direct engine/ledger branches and retained adjacent resilience
proof; this review does not claim the new journey measures internal counters.

Compared with the prior candidate, managed Gateway invocation adds no PostgreSQL,
Scribe, object-store, Vault or KMS wait. Warm resolution reads the admitted
snapshot; cold misses retain the sanctioned catalog reload and background queued
preparation. The pending flag creates no independent cache or TTL. Background
reload opens exact stored envelopes after the read transaction; admitted Arc
snapshots remain immutable and generation/listener fences are unchanged.

Auth staging adds a short tenant read/commit before sealing outside transactions;
this is credential administration, not one of the defined caller-waited hot paths.
Operator change removes repeated administrative read/seal/write attempts. Operator
rewrap changes background persistence from a whole-tenant swap transaction to one
short fenced transaction per completed row; no remote call runs inside any of
them. These are the only added/moved waits in the assigned correction paths. No
new hot-path wait requires a spec exception or performance gate.

## Reconciled coverage and specialist applicability

| Existing requirement / original scenarios | Carry-forward or changed-path proof | Result |
| --- | --- | --- |
| REQ-001 / 1–3,6 | Prior common facade composition, owner use and competing-system deletion are unchanged. Corrected consumers still use that facade. | PASS |
| REQ-002 / 1,3 | Shared DEK/nonce/envelope implementation unchanged; R1 adds no cipher owner. Prior accepted crypto proof remains valid. | PASS |
| REQ-003 / 3–4,7 | Finding 1 source and actual candidate authentication assertions establish concrete human connection binding; corrected SQL maintenance uses identical context; unchanged tenant/Operator/Gateway/platform binding carries forward. | PASS |
| REQ-004 / 3–4,7 | Trusted configured key selection and refusal logic unchanged; pending Gateway projection does not retry another key source or reuse stale plaintext. | PASS |
| REQ-005 / 1–3,6–7 | Prior backend/init/restart proof remains valid within accepted cloud limits. Explicit new user credential-source parity request is unimplemented and separately identified above. | PASS for prior assignment; user addition pending |
| REQ-008 / 3–4 | Finding 4 lost-fence conflict fixed at owner; auth observed candidate ID and revision fence preparation, SQL capability/RLS and redacted errors preserved. | PASS |
| REQ-009 / 2,5 | Finding 3 completed-failure 503, resource isolation/recovery and zero added hot-path IO validated; background per-row maintenance preserves lifecycle. | PASS |
| REQ-012 / 1–2,6 | Prior configuration/schema/docs cutover unchanged; finding 2 now preserves committed maintenance progress. | PASS |

This first remediation review carries prior unchanged acceptance evidence forward
without restarting unrelated discovery. Current fresh validator covers auth context,
SQL/CAS, Operator maintenance/concurrency, rules and evidence. A separate system
reviewer covers the materially changed Gateway failure/accounting/recovery boundary.
That selected report is present and validated. No additional maintainer/repo/domain
reviewer is necessary for these bounded owner corrections; those concerns are
explicitly traced here and prior independent evidence remains valid. Missing
expertise or hidden unassigned boundary was not found.

## Current execution reconciliation

Root's refreshed `mise run test:operators:integration` passed **20/20** on the
candidate, nextest `886377f2-7413-4526-86ee-1f0050fba851` (479.137 seconds).
This includes the rewrap progress test after its JSON assertion correction, the
competing replacements/winning delivery test and the disable race; it closes the
user-noted suite-freshness limitation. No production or assertion mutation was
required by review. The root-owned verification.md records exact commands and
completed exact focused execution.

The exact three Operator closure selectors additionally passed **3/3**, nextest
`cbbd86ec-b97c-4804-9299-bf934ed48d72`. The exact Gateway completed-failure/recovery
journey passed **1/1**, nextest `c55c6745-2835-4b0a-a732-52c4fc62001a`. The two exact
auth copied-owner/maintenance consumers passed **2/2** against real local
Keycloak/Dex, nextest `93d63293-9097-4d74-9a32-3262646080ac`. All ran sequentially
on the unchanged candidate with repository-managed Postgres; verification.md
records full selectors, fixtures and setup. Thus every original finding has
current executable proof at its required consumer/dependency boundary in addition
to independently inspected implementation and assertions.

## Final disposition

**FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3 and FIND-TASK-002-4 are CLOSED.**
No new independently discovered blocking finding is retained. R1 behavioral closure
is PASS; R1 recorded TDD is PASS for its four corrective scenarios, while original
TASK-002 scenarios 2/3 remain historically VIOLATED. This is the first remediation
round, with meaningful closure rather than an unsuccessful repeated fix. No broader
convergence escalation or specification revision is needed.

Overall TASK-002 remains **FIX_REQUIRED solely for the explicit user-requested GCP
credential-resolution addition**. Reviewed next-task boundary
`TASK-002-R2-shared-google-credentials.md` matches that request: two existing inputs,
one shared resolver and both construction callers, unchanged precedence/provider
chain/refresh/redaction, focused constructor proof and setup documentation. It does
not relabel the user request as FIND-5, reopen closed findings, add a credential
mode/schema/provider SDK or demand a new live-cloud gate. The task was created by
this maintainer-invoked review, not by the earlier conversational planning turn.

Final HEAD remains `52f69ed2da3846733be0d314c6627c95d308e8dd`; tracked source
remains clean. No production changes or runtime commands were executed by this
validator. Root execution results and historical implementation checks are clearly
distinguished; live cloud, Linux startup and unrequested benchmarks remain the
previously accepted verification limits.
