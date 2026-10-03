# TASK-005 R3 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior remediations:
  - `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`
  - `changes/active/oidc-production-readiness/review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`

The candidate remained at the stated commit throughout discovery, follow-up,
validation, and verdict preparation. The complete base-to-candidate range was
reviewed, with the latest remediation diff used only to locate changed owners.
The repository has no `.codegraph/` directory.

The caller's standing direction controlled the audit: Wyrd uses standard OAuth
2.0/OIDC conventions and vetted libraries without invented machinery.
Documentation blocks only when it materially misstates shipped behavior or
omits task-required content. Placement, naming, structure, phrasing, optional
completeness, and precision preferences do not block. The locked RFC 8693
API-key exchange, ingress-owned device-page rate limiting, best-effort RFC 7009
logout revocation, origin-normalized base URL, and access-token validity until
expiry were not reopened.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
| --- | --- | --- |
| REQ-001 / AC-001: OIDC remains optional and operator, UI, SDK, and machine paths remain available | Architecture and public docs preserve API-key recovery, OIDC-off use, and independent machine identity; no executable behavior changed | PASS |
| REQ-005: only stored provider or secret-bearing workload-issuer secrets require sealing | Trusted-issuer, provider-client, configuration, and recovery guidance now matches the shipped secret owners | PASS; `FIND-TASK-005-2` remains closed |
| REQ-018: operator responsibilities, setup, recovery, client selection, human/workload separation, tenancy, and deployment guidance are accurate | Core setup and recovery flows are present, but the operator guide overstates bearer coverage, the cross-plane glossary assigns tenant semantics to platform identities, and implicit-tenant guidance misstates Host authority and self-hosted topology | FAIL — `FIND-TASK-005-9`, `FIND-TASK-005-10`, `FIND-TASK-005-11` |
| REQ-021: OAuth endpoints expose their shipped standard, grant-specific contracts | Endpoint-specific request, response, refusal, and status shapes are corrected, but `/auth/token` documentation still requires registered-client identification for clientless RFC 8693 and RFC 7523 machine grants | FAIL — `FIND-TASK-005-3` |
| INV-001 / INV-004: unverified routing data cannot become tenant authority and provider trust fails closed | Runtime remains correct, but public configuration guidance says every hosted request names its tenant through `Host`, obscuring the verified-token boundary | FAIL — `FIND-TASK-005-11` |
| INV-003: platform, tenant-user, and workload planes remain distinct | Routes and runtime owners remain separate, but the public cross-plane glossary says every principal/token is tenant-owned and every access token carries tenant/role claims | FAIL — `FIND-TASK-005-10` |
| INV-005: UI and SDKs project server-owned identity and permissions | BFF custody, shared-client ownership, saved-login selection, and generated language projections remain aligned | PASS |
| INV-006 and task non-goals | No hosted signup, social login, SAML/SCIM documentation, commercial stub, provider implementation branch, certified-provider list, compatibility route, or new protocol mechanism entered the diff | PASS |
| AC-002 / AC-003: self-hosted and hosted OIDC setup and isolation are documented | Callback, connection ownership, role mapping, and tenant isolation are covered, but the supported one-or-more-tenant self-hosted topology and Host's narrow pre-issuance role are misstated | FAIL — `FIND-TASK-005-11` |
| AC-004 and carried FIND-TASK-004-8: first-class clients describe device login, precedence, tenant selection, and newest-login behavior | Shared client, CLI, Python, TypeScript, generated declarations, and the corrected Rust example agree | PASS; `FIND-TASK-005-8` remains closed |
| AC-005 / AC-006 / AC-007: machine independence, provider replacement, recovery, fault behavior, logout, and bounded token lifetime are accurate | Activation-stamp, BFF cookie, public-client IdP setup, best-effort logout, and access-token-expiry corrections match source | PASS; `FIND-TASK-005-5`, `-6`, and `-7` remain closed |
| AC-008: provider-agnostic standard OIDC | Generic setup and five provider examples remain standard-only with no provider runtime branch | PASS |
| AC-009: architecture, docs, CLI/UI/SDK surfaces, generated declarations, and shipped behavior agree | Mechanical parity checks pass, but four validated public/source documentation boundaries remain materially false | FAIL — `FIND-TASK-005-3`, `-9`, `-10`, `-11` |
| Documentation-only scope and locked decisions | No runtime, dependency, endpoint, storage, retry, or compatibility behavior changed; all standing decisions remain intact | PASS |

## Independent review results

| Review | Result | Material proposals |
| --- | --- | --- |
| Behavior | FAIL | `BHV-R3-001`, `BHV-R3-002` |
| Invariants | FAIL | `INV-R3-001` |
| Repository standards | PASS | None |
| Maintainer | PASS | None |
| System resilience | PASS | None; candidate has no runtime or deployment effect |
| OAuth/OIDC security domain | FAIL | `SEC-R3-001` |
| Focused follow-up | RESOLVED | All three disputed source paths were materially misleading, not precision preferences |
| Structured Ponytail validation | COMPLETE | Four retained findings: `FIND-TASK-005-3`, `-9`, `-10`, `-11` |

The follow-up was required because the repository, maintainer, and system
reviews reported no material defect while the behavior, invariant, and
security reviews raised reachable documentation-contract failures. It traced
the optional OAuth client through every grant arm, the platform human and
token shapes through the glossary, and Host use through its sole workload
exchange caller. The fresh Ponytail validator independently confirmed the
material consequences, rejected the no-finding conclusions for those paths,
and produced the final ledger.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
| --- | --- | --- | --- |
| `FIND-TASK-005-3` | REVISED | INCORRECT | Scope registered-client requirements at `/auth/token` to authorization-code, refresh, and device-code grants; keep RFC 8693 API-key/delegation and RFC 7523 JWT-bearer grants clientless, device authorization/revocation client-bound, and platform exchange clientless |
| `FIND-TASK-005-9` | CONFIRMED | INCORRECT | Scope signed Wyrd bearer authentication to protected/authenticated resource requests so public metadata and auth/bootstrap routes are not documented as requiring the token they establish |
| `FIND-TASK-005-10` | CONFIRMED | INCORRECT | Qualify the cross-plane glossary so platform human `User`s and access-only sessions remain tenantless and do not advertise tenant/role claims that cannot exist |
| `FIND-TASK-005-11` | CONFIRMED | INCORRECT | State that protected tenant authority comes from the verified Wyrd token, Host/form tenant is only an untrusted workload JWT-bearer candidate selector, boot uses the configured slug because it has no request context, and self-hosting supports one or more tenants |

The complete source traces, consequences, correction boundaries, and focused
proof are in `findings-validation.md`. None requires a specification revision,
runtime change, new client, token field, route, header, middleware exception,
tenant lock, compatibility surface, or custom OAuth/OIDC behavior.

## Prior-finding closure

| Prior finding | Status |
| --- | --- |
| `FIND-TASK-005-1` | CLOSED: form-native OAuth errors and Wyrd-code logging are scoped to their actual owners. |
| `FIND-TASK-005-2` | CLOSED: trusted issuers are workload-only and secret-bearing variants retain sealing. |
| `FIND-TASK-005-3` | NOT CLOSED, REVISED: the platform route and access-only behavior are correct, but replacement text still generalizes registered-client identification across clientless tenant machine grants. |
| `FIND-TASK-005-4` | CLOSED: form and browser endpoint shapes, success bodies, refusals, and statuses are distinct and accurate. |
| `FIND-TASK-005-5` | CLOSED: the stateless Wyrd API and BFF-owned encrypted cookie are distinguished. |
| `FIND-TASK-005-6` | CLOSED: activation uses the exact-revision persisted stamp with no provider re-probe. |
| `FIND-TASK-005-7` | CLOSED: generic tenant IdP setup permits shipped confidential and public clients. |
| `FIND-TASK-005-8` | CLOSED: the Rust example uses `ClientConfig::credential`. |

## Verification and limits

Fresh review-time verification passed:

- `mise run docs:check`
- `mise run codegen:check`
- `mise run fmt:check`
- `mise run lints`

These are the narrow documentation, generated-contract, Rust formatting, and
Rust lint lanes covering the cumulative/latest remediation write set. They do
not prove prose semantics. No journey, live-provider, browser, language, or
repository aggregate suite was run or required.

The cumulative `git diff --check` still reports the already-known extra blank
line at EOF in `review/TASK-005-r1/verdict.md`. That prior review-artifact
whitespace has no shipped or task-required documentation consequence and is
non-blocking under the standing direction.

## Verdict

**FIX_REQUIRED**

Four bounded documentation/source-documentation corrections remain. They are
packaged in `TASK-005-R3-auth-and-tenancy-doc-closure.md` and require no new
product, public API, architecture, security, compatibility, concurrency,
resource-ownership, or persistent-data decision.
