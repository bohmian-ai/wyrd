# Focused validation: FIND-009-3

**REJECTED as an implementation finding under the user's explicit clarification.**
The proposed TenantConn conversion must be removed from the remediation. There
is a documentation contradiction to correct, not a demonstrated isolation defect
requiring replacement of this existing single-statement owner.

Subject: base `036fc48fe26fb87648a277b10099bc475d293b92` to unchanged candidate
`b1ff72dc34c344b596c85cf408e6a9217685be35` (HEAD independently checked).
This continuation validates only FIND-009-3 and its proposed remediation;
it makes no disposition on the other findings or the overall review verdict.

## Evidence and scope

- `scripts/check_tenant_isolation.py:64–76` explicitly describes three exact
  statement-scoped owners: `OracleTableCuts` in
  `vala-sql/src/queries/oracle_reader_authority.rs`, `VerificationTargets` in
  `wyrd-sql/src/queries/verification_target.rs`, and `GatewayAdmissions` in
  `wyrd-sql/src/queries/gateway_admission.rs`. The exception requires the named
  struct to hold a PgConnection and DataTenantId; it is not a module-wide or
  arbitrary raw-connection exception. These same lines are present at BASE;
  the checker has no diff in the reviewed range. This is evidence of an
  intentional pre-existing owner contract, not proof supplied by a green gate.
- BASE `verification_target.rs:74–98` already has the raw connection, typed
  tenant field, constructor, and explicit same-tenant transaction invariant.
  Candidate `:76–125` preserves that capability and still executes one
  parameterized `SELECT * FROM wyrd.verification_target(...)`. Its extension
  adds the owner UID and returned owner Card to the existing read, without
  adding a second query or a new exception owner.
- Original migration `20261021000000_verification_target.sql:44–79` and
  replacement migration `20261026000000_verification_deployment_environment.sql:40–70`
  both use SECURITY INVOKER with an empty search_path, reject an existing
  different tenant binding before reading, and bind the parameter tenant
  transaction-locally. Both filter Cards and SYSTEM principals by that tenant.
  The replacement extends the same Card UID set with `p_owner_card_uid`;
  the owner is subject to the same tenant and non-deleted predicates.
  Autocommit ends the local binding with the statement; an existing caller
  transaction retains its own lifetime and same-tenant visibility. These are
  the SQL properties supporting isolation, rather than the allowlist alone.
- `verification/cache.rs:217–224,299–332` retains both modes: pool misses acquire
  a plain connection for that one statement, while Conn reads use the caller's
  borrowed connection. The cache key includes tenant and owner UID. Direct
  resolution and enqueue preparation supply `caller.data_tenant_id` at
  `components/verification/service.rs:695–700,865–870`; this is authenticated
  caller context, not a new tenant selector from request input.
- `verification/submissions.rs:157–182` acquires its tenant connection with the
  same `tenant` passed to the target read. `verification/runner.rs:315–329`
  derives the read tenant from `conn.data_tenant_id()` before lending
  `conn.transaction()`. Lending that connection does not close, replace, or
  commit the caller transaction. The SQL mismatch refusal still applies on a
  database read. Nothing in this extension authorizes cross-tenant work.
- Existing integration test
  `pg_verifier_executions.rs:3113–3190::a_verification_target_reads_its_tenant_in_one_statement`
  exercises the plain-connection path, SYSTEM principal, owner/subject overlap,
  self-target overlap, invisible foreign tenant Cards, and rejection of a
  differently bound tenant transaction. Its source is relevant behavioral
  evidence; its name does not measure network round trips. The single-query
  implementation supplies the statement-count evidence. The test was not run
  in this focused source validation.

The blanket rules in `crates/wyrd/wyrd-sql/AGENTS.md` Connections/Transactions
and `architecture/wyrd-security-posture.md:725–743` omit this existing exception.
Without clarification, that contradiction explains the prior type-boundary
finding. The user's explicit instruction now resolves it in favor of preserving
the existing isolated hot paths. Materially extending the returned Card set
does not itself invalidate the already named VerificationTargets exception.
The prior reports themselves allege no tenant escape.

## Minimal correction to the review packet

1. Mark FIND-009-3 rejected in `findings-validation.md` and `verdict.md` and
   remove it from their failed requirement mappings. Retain all independent
   findings; this rejection alone does not establish PASS.
2. Correct `TASK-002-R1-execution-context.md`: remove FIND-009-3 from
   `remediates`, its conversion section at :141–157, scenario 5 at :228–241,
   its acceptance row at :251, and proposed proof E at :299–303. Remove the
   outcome requirement that implies every target read must use TenantConn.
   Preserve the target workflow as a regression constraint for the other
   functional corrections; do not create a new test merely to demand a raw-type
   conversion or claim a fabricated runtime RED.
3. Clarify the existing exception in the owning SQL AGENTS connection rules and
   security posture's tenant-binding section: TenantConn remains the standard;
   only the three existing exact owners above may retain their established
   statement-scoped tenant binding. Name the request-role/RLS boundary,
   parameter-bound tenant, transaction-local lifetime, and applicable
   mismatched-binding refusal. Do not authorize additional owners or a blanket
   raw-connection escape. The checker records the scope; governing documents
   should state it explicitly rather than relying on readers to discover it.

Preserve one autocommit SQL round trip on a direct target cache miss, with no
added BEGIN/binding/COMMIT exchanges; retain caller-owned transactions for
acceptance and runner reads. Preserve RLS, mismatch refusal, target readiness,
owner filtering, cache tenant/owner keys, listener readiness, invalidation and
accepted-context behavior. Do not replace the reader/cache, add a wrapper,
widen to OperatorPool, remove local SQL binding, or edit an applied migration
to satisfy the rejected finding. A future real isolation defect would require
its own source-backed correction without broadening these exceptions.

Skipped runtime tests, generators, source edits and unrelated audit. Risk:
the other review findings remain outside this focused disposition; the
governing-document contradiction remains until the packet correction lands.
