---
id: TASK-003-R2
kind: remediation
status: implemented
spec: SPEC-stored-secret-encryption
spec_revision: 1
original_task: TASK-003-resource-rotate-and-revoke.md
findings: [FIND-TASK-003-6]
---

# Document the issuer replacement store refusal accurately

Reviewed candidate: `d3db737e5fccacbe02421ea135fd5327b6335d79`; prior candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`; original base: `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`. Approved contract: SPEC-stored-secret-encryption revision 1, REQ-008 and the existing agent-surface HTTP/OpenAPI authority. Original task: `../../tasks/TASK-003-resource-rotate-and-revoke.md`. Diagnosis: `findings-validation.md`, FIND-TASK-003-6; independent check: `focused-contract-validation.md`.

## Gap and correction boundary

An administrator replacing a trusted issuer's secret receives `WYRD_AUTH_503_VERIFY_UNAVAILABLE` if the store acquisition, read, CAS or commit fails. The new PUT operation's 503 OpenAPI annotation currently says store or KMS failure while listing only `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, the KMS branch's code.

Correct only the new `replace_trusted_issuer_secret` response annotation in `crates/wyrd/wyrd-server/src/components/admin/routes.rs` to list both actual stable codes. Add or extend a focused assertion against the real served document in the existing `pg_openapi_contract` owner for `PUT /v1/admin/trusted-issuers/client-secret`.

The existing `sql_unavailable` mapper, runtime statuses and code families remain authoritative. Do not change runtime error behavior, SQL, encryption, CAS, audit or permissions to make the declaration true. No migration, SDK/MCP/UI projection, provider integration, broad error inventory or duplicated journey matrix is needed. FIND-TASK-003-1 through -5 are closed; do not reimplement those corrections.

## Observable closure

| Finding / authority | Scenario and expected observation | Proof |
| --- | --- | --- |
| FIND-TASK-003-6; REQ-008 and agent surfaces | Boot the real server, obtain its served OpenAPI document and inspect the new PUT's 503 response; it declares both `WYRD_AUTH_503_VERIFY_UNAVAILABLE` for store refusal and `WYRD_SERVER_503_SERVICE_UNAVAILABLE` for KMS refusal with the existing problem schema/media type. | One focused assertion in the existing server `pg_openapi_contract` integration owner. **PASS.** `pg_openapi_contract::trusted_issuer_secret_replacement_declares_both_unavailable_codes` (`crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs`) boots `WyrdTestServer`, reads `/openapi.json`, and asserts the PUT `503` is `application/problem+json` with `#/components/schemas/WyrdProblem` and names exactly `{WYRD_AUTH_503_VERIFY_UNAVAILABLE, WYRD_SERVER_503_SERVICE_UNAVAILABLE}`. RED: `left: {"WYRD_SERVER_503_SERVICE_UNAVAILABLE"}` (behavioral, missing code). GREEN after the annotation fix in `replace_trusted_issuer_secret`, together with `every_problem_response_declares_its_media_type_and_stable_code` (2/2; both codes agree with the catalog status). TDD: PASS. |

Reuse the existing typed response annotation and contract-test document helpers. This is a declaration/assertion correction; existing runtime failure behavior requires no new production code or failure-injection machinery.

Discover the actual added or extended test selector and run it exactly through the existing managed Postgres setup:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=<actual focused test name>)"'
```

Run the affected format/lint checks; keep the focused server contract proof. No whole journey aggregate, live-provider run or benchmark is needed for this annotation/test correction. Record the actual command and result in this task before re-review.

## Evidence

Command: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_openapi_contract::trusted_issuer_secret_replacement_declares_both_unavailable_codes) | test(=pg_openapi_contract::every_problem_response_declares_its_media_type_and_stable_code)"'` — 2 passed. Also `mise run fmt`, `mise run lints`, `mise run codegen:check`, `git diff --check` clean; `mise run docs:generate` produced no drift. Only the annotation text and the test changed; runtime errors, SQL, sealing, CAS, audit, and permissions are untouched.
