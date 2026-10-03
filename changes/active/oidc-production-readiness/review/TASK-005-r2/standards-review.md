# Repository Standards Review — TASK-005 R2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Candidate checked at review start and completion: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Scope: complete base-to-candidate diff, original TASK-005, the TASK-005-r1 verdict and validated findings, TASK-005-R1, surrounding owners/consumers, and recorded verification.
- CodeGraph: not used because this repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage result |
|---|---|---|
| Architecture identity/OAuth authority (`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`) | `AGENTS.md` §§1–2, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-doctrine.mdx`; approved spec revision 11; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Complete; endpoint-specific success shapes, refresh behavior, BFF custody, workload issuer scope, and locked decisions align after remediation |
| Rust source documentation in `wyrd-client`, CLI, server auth/boot, SQL, PyO3, and N-API owners | `AGENTS.md` §§3–9, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `pyo3-boundaries.md`; security posture | Complete; changed items retain substantive rustdoc, existing owner boundaries, top-level imports, and existing SQL capability types (`&mut TenantConn<'_>` rather than a raw pool) |
| Python wrapper docs and generated `.pyi` declarations | `AGENTS.md` §§7–8, 11–12, 16; `python-api-and-stubs.md`; `pyo3-boundaries.md`; `testing-workflows.md` | Complete; PyO3 source remains the owner, public/source stubs agree, and generated parity has recorded proof |
| TypeScript wrapper/native docs and generated `.d.ts`/`.d.cts` | `AGENTS.md` §§2–4, 9, 11–12, 16; `typescript-guide.md`; client pattern; `testing-workflows.md` | Complete; the binding remains a thin projection of `wyrd-client`, declarations agree, and original-task evidence records `ts:napi:check` |
| Generated API/error documentation and its Python generator | `AGENTS.md` §§2, 9, 11–12, 16; doctrine public-surface rules; `errors.md`; `python-api-and-stubs.md`; `testing-workflows.md` | Complete; generated files follow their source, but the global exception statement omits browser-facing OAuth error shapes (`STD-TASK-005-R2-1`) |
| Self-hosted, operator, agent, CLI, client, cloud-identity, Kubernetes, and UI documentation | `AGENTS.md` §§2, 9, 11–12; `wyrd-design.md`; `wyrd-security-posture.md`; doctrine; approved spec; TASK-005/TASK-005-R1; `patterns.md` | Complete; one shipped HTTP-status mismatch remains (`STD-TASK-005-R2-2`); placement, wording, and structure preferences were excluded |
| Active task and prior review/remediation artifacts | `AGENTS.md` §§11–14; `spec-driven-development.md`; `testing-workflows.md`; `wyrd-task-review` artifact contract | Complete; all required r1 artifacts and remediation evidence are present, but the recorded `git diff --check` result is contradicted by the immutable range (`STD-TASK-005-R2-3`) |

## Rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Architecture, HTTP/OpenAPI, SDK, CLI, UI, and docs must project the same server-owned public contract. | The candidate now aligns OAuth success shapes, platform/tenant route ownership, refresh rotation, API-key exchange, BFF cookie custody, activation stamps, saved-login selection, and issuer scope. The global error-shape exception and operator status prose still disagree with reachable handlers. | **FAIL** (`STD-TASK-005-R2-1`, `STD-TASK-005-R2-2`) |
| Documentation must describe shipped behavior; only wrong, misleading, or required omissions block. | Both retained documentation findings name concrete reachable response behavior and client consequences. No placement, naming, structure, or stylistic preference is treated as a finding. | **FAIL** |
| Changed Rust documentation must explain intent, workflow role, and relevant effects/errors. | `ClientConfig::tenant`, `AuthCommand::{Logout,Status}`, OAuth module docs, `rewrap_sealed_secrets`, refresh-chain docs, `PyWyrdClient::__new__`, and `connect_wyrd_client` remain substantive and owner-local. No executable Rust item changed. | PASS |
| Tenant SQL must use `TenantConn` and leave transaction lifecycle with the caller. | The only touched SQL is rustdoc; production signatures remain `&mut TenantConn<'_>`, use the existing transaction, and do not commit or roll back. No raw `PgPool` propagated. | PASS |
| Generated artifacts must be changed through their owners and remain in parity. | `generate_api_docs.py` changes accompany generated API pages; PyO3/N-API source docs accompany generated Python/TypeScript declarations. Recorded `codegen:check` and original-task `ts:napi:check` are green. | PASS |
| First-class SDKs must project shared `wyrd-client` behavior rather than duplicate it. | Python and TypeScript constructors still call `client_from_options`; their changes are documentation only. | PASS |
| No legacy handoff/private-BFF-channel/server browser-session/sealed-completion/session-keyring contract, compatibility alias, SAML/SCIM behavior, hosted-signup stub, or provider-specific implementation may enter permanent surfaces. | Complete cumulative inspection found none. Common-provider setup notes describe standard configuration only; locked RFC 8693 API-key exchange, ingress device-page limiting, best-effort RFC 7009 revocation, origin reduction, and access-token validity through expiry remain intact. | PASS |
| Verification must use the narrowest complete lanes and must not claim a red check is green. | Docs/codegen/format/lint evidence is appropriately narrow, and original-task evidence covers the generated language declarations. However, `git diff --check base..candidate` exits 2 on the r1 verdict despite the task recording exit 0. | **FAIL** (`STD-TASK-005-R2-3`) |
| No gate may be weakened or bypassed. | No check, allowlist, lint suppression, ignored test, selector, or test assertion changed. | PASS |
| Permanent source must not carry task/agent history. | Task IDs and review history are confined to the active change packet; product source, architecture, SDK, and public docs introduce none. | PASS |

## Material findings

### STD-TASK-005-R2-1 — Global error guidance still excludes only the four form endpoints, although the shipped authorization endpoint has RFC redirect and HTML refusals

- **Violated rule:** `AGENTS.md` §§2 and 9 require generated docs and HTTP contracts to project the shipped server contract; TASK-005 requires the authorize surface and sanctioned OAuth error exception to be documented accurately.
- **Classification:** public-contract documentation error.
- **Locations:** `docs/scripts/generate_api_docs.py:60-66` and generated `docs/src/content/docs/api/openapi.md:25-32`; the same over-broad framing begins at `docs/scripts/generate_api_docs.py:123` / generated `docs/src/content/docs/api/errors.md:11-13` and `docs/src/content/docs/for-agents/error-remediation.svx:15-17`.
- **Evidence:** The changed prose says every operation/error is Problem Details except `/auth/token`, `/auth/platform/token`, `/auth/device_authorization`, and `/auth/revoke`. The reachable `GET /auth/authorize` owner instead redirects a validated client to its exact redirect URI with an RFC 6749 §4.1.2.1 `error`, and shows HTML for an unknown client or redirect (`crates/wyrd/wyrd-server/src/auth/authorize.rs:26-73,83-124`). Its OpenAPI declaration explicitly advertises `303` redirect errors and a mixed HTML/Problem response. This is a standard OAuth authorization-endpoint shape, not a fifth form-endpoint envelope.
- **Observable consequence:** A client author consuming the generated guidance can treat an authorization refusal as Problem Details and miss the `Location` query error or browser HTML response that Wyrd actually ships.
- **Required correction:** Qualify the global Problem Details claim so it does not cover browser-facing OAuth authorization interactions, and state the existing standard split: the four form endpoints use OAuth JSON while `/auth/authorize` uses the RFC 6749 authorization redirect (or local HTML before a client/redirect is trusted). Change the generator first and regenerate its pages; add no wrapper, alias, or new error identity.
- **Focused proof:** Compare the corrected generator/output with `authorize` and its served OpenAPI responses, then run only `mise run docs:check` and `mise run codegen:check`.

### STD-TASK-005-R2-2 — The SSO guide omits the shipped `500` and `503` OAuth refusal statuses

- **Violated rule:** `AGENTS.md` §§2 and 9 require public documentation to match the shipped wire contract; TASK-005/TASK-005-R1 require endpoint-specific OAuth contracts and failure guidance to be accurate.
- **Classification:** public-contract documentation error.
- **Location:** `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`.
- **Evidence:** The guide says form-endpoint refusals use `400`, or `401` for `invalid_client`. `OAuthError::status` returns `500` for `server_error` and `503` for `temporarily_unavailable` (`crates/wyrd/wyrd-server/src/auth/oauth.rs:65-76`), and the token, device-authorization, revocation, and platform-token OpenAPI declarations expose those reachable statuses (`components/auth/routes.rs:98-105`, `auth/cli_login.rs:60-68,284-290`, `components/platform/routes.rs:82-86`).
- **Observable consequence:** Operators and conventional OAuth clients can classify a dependency/audit outage as an undocumented response and apply the wrong retry or escalation behavior.
- **Required correction:** Describe the statuses Wyrd already ships: `400` for ordinary protocol refusal, `401` plus `WWW-Authenticate` for `invalid_client`, `500` for `server_error`, and `503` for `temporarily_unavailable`. Preserve the RFC error JSON, no-store headers, and current handlers; do not add a Wyrd-specific response or reopen grant behavior.
- **Focused proof:** Statically compare the corrected paragraph to `OAuthError::status` and the existing endpoint declarations, then run only `mise run docs:check`.

### STD-TASK-005-R2-3 — The immutable candidate does not satisfy its recorded `git diff --check` proof

- **Violated rule:** `AGENTS.md` §§11–12 require every selected verification command to pass and prohibit claiming completion with a red check; TASK-005-R1 explicitly requires `git diff --check`.
- **Classification:** verification/completion violation.
- **Location:** `changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md:99`; recorded claim at `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md:222-223` and `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md:158-160`.
- **Evidence:** Against the immutable subject, both `git diff --check 134f605367e65b41f1977d6c70ac8ca8b277a69e..323ce32118ec72752a7736b8d42dd957abf6a094` and the remediation-range command exit 2 with `TASK-005-r1/verdict.md:99: new blank line at EOF.` The implementation/task evidence records that command as exit 0.
- **Observable consequence:** The task's required completion proof is false, and the exact required check is red for the reviewed candidate.
- **Required correction:** Remove the extra blank line at EOF from the existing r1 verdict artifact and rerun `git diff --check` over the complete cumulative range used by the review. Do not alter the verdict content or broaden verification.
- **Focused proof:** `git diff --check 134f605367e65b41f1977d6c70ac8ca8b277a69e..<corrected-candidate>` exits 0.

## Non-blocking notes

None. Placement, naming, structure, wording preferences, and unrelated pre-existing debt were excluded.

## Verification assessment

The recorded `docs:check`, `codegen:check`, `fmt`, and `lints` lanes are the correct narrow lanes for the TASK-005-R1 documentation/rustdoc write set. The original TASK-005 evidence also records the generated-language checks needed for the cumulative Python and TypeScript declaration changes. No full journey suite or aggregate is required or recommended. Those static lanes cannot detect the two semantic contract mismatches above, and the independently checked cumulative `git diff --check` result is red as described in `STD-TASK-005-R2-3`.

## Overall result

**FAIL**

Authority and changed-surface coverage is complete. Three material repository-rule failures remain: two inaccurate public-contract statements and one false/red required verification result.
