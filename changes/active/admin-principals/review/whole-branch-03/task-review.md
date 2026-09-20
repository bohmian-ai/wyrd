# Task implementation review — admin principals whole-branch re-review 03

## Review Findings

### Critical

None.

### Important

- **`TREV-R3-1` — INCORRECT — [`crates/wyrd/wyrd-auth/src/platform_authz.rs:104`](../../../../../crates/wyrd/wyrd-auth/src/platform_authz.rs), [`crates/wyrd/wyrd-server/src/components/platform/identity.rs:118`](../../../../../crates/wyrd/wyrd-server/src/components/platform/identity.rs), [`crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:134`](../../../../../crates/wyrd/wyrd-server/src/components/platform/provisioning.rs).** Platform authorization records do not reliably identify the resource acted upon. `PlatformAuthorization::authorize` accepts only an optional tenant and otherwise writes literal `platform`; the shared identity/credential caller always supplies `None`, so registration, suspension, OIDC configuration, credential issue, and credential revoke decisions cannot identify their target. More seriously, provisioning audits the newly generated tenant ID before `insert_provisioning_tenant` can return `TenantClaim::Resumed`; a retry therefore commits an allowance naming a proposed, discarded ID rather than the tenant actually resumed. This violates REQ-037 and AC-009 and makes retained audit evidence misleading. **Required correction:** keep the canonical append and transaction owner, but make the platform authorization path accept the exact operation resource, have identity and credential callers provide their target IDs, and make resumed provisioning record the final claimed tenant in the same transaction before commit. Add assertions for identity, credential, fresh-provision, and resumed-provision resource values.

- **`FIND-admin-principals-R2-3` — INCORRECT (retained and revised) — [`crates/wyrd/wyrd-auth/src/refresh.rs:240`](../../../../../crates/wyrd/wyrd-auth/src/refresh.rs), [`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:3744`](../../../../../crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs).** API-key exchange now carries its credential ID through `cid`, but refresh rotation explicitly creates the next access token with `credential_id: None` even though the durable consumed refresh row ID is available as `rotated_from`. A TenantAdmin can therefore refresh, perform the covered administrative operation exercised by the journey, and produce an authorization record with no credential attribution. The journey proves token use and replay refusal but never inspects that audit field. This remains contrary to REQ-037/AC-009's credential-attributed decision record; federated sessions are the explicit no-credential case, not stored refresh credentials. **Required correction:** use the existing `cid`/principal/audit flow to attribute the durable refresh credential that established the rotated session, preserve `None` for genuinely federated sessions, and assert the exact non-secret ID on the protected decision after refresh.

- **`FIND-admin-principals-13` — INCORRECT (retained and revised) — [`crates/wyrd/wyrd-server/src/http/openapi.rs:329`](../../../../../crates/wyrd/wyrd-server/src/http/openapi.rs).** The generated document contains the administrative paths, typed bodies, auth scheme, and problem media type, but its stable-error check deliberately applies only to `Auth` and `Admin` tags and skips the administrative `Platform` and `Principals` tags. Even within checked tags, it only looks for any `_<status>_` substring, so one named code can conceal other reachable stable errors at the same status; for example `/auth/token` declares the invalid-key 401 while refresh-reuse/revocation are reachable, and platform operation descriptions generally contain prose rather than their reachable catalog codes. AC-014 and REQ-036 require generated stable error contracts for every administrative path, not merely one status-shaped string. **Required correction:** extend the existing generated operation/error metadata mechanism to enumerate every reachable stable code for all four administrative tags, regenerate OpenAPI, and test exact per-operation code sets plus problem media types.

- **`TREV-R3-2` — MISSING — [`changes/active/admin-principals/review/whole-branch-02/TASK-001-008-R2-close-re-review-findings.md:345`](../whole-branch-02/TASK-001-008-R2-close-re-review-findings.md).** The recorded R2 verification does not meet VER-002's exact-test evidence requirement. Two cited exact selectors do not exist: `a_failed_mutation_discards_its_own_allowance` is actually `a_failed_platform_mutation_leaves_no_allowance`, and `a_decision_records_the_kind_it_was_made_by` is actually `a_decision_records_the_stored_principal_kind`; the refresh, cached tenant-admission, and constant-cost verifier proofs are reported only through aggregate lanes despite being specifically named acceptance evidence. Those commands can pass after selecting no test or do not establish the claimed proof. **Required correction:** run and record the existing tests with their exact package, target, and `test(=...)` selectors through `mise exec --` and the repository-managed setup, including `a_tenant_administrator_refreshes_and_cannot_replay` and `every_invalid_api_key_costs_exactly_one_verification`. This evidence finding may be closed by the orchestrator's fresh sequential verification; no production-code change is implied.

### Suggestions

None. Optional cleanup and unrelated pre-existing debt are excluded from this acceptance audit.

## Open Questions

None. The approved specification resolves the behavior needed for each correction above.

## Immutable Subject and Scope

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`
- Authority: `changes/active/admin-principals/spec.md`, revision 7; `TASK-001` through `TASK-008`; both prior whole-branch review directories and their R1/R2 remediation tasks; repository `AGENTS.md` and applicable architecture/testing authorities.
- Reviewed range: the complete cumulative base-to-candidate diff, including the remediation delta, source, migrations, generated artifacts, tests, documentation, and prior evidence.
- The branch owner's explicit waiver covers all of `FIND-TASK-001-10`, including trailers and the 49 historical Claude author/committer identities. It is not reopened.
- The branch owner explicitly approved the verified-change-contract work in the cumulative candidate. It is not classified as drift.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–REQ-011; TASK-001; AC-005/AC-010: closed principal/kind model, roles, credentials, safe secret handling, multiple credentials, constant verifier work | `wyrd-spec/src/principal.rs`; `wyrd-auth/src/exchange_api_key.rs`; `credential_verify.rs`; SQL principal/credential queries and migrations | Unit/integration/journey tests exist, including exact-cost assertions; fresh execution owned by orchestrator | PASS |
| REQ-012–REQ-019; TASK-002: OIDC discovery/binding and TenantAdmin sessions | OIDC owner modules, stored bindings, issuer administration, tenant token exchange and refresh | Focused OIDC and platform journeys exist | PASS, except refreshed credential attribution under `FIND-admin-principals-R2-3` |
| REQ-020–REQ-024; TASK-003; AC-001: explicit deployment initialization and bootstrap credential | shipped `wyrd-server init`; `boot/init.rs`; process journey invokes `CARGO_BIN_EXE_wyrd-server` and proves repeat behavior | Exact process test exists; fresh execution pending | PASS |
| REQ-025–REQ-028; TASK-004; AC-007/AC-008: platform tenant provisioning, retry, recovery, and usable administrator | `components/platform/provisioning.rs`; retry retires abandoned credentials; `recover-root` and recovery journey | Journeys cover retry and total root loss | **FAIL** — resumed provisioning records the wrong audit resource (`TREV-R3-1`) |
| REQ-029–REQ-031; TASK-005; AC-002/AC-004/AC-012/AC-017: tenant administrator mutations, fail-closed audit, suspension and reactivation | tenant issuer/binding/revoke effects now use the allowance transaction; tenant admission is resolved per request | Integration/journey tests cover failure and cached-session suspension/resume | PASS, subject to cross-cutting audit-field failures below |
| REQ-032–REQ-033; TASK-006; AC-006: reserved namespace and delegated principal lifecycle | principal/card policy and delegated lifecycle implementations | Unit/integration coverage recorded | PASS |
| REQ-034–REQ-035; TASK-007: human tenant identity is external and Card-free | OIDC-backed subject resolution and authority documentation | OIDC tests and docs checks recorded | PASS |
| REQ-036: typed language-agnostic administrative HTTP contracts and stable generated errors | typed handlers and generated OpenAPI exist | `codegen:check` reported, but the source assertion skips administrative tags and exact code sets | **FAIL** — `FIND-admin-principals-13` |
| REQ-037–REQ-040; AC-009/AC-011/AC-013: canonical complete audit attribution and publication | canonical `vala.audit_staging` append/publisher path; authorization carries principal kind and initial API-key ID | Audit tests cover append/publication and selected fields | **FAIL** — exact resource is absent/wrong and refresh loses credential ID (`TREV-R3-1`, `FIND-admin-principals-R2-3`) |
| REQ-041–REQ-046; AC-015–AC-017: platform route plane, role separation, transactionally audited mutations, admission gates | platform caller/session types, permission checks, mutation-on-allowance transaction, per-request tenant state gate | Platform journey and failure-injection coverage exist | **FAIL** only where the committed platform audit resource is generic or wrong (`TREV-R3-1`) |
| REQ-047; TASK-008; AC-003/AC-014: consolidation, generated contracts, operator docs | schema/index consolidation, authority docs, OpenAPI generation | Structural/doc/codegen evidence recorded | **FAIL** only for incomplete stable-error declarations (`FIND-admin-principals-13`) |
| INV-001–INV-006: tenant authority, secret hashes, no wildcard platform grant, role separation, no tenant takeover | typed principals/permissions, hashed credentials, separate platform context and endpoints | Negative auth tests and journeys exist | PASS |
| INV-007–INV-010: fail-closed canonical audit, complete attribution, no parallel audit store | one staging table/publisher; mutation transaction improvements | Failure injection and publication coverage exist | **FAIL** for resource and refreshed credential fields; no new audit table remains |
| INV-011–INV-015: RLS, stable errors, Card-free identity, no platform tenant data access, bootstrap-only startup | tenant connections/RLS, typed errors, startup/init split | SQL/auth/process tests exist | **FAIL** only for generated error-contract completeness (`FIND-admin-principals-13`) |
| Non-goals and scope boundaries: no UI/CLI/Python/TS admin client expansion; no human Card; no legacy aliases; approved adjacent contract work only | Cumulative diff and authorities | N/A | PASS |
| VER-001–VER-006: format/lints/scoped lanes, exact named tests, no broad gate, codegen/docs | R2 packet records aggregate green lanes | Two exact selectors are invalid and several named proofs lack exact commands; this reviewer was instructed not to execute Cargo/mise | **FAIL pending fresh proof** — `TREV-R3-2` |

## Prior-Finding Closure

| Prior finding | Revalidation result | Independent source basis |
|---|---|---|
| `FIND-admin-principals-1` | CLOSED | Allowed platform mutation now receives the open audited `TenantConn`; effect and allowance commit together. |
| `FIND-admin-principals-2` | CLOSED | Platform query owners no longer expose/use a raw `sqlx::Transaction`; existing `TenantConn` and query-layer pool ownership are reused. |
| `FIND-admin-principals-3` | CLOSED | Reachable public error paths map to catalogued safe problems rather than exposing database/source strings. |
| `FIND-admin-principals-4` | CLOSED | Design and security authorities now define the two planes, closed kinds, and Card-free human identity. |
| `FIND-admin-principals-8` | CLOSED | Last-admin protection counts usable administrators and serializes the decision; pinned-current and concurrent cases are covered. |
| `FIND-admin-principals-13` | **RETAINED/REVISED** | Paths/media/auth scheme were repaired, but exact stable-error declarations remain incomplete. |
| `FIND-004-3` | CLOSED | Resumed provisioning retires abandoned tenant-admin credentials before issuing the sole usable replacement. |
| `FIND-005-1` | CLOSED | Tenant issuer, binding, and revoke effects use and commit the allowance transaction. |
| `FIND-003-2` | CLOSED | The process test invokes the shipped `init` subcommand and proves repeat behavior. |
| `FIND-004-5` | CLOSED | `recover-root` restores control after all platform credentials are lost and the journey exercises recovered authority. |
| `FIND-TASK-001-10` | **WAIVED** | Explicit branch-owner authority covers trailers and all historical Claude identity lines. |
| `FIND-admin-principals-R2-2` | CLOSED | Stored principal kind is carried through platform context into the canonical audit event. |
| `FIND-admin-principals-R2-3` | **RETAINED/REVISED** | Initial API-key flow is fixed, but refresh rotation deliberately clears credential attribution. |
| `FIND-admin-principals-R2-4` | CLOSED | TenantAdmin refresh rotates successfully and replay is refused in the protected journey. |
| `FIND-admin-principals-R2-5` | CLOSED | Tenant admission is checked per request rather than hidden behind the principal epoch cache. |
| `FIND-admin-principals-R2-6` | CLOSED | Malformed and invalid tenant API keys perform the same verifier work and return the same public error. |

All whole-branch-01 findings not carried into whole-branch-02 remain closed after source reinspection. The two retained IDs above preserve their original defect roots; `TREV-R3-1` is a newly observed audit-resource defect, and `TREV-R3-2` is a verification-evidence defect for Wave 2 to validate.

## Verification Notes

- Per assignment, this reviewer did not run Cargo or mise. The R2 report claims the scoped lanes passed sequentially, but VER-002 is not credible as recorded because two selectors name no existing test and other named proofs are aggregate-only.
- Source inspection confirms that the intended regression tests exist under different exact names: `a_failed_platform_mutation_leaves_no_allowance`, `platform_authz::pg_tests::a_decision_records_the_stored_principal_kind`, `a_tenant_administrator_refreshes_and_cannot_replay`, and `exchange_api_key::pg_tests::every_invalid_api_key_costs_exactly_one_verification`.
- The orchestrator owns current sequential execution. Fresh green runs can close `TREV-R3-2`, but cannot close the three source-level implementation findings without a new immutable candidate.
- The candidate was `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132` at the beginning and end of this report write.

## Overall Result

**FAIL.** Most R2 repairs are real and close 14 of the 16 prior findings (one by explicit waiver), but the candidate still lacks exact audit-resource fidelity, loses credential attribution after refresh, and does not generate a complete stable-error contract for every administrative operation. Recorded exact-test evidence is also incomplete until the orchestrator supplies fresh proof.
