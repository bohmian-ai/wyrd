# TASK-012 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Candidate identity was rechecked before writing this report and remained unchanged.

## Authority coverage

| Changed surface | Owning authority and nearby pattern | Maintainer assessment |
|---|---|---|
| Workspace, `wyrd-client`, CLI, and workspace-hack dependency manifests | Task's exact `oauth2 = 5.0.0` / `webbrowser = 1.2.4` direction; `AGENTS.md` dependency-cost and feature rules | The dependencies are placed at their narrow owners, `oauth2` keeps default features disabled, and no new feature/configuration knob was introduced. |
| `wyrd-client/src/auth.rs`: `AuthHttp`, `TokenExchange`, OAuth response/error projection | `AGENTS.md` §§4-6 and §§15-16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client model; maintainer style owner/method and documentation guidance | `TokenExchange` remains the cohesive dependency-owning client-auth owner, and the adapter is required by the workspace `reqwest` version. Two material maintenance defects remain: the broad form-exchange surface still admits device/refresh grants, and touched/new item documentation is incomplete. |
| `wyrd-client/src/saved_login.rs`: locked refresh | REQ-012; existing `SavedLogins`/`SavedLoginSource` owner | Renewal stays at the existing owner and calls the new OAuth refresh method while retaining lock/reread/save sequencing. |
| `wyrd-cli/src/auth/login.rs` and `refresh.rs` | REQ-011; task's `webbrowser` and OAuth requirements; maintainer style workflow-owner guidance | `LoginFlow` remains the workflow owner. The per-OS launcher and manual polling loop were deleted; browser failure, `--no-browser`, Ctrl-C, and save behavior remain readable at the call site. |
| `wyrd-testing/src/human_login.rs` | `AGENTS.md` test-runtime ownership and journey guidance | The shared journey helper now reuses the production OAuth poll instead of preserving a second polling algorithm. |
| `wyrd-client/tests/transport/http.rs`, CLI journey, and auth unit tests | `AGENTS.md` §§11 and 16; maintainer style outcome-oriented test guidance | The transport fixture now speaks the RFC token response shape and is documented. The CLI journey still reaches the hand form exchange for device polling, exposing the production-surface drift described below. One modified auth fixture helper lacks required rustdoc. |
| Rust/Python/TypeScript consumer parity | `architecture/wyrd-design.md` and doctrine client model | No language-specific implementation or generated declaration changed. All SDKs continue to consume `wyrd-client`; the changed OAuth response type is used by Rust owners and journeys only. |

## Changed-symbol and caller trace

- `AuthHttp` is owned by `TokenExchange` and used by both `oauth2::AsyncHttpClient` and the retained RFC 8693/RFC 7523/RFC 7009 form path.
- `TokenExchange::device_authorization` and `device_access_token` are called by `LoginFlow`, `HumanSso`, and the CLI journey.
- `TokenExchange::refresh` is called by `SavedLogins::renew`, `wyrd auth refresh`, and the CLI journey.
- `TokenExchange::revoke_refresh_token` is called by CLI logout and `HumanSso::revoke`.
- `TokenExchange::exchange` is called by `AuthMiddleware::post_token_request` for RFC 8693 and RFC 7523, but its public `TokenRequest` parameter also remains reachable with `DeviceCode` and `RefreshToken`; the CLI journey uses the `DeviceCode` route directly.
- The three first-class SDKs reach saved-login renewal through the shared `SavedLoginSource`/`AuthMiddleware` path; no SDK-specific token logic was added.

## Material findings

### MNT-TASK-012-1 — The production form-exchange surface still provides a second device/refresh implementation

- Changed locations: `crates/shared/wyrd-client/src/auth.rs:261-275`, `crates/shared/wyrd-client/src/auth.rs:394-438`, and `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:288-304,341-346`.
- Governing rule: the approved task requires device and refresh to use `oauth2`, retains the hand form POST only for RFC 8693/RFC 7523 and the lead-approved RFC 7009 exception, and requires one conventional OAuth client path. `AGENTS.md` §15 requires the first correct standard/dependency-backed option and rejects duplicate ways to perform the same behavior.
- Evidence: `TokenExchange::exchange` remains public and accepts the full `wyrd_spec::auth::TokenRequest`, including `DeviceCode` and `RefreshToken`. Its private `grant`/`post_form` tail serializes any such variant manually. The CLI journey demonstrates that the device-code variant remains reachable through this production API even though `device_access_token` and `refresh` are now the intended `oauth2` methods.
- Concrete maintenance cost: the type and docs tell maintainers that device/refresh use `oauth2`, while the public API still offers a second path with different polling, response parsing, and future protocol behavior. A caller can bypass the vetted implementation without leaving `wyrd-client`, so future fixes to the OAuth path do not establish one client-wide behavior.
- Smallest testable correction: constrain the production form-exchange operation to the two grants it still owns (RFC 8693 token exchange and RFC 7523 JWT bearer), leaving device and refresh reachable only through their `oauth2`-backed methods. Drive one-shot pending/wrong/replay assertions through a test-local raw wire request rather than keeping a production bypass for the journey. Preserve the lead-approved single RFC 7009 form POST unchanged.

### MNT-TASK-012-2 — New and materially modified Rust items do not meet the repository's rustdoc contract

- Changed locations: `crates/shared/wyrd-client/src/auth.rs:131` (`AuthHttp`'s tuple field), `crates/shared/wyrd-client/src/auth.rs:151-152` (new associated types), `crates/shared/wyrd-client/src/auth.rs:350-361` (`refresh` cancellation/partial-progress contract), and `crates/shared/wyrd-client/src/auth.rs:1142-1150` (`token_body`).
- Governing rule: `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc on every new or materially modified Rust item, including fields, associated types, and test helpers, and require relevant cancellation/partial-progress behavior for async durable operations. Missing rustdoc is explicitly a hard blocker.
- Evidence: the `AuthHttp(Client)` field and its two `AsyncHttpClient` associated types have no rustdoc; the modified `token_body` test helper has none. The new `refresh` method documents ordinary errors but not the consequential uncertain-completion boundary: cancellation or transport loss can occur after the server rotates the token and before the replacement is returned, after which retrying the predecessor triggers the specified reuse response.
- Concrete maintenance cost: a maintainer cannot tell from the owner where the wrapped client's redirect invariant resides, what the adapter's boxed future represents, why the fixture shape is authoritative, or whether refresh can be safely retried after cancellation. The last ambiguity directly affects safe changes to the saved-login renewal workflow.
- Smallest testable correction: document the new field, associated types, and modified fixture helper, and add the refresh cancellation/partial-progress contract at `TokenExchange::refresh`. Keep this as documentation on the existing owner; no new abstraction, option, check, or file is warranted.

## Calibration notes

- The custom RFC 7009 form POST is not a finding. Lead direction fixes it as the one conventional exception required for loopback URLs rejected by `oauth2` 5.0's revocation builder.
- The Windows proof is not reopened. The task's `cargo check -p webbrowser --target x86_64-pc-windows-msvc` evidence and deletion of CLI per-OS launch code are the accepted boundary on this host.
- `AuthHttp` itself is justified rather than speculative: `oauth2`'s bundled reqwest integration targets a different reqwest major, while this adapter preserves the repository's shared client and redirect policy.
- Import ordering and equally clear local naming alternatives were not promoted to findings.

## Verification assessment

The task records successful focused shared-client, CLI, codegen/declaration, boundary, formatting, lint, and exact identity-journey lanes. Per orchestrator coordination, this review did not start competing Cargo or mise work. Closure needs only the narrow write-set proof for the remediation: the focused auth/CLI tests touched by the constrained form surface and documentation, followed by the applicable shared-client/CLI format and lint lanes. Full journey suites and repository aggregates are neither required nor recommended at task-remediation review.

## Result

**FAIL**

The owner structure and library migration are mostly maintainable, but the candidate still exposes a duplicate hand device/refresh path and violates the repository's hard rustdoc contract on touched items.
