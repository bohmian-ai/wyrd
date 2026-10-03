# Maintainer review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`

The candidate remained checked out at the stated commit while this review was
performed. I reviewed the complete base-to-candidate diff and expanded from the
provided navigation map into the owning modules, generated declarations,
credential-selection implementation, CLI logout path, refresh-token queries,
server sealing-key boot path, web-app browser-session owner, and relevant tests.

## Changed-surface coverage

| Changed surface | Owning source, callers, and parity inspected | Maintainer result |
|---|---|---|
| `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md` | Compared the new human-sign-in and OAuth authority with specification REQ-009/011/012/016/018/021, `architecture/wyrd-doctrine.mdx`, `wyrd-auth` issuance/refresh/revocation, shared-client auth, and the web-app session owner. The authority uses the shipped standard OAuth/OIDC model and does not introduce a parallel mechanism. | PASS |
| Shared-client configuration rustdoc | Read `ClientConfig::tenant`, `resolve_credential`, `SavedLogins::select`, canonical-origin handling, and the selection tests. The source documentation accurately describes newest-login selection, explicit tenant mismatch refusal, and origin-scoped matching. | PASS |
| CLI help and source documentation | Read `AuthCommand`, `login::logout`, `login::status`, `RefreshArgs`, and the saved-login summary. Help now matches delete-first, best-effort RFC 7009 revocation and the token-free status fields. | PASS |
| Server and SQL rustdoc | Read `rewrap_sealed_secrets`, its boot/test callers, `SealedSecretRewrap`, `active_refresh`, `revoke_refresh`, and chain/family revocation. The revised comments remove retired browser-session persistence claims and describe the actual provider/workload-secret and confidential-client behavior. | PASS |
| Python public source and generated declarations | Compared `PyWyrdClient::__new__` with both generated `.pyi` projections and the shared `client_from_options` path. Signatures, error conditions, and selection semantics remain aligned; the generated files were changed through their source contract. | PASS |
| TypeScript public source, N-API source, and generated declarations | Compared `WyrdClient.connect`, `connect_wyrd_client`, `index.d.ts`, and `index.d.cts`. The public option type, error projection, newest-login rule, and tenant-mismatch condition agree across layers. | PASS |
| Generated API documentation | Compared `docs/scripts/generate_api_docs.py` with `docs/src/content/docs/api/errors.md` and the OAuth handlers' response boundary. The checked-in page matches its generator and records the sanctioned OAuth error exception. | PASS |
| Public concept, client, CLI, self-hosting, and agent documentation | Read all changed pages and followed their claims to the client, CLI, server auth, OIDC relying party, refresh storage, sealing-key rotation, and BFF session code. The pages generally form a navigable standard OAuth/OIDC account, but the operator authentication page retains one directly contradictory session statement. See `MAINT-001`. | FAIL |
| UI README | Compared the production-auth description with `browser-sessions.ts`, login/logout routes, hooks, and integration tests. It accurately locates the encrypted cookie in the BFF and does not resurrect the removed server-side session design. | PASS |
| Task implementation-evidence record | Checked its acceptance mapping and narrow verification record against the actual write set. It provides useful navigation without leaking task IDs into product source. Its claimed semantic documentation closure is incomplete because of `MAINT-001`. | FAIL |

## Material finding

### MAINT-001 — the operator authentication page denies the shipped web-app session cookie

- **Location:** `docs/src/content/docs/self-hosting/authentication.svx:11`
- **Governing principle:** TASK-005 requires public self-hosted documentation to
  match the shipped standard flows; specification REQ-009 requires the BFF's
  encrypted `Secure`, `HttpOnly`, `SameSite=Lax` cookie; the maintainer guide
  requires documentation to clarify the contract and remain consistent with
  public typed behavior.
- **Evidence:** The page tells operators, without limiting the statement to the
  Rust server request boundary, that "There is no session cookie." The same
  candidate documents and implements the opposite at
  `architecture/wyrd-security-posture.md:200-205`,
  `docs/src/content/docs/concepts/authentication.svx:163-170`, and
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:101-107`:
  the BFF owns an encrypted browser cookie and presents a Wyrd access token to
  the server. The neighboring concept page was corrected to distinguish these
  two boundaries, but the operator page was not.
- **Concrete maintenance cost:** An operator following the self-hosting page is
  given two incompatible session models in the same documentation set. That
  makes the BFF cookie key, cookie lifecycle, and logout behavior appear
  accidental even though they are required production behavior, and invites a
  later maintainer to remove or misconfigure the actual session boundary.
- **Smallest testable correction:** Replace the categorical sentence with the
  already-established boundary wording: the Wyrd server keeps no browser
  session or ambient identity, while the web-app BFF keeps the encrypted
  HttpOnly cookie and presents a Wyrd access token on each server request.
  Preserve the approved cookie, best-effort logout, and access-token-until-expiry
  behavior. Prove the documentation correction with `mise run docs:check` only.

## Verification assessment

TASK-005 records successful narrow write-set verification for `docs:check`,
`codegen:check`, `ts:napi:check`, Rust format/lints, Python format/lints, and
`git diff --check`, plus owner-test listings rather than rerunning journeys as
the task requires. Those checks are appropriate and no broad aggregate is
needed here. They establish rendering, generation, declaration, and static
parity, but cannot establish the semantic truth of the contradictory sentence
in `MAINT-001`.

No placement, naming, structure, or wording preference is elevated to a
finding. The shipped RFC 8693 API-key exchange token type, ingress-owned device
rate limiting, best-effort RFC 7009 logout revocation, origin-reduced client
base URL, and access-token validity through expiry were treated as fixed
decisions and were not reopened.

## Overall result

**FAIL**

The changed surfaces are otherwise cohesive and preserve one shared client and
standard OAuth/OIDC path, but the public operator documentation still makes one
materially false statement about the required BFF cookie. Correcting that
single sentence is sufficient for this maintainer review.
