# Repository Standards Review — TASK-005

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Candidate checked at review start and completion: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Scope reviewed: the complete base-to-candidate diff (27 changed files), surrounding owners and consumers, the approved specification revision 11, TASK-005, and its recorded verification evidence.
- CodeGraph: not used because the repository has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage result |
|---|---|---|
| Architecture identity and OAuth authority (`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`) | `AGENTS.md` §§1–2, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-design.md` client model/runtime identity/human sign-in; `architecture/wyrd-security-posture.md`; approved spec REQ-001, REQ-005, REQ-018, REQ-021 and INV-001/003/004/005/006 | Complete; one material contract-accuracy failure, `STD-TASK-005-1` |
| Public docs and generated error catalog (`docs/src`, `docs/scripts/generate_api_docs.py`) | `AGENTS.md` §§2, 9, 11–12, 16; doctrine public-surface rules; `architecture/references/architecture/patterns.md` client/agent/verification patterns; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md`; approved spec and TASK-005 | Complete; generator and generated page move together, prohibited legacy flow vocabulary is absent, but the OAuth success-shape statement fails as described below |
| Shared Rust client configuration rustdoc (`wyrd-client::ClientConfig`) | `AGENTS.md` §§3–6, 9, 16; `architecture/references/languages/rust-core.md`; client pattern; active design client model | Complete; documentation matches `SavedLogins::select` and origin canonicalization, and no behavior or API shape changed |
| CLI auth help/rustdoc (`AuthCommand`) | `AGENTS.md` §§4, 9, 16; design agent-first/headless surface; Rust core documentation rules; approved saved-login/logout decisions | Complete; help accurately states local-first, best-effort revocation and the token-free status projection |
| Server boot and SQL rustdoc (`rewrap_sealed_secrets`, refresh revocation queries) | `AGENTS.md` §§3–6, 9, 15–16; `architecture/agent-rules.md` SQL capability rules; Rust core Postgres/tenant boundary; security posture secret lifecycle | Complete; changed production SQL remains on `&mut TenantConn<'_>`, callees do not commit, and rustdoc no longer claims browser-session ciphertext exists |
| Python wrapper documentation and generated `.pyi` files | `AGENTS.md` §§7–8, 16; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; client pattern | Complete; source owner is `sdks/wyrd-sdk-python/src/client.rs`, public and source stubs agree, signatures are unchanged, and recorded `codegen:check` is green |
| TypeScript public wrapper, native binding rustdoc, generated `.d.ts`/`.d.cts` | `AGENTS.md` §§2–4, 9, 16; `architecture/references/languages/typescript-guide.md`; Rust core documentation rules; client pattern | Complete; native boundary continues to delegate to shared `wyrd-client`, public wrapper semantics agree, generated declarations match each other, and recorded `ts:napi:check` is green |
| UI README | `AGENTS.md` §§2, 9, 12; doctrine public-surface rules; design human sign-in; security posture OAuth/BFF contract | Complete; it documents shipped production sign-in without making the UI the source of truth |
| Active task/evidence update | `AGENTS.md` §§11–14; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md`; TASK-005 verification restriction | Complete; evidence uses the narrow write-set lanes and lists rather than reruns journeys |

## Rule results

| Repository rule | Evidence | Result |
|---|---|---|
| Public docs, architecture, SDKs, CLI, and UI must project the same server-owned contract. | The candidate aligns the saved-login chain, tenant mismatch, client types, refresh rotation, best-effort logout, callback, and RFC endpoints across the touched surfaces. The broad success-response claim at `architecture/wyrd-security-posture.md:208-212` and `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152` does not match the handlers or approved spec. | **FAIL** (`STD-TASK-005-1`) |
| Documentation must describe shipped behavior and materially modified Rust items require substantive rustdoc. | All touched Rust changes are documentation-only and describe their existing owners and workflow effects. `ClientConfig::tenant`, `AuthCommand::{Logout,Status}`, `rewrap_sealed_secrets`, `revoke_refresh`, `PyWyrdClient::__new__`, and `connect_wyrd_client` retain substantive owner-local documentation. | PASS |
| Tenant SQL accepts `TenantConn` and leaves transaction lifecycle to the caller. | `refresh_tokens.rs` production query signatures remain `&mut TenantConn<'_>` and execute through `conn.transaction()` without `commit` or `rollback`. No raw pool or new SQL path entered the diff. | PASS |
| Python-visible documentation and stubs are generated from their Rust/PyO3 owner and remain public-surface consistent. | `sdks/wyrd-sdk-python/src/client.rs` contains the source documentation; both `.pyi` projections carry the same signature and contract. TASK-005 records `codegen:check`, `py:format`, and `py:lints` passing. No runtime type/export changed, so a new Python journey or `py:typecheck` proof is not required for this doc-only delta. | PASS |
| N-API declarations are generated, committed, and verified; TypeScript must not fork authentication behavior. | Native rustdoc calls `wyrd_client::bifrost::client_from_options`; the TypeScript wrapper contains no parallel resolver. `index.d.ts` and `index.d.cts` agree and TASK-005 records `ts:napi:check` passing. | PASS |
| Generated artifacts must not be maintained as an independent authority. | The error catalog generator and output changed together; Python source documentation and stubs changed together; TypeScript native source and both generated declarations changed together. Recorded `codegen:check` and `ts:napi:check` passed. | PASS |
| Legacy handoff/BFF-channel/browser-session-row/sealed-completion/session-keyring contracts must not remain, and no SAML/SCIM/provider-specific implementation may be introduced. | Static inspection of the complete diff and current touched documentation found no prohibited handoff or private-channel contract. Provider entries are setup notes over standard OIDC; no executable provider branch, SAML, SCIM, hosted signup, or social login was added. | PASS |
| Verification must be the narrowest complete set for the write set, with no journey or broad aggregate rerun in this task. | TASK-005 records exit 0 for `docs:check`, `codegen:check`, `ts:napi:check`, `fmt`, `lints`, `py:format`, `py:lints`, and `git diff --check`; owner journeys were only listed and inherited from owning-task evidence. The UI-tree delta is README-only, so omitting the Svelte compiler check does not leave executable UI text unverified. | PASS |
| No gate may be weakened or bypassed. | The diff changes no check, lint allowance, ignore marker, test selector, or boundary allowlist. | PASS |
| Permanent code must not cite tasks or agents. | Task references remain inside the active change packet; no production source, SDK, docs page, or architecture passage introduces task/agent history. | PASS |

## Material findings

### STD-TASK-005-1 — OAuth success responses are documented as one RFC 6749 §5.1 shape when two endpoints intentionally use different standard responses

- **Violated rule:** `AGENTS.md` §§2 and 9 require architecture and every public surface to align with the shipped wire contract; TASK-005 requires docs to match the standard OAuth flows; approved spec REQ-021 limits RFC 6749 §5.1 to token success responses.
- **Classification:** public-contract documentation error.
- **Locations:** `architecture/wyrd-security-posture.md:208-212`; `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`.
- **Evidence:** The new text says the token, platform-token, device-authorization, and revocation endpoints all answer RFC 6749 §5.1 success JSON. The approved spec says only token success responses use §5.1 (`spec.md:174-179`). The shipped `device_authorization` handler returns the RFC 8628 `DeviceAuthorization` body (`crates/wyrd/wyrd-server/src/auth/cli_login.rs:43-77`), while `revoke` returns an empty `200` response (`cli_login.rs:265-307`) as RFC 7009 requires. The endpoint table immediately above the prose already identifies those standards.
- **Observable consequence:** An operator or client author reading the new normative security authority or self-hosting guide can implement the wrong success decoder for device authorization or expect a token response body from revocation. Because the same overstatement appears in architecture authority and public docs, it can also drive later implementation drift away from the approved conventional protocol behavior.
- **Required correction:** Keep the shared form-encoding, OAuth error JSON, `no-store`, and Wyrd-error-exception statements, but describe success responses by their actual standard owner: `/auth/token` and `/auth/platform/token` use the RFC 6749 §5.1 token response; `/auth/device_authorization` uses the RFC 8628 §3.2 device-authorization response; `/auth/revoke` returns empty `200` per RFC 7009 §2.2. Do not add a wrapper or custom response shape.
- **Focused proof:** Run `mise run docs:check`; statically compare the corrected authority and guide against approved spec REQ-021 and the existing handler/OpenAPI declarations above. No journey or aggregate is needed for this documentation-only correction.

## Non-blocking notes

None. Placement, naming, structure, and wording preferences were excluded from findings.

## Verification assessment

The task’s recorded lanes are appropriately narrow for a documentation, rustdoc, and generated-declaration write set. I did not rerun journey suites or broad aggregates. I independently ran `git diff --check` for the immutable range; it passed. The verification evidence is credible for syntax, generation parity, formatting, and linting, but those checks cannot detect the semantic OAuth success-shape error above.

## Overall result

**FAIL**

Repository-rule coverage is complete, but `STD-TASK-005-1` is a material public-contract documentation failure. The correction is bounded, standard-conventional, and documentation-only.
