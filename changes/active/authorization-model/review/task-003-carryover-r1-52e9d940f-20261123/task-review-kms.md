# R1 KMS seam specialist review

Subject: prior `b7206eafefbf7e981898eca3fc22a5d5ae76b815` → candidate
`52e9d940fd57c49a3ab5084fb319171e21eafb37`; original base `4355587f12`.
Reviewed the PR242 merge `70a56a136` only where consumed by this change and the
port `e59d2dd9`, with subsequent R1 source included. HEAD matched the candidate.
The approved revision-9 spec and security posture are intentionally dirty,
held outside the candidate: their inspected blob hashes were respectively
`55689bae96c62e78b1ea8d765a32629664b4e603` and
`0210235fb05322ef6e888526c4c9ba6bec02b234`. Neither was modified.

Scope: REQ-019 / AC-014 and R1 item 5: stored provider, SMTP and challenge
secrets; derivation singleton, cache, creation race and unavailability;
rotation/renewal maintenance; creator cap and lock stability; affected proof
lifetime and trusted owner contexts. This is a specialist report, not the
complete task verdict. No unrelated PR242 or Bifrost audit, cycle-11 acceptance,
cycles 12–20, or benchmark qualification is asserted.

Authority read: root and SQL AGENTS, task-review skill, spec-driven-development,
maintainer style, agent-docs index, TESTING, authorization-model REQ-019 and
AC-014, security posture's stored-secret and email-proof sections, and
stored-secret-encryption REQ-001–008 and maintenance contract. CodeGraph was
consulted before source navigation. R1 item 5 and its reported command/results
were inspected; other reviewers' conclusions were not used.

## Coverage

| Promise / R1 scenario | Source and assertions examined | Assessment |
|---|---|---|
| One durable random derivation key; replicas keep the winner | `wyrd-auth/src/derivation.rs:95–132`: successful-only `OnceCell`, OS-random `SecretKey`, KMS wrap, read winner and unwrap. `wyrd-sql/.../platform/social.rs:410–441`: operator-only capability and singleton conflict-do-nothing followed by a separate read statement, so an insert losing to a committed peer reads the peer's key. `20261123000000_platform_derivation_key.sql`: true-only boolean primary key, non-null envelope, forced RLS with operator-session policy. Cap journey `signup.rs:1630–1691` concurrently begins email flows on two replicas, asserts one row, verifies the other replica's flow. | Source supports requirement. Runtime assertion is meaningful cross-replica consumption, not merely row count. Fresh execution is coordinated by root. |
| Separated per-use keys and stable creator identity | `derivation.rs:150–198`: canonical length-delimited HMAC domains and labels, AES-GCM authenticates seal label. Unit assertions `:219–243` compare scope/key separation and wrong-label/key/truncated refusal. Social and email signup share a sealed tagged enum; its authenticated purpose is checked by each redemption path. Creator/source/address/sign-in scopes remain distinct. | Source and unit assertions support separation. No replacement operation or alternative derivation key found. |
| Stored secrets bind to the trusted owner | `social.rs:1281–1299`: explicit platform purposes, fixed challenge owner, provider owner and client-id authority; `mail.rs:34–42`: platform relay owner and username authority. Openers rebuild those contexts from stored metadata (`social.rs:587–599,736–743`, `mail.rs:318–327`). Four new maintenance mappings reconstruct the same context (`platform/stored_secrets.rs:322–325`; SQL `human_connections.rs:526–549`). | No context mismatch or tenant-scope substitution found in the port. KMS's existing envelope validation is reused. |
| Routine/recovery maintenance includes all four stores | `StoredSecretTable::ALL`, trusted metadata listing, exact-envelope compare-and-swap at SQL `human_connections.rs:526–549,579–595`; maintenance calls rewrap/renew outside a row transaction (`stored_secrets.rs:230–249`). Routine pass preserves the derivation plaintext; recovery reseals that same plaintext. | Source supports inclusion and preserves key/pseudonyms. Resource configuration has a separate transaction-boundary defect below. |
| Cap, provider, mail and challenge survive wrap-key retirement | Cap journey `signup.rs:1754–1868`: old-key proof on new-key replica; routine maintenance `remaining == 0`; new-only replica opens provider secret and proves identity; original challenge secret must reach Siteverify twice; email proof is delivered using relay password. In-flight completion reaches cap refusal and leaves the proposed tenant absent. `provisioning.rs:105–165`: one 32-byte creator key selects the same transaction advisory lock and persistent count, with failed/provisioning rows counted and only deleted rows excluded. | Strong combined consumer assertions for the reported R1 regression. The in-flight retirement branch is a valid capped proof, not a successful new workspace; the later cap-release/concurrency branches cover successful claim behavior. Fresh execution is root-owned. |
| Key unavailable refuses; password remains usable | `derivation.rs:106–132` has no generated replacement when an existing row fails read/open; failed initialization is retried, successful loaded key is retained. Keyless journey `signup.rs:2001–2122` asserts social/email starts and email verification 503/code/no cookie/no admission count change; tenant social start now refuses at admission and asserts local-record count unchanged; password login succeeds. | Source and actual negative assertions cover the current revision-9 path. The stale R1 prose saying tenant social start can succeed describes the earlier revision-8 slice, not current source. |
| Unreadable configured challenge refuses creation | `social.rs:718–743` distinguishes absent configuration from unreadable secret; read/open failure propagates unavailable. Challenge journey's foreign-key replica requires 503 and no tenant. Challenge is checked before workspace claim. | No fail-open path found. Missing/empty challenge response returns its own refusal before opening a secret, still creates nothing. |
| Port preserves proof deadlines and one-use semantics | `social.rs:1200–1226,1233–1270,1544–1558,1590–1649,1684–1736,1974–1988`: PostgreSQL-anchored deadlines, authenticated browser envelope, proof hash/browser binding, exact provider revision, spent-proof metadata retained past maximum envelope lifetime. Port replaces key source while retaining these checks. | No port-induced deadline or browser/purpose substitution regression found. This does not accept unstarted contact/discovery/reset capabilities. |

## Proposed blocking finding

### KMS-1 — VIOLATION: the three new secret writers hold a platform transaction during remote wrapping

- **Authority:** authorization-model REQ-019 adopts the shared stored-secret
  facade and owner semantics; stored-secret-encryption **REQ-008** explicitly
  requires preparing encryption outside database locks/transactions that would
  wait on remote key calls. This is an explicit boundary, not a proposed
  performance improvement.
- **Locations:** `wyrd-auth/src/social.rs:226` (provider configure), `:669`
  (challenge settings), and `wyrd-auth/src/mail.rs:146` (SMTP configure).
  Actual callers are `wyrd-server/src/components/platform/social.rs:195–205`,
  `:381–391`, and `:461–469`. Each awaits `authorize`, receives the live
  audited `TenantConn`, passes it into the owner, and commits only after
  `WyrdKms::seal(...).await` finishes. `PlatformAuthorization::authorize`
  delegates to `begin` and returns a real operator transaction, not a detached
  authorization token (`wyrd-auth/src/platform_authz.rs:157–169,211–216`).
- **Start / trigger:** a supported deployment uses GCP, AWS or Transit KMS;
  an authorized platform administrator stores a provider client, replaces an
  SMTP password, or supplies a signup challenge secret. Wrapping is slow or
  unavailable at the remote service.
- **Expected:** authorize and stage the decision, prepare the envelope with
  no held database transaction, then open the short transaction for the
  complete authorized write; failed encryption leaves committed settings
  intact.
- **Actual / evidence:** source order holds the operator transaction and its
  checked-out database connection across the remote wrap await in all three
  writers. These were local keyring calls before the KMS port; the new remote
  dependency changes their lifecycle. Existing file-backed journeys cannot
  establish the prohibited wait is absent. No claim of a measured outage or
  row lock at this point is made.
- **Consequence:** remote key-service latency consumes platform database
  connections for the entire remote call and violates the mandatory
  transaction-lifetime boundary. This is control-plane work, not a new
  verification/gateway/Oracle/Scribe hot-path wait.
- **Smallest correction:** stage the existing permission decision without
  opening its transaction, validate and prepare each new envelope, then begin
  and commit its existing complete row write. Reuse
  `PlatformAuthorization::decide` / `begin` and the existing platform OIDC
  configuration workflow (`components/platform/identity.rs:228–241`), with
  preparation kept in the same auth/mail owner. Preserve one decision audit,
  validation, redaction, unchanged-secret retention for challenge updates,
  and transaction rollback on failure. No new generic crypto service or
  alternative authorization path is needed.
- **Focused closure proof:** delayed or refused Transit wrapping through the
  actual three HTTP write paths with repository Postgres; establish no held
  write transaction during that wait, no committed settings changes on
  failure, and successful full writes after wrapping resumes. Keep existing
  positive rotation/retirement journeys. A source trace can establish ordering
  once the writer is structurally split; no benchmark or synthetic load is
  needed.

Independent validation is required before retaining KMS-1. No other blocking
implementation defect is proposed within this seam.

## Verification and process limits

R1 records [J], a managed-Postgres integration selection of signup/methods/
stored-secrets plus the bound Google login journey (26 passing), and [U], the
two derivation unit tests (2 passing). Source assertion inspection above is
independent; those summaries are reported execution, not execution personally
performed by this specialist. Root is running fresh exact selectors at the
candidate and owns the execution receipt and final coverage reconciliation.
No Cargo command was run here, avoiding competing builds.

Root subsequently reported candidate-bound fresh execution, recorded in
[`verification.md`](verification.md): all six exact signup journeys passed
(run `97730818-58ed-4553-9df4-521ef8ccd83a`), both exact derivation unit
selectors passed (run `adaa9209-dc61-4fc9-918d-684b956bc4d5`), and both
migration/idempotence checks passed. These resolve the runtime execution
qualifications in the coverage table. The tests still do not exercise remote
wrapping while configuration holds a database transaction, so they do not
close KMS-1.

Preserve **TDD: VIOLATED** for the KMS port exactly as R1 records: production
preceded rotation/retirement/race/unavailable assertions. Successful later
execution establishes behavior, not historical RED. No historical RED repair
or code churn is requested.

Skipped unrelated PR242 owners, cloud credentialed runs and benchmarks; the
material remaining risk is the held transaction during remote configuration
wrapping described in KMS-1.
