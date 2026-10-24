# Independent findings validation — TASK-001

**Result: FIX_REQUIRED.** Three bounded findings remain. This is the first review; there are no prior finding IDs or remediation closures.

Subject: repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry`, base `f38dff80327bb771975e2cdd8fbed283ab145159`, candidate `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`. HEAD matched the candidate before and after inspection. The reviewed subject is the cumulative base-to-candidate change, not the last commit. No `.codegraph/` exists. Only this report was written.

Inputs: subject and claim comparison; all seven discovery reports; approved specification revision 12 and its supplements; original TASK-001; root and nested SQL/Python/TypeScript/docs rules; applicable Card/reference/config, tenant-binding, error, rustdoc and maintainer authority; actual cumulative Git changes and current owner bodies/callers. Recorded verification was treated as recorded evidence, not independent execution. No Cargo, build, server, database or test command ran. Static `git diff --check` passed.

## Disposition of every discovery finding

| Discovery ID | Disposition | Retained ID / resolution |
|---|---|---|
| BEH-001 | CONFIRMED | FIND-TASK-001-1: the two by-ref receiving DTOs discard the removed selector. |
| INV-001 | REVISED | FIND-TASK-001-1; reject its proposed extension to VersionListQuery, explained below. |
| SYSTEM-001 | REVISED | FIND-TASK-001-1; retain request-to-delete consequence, reject VersionListQuery expansion and mandatory artifact-bearing proof. |
| SEC-001 | CONFIRMED | FIND-TASK-001-1; its typed-response correction is consolidated in FIND-TASK-001-2. No tenant/RBAC bypass is alleged. |
| BEH-002 | REVISED | FIND-TASK-001-2; select existing Validation, specify the adapter boundary and preserve unrelated extractor statuses. |
| INV-002 | REVISED | FIND-TASK-001-2; same precise correction. |
| REPO-001 | REVISED | FIND-TASK-001-3; source-confirmed omissions consolidated with MNT-001 and bounded inventory below, rather than an open-ended repository documentation sweep. |
| MNT-001 | REVISED | FIND-TASK-001-3; retain all three registry/helper items and the three missing panic sections. |
| Data review | Validated empty | No proposed finding. Flat indexes, tenant capabilities, version locking, exact-UID relationship recheck and declaration/cleanup transaction owners give no additional retained claim. |

No optional note becomes a finding: fixture organization, broad status assertions in unrelated SDK cases, unchanged raw connection debt, dormant bind-scoped-reference code and stale explanatory security wording are excluded. They establish no additional reachable implementation defect under this task's acceptance threshold.

## FIND-TASK-001-1 — Removed by-ref HTTP space selectors silently operate

**Status:** REVISED (deduplicated). **Classification:** INCORRECT. **Discovery:** BEH-001, INV-001, SYSTEM-001, SEC-001.

**Obligation:** TASK-001 Scenarios 1/3 and AC-006 require removed forms to fail with actionable typed validation; REQ-019 removes selectors and REQ-031 forbids compatibility reads. Authenticated tenant/kind/name/version is the only resulting registry identity.

**Exact source:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:630` DeleteCardRefQuery, `:637` CardRefQuery, consumers `:139` get_card_by_ref_http and `:611` delete_card_by_ref_http. Both derive ordinary Deserialize and lack `deny_unknown_fields` after deleting their required space member.

**Reachability and common cause:** An ordinary authenticated writer sends `DELETE /v1/cards/by-ref?kind=Prompt&name=flat-http&version=1.0.0&space=wrong` while that unreferenced Active Prompt exists. Native query deserialization discards the unknown space key. The DTO's into_card_ref produces a valid flat reference. The route authorizes cards:write, then `service.rs:2120` opens the caller's tenant transaction, invokes SQL `queries/cards/delete.rs:168` soft_delete_card_by_ref, and commits. SQL correctly selects kind/name/version under RLS and delegates to the existing locked lifecycle owner. `finish_card_delete` subsequently cleans artifacts. GET follows the analogous route → shared authorized read → tenant lookup → immutable hydration chain. Thus an obsolete coordinate can return a Card or commit its deletion instead of refusing input. Tenant binding and grants still apply.

**Sibling/owner check:** repository searches identify exactly those two private by-ref DTO consumers. ListCardsRequest already rejects unknown fields; Metadata/CardRef and MCP's read contracts are strict. CLI uses normal removed-argument refusal, and SDK selectors have removed the coordinate. Both UID and by-ref deletion converge on the existing lifecycle/cleanup owner. Nothing should be added there: the invalid state originates where the HTTP DTO discards input.

**Minimal correction:** add the existing strict serde struct posture to both by-ref query DTOs. Keep valid flat selector parsing, authentication, authorization/audit and service/SQL ownership. Project their rejection through FIND-TASK-001-2. No space field, alias, special parser, SQL guard, cleanup change or new identity dimension.

**VersionListQuery scope resolution:** reject that portion of INV-001/SYSTEM-001. The base and candidate VersionListQuery both contain only include_prerelease; no space query parameter was removed from that contract. The old versions selector was `/cards/{kind}/{space}/{name}/versions`, and the candidate removes that route in favor of `/cards/{kind}/{name}/versions`. Adding `?space=` to the new URL is not an old supported selector. Requiring every Card URL to reject arbitrary unknown query keys would invent a broader query policy (latest/UID routes do not even extract such a query). The two by-ref DTOs are different: their actual old supported query member was removed and is now silently ignored. Limit strictness remediation to these owners and the already-strict changed list request.

**Focused closure proof:** extend existing `pg_card_declaration_route::flat_card_http_contract` using its existing real declared Prompt. GET and DELETE by-ref with both default and a distinct nonexistent space value must return typed 400 (FIND-TASK-001-2). After each refused DELETE, valid flat GET must return the original UID and remain readable/Active. Finish with valid flat DELETE/read-after-delete and preserve existing grants/tenant regression assertions. An artifact-bearing fixture is unnecessary: refusing before service invocation plus unchanged readable identity proves the destructive path never starts; cleanup's existing lifecycle behavior is unchanged.

## FIND-TASK-001-2 — Changed HTTP decode refusals bypass the public error mapper

**Status:** REVISED. **Classification:** INCORRECT. **Discovery:** BEH-002, INV-002; overlapping correction from SEC-001.

**Obligation:** TASK-001's existing typed validation promise, REQ-032/AC-010 stable surface errors, and `agent_docs/errors.md`: public HTTP problems come from WyrdError::as_problem_json and repairable validation identifies the field/expected shape.

**Exact source:** `routes.rs:425` raw Json<DeclareCardRequest>, `:305` raw Query<ListCardsRequest>; `:142`/`:614` raw by-ref query extractors once made strict. Upstream producers are strict Metadata (`wyrd-spec/src/envelope.rs:163`), CardRef (`reference.rs:22`), and ListCardsRequest (`registry/reads.rs:66`).

**Reachability:** DeclareCardRequest contains CardSubmission metadata, so removed metadata.space fails JSON data decoding before declare_card_http executes. Removed space inside a reference-bearing spec is rejected by the existing typed spec/reference owner; preserve its existing mapped registry errors where it is decoded in the service. GET `/v1/cards?space=prod` fails strict Query decoding before list_cards_http. Raw Axum rejections implement their own response, bypassing WyrdErrorResponse. JSON data errors return framework 422/plain text; query decode errors return framework 400/plain text. Neither response has the promised code/remediation/problem shape. The response mapper's existence does not intercept extractor failures: router apply_protected_edge maps tower errors, and request-ID middleware only enriches responses already marked problem+json. Current flat_card_http_contract accepts any client status and searches raw unknown-field text, so its recorded pass does not disprove this gap.

**Existing mechanism and selected error:** use Result<Query<T>, QueryRejection> / Result<Json<T>, JsonRejection> at these existing adapters, as principals/routes.rs:604 and gateway/routes.rs already do. Select **WyrdError::Validation**, stable **WYRD_SPEC_400_VALIDATION**, status **400**, rendered by WyrdErrorResponse. This is the existing generic request-shape error and is already advertised by declaration/list/by-ref OpenAPI responses; no new catalog variant or schema decision is needed. Do not force nested reference errors already mapped by the service into a different family: RegistryInvalidCardSpec is valid there and remains advertised.

**Precise correction boundary and adjacent behavior:** normalize QueryRejection at list and both by-ref adapters (these are existing 400 decode failures). On declaration, normalize **JsonDataError** shape/type refusals into the selected Validation response. Retain the native extractor, authentication and limits. Preserve the existing responses/statuses for other JsonRejection cases: malformed JSON syntax (400), missing/unsupported JSON content type (415), and body-read/length refusal (including 413). The shared Wyrd body-limit layer already returns its typed PayloadTooLarge before the handler; do not change it. The adapter can emit its ordinary success/Wyrd responses and pass other extractor rejections through untouched, using existing IntoResponse behavior. This avoids blanket conversion of 413/415 to 400 and avoids an unrelated general HTTP normalization project.

For removed-field rejection provide safe field/location and expected flat request shape in structured details and an actionable detail naming space. Reuse the native rejection cause where safe; do not invent a second JSON decoder or duplicate validation and do not echo submitted secret-bearing values. Authorization and declaration work remain after successful extraction, with their existing audit/transaction boundaries. No mapper middleware, new error family or replacement transport.

**Focused closure proof:** strengthen the same HTTP test with exact status, `application/problem+json`, code and actionable space-field detail for metadata.space, list space, and the by-ref refusals. Submit a nested CardRef carrying space and assert the existing typed registry validation refusal/no declaration writes (it may already take the service mapper; do not manufacture a change there). Use the existing no-declaration-writes helper for refused declarations. Check malformed syntax, absent/wrong content type and oversized body retain existing 400/415/413 behavior using the same request fixture or existing owner tests; no extra harness. Valid declarations/reads and permission failures retain their current behavior.

## FIND-TASK-001-3 — Required rustdoc is missing on material changed items

**Status:** REVISED. **Classification:** VIOLATION. **Discovery:** REPO-001, MNT-001.

Root AGENTS.md makes missing changed-item rustdoc blocking. `agent_docs/rust.md:233` explicitly includes private/test items, fields and tests, requires Errors/Panics sections and meaningful workflow/invariant descriptions. Passing compilation/Clippy cannot close that source gate.

All discovery examples are source-confirmed. The correction is documentation in the existing owners, without behavior changes or a new checker. Below is the bounded closure inventory: newly added items, changed signatures, changed matching/default/error boundaries and rewritten test outcomes. Fixture-only removed literal members, mechanical callers of removed arguments, deleted items and unchanged nearby helpers are excluded; their old missing documentation is not new scope. In-place changes made for findings 1/2 must also meet this rule.

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

The changed-item producer/consumer paths were checked: parser → loader PathResolver; Node/Edge hashes and graph-ready/build/canonical-order; constructors → cached defaults and holder-to-envelope conversion; CLI get/pull/delete → selector and list/check output; exact and list routes → service → tenant SQL; EffectiveSpecs → batch lookup; declaration fresh/reused nodes → relationship persistence; hydration reads → inbound references; resolve_existing/pin-existing-inline-Prompt → find_card_by_ref; tests → their changed helpers. Each listed item has a current caller or is an expressly required test proof. No behavior needs to move or be rewritten to add these docs. Nearby graph composition and server resolve fixture helpers already show meaningful Panics docs; SQL get_card_by_ref/recheck_active_card_refs show Errors/transaction contracts.

Source review of these sections is the proof for comments; no runtime test is required for documentation alone. Errors sections must describe the body actually present, and panic sections must name fixture/assertion assumptions, not generic placeholders.

## Minimal verification and preserved scope

Run the extended existing HTTP proof once after the adapter correction:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_declaration_route::flat_card_http_contract)'"
```

Use `mise run fmt:check` and `git diff --check` for formatting plus source inspection of the bounded rustdoc inventory. For the actual server adapter write set, a focused compile/lint is sufficient: `mise exec -- cargo clippy --locked -p wyrd-server --lib -- -D warnings`; the repository's full `mise run lints` remains the caller's final aggregate obligation, not a request for repeated workspace builds during this review. If documentation edits expose an actual link/type compilation failure, use only the affected crate's existing check rather than new runtime tests. No codegen change is required when the existing schema/error catalog is retained.

Do not rerun all SDK journeys, introduce an artifact harness, change permissions/audit/cleanup, normalize unrelated endpoints, tighten VersionListQuery, modify old migrations, or add a compatibility policy. Recorded SQL and journey passes support preserved behavior but do not replace the missing HTTP negative assertions. OIDC, production UI and private-login environment limitations remain unverified recorded limits and are not silently claimed closed.

Skipped independent runtime/build execution; source was kept immutable. Risk until remediation: removed by-ref input can still delete a tenant-local Card, removed-field decode errors can still lack the public Wyrd problem contract, and the changed-item rustdoc merge gate remains unmet.
