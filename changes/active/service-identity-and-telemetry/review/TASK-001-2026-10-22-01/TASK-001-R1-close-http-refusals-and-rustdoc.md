---
id: TASK-001-R1
kind: remediation
status: complete
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
requirements: [REQ-019, REQ-031, REQ-032, AC-006, AC-010]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3]
---

# Close flat Card HTTP refusal and changed-item documentation gaps

Implement through `$wyrd-implement`. This is one bounded remediation under approved behavior, not a new design or implementation of remaining change tasks.

## Subject and authority

- Approved spec: `changes/active/service-identity-and-telemetry/spec.md`, revision 12 and normative supplements.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-001-flat-card-identity.md`.
- Cumulative base: `f38dff80327bb771975e2cdd8fbed283ab145159`.
- Reviewed candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`.
- Review: sibling `verdict.md` and `findings-validation.md`. First remediation, no prior findings.
- Rules: root/nested AGENTS.md, Card/reference and registry design authority, `agent_docs/errors.md`, `agent_docs/rust.md`, and `TESTING.md`.

## Diagnosis and selected correction

### FIND-TASK-001-1: obsolete by-reference selectors still read and delete

The receiving DTOs in `crates/wyrd/wyrd-server/src/components/cards/routes.rs:630` (DeleteCardRefQuery) and `:637` (CardRefQuery) lost their required space field but retain ordinary Deserialize. Serde discards the unknown key. A permitted caller can send `DELETE /v1/cards/by-ref?kind=Prompt&name=flat-http&version=1.0.0&space=wrong`; the handler creates the flat reference, authorizes it, and calls `service.rs:2120`. The tenant SQL lookup correctly selects kind/name/version and the service commits before artifact cleanup. GET similarly returns the flat Card. This violates removed-form refusal, not tenant isolation or grants. The existing HTTP proof never sends the old query to these routes.

Make both receiving DTOs reject unknown fields through the existing strict serde posture. Preserve flat parsing/UID assertions, grants/audits and service/SQL ownership. Map refusals through finding 2. Invalid input originates at the DTO, so no SQL, cleanup or downstream guard belongs here. Do not restore space, add aliases or invent another parser.

Do not tighten VersionListQuery: it only accepted include_prerelease in both base and candidate. That endpoint lost a path segment; its old route already fails. Arbitrary query strictness there is outside the task. ListCardsRequest is already strict and needs only error projection.

### FIND-TASK-001-2: newly rejected HTTP shapes return framework errors

At `routes.rs:425`, bare Json<DeclareCardRequest> rejects metadata.space before the handler, yielding Axum 422/plain text. At `:305`, Query<ListCardsRequest> rejects space with framework 400/plain text. By-ref extractors at `:142` and `:614` will have the same gap after finding 1. WyrdErrorResponse is bypassed, and middleware does not normalize extractor responses. The flat HTTP proof permits any client error and a text substring, so its pass does not establish the required stable typed refusal.

Reuse the Result<Query<T>, QueryRejection> and Result<Json<T>, JsonRejection> adapter approach already used by principals/gateway routes. Select existing WyrdError::Validation, code WYRD_SPEC_400_VALIDATION, status 400, rendered by WyrdErrorResponse; this error is already advertised. Normalize query decode failures at list and both by-ref adapters. Normalize declaration JsonDataError shape/type refusals. Preserve native syntax failure (400), absent/unsupported content type (415), and body-read/limit failure (including 413) responses, passing non-data rejections through unchanged. Retain native extraction/body limits, including the shared typed PayloadTooLarge boundary. No global rewriter, second decoder, new error family or transport.

Provide safe structured field/location and expected flat shape plus actionable detail naming space; never echo secret-bearing values. A space-bearing nested reference inside CardSubmission.spec reaches the existing service decoder because spec is Value; preserve its typed registry validation family rather than forcing a different error there. Successful extraction still uses current auth/audit/declaration transactions.

### FIND-TASK-001-3: materially changed Rust items miss mandatory rustdoc

Root rules explicitly block missing changed-item rustdoc, including private/test items and Errors/Panics sections. Source confirms omissions on changed identity/default/error branches, signatures and rewritten proofs. Green compiler/lint results do not close this gate. Document the existing items below in place, accurately describing their workflow/invariants; no behavior change, checker, or untouched-debt sweep. Candidate line numbers are navigation anchors.

| Candidate source / item | Required correction |
|---|---|
| `crates/shared/wyrd-loader/src/parse.rs:196` parse_single_envelope | Add intent/diagnostic-boundary docs and Errors for envelope/spec decoding. |
| Same file `:327` card_ref_for | Add Errors for missing exact version; do not restore removed space refusal. |
| `crates/wyrd-spec/src/graph/mod.rs:56` hash_card_ref | Document Node/Edge hashing inputs, including UID; do not conflate Hash with UID-free sibling matching. |
| Same file `:85` graph_ready_submissions | Add Errors for empty input and placeholder construction; cloned authored version intent is retained. |
| Same file `:184` submission_card_ref | Document exact-pin prerequisite/None and preserved optional UID. |
| `crates/wyrd-spec/src/graph/canonical.rs:150` submission, `:170` key | Add fixture/order-key docs; Panics on submission for invalid fixture name. |
| Same file `:182` canonical_order_is_deterministic, `:204` canonical_order_ignores_wire_order | Add test intent and Panics; these tests change the ordering fixtures/expected key tuples. |
| `crates/wyrd/wyrd-cards/src/identity.rs:28` apply_repo_defaults_from, `:124` apply_with_config | Document map-only default application and intentionally soft absent/malformed config; describe fixture return role. |
| Same file `:138`, `:208`, `:230`, `:237` four repository-default tests | Add intent and Panics for the changed map-only/default-failure outcomes. |
| `crates/wyrd/wyrd-cards/src/card_ref.rs:152` CardRefPy::__new__ | Add Errors for kind/name/version/UID validation. |
| `crates/wyrd/wyrd-cards/src/data.rs:265` DataCard::from_card; `model.rs:268` ModelCard::from_card | Add Errors for envelope kind/API/spec validation, missing resolved pin/UID, and unresolved paths. Both materially remove a prior identity failure branch. |
| `crates/wyrd/wyrd-cards/src/prompt.rs:259` to_envelope_metadata | Add conversion intent and Errors for name/version/UID validation. |
| `crates/wyrd/wyrd-cards/src/prompt/io.rs:29` read_local_card_file | Add Errors for extension/IO/JSON-YAML/object/envelope validation; preserve local authoring defaults. |
| `crates/wyrd/wyrd-cli/src/card.rs:623` selector_from_args | Add selector precedence/exact-delete role and Errors for malformed/missing coordinates. |
| Same file `:730` check_cards, `:746` print_check_text, `:805` print_list_text | Add concise output/authoring projection contracts, including auto-version display. |
| `crates/wyrd/wyrd-server/src/components/cards/routes.rs:630`, `:637`, `:644`, `:665` both DTOs/remaining fields and into_card_ref methods | Add selector intent/field docs and Errors for parsing/rejection; preserve trust-boundary responsibility. |
| Same file `:139` get_card_by_ref_http, `:208` get_latest_card_http, `:252` list_versions_http | Add Errors for their materially changed flat selectors and delegated validation/permission/registry failures. |
| Same file `:302` list_cards_http | Add Errors when updating its extractor for finding 2; describe framework-to-public mapping without promising unrelated normalization. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs:175` list_card_versions, `:189` list_cards | Add Errors for tenant/query/version/cursor/row conversion/commit failures as applicable; retain caller-facing read transaction role. |
| Same file `:453` summary_from_row | Add projection docs and Errors for stored kind/name/version/status/UID decoding. |
| Same file `:1719` find_submission | Add Errors for graph node missing its authored submission after flat matching. |
| `crates/wyrd/wyrd-sql/src/queries/cards/declare.rs:461` select_card_uids_by_ref_batch | Add Errors for query failure, unmatched returned identity and invalid stored UID; document omission of missing/non-Active rows (not a promise to resolve all refs). No panic remains to document. |
| `crates/wyrd/wyrd-sql/src/queries/cards/get.rs:143` find_card_by_ref | Add Errors for DB/stored-row failures; None means no non-deleted row is visible at the identity, distinct from failure. |
| `crates/wyrd/wyrd-sql/src/queries/cards/delete.rs:168` soft_delete_card_by_ref | Add Errors and caller-owned transaction/lifecycle role for flat lookup followed by existing locked delete. |
| `crates/wyrd/wyrd-sql/src/queries/cards/relationships.rs:65` persist_outbound_relationships, `:99` inbound_relationships | Add Errors for missing target UID/write failure and read/stored-reference failures respectively; no helper commits. |
| `crates/wyrd/wyrd-sql/src/queries/cards/list.rs:174` push_card_filters | Add required Errors section for range-to-bounds failure (existing prose already states it); describe the existing metadata-query failures accurately. |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:315` make_card_ref | Add flat fixture identity docs and Panics for invalid fixture name/version. |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:791` insert_test_service_account | Add fixture principal-binding/caller-owned transaction docs and Panics on insert failure. |
| `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1114` nested register | Add owned tenant-transaction/operation/Card fixture workflow; Errors for returned insert failures and Panics for connection/operation/UID/commit setup failures. |
| `crates/wyrd-spec/src/reference.rs:871` flat_card_reference_contract | Add Panics for parsing/negative-contract assertion failures. |
| `crates/wyrd/wyrd-config/src/config.rs:195` removed_space_defaults_are_rejected | Add Panics for config/negative-schema assertions. |
| `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:186` issue_key_refuses_removed_space_flag | Add Panics for Clap refusal/assertion failures. |
| `crates/shared/wyrd-loader/src/config.rs:58` config_absent_uses_system_defaults, `:69` config_precedence_is_handled_by_apply_defaults | Add test intent/Panics for their map-default outcomes replacing scalar-space setup/assertions. |
| `crates/wyrd/wyrd-config/src/config.rs:185` parse_empty_file_yields_empty_config; `merge.rs:315` merge_defaults_only_for_unconfigured_kind | Add test intent/Panics for changed map-only/default-kind assertions. |


Items edited for findings 1/2 must also document the new rejection boundary and preserved statuses. Errors/Panics sections describe the actual body and fixture/assertion assumptions, not generic placeholders.

## Preserved behavior and non-goals

Retain tenant/kind/name/version identity, exact UID assertions, grants/audits, version coordination, relationship lifecycle, declaration atomicity, artifact integrity, map precedence and recovery owners. No compatibility aliases, replacement namespace, historical conversion, applied-migration edits, account redesign, storage work, UI redesign or final docsite cleanup. No general query policy or unrelated HTTP normalization. No dependency, harness, abstraction or catalog/schema change is needed.

## Ordered proof and acceptance

1. Extend the existing real-server HTTP proof before adapter changes. Establish RED from ignored selectors or untyped refusal, not compilation/setup. For FIND-TASK-001-1, GET/DELETE by-ref with default and a distinct nonexistent space must return typed 400. After each refused DELETE, flat GET returns the original UID/readable Active Card. Finish with valid flat DELETE/read-after-delete. A standalone Prompt suffices; refusal before service invocation closes the destructive path without an artifact fixture.
2. For FIND-TASK-001-2, assert exact 400, application/problem+json, WYRD_SPEC_400_VALIDATION and actionable field information for metadata.space, list-space and by-ref-space refusal. Use the existing no-writes proof for rejected declarations. Nested reference space must retain its existing typed registry validation refusal. Check syntax/content-type/body-limit statuses remain 400/415/413 with existing fixtures/owner tests; preserve valid flat operations and current permission/tenant negatives.
3. Apply minimal adapters and rerun GREEN, recording exact selector/nonzero count. Refactor only if needed for existing owner patterns.
4. For FIND-TASK-001-3, source-review all inventory items and newly changed Rust items for intent, invariants and accurate Errors/Panics. Record closure by inventory group. Comments alone need no runtime test.

Exact focused runtime command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_declaration_route::flat_card_http_contract)'"
```

Sequential focused static checks:

```bash
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-server --lib -- -D warnings
git diff --check
```

Use `mise run fmt` first if needed. If documentation exposes a real doc-link/type failure, use the affected crate's narrow check. No full SDK journeys or broad aggregates in this remediation; integrated change review owns them. No codegen run is needed if existing schemas/catalog stay unchanged. Separate newly executed evidence from recorded TASK-001 results and preserve OIDC/UI/saved-login limits.

## Completion and stop conditions

Append RED/GREEN commands/counts, per-finding closure and documentation inventory closure. Stop for a genuinely new public API/error/schema, compatibility, tenancy, permission, resource-owner or persistence decision rather than making it here. A later task review must audit original base through the new candidate alongside this verdict, ledger and task; never only the latest fix diff.

## Implementation Evidence

Candidate: `f5aa6f2cb` (HTTP refusals) and `73843e810` (rustdoc) on top of reviewed `0a29bae4f`. Evidence below was executed for this remediation. The TASK-001 results and limits recorded earlier are unchanged: OIDC discovery, absent UI build, saved-login permissions, and unrun `identity_e2e`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1: GET and DELETE by-ref with `space=default` and `space=nowhere` are refused before acting; the Card stays readable and Active; flat DELETE then removes it | `cards/routes.rs` `CardRefQuery` has `#[serde(deny_unknown_fields)]`. The duplicate `DeleteCardRefQuery` is deleted, and both routes decode through `card_query` before authorization or the service | `flat_card_http_contract` loops over both space values, checking GET and DELETE refusal, then a flat GET returning the same UID with phase `active`. The flat DELETE returns 200, and a later read returns 404 | PASS |
| FIND-TASK-001-2: `metadata.space`, list `space`, and by-ref `space` each return 400 `application/problem+json` `WYRD_SPEC_400_VALIDATION`, with a location and a reason naming `space` | `card_query`/`invalid_shape` produce `WyrdError::Validation` with `details.location` and `details.reason`. `declare_card_http` maps only `JsonRejection::JsonDataError` and returns every other rejection natively | `assert_space_refusal` checks status, content type, code, location (`body`/`query`) and "unknown field `space`" for the declaration, list, and both by-ref refusals. `assert_no_declaration_writes` holds for the refused declaration | PASS |
| Nested `space` stays the service-level typed error; syntax, content-type and body-limit statuses are preserved | `spec` remains `Value` decoded by the service. Non-data `JsonRejection`s are returned through `into_response()`, and the body-limit middleware is untouched | Same test: nested `spec.prompt.space` gives `WYRD_REGISTRY_400_INVALID_CARD_SPEC` with no writes; truncated JSON gives 400; a missing content type gives 415. 413 is still owned by the `body_limit` middleware and its tests | PASS |
| Existing proofs of the old 422 text updated to the typed contract | `verifier_contract_refusals_leave_no_writes` (retired kinds) and `pg_openapi_contract::agent_contracts_replace_workflow` now assert 400 `WYRD_SPEC_400_VALIDATION` plus the reason. Both assertions are stricter than before | `pg_card_declaration_route::*` and `pg_openapi_contract::*` 55/55 | PASS |
| FIND-TASK-001-3: every inventory item has rustdoc with accurate Errors and Panics sections | `73843e810` and the route/service docs in `f5aa6f2cb`. See the inventory closure below | Source review of each item. `cargo doc --document-private-items` shows no intra-doc warnings on changed lines. `check:docs` passes. Clippy (`-D warnings`) is clean on all touched crates | PASS |

RED: `flat_card_http_contract` failed at the first refusal: `metadata.space` declaration `left: 422, right: 400`, Axum plain text. That is a runtime failure, not a compile or setup failure. The by-ref RED is shown by source rather than by a separate run: before the fix, the DTOs used plain `Deserialize`, which discards unknown keys. GREEN commands:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_declaration_route::flat_card_http_contract)'"   # 1 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_card_declaration_route::/) | test(/^pg_openapi_contract::/)'"   # 55 passed
mise run fmt:check                                                # pass
mise exec -- cargo clippy --locked -p wyrd-server --lib -- -D warnings   # pass
git diff --check f38dff803 HEAD                                   # pass
```

Inventory closure, by group. Every item gained intent plus `# Errors` or `# Panics` where it applies:
- **Loader:** `parse_single_envelope`, `card_ref_for`, and the tests `config_absent_uses_system_defaults` and `config_precedence_is_handled_by_apply_defaults`.
- **Spec graph:** `hash_card_ref`, `graph_ready_submissions`, `submission_card_ref`, and the `canonical.rs` items `submission`, `key` and the two ordering tests.
- **Authoring:** `identity.rs` `apply_repo_defaults_from`, `apply_with_config` and its four tests; `CardRefPy::__new__`; `DataCard::from_card`; `ModelCard::from_card`; `to_envelope_metadata`; `read_local_card_file`.
- **CLI:** `selector_from_args`, `check_cards`, `print_check_text`, `print_list_text`, and `issue_key_refuses_removed_space_flag`.
- **Server:** the `CardRefQuery` fields and `into_card_ref`; `get_card_by_ref_http`, `get_latest_card_http`, `list_versions_http`, `list_cards_http`, `delete_card_by_ref_http`, `declare_card_http`; `list_card_versions`, `list_cards`, `summary_from_row`, `find_submission`; `insert_test_service_account`.
- **SQL:** `select_card_uids_by_ref_batch`, `find_card_by_ref`, `soft_delete_card_by_ref`, `persist_outbound_relationships`, `inbound_relationships`, `push_card_filters`, and the nested `register` in `pg_cards_declare.rs`.
- **Auth:** `make_card_ref`.
- **Contract tests:** `flat_card_reference_contract`, `removed_space_defaults_are_rejected`, `parse_empty_file_yields_empty_config`, `merge_defaults_only_for_unconfigured_kind`.
- **New items:** `card_query`, `invalid_shape`, `assert_space_refusal`.

Non-goals confirmed:
- No space restoration, aliases, or new error code, schema, catalog, dependency, migration or route.
- `VersionListQuery` is untouched.
- No global rejection rewriter.
- Grants, audits, service and SQL ownership are unchanged.
- The only files changed are the cards routes and service, the two server integration test files, and doc-only edits to the inventory files.

Residual risk: the `body` reason carries serde's decode text. Card envelopes are not secret-bearing, and an unknown-field message names keys, not values. An invalid-type error on a typed envelope field can still quote that field's value.

Status: IMPLEMENTED.
