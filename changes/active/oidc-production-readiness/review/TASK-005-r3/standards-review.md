# Repository Standards Review — TASK-005 R3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Scope: complete base-to-candidate diff; approved specification revision 11;
  original TASK-005; the R1 and R2 review packets and remediation tasks; and
  the latest R2 implementation diff from
  `323ce32118ec72752a7736b8d42dd957abf6a094` to the candidate.
- CodeGraph: not used because the repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage result |
|---|---|---|
| Architecture identity and OAuth authority (`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`) | `AGENTS.md` §§1–2, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-doctrine.mdx`; approved spec revision 11; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Complete. The current authority distinguishes the tenant and platform planes, the four form endpoints, browser interactions, endpoint-specific success bodies, and the shipped `400`/`401`/`500`/`503` error statuses. |
| Rust source documentation in shared client, CLI, server auth/boot, and SQL owners | `AGENTS.md` §§3–6, 9, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md`; security posture | Complete. Changes are documentation-only; owner shape, imports, async boundaries, and production signatures are unchanged. The SQL owner still accepts `&mut TenantConn<'_>` and does not own commit or rollback. |
| Python wrapper documentation and generated `.pyi` projections | `AGENTS.md` §§7–8, 11–12, 16; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/testing-workflows.md` | Complete. `sdks/wyrd-sdk-python/src/client.rs` remains the source owner, the two generated projections agree, and no runtime signature, registration, or Python behavior changed. |
| TypeScript wrapper/native documentation and generated `.d.ts`/`.d.cts` projections | `AGENTS.md` §§2–4, 9, 11–12, 16; `architecture/references/languages/typescript-guide.md`; client pattern; `architecture/references/languages/testing-workflows.md` | Complete. The native layer still delegates credential resolution to shared `wyrd-client`; declarations agree with the source and no TypeScript runtime contract changed. |
| Generated API/error pages and `docs/scripts/generate_api_docs.py` | `AGENTS.md` §§2, 9, 11–12, 16; doctrine public-surface rules; `architecture/references/languages/errors.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md` | Complete. The generator and outputs agree, JSON API Problem Details are no longer generalized across the form or browser OAuth endpoints, and the generated guidance preserves the endpoint-owned wire shapes. |
| Self-hosting, operator, CLI, client, cloud-identity, Kubernetes, concepts, agent, and UI documentation | `AGENTS.md` §§2, 9, 11–12; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; doctrine; approved spec; TASK-005 and both remediation tasks; `architecture/references/architecture/patterns.md` | Complete. The shipped standard OAuth/OIDC paths, public/confidential tenant IdP client choices, saved-login behavior, BFF cookie custody, secret ownership, and platform/tenant split are accurately described. No placement, phrasing, or completeness preference is elevated into a finding. |
| Active task, remediation, review, and evidence artifacts | `AGENTS.md` §§11–14; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md`; `$wyrd-task-review` artifact contract | Complete. The cumulative packet preserves the original task and both remediation rounds, and the recorded docs/codegen/format/lint lanes are the narrow lanes selected for the documentation, generator, generated-page, and Rust-rustdoc write set. |

## Rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Architecture, HTTP/OpenAPI guidance, SDKs, CLI, UI, and docs project one server-owned public contract. | `architecture/wyrd-design.md:555-580` and `architecture/wyrd-security-posture.md:191-230` now describe the same client, grant, custody, form, browser, status, and access-token-lifetime boundaries as `crates/wyrd/wyrd-server/src/auth/oauth.rs:1-17,68-168`, `auth/authorize.rs:76-137`, `auth/cli_login.rs:76-307`, and `components/platform/routes.rs:63-153`. The public operator summary at `docs/src/content/docs/self-hosting/sso-and-oidc.svx:139-154` carries the same split. | PASS |
| Documentation is part of correctness, and materially modified Rust items have substantive owner-local rustdoc. | The changed items remain documented at `crates/shared/wyrd-client/src/config.rs:39-60`, `crates/wyrd/wyrd-cli/src/auth/mod.rs:20-35`, `crates/wyrd/wyrd-server/src/auth/oauth.rs:1-17`, `crates/wyrd/wyrd-server/src/boot/mod.rs:1504-1533`, `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:204-220`, `sdks/wyrd-sdk-python/src/client.rs:34-49`, and `sdks/wyrd-sdk-ts/native/src/client.rs:49-66`. The text names workflow role and relevant error or lifecycle behavior rather than restating symbol names. | PASS |
| Tenant SQL uses the sanctioned capability and leaves transaction lifecycle to its caller. | The cumulative change touches only rustdoc in `refresh_tokens.rs`; its production functions at lines 79-287 continue to accept `&mut TenantConn<'_>`. No raw `PgPool`, caller-passed transaction, `commit`, or `rollback` entered the changed owner. | PASS |
| First-class SDK bindings project shared `wyrd-client` behavior rather than duplicate credential selection or renewal. | `ClientConfig::credential` and `ClientConfig::tenant` remain owned at `crates/shared/wyrd-client/src/config.rs:39-60`; origin normalization and saved-login selection remain in `config.rs:198` and `saved_login.rs:121,212`. Python and TypeScript changes are documentation projections only. | PASS |
| Python and TypeScript generated declarations follow their source owners and retain public parity. | PyO3 source documentation and both `.pyi` projections agree on newest-login selection and `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE`; N-API source documentation and `index.d.ts`/`index.d.cts` agree. The recorded `codegen:check` is green, and no signature or exported type changed. | PASS |
| Generated API documentation is changed through its generator, and public error guidance matches endpoint-specific standard behavior. | `docs/scripts/generate_api_docs.py:41-69,107-139` owns the corresponding `docs/src/content/docs/api/openapi.md` and `api/errors.md` text. The form-endpoint exception and browser redirect/HTML outcomes match `OAuthError`, `authorize`, callback, and device handlers. Recorded `docs:check` and `codegen:check` passed. | PASS |
| No legacy handoff/private-BFF-channel/server browser-session/sealed-completion/session-keyring contract, compatibility alias, SAML/SCIM behavior, hosted-signup stub, or provider-specific executable path enters permanent surfaces. | Complete cumulative inspection found none. `docs/src/content/docs/self-hosting/authentication.svx:11` accurately separates the stateless API request boundary from the BFF cookie; `sso-and-oidc.svx:57-77` describes standard provider configuration without adding provider code. The Rust example uses existing `ClientConfig::credential`, not an alias. | PASS |
| Locked standard decisions remain closed. | `sso-and-oidc.svx:55,137,147-154`, `client-configuration.svx:106-141`, and `wyrd-security-posture.md:141-230` preserve ingress rate limiting, best-effort RFC 7009 revocation, RFC 8693 API-key exchange with `urn:wyrd:oauth:token-type:api_key`, origin-normalized saved-login selection, and access-token validity through expiry. | PASS |
| Verification stays narrow and no gate is weakened. | Available evidence records `docs:check`, `codegen:check`, `fmt`, and `lints` passing. The diff changes no check, allowlist, lint suppression, ignored test, selector, assertion, or runtime behavior, and it does not require a journey or aggregate rerun. | PASS |
| Permanent product source does not carry task or agent history. | Task IDs and reviewer history remain under `changes/active/oidc-production-readiness`; product source, architecture, generated declarations, and public docs contain no new task/agent references. | PASS |

## Material findings

None.

## Non-blocking verification note

`git diff --check 134f605367e65b41f1977d6c70ac8ca8b277a69e..bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
still reports one extra blank line at EOF in the prior
`review/TASK-005-r1/verdict.md`. This is the same previously validated
review-artifact-only condition recorded in R2; it has no shipped behavioral,
security, tenancy, durability, or public-contract consequence. Under the
standing direction, it is not a material finding and does not reopen a
documentation round.

## Verification assessment

The available lanes are the narrow complete set for the cumulative shipped
write set and the latest documentation-only remediation. They establish docs
render/build/link/a11y health, generated parity, Rust formatting, and Clippy.
Semantic public-contract accuracy was checked directly against the current
handlers and shared client owners. Full journeys, language suites, live IdP
tests, and repository aggregates were neither run nor required.

## Overall result

**PASS**

Repository-rule coverage is complete and no material standards violation
remains.
