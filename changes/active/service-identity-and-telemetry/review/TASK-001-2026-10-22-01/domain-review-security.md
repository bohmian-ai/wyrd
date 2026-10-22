# TASK-001 security and authorization domain review

Result: **FAIL**

Subject: `f38dff80327bb771975e2cdd8fbed283ab145159...0a29bae4f4ff20e98d2c7937f3943877ac31bc62`. HEAD was the candidate when inspection started and when this report was written. The source was kept immutable. No sibling review or user completion summary was consulted.

## Reviewed boundary and authority

This review follows identity from authored/wire Card references through tenant-bound resolution, Card-backed principal projection, credential/workload lookup, signed scope claims, receiving permission checks, HTTP/MCP reads and deletion, and SDK identity checks. Persistence concurrency and recovery are outside this assigned domain. TASK-002's stable-account redesign is explicitly not an acceptance requirement for TASK-001: the existing Card-created principal and initial workload-role grant must remain until that task changes them.

Authorities inspected: root AGENTS.md; SQL AGENTS.md; architecture/README.md; wyrd-design.md reference-slot and flat identity authority; wyrd-security-posture.md principles, trust boundaries, principal/credential lifecycle, access tokens, request principal and permission model; approved revision-12 specification REQ-018–020, INV-001/002/006/007, AC-006/010; auth supplement's current/proposed distinction and tenant/account/context separation; original TASK-001 scenarios, preserved permissions/UID/tenant behavior and recorded Implementation Evidence; changes/spec-driven-development.md; agent_docs/index.md and maintainer-style.md. There is no `.codegraph/` directory.

The cumulative base-to-candidate diff, rather than only the latest commit, was the review input. The changed-file inventory was expanded into these security-relevant owners and consumers:

| Surface | Source inspected and conclusion |
| --- | --- |
| Flat contract and claims | `wyrd-spec/src/reference.rs` CardRef, identity key, parser, CardRefScope serialization/deserialization and membership; `auth/card_scope.rs`; metadata and registry DTO changes. CardRef objects reject removed fields, text rejects extra slash segments, and scope text retains optional UIDs. Flattening removes an axis without adding another namespace or tenant selector. |
| Credential/account projection | `wyrd-sql/src/queries/cards/auth_projection.rs`, `queries/auth/service_accounts.rs`; `wyrd-auth/src/issue_api_key.rs`, `jwt_bearer.rs`, `issuance.rs`, `card_scope.rs`; cumulative changes in exchange/refresh/revoke and resolvers. Projection keeps tenant/principal-kind/Card-kind/Card-UID conflict identity and grants only on first insertion; credential lookup retains JSON containment for optional UID and refuses multiple matches. Grants are still resolved by the existing issuer, independently of reference spelling. |
| Signed principal and scope | Shared auth issue/verify and runtime principal/request-context cumulative changes are test-constructor removal of space, not authentication or permission-policy changes. Surrounding verifier checks expected tenant, keeps platform/System separation, derives runtime permissions from signed claims and seeds existing own-card scope. Issuance resolves graph members through TenantConn and stamps registry UIDs. No new root/account/permission authority is introduced. |
| Receiving HTTP/MCP boundary | Server cards routes/service/resolve/mapping, auth/card_scope, MCP cards, hosted registry, cache and gateway changed consumers. Caller derives `data_tenant_id` from verified principal (`components/auth/caller_extractor.rs:59`); registry operations open a tenant connection using that value. HTTP/MCP share permission/audit owners; hosted reads keep the accepted caller and exact-reference UID check. Removed by-ref query input is the material exception below. |
| Registry tenant and UID seam | TenantConn binding; original cards, relationships and auth RLS definitions; flat migration; cards get/list/delete/declare/relationships/version query changes and row projections. New query signatures keep TenantConn; no caller payload supplies its effective tenant. New registry uniqueness includes tenant and kind. Relationship recheck retains exact expected UID and active status. Migration does not disable forced RLS, change its policies, or introduce a definer bypass. The replacement verification-target function remains SECURITY INVOKER with pinned empty search_path and existing bound-tenant check. |
| Client/public consumers | Shared Cards selectors, get/delete/resolve_latest, pull and offline-state changes; CLI key issuance and workload binding; Python/TS and Rust journey/fixture diffs insofar as they carry identity/credentials; server config/boot workload binding and identity_e2e changes; Skald/Vala correlation fixture changes. GET still checks optional UID/name/version on the response; CLI removes the flag; workload config rejects unknown fields; MCP CardRef/ListCardsRequest reject unknown fields. No credential broadening was found in the inspected journey fixture changes. |
| Existing permission proof | Server declaration/read/delete permission/tenant tests, flat HTTP test, SQL flat-registry test, auth/scope test changes, principal-role journey changes and recorded lanes. These support intended preservation but do not exercise the ignored by-ref query axis. |

## Material proposed finding

### SEC-001 — Removed HTTP space selectors are accepted and discarded, including on deletion

Classification: **INCORRECT**. Priority: **P1** because a formerly meaningful destructive selector is silently discarded. This is a public input-contract defect, not a claim of cross-tenant access or an RBAC bypass.

**Violated obligation.** TASK-001 says removed forms must fail with actionable existing typed validation, and Scenario 3 explicitly includes HTTP and old selectors. AC-006 requires removed space forms to fail. The task also preserves exact selection and permissions rather than interpreting a removed identity dimension as a wildcard.

**Locations.** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:630` (`DeleteCardRefQuery`) and `:637` (`CardRefQuery`); their extractors at `:142` and `:614`.

**Evidence and reachable consequence.** Both query structs derive Deserialize with no `deny_unknown_fields`. Their changed definitions remove the required `space` field. Consequently an authenticated request such as:

```text
DELETE /v1/cards/by-ref?kind=Prompt&space=nonexistent&name=flat-http&version=1.0.0
```

successfully decodes the remaining three fields and drops `space`. `DeleteCardRefQuery::into_card_ref` builds a flat CardRef, the existing `cards:write` check succeeds for a permitted writer, `service::delete_card_by_ref` (`service.rs:2120`) opens the writer's tenant transaction, and `soft_delete_card_by_ref` (`wyrd-sql/src/queries/cards/delete.rs:168`) selects solely by kind/name/version before committing the soft delete and artifact cleanup. An old caller that selected a nonexistent/different space therefore deletes the local flat Card instead of receiving the required removed-input refusal. GET has the same silent reinterpretation and returns the flat Card.

The sibling `ListCardsRequest` already has `deny_unknown_fields` (`wyrd-spec/src/registry/reads.rs:66`); MCP CardRef parsing is also strict. The defect is at these two receiving HTTP query DTOs, not in the SQL lookup or permission evaluator. The correct flat SQL lookup must remain space-free.

**Proof assessment.** `flat_card_http_contract` (`pg_card_declaration_route.rs:4688`) checks a valid flat by-ref GET, the deleted space-bearing latest path, and a declaration carrying metadata.space. It never sends the removed query parameter to GET or DELETE, so its recorded pass cannot close this path. No runtime reproduction was executed in this review; the finding is established by the Deserialize definitions and complete route-to-commit trace.

**Smallest testable correction.** Make both receiving by-ref query DTOs reject unknown fields, following the existing strict DTO contract. Map query-decode refusal through the existing typed Wyrd validation/problem response pattern (for example the Result<Query<_>, QueryRejection> handling already used by principals/routes.rs:604); a bare Axum text rejection alone does not fulfill the typed-error promise. Do not restore space to lookup, add a compatibility alias, modify grants, or add downstream SQL guards. Extend the existing HTTP journey with removed-space GET and DELETE requests against a real registered Card. Assert actionable typed 400 refusal naming space, and read the Card afterward to prove DELETE changed nothing. Keep valid flat reads/deletes and insufficient-grant/foreign-tenant refusals intact.

## Verification limits

This was a static, read-only domain review. No Cargo command, server process, migration, or test was started, avoiding overlap with other reviewers. The original task records successful SQL, auth-related owner lanes and surface journeys; those results were read as claims and compared with test source, not independently reproduced. The real OIDC identity_e2e lane is expressly recorded as compiled but not run. Neither it nor a green aggregate substitutes for the missing removed-query proof.

No additional material RBAC, signed-claim, platform/tenant separation, or tenant-binding regression was established in the inspected cumulative change. TASK-002 redesign, generalized auth hardening and pre-existing unchanged behavior were not proposed as remediation.

Skipped: runtime tests and OIDC federation execution. Risk: the current HTTP DELETE boundary can silently ignore an old space selector and delete the flat tenant-local Card.
