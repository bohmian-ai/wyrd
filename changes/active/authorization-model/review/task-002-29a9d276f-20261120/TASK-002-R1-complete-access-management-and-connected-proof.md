---
id: TASK-002-R1
kind: remediation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-006, REQ-008]
original_task: TASK-002
findings: [FIND-TASK-002-1, FIND-TASK-002-2]
---

# Complete Access account management and connected retained-review proof

An administrator must be able to stop a managed account through Access. The existing Alice journey must prove that the account/grant created there actually reads the promised payments reviews and evidence while ledger remains hidden.

## Review identities and authority

Original task: `changes/active/authorization-model/tasks/TASK-002-access-ui.md`.
Approved specification: SPEC-authorization-model revision 2.
Reviewed base: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
Reviewed candidate: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
Validated findings: [findings-validation.md](findings-validation.md); acceptance decision: [verdict.md](verdict.md).

REQ-006/AC-001 governs retained data and connected consumers. REQ-008 and the spec's explicitly referenced agreed administration design govern existing account management. Apply root/nested instructions and wyrd-ui. No new migration or durable lifecycle is authorized.

## FIND-TASK-002-1: consume the grant's actual retained outputs

Diagnosis: `ui-pages.integration.test.ts:2650` admits payments/ledger PR deliveries, but Alice's browser/CLI/SDK only lists or gets Cards. No assertion reads payments review sets, member executions, evidence or filtered counts. Python/MCP tests cited from TASK-001 use separate created accounts/grants; they support backend enforcement but do not consume this UI-created output. The old UI review test uses admin/tenant-wide credentials.

Extend the existing Alice production journey and PR fixtures. Await real materialized review outputs, then consume them using Alice's actual identity and redeemed credential. Exercise the payments review views and retained data, asserting correct membership/evidence and that ledger contributes no items, pages or counts. Independently refuse an ungranted referenced Card definition. Reuse existing public CLI/SDK/MCP consumers and repository-Postgres host for required downstream surfaces, including the acknowledged Python/MCP seam; avoid duplicating unchanged backend fault-injection or broad suites.

This is an evidence correction. Keep production authorization unchanged unless the stronger test demonstrates a defect; diagnose any discovered failure at its existing owner.

## FIND-TASK-002-2: drive the existing account lifecycle from Access

Diagnosis: selected User/Service status is display-only in Access `+page.svelte:163–175`; the complete `+page.server.ts:108–189` action table lacks principal suspension. The existing owner is `Principals::revoke_principal` in `crates/shared/wyrd-client/src/principals/handle.rs`, projected by `POST /v1/principals/{id}/revoke` and `wyrd-server/src/auth/revoke.rs`.

Add the selected-account control/action using that existing typed kind/reason contract and the current BFF session/action/problem mechanism. Present the server's actual resulting state and refusal. Preserve principal identity and grants, tenant isolation, authorization propagation, cancellation and credential semantics; removing a binding or rotating one key does not substitute for stopping an account. Use established suspension/revocation semantics. Do not invent resume, reversible lifecycle state, profile-field editing, persistence, or another authority path. If different lifecycle semantics are needed, return that material decision to the maintainer.

## Observable closure and execution

| Finding / requirement | Scenario and required observation | Narrow proof |
| --- | --- | --- |
| FIND-TASK-002-1 / REQ-006, AC-001 | UI-created Alice reads actual payments review sets/members/evidence; ledger contributes no corresponding results/counts/pages; ungranted referenced definition refused; downstream consumers use this created account/credential | Existing production Alice journey plus focused public consumer selectors using its output; real BFF/server/Postgres<br>**Evidence.** Selector S1, `ui-pages.integration.test.ts`:2651; new assertions 2771–2853. Browser: Repository activity lists one execution and opens the member (2828–2833); Verify lists payments alone (2842–2847); ledger's execution page is byte-identical to a never-issued ID (2848–2853); Verifier `code-review` refused on the member page, by link and to her SDK while the admin reads it (2834–2841). Her CLI login: TS SDK rows, count 1 against the admin's 2, empty `OFFSET 1` page, Variant read by key (2796–2801); CLI `query` and `execution status`, ledger `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` (2803–2808); Python rows and count (2810–2816); MCP `bifrost.query` and `verification.get_execution`, ledger refused (2818–2826). **RED:** as Alice every Variant read by key failed `WYRD_VALA_400_QUERY_INVALID_SQL` and Verify rendered that error in every panel while the admin's same queries ran (D2); unit selector O1 failed `plan collects: Plan("variant_get_text requires a Variant and a path")`. **GREEN:** O1 passes (221/221 `oracle::` unit tests); S1 passes after `mise run ts:build:testing`. **Limit:** this deployment runs no Verifier, so the member stays `pending` and results/evidence/event tables are empty; the retained evidence read is the review set's `execution_inputs` row. Python asserts ids and counts only (its Arrow result returns a Variant as raw struct bytes). **Behavior: PASS. TDD:** assertions were written after exploratory probes against the running journey; the production fix followed O1's RED. |
| FIND-TASK-002-2 / REQ-008 | Admin suspends an active User and Service in Access; server state reflects existing lifecycle and subsequent real credential use is refused | Focused production Access journey, real server/credential consumers<br>**Evidence.** Selector S2, `ui-pages.integration.test.ts`:3028. Service: keyboard Enter on the control, `⊘ suspended`, status message, control gone, keys notice, server state equals the prior state with `status: suspended`, key exchange refused `WYRD_AUTH_401_API_KEY_INVALID` (3095–3103). User: server state suspended with bindings kept, her signed-in browser redirected to sign-in (3105–3121). 390px dark, no overflow, focus ring on the control (same test). **RED:** `?/suspend` answered `{type:'error'}` (no such action). **GREEN:** S2 passes. Diagnoses D1, D3. **Behavior: PASS. TDD: PASS** for the action and for D1's server fix (K1 RED first); D1 was first worked around in the page, then fixed at the server and the workaround removed. |
| FIND-TASK-002-2 / REQ-008, tenant/admin boundary | Non-admin direct BFF action cannot suspend; server refusal is faithfully displayed and preserves state | Focused production negative journey through existing action path<br>**Evidence.** S2: a signed-in non-admin's direct `?/suspend` post returns failure 403 `WYRD_PERMISSION_403_DENIED_RBAC` and the target's server state is unchanged (3061 ff.); an admin's reason the server refuses (`password=…`) shows the alert "Nothing was changed" with the BFF's safe code `WYRD_SPEC_400_VALIDATION` and the account stays `✓ active` (3070 ff.). **RED/GREEN** as S2. **Behavior: PASS. TDD: PASS.** |

Record exact selectors before RED for new behavior, then actual RED/GREEN and assertion locations. Preserve original TDD violations/unverified history rather than reconstructing it. New test selectors and private implementation details remain implementer-owned.

Existing runner, after the original task's TS/testing/production UI prerequisites:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.integration.test.ts -t '<focused selector>'
```

Reuse TASK-001's established language/MCP runner for the narrow connected consumers; record exact commands. Run `mise run ui:check`, affected owner format/lint checks and `git diff --check`; use codegen only if contract sources change. Check the new control's keyboard/focus/light/dark behavior through existing UI conventions. No backend contract change is expected. Account administration reuses the existing server path; list any actual added/moved hot-path external waits if scope changes.

Non-goals: full integrated suites, benchmark qualification already excluded by the maintainer, unrelated capacity assertion repairs, cross-process build locking, new lifecycle APIs, and duplicated backend authorization/cache testing. The known broad 64/66 result remains a final-integration limit and must not be reported green. Return evidence to the caller; do not launch review, commit, merge or push.

## Implementation evidence

Selectors and commands, each run once at the cited result:

- S1: the runner above with `-t 'Alice is given payments in Settings'`.
- S2: the runner above with `-t 'an administrator suspends a User and a Service account in Access'`.
- K1: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle)'`.
- O1: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::verification_rows_are_filtered_by_the_readers_card_scope)'`; the `oracle::` unit set with `-E 'test(/^oracle::/)'`.
- `mise run ts:build:testing`, `mise run fmt`, `mise run lints`, `mise run ui:check` (101 unit tests), `git diff --check`: all clean.

Reuse: the control posts the existing `POST /v1/principals/{id}/revoke` through the Access page's existing `act`/`call`/problem path; no lifecycle, persistence, migration, route or contract was added. Consumers reuse the journey's own CLI login (`cliEnv`), `Bifrost.sql`, the Python SDK virtualenv and the MCP client already used by this file.

Diagnoses:

- **D1. Symptom:** after suspending a Service, Access showed "Access administration is unavailable `WYRD_SPEC_404_NOT_FOUND`". **Evidence:** the page load's key listing (`GET /v1/principals/{id}/credentials`) answered 404 for the account the principal listing returned as suspended. **Cause:** `require_principal` (`wyrd-server/src/components/principals/routes.rs`) tested existence through `service_account_by_id`, which filtered `status = 'active'`, so a suspended Service read as absent; `user_by_id` has no such filter, so a suspended User was found. **Fix site:** `service_account_by_id` (`wyrd-sql/src/queries/auth/service_accounts.rs`) now finds a public machine principal in any status, as `user_by_id` does. Callers checked: token issuance (`wyrd-auth/src/issuance.rs`) filters on `status == "active"` exactly as its User branch; principal revocation is now idempotent for a suspended Service as it already was for a User; binding listings (`principals/access.rs`) now report a suspended administrator's kind correctly; credential listing answers for a suspended principal; credential issuance refuses a suspended User or Service with `WYRD_SPEC_409_CONFLICT`. The SYSTEM writer stays excluded. The page reads keys for every Service and hides only the issue control when suspended. **Proof:** selector K1 (RED: issue for a suspended User answered 400, not 409; GREEN with the `pg_browser_sessions`, `identity_e2e` and `platform_admin_e2e` sets, 48 passed); wyrd-auth and wyrd-sql principal sets (71 passed); CLI principal journeys (4 passed); S2 asserts the suspended account's key row stays listed. `mise run codegen:check` clean. **Contract note:** existing routes only; a suspended Service's key listing changes from 404 to 200 and issuance to one from 404 to 409, by maintainer instruction to follow the common practice that a disabled account stays readable.
- **D2. Symptom:** a Card-family reader's Variant read by key (`->`, `->>`, `variant_get_text`) over `bifrost.verification.*` was refused as invalid SQL; her Verify page failed in every panel. Whole-column reads and counts worked. **Evidence:** admin and Alice probes on one server; O1's RED. **Cause** (read-only diagnostician, source trace): `restrict_verification_rows` masked each column as `CASE … END` and kept the extension tag only on the projection alias; the optimizer's projection inlining and the physical planner drop that alias, so the Variant function saw an untagged root. A tenant-wide reader gets a filter only. **Fix site:** `restrict_verification_rows` (`vala-bifrost-redux/src/oracle/mod.rs`), the single owner for every managed verification scan (memtable, Parquet, views, joins, subqueries; peers receive the rewritten plan): the mask is cast to the column's own field. Tenant-wide readers, other namespaces and null masking are unchanged and covered by the same test. **Waits:** none added or moved; the change is a synchronous plan rewrite on the Oracle path. **Not verified:** shredded-leaf pushdown and peer serialization of the field-typed cast for scoped readers; no benchmark was run (excluded for this task by the maintainer).
- **D3. Symptom:** the suspended User's browser kept reading. **Evidence:** the server answered the cookie as `person-…`, still `active`; the account listed `suspended` was the tenant's seeded administrator User. **Cause:** the test selected the first User in the tenant, not the invited one (read-only diagnostician found no server path that serves a suspended User's session). **Fix site:** the test's selection; production unchanged.
