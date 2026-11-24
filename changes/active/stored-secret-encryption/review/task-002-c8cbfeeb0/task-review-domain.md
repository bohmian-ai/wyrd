# TASK-002 domain review

Subject: base `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`, candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24`, approved SPEC-stored-secret-encryption
revision 1, original TASK-002-standardize-secret-owners.md. HEAD remained the candidate;
no source edits or executable test runs were made by this reviewer.

## Boundary and method

Independently traced security, ownership/context, tenant capabilities, Operator
concurrency, and preserved maintenance. Read the assigned spec/task, task-review
skill, root/SQL AGENTS, agent_docs index/rust/maintainer conventions, and stored-secret,
tenant-isolation, Operator and configuration/recovery authority. Reviewed cumulative
diff and current consuming source, not another reviewer's conclusions. Gateway
preparation and cold/warm behavior belong to the separate system reviewer.

## Coverage

| Assigned promise | Source and proof checked | Assessment |
|---|---|---|
| REQ-001/002/004 common encryption and original owner separation | Auth ClientSecretOwner uses WyrdKms seal/open; platform uses explicit Platform scope; OperatorSecrets delegates seal/open/rewrap; common facade supplies fresh per-value DEKs and zeroized plaintext. Shared facade is retained, not constructed per read. | Satisfied for traced cutover paths; predecessor KMS proof remains relevant. |
| REQ-003 auth binding | `pg_resolvers.rs:534–542,610–636`, `connections.rs:229–255`; issuer/client/method/purpose/tenant bound, but human connection identity is omitted. | FAIL: DOMAIN-1. |
| REQ-003 Operator binding and refusal through actual consumer | `secrets.rs` SecretIdentity includes tenant, connection UUID, provider, name and secret_version; delivery `verification/operators.rs:350–397` reads tenant row, checks active and authority, drops read transaction, then opens. `copied_or_altered_envelopes_fail_closed_at_delivery` copies within/across tenants and tampers, asserts credential_invalid/no provider call while intact secret delivers. | Supported; cross-owner/human binding is not proved by these Operator cases. |
| REQ-008 existing Operator updates | `service.rs:259–313` reads/commits before sealing; UPDATE_SQL fences config/status/version/envelope, conditional update is committed, bounded retries reprepare from winner. `rotation_racing_a_disable_keeps_the_disable` delays Transit, disables mid-wrap, asserts disabled on both views, version increments once and wrap reattempted. SQL `updates_rotate_in_place_and_are_fenced_on_the_observed_row` asserts losing updates/rewrap change nothing. | Supported for preserving concurrent disable and complete envelope atomicity. Task scenario 4 explicitly records the reprepare loop; it was introduced by this candidate, not present in the base. |
| REQ-008 auth/platform writes outside remote transaction waits | Human stage before put_candidate slot transaction; workload admin seals before acquire_conn; boot prepare loop before tenant transaction; platform configure decides, discovers, seals, then begins operation transaction. Resolver reads release read transaction before remote open. | Structural constraint satisfied on traced paths. Existing owner lifecycle and candidate test/activation checks retained. |
| REQ-009/012 existing maintenance safety | Operator `rewrap_pass` retains pass and tenant timeouts and cancellation; rewrap swaps exact secret version/envelope and cannot re-enable disabled status. However it no longer retains bounded per-tenant work/progress; reads/prepares whole tenant before commit. | FAIL: DOMAIN-2. |
| REQ-012 initial schema/tenant separation | Named initial changes consolidate Operator ciphertext, remove split columns/index; SQL owner takes TenantConn on tenant paths and OperatorPool for explicit discovery; RLS unchanged. Auth/platform readers and rewrap context reconstruct trusted metadata. | Spec authorizes these initial-definition replacements; no unauthorized table or migration introduced. |

## Proposed blockers for independent validation

### DOMAIN-1 — INCORRECT: human connection envelopes authenticate the issuer, not the connection

Authority: REQ-003 explicitly requires owner identity and says auth credentials bind
their connection/issuer identity; security posture Stored secrets refuses an envelope
copied into another owner's row. TASK-002 scenario 4 requires distinct-owner copy
refusal through consumers.

Location: `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:534–542` (common context),
`:629–636` (human row decoder); writer
`crates/wyrd/wyrd-auth/src/connections.rs:237–249`; maintenance projection
`crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:467–469`.

Starting condition and trigger: one tenant has its Active human connection and a new
Candidate for the same OIDC issuer and OAuth client/auth method. This is an ordinary
credential replacement flow; the schema permits one row of each state, and the new
candidate has another connection_id. Copy the Active row's encrypted secret bytes
into the Candidate row (the same database-substitution threat the task's Operator
and cross-tenant tests exercise), then begin/complete candidate test sign-in.

Expected: another connection's envelope refuses before credential use. Actual:
writer and both decoders construct identical context from tenant,
AuthHumanConnection, issuer URL, client ID and auth discriminant. Context always
sets owner to issuer_url and version None; neither connection_id nor row revision
participates. `human_connection_trusted_issuer` reads connection_id/revision in its
HumanConnectionRow but discards both when rebuilding cryptographic context. Thus
this copied envelope opens and the old secret can be submitted to the token endpoint.
If the provider still accepts the prior secret during rotation, sign-in succeeds
instead of refusing substitution. The same missing owner identity allows copies
among retained inactive and current rows sharing issuer/client/method.

Evidence: INSERT/REPLACE_CANDIDATE_SQL assigns separate identity or newer revision;
Active/Candidate uniqueness is per state, not per issuer/client. `tested_candidate`
(`connections.rs:417–446`) routes the Candidate row through the same decoder, and
`active` does likewise at `:675–687`. Both authenticated envelope layers receive
exactly the same context, so no backend can distinguish these owner rows. Existing
cross-tenant tests change tenant context and do not detect this same-tenant omission.

Consequence: required owner-substitution protection is absent on a supported auth
credential replacement path; this is not a new authentication feature.

Smallest correction: keep ClientSecretOwner/common facade but bind the concrete
human connection identity when sealing, opening and rewrapping, carrying trusted
identity through candidate staging/commit with its existing slot/CAS safeguards.
Keep remote key waits outside locks and preserve auth revision/session semantics;
no new encryption implementation or table. Prove with two real same-tenant human
connections sharing issuer/client/method and different secrets: copied ciphertext
must refuse at the candidate authentication consumer while intact rows work.

### DOMAIN-2 — REGRESSION: Operator maintenance loses bounded progress for healthy remote backends

Authority: TASK-002 Existing owners and scope explicitly says retain existing
maintenance safety and do not remove safeguards before Task 4; REQ-012 requires
cutover with existing lifecycle, and REQ-009 retains owner lifetime. Existing
bounded/cancellable maintenance is also the established mechanism REQ-010 (Task 4)
will extend; this finding does not demand implementing that future task.

Location: `crates/wyrd/wyrd-server/src/components/operators/secrets.rs:230–262`,
with timeout at `:194–208`; defaults
`crates/wyrd/wyrd-server/src/verification/mod.rs:234–235`.

Starting condition and trigger: after selecting a new logical wrapping key, a
supported tenant has enough stale connections that healthy, finite sequential
remote unwrap/wrap requests exceed the 30-second tenant budget in aggregate.
For example, hundreds of connections at ordinary remote request latency can
exceed this budget while each individual request is healthy. The periodic
maintenance pass runs.

Expected: bounded work commits progress and later passes finish retained rows;
cancellation/failure preserves committed batches. Actual: list_connections fetches
all tenant rows, then every noncurrent envelope is rewrapped and accumulated in
memory before any swap/commit opens. Timeout cancels during preparation, drops
all completed results, and returns no progress. The next periodic pass reads the
same stale set and repeats forever even though the key provider is healthy.
Old-key retirement/recovery remains blocked indefinitely for this tenant.

Evidence: prior `keys.rs` had REWRAP_BATCH=100 and stale_key_connections(...,
REWRAP_BATCH), then committed that bounded batch. The replacement removes STALE_SQL
and its limit and reuses unbounded list_connections. All prepared envelopes are
saved only after the entire for-loop at lines 234–248. Existing slow-provider test
`slow_rewrap_never_holds_back_another_tenants_delivery` covers provider outage,
shutdown and three-row recovery; it does not exercise aggregate healthy work
exceeding the budget. `stalled_rewrap_discovery_returns_within_the_pass_budget`
checks discovery timeout, not incremental progress.

Smallest correction: preserve bounded, resumable progress in OperatorSecrets and
its existing query/swap owner, committing completed bounded work rather than
requiring a whole tenant's remote work to finish inside one timeout. Reuse exact
envelope/version fences; retain no remote IO in transactions, cancellation and
other-tenant delivery independence. No job/progress table or new scheduler.
Prove with the actual remote facade/local Transit boundary and a small tenant
budget: enough individually successful delayed rows must span multiple passes;
one pass commits a subset and repeated passes converge without overwriting a
concurrent replacement or disable.

## Response-semantics reconciliation required

REQ-008 says “A racing replacement/revocation returns a conflict rather than losing
the change.” TASK-002 scenario 4 instead records that a replacement which loses its
first fence rereads the disabled row, preserves Disabled, seals again and returns
200. The current consumer test explicitly requires that result. Base service.rs
held FOR UPDATE during encryption and had no CHANGE_ATTEMPTS loop; the loop is a
candidate implementation choice, not an unchanged pre-existing retry contract.

There is no data-loss or re-enable defect in that retry path: the disable remains
committed and replacement uses its status. This review does not propose removing
safe retries or changing responses based on preference. The primary/validator must
reconcile whether REQ-008's conflict promise governs these existing PATCH/DELETE
operations now or only the new named admin actions in TASK-003. If both the literal
spec response and task mechanics govern this path, this is a task/authority
contradiction, not a third implementation finding or permission to silently rewrite
the specification. The current user summary confirms the preserved-disable outcome
but does not explicitly settle 200/reprepare versus 409 response semantics.

## Evidence limits and process

This specialist performed source/assertion/recorded-evidence audit only. It did not
rerun Cargo or cloud/provider checks. The task records focused owner, SQL and Vault
runs; root coordinates any fresh execution. Live GCP/AWS and Linux startup CI limits
are explicitly accepted by the maintainer and are not findings here. Scenario 2/3
TDD history remains violated as recorded; later tests do not repair that history.
No benchmark is requested or required under current root AGENTS and TASK-002.

Boundary recommendation: FIX_REQUIRED if either proposal survives fresh independent
validation. No source fix, migration, redesign or additional admin surface was made.
