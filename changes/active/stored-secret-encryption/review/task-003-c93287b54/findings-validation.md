# TASK-003 independent findings validation

Subject: base `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`, candidate `c93287b54bf944c8c8cc9323f81c12ba5174369c`. Approved contract: SPEC-stored-secret-encryption revision 1 and the current maintainer-confirmed HTTP/CLI scope. Original task: `tasks/TASK-003-resource-rotate-and-revoke.md`.

The fresh validator independently read both discovery reports, the actual cumulative diff, affected owners and consumers, assertions, assigned requirements, and applicable contributor/security/SQL/surface authorities. Root's executed results are recorded in `verification.md`; this validator ran no Cargo command and changed no production source. No new validator-origin finding is retained.

## Reconciliation

| Discovery ID | Decision | Stable finding | Reason |
| --- | --- | --- | --- |
| B-1 | CONFIRMED | FIND-TASK-003-1 | Public trusted-issuer create is INSERT-only; delete either refuses bound issuers or destroys their bindings. Boot upsert is not an authorized resource replacement action. REQ-006 expressly requires preserving identity and client references. |
| B-2 | REVISED | FIND-TASK-003-2 | Retain the missing composed human/trusted renewal journeys, human session observation, and Gateway post-renewal provider observation. Existing lower-tier race tests remain useful; do not duplicate their full matrix or demand live Vault per owner. Session use across renewal is sufficient; do not mandate a new refresh workflow. |
| B-3 | CONFIRMED | FIND-TASK-003-3 | Exact served-contract expectation omits the new sixth operation; root reproduced this assertion failure against the candidate. |
| B-4 | CONFIRMED | FIND-TASK-003-4 | Common errors incorrectly promise 409 for disabled Operators (renewal succeeds) and inactive human connections (404). Exposed mutation timestamps also disprove the blanket unchanged-view statement. |
| DOMAIN-001 | CONFIRMED | FIND-TASK-003-5 | The new route obtains an unstaged allowed event, then stages it only after later DB acquisition and locking. Either failure loses an already-evaluated permission decision, contrary to the explicit outbox contract. |

## Retained findings

### FIND-TASK-003-1 — MISSING: trusted-issuer replacement cannot preserve bound references

**Authority:** REQ-006 and REQ-008; TASK-003 scenario 2. **Locations:** `crates/wyrd/wyrd-server/src/components/admin/routes.rs:355`, `:481`; `docs/src/content/docs/reference/stored-secret-encryption.md:129`.

**Scenario:** A confidential trusted issuer already has workload bindings and needs a replacement secret because its upstream credential expired or leaked. The owner-authorized action must seal a supplied replacement without decrypting the old envelope, preserve issuer identity/bindings, and leave committed state intact if preparation fails.

**Actual evidence:** `create_trusted_issuer` calls `insert_trusted_issuer`, whose SQL is plain INSERT and rejects the existing identity. `delete_trusted_issuer_route` deliberately enforces the binding foreign key unless cascade deletes all referencing bindings first. The documented delete/add path therefore either refuses or destroys client references; failure after deletion also leaves the issuer absent. The boot `upsert_trusted_issuer` is not a runtime HTTP/CLI replacement and does not provide this action's read/prepare/fenced-write workflow. TASK-003 acknowledges the missing in-place action; that record cannot waive the assigned behavioral contract. The current user's shipping summary does not explicitly revise REQ-006.

**Consequence/correction:** Existing bound issuers cannot perform the promised credential replacement. Extend the existing trusted-issuer administration/typed secret owner and SQL boundary to replace the supplied credential through HTTP/CLI, preserving bindings and using existing authorization, facade and current-state fencing. No new authentication integration, schema, SDK or MCP surface is needed. Prove bound-reference retention, new-secret consumption, replacement with unreadable old bytes, preparation failure, and a losing concurrent revocation through focused existing journeys.

### FIND-TASK-003-2 — EVIDENCE_GAP: renewal proof stops before required users and consumers

**Authority:** AGENTS.md Testing 1; REQ-006/009; TASK-003 scenarios 1 and 5. **Locations:** `crates/wyrd/wyrd-cli/tests/secret_sources.rs:166`, `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1528`, `components/admin/identity.rs:948`, `components/gateway/pg_administration_tests.rs:621`.

**Scenario/expected proof:** The administrator renews through shipped HTTP/CLI, then existing authentication/session/provider consumers continue with the same reference and plaintext. Every shipped surface has a real-server/Postgres journey.

**Actual evidence:** The issuer CLI test returns a fabricated view from wiremock. Human and issuer owner tests call handlers directly; human setup manually stamps/promotes the candidate, and subsequent assertions open `active_connection` rather than use a login or an existing session. Gateway owner assertions decrypt a newly loaded snapshot; its real CLI journey checks the view and redaction but never sends a provider invocation after renewal. Repository searches of server/testing/CLI journey callers find no other renewal path closing these gaps. Root's GREEN runs establish these tests pass, not the missing observations.

**Consequence/correction:** There is no evidence that production is broken, but required runtime proof is incomplete. Extend existing real-server identity journeys for human HTTP renewal and trusted CLI renewal followed by actual confidential authentication and retained human-session use. Extend an existing Gateway journey to invoke after renewal and observe the unchanged provider credential. Keep lower-tier races, group equivalent negative cases, and add no SDK/MCP exposure or duplicated full owner matrix. If existing unrecorded executable proof supplies the same observations, binding its valid execution is sufficient.

### FIND-TASK-003-3 — REGRESSION: served Operator contract check fails

**Authority:** AGENTS.md mandatory affected checks; `agent_docs/agent-surfaces.md` served-document proof. **Location:** `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:2128`.

**Scenario/expected:** Start the candidate and run `pg_openapi_contract::operator_connection_contract_publishes_five_typed_operations`; the exact operation set must match the mounted surface. **Actual:** The set lists five old operations while the served document contains six, including `POST /v1/operator-connections/{connection_id}/renew-encryption`. Root reproduced the assertion failure after successful server boot and document GET, nextest `cef3e312-e11e-4e0f-989a-2e89bd758fe2`, exit 100. This is not an environment/setup failure.

**Consequence/correction:** The affected served-contract gate is red. Extend the existing exact-set test to the sixth operation and its bodyless request, redacted response and UUID locator. Keep the exact-set assertion; run this exact test under managed Postgres.

### FIND-TASK-003-4 — INCORRECT: renewal reference promises the wrong owner responses

**Authority:** REQ-012 and TASK-003 public action/effects documentation. **Locations:** `docs/src/content/docs/reference/stored-secret-encryption.md:120`, `:137`, and generated `docs/public/llms-full.txt`.

**Scenario/expected:** An administrator renews a disabled Operator or inactive human connection and interprets its documented result. Documentation must describe the actual existing lifecycle, and must distinguish semantic invariants from metadata updates.

**Actual evidence:** The generic row says revoked/disabled resources return 409. `OperatorConnectionControl::renew_encryption` expressly preserves disabled state while successfully renewing, covered at `pg_operator_connection_routes.rs:294`; human staging excludes inactive rows and returns 404, asserted at `admin/identity.rs:1052`. The opening prose promises an unchanged view, although the Operator update SQL and new Gateway/human renewal SQL update exposed `updated_at` (Operator also records `updated_by`).

**Consequence/correction:** Agents receive an inaccurate response/state contract. Document per-owner refusal conditions and unchanged identity/credential/revision/session semantics while allowing modification metadata. Regenerate the existing docs projection; preserve production lifecycle semantics.

### FIND-TASK-003-5 — VIOLATION: human renewal loses an allowed audit on persistence failure

**Authority:** AGENTS.md Audit; security posture § Audit integrity and privacy requires staging once known outside the transaction; `crates/wyrd/wyrd-server/src/audit/mod.rs:140` explicitly requires allowed events staged before the operation. **Locations:** new `components/admin/identity.rs:457`, new `wyrd-auth/src/connections.rs:440`, existing `begin_locked` at `:853`.

**Scenario:** An authorized renewal reads its row and successfully renews using KMS; acquisition of the write transaction or slot lock then fails. **Expected:** The evaluated allowance is staged once in the process outbox and the failed operation leaves the old envelope committed, without waiting on audit.

**Actual evidence:** `decide` uses `authorize_recording_denial`, returning an unstaged allowed event. The new route stages preparation refusals but passes successful preparation to `renew_encryption`. `begin_locked` awaits `self.begin` and `lock_human_connection_slot` before `self.audit.stage`; either `?` drops the unstaged event. No outer error branch stages it. Success-path audit counts do not cover this branch. The helper is inherited, but the new renewal route newly relies on the violating path; the correction need only cover this owner flow.

**Consequence/correction:** An evaluated administration permission decision disappears during a supported database failure. Stage it once when known using the existing outbox, and prevent duplicate staging later. Do not add an audit database write or make the operation depend on audit durability. Focused failure proof must observe unchanged envelope and exactly one allowance after successful preparation followed by acquisition/lock failure; retain one event for success, KMS refusal and denial.

## Coverage and limits

The five-owner CAS/tenant/context and no-transaction-during-KMS traces support the domain report's sound-state conclusions. Operator config/status/version/envelope fencing, Gateway active/envelope fencing, human live-state/envelope plus slot lock, issuer URL/envelope tenant RLS, and platform authorized singleton/envelope CAS do not overwrite a racing replacement/revocation. Existing refresh/notification and session revision owners remain in place. No changed runtime hot path, migration, provider authentication stack, or SDK/MCP exposure requires another specialist or benchmark. No optional maintainability proposal is retained.

Executed candidate proof: seven focused server tests and three CLI tests passed; two served OpenAPI tests passed and one failed, as detailed in `verification.md`. Owner Transit is a controlled HTTP mock with real Postgres. Prior TASK-001 real Vault facade execution may be carried forward; it is not live-Vault owner proof. Full gates were reported by implementation and were not rerun here. TDD remains **VIOLATED** as recorded; no retroactive RED requirement is proposed.
