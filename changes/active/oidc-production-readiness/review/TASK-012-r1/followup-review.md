# TASK-012 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate and reviewed `HEAD`: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`

This pass investigated only the three conflicts assigned by the orchestrator. It
read all eight discovery reports in this review directory and independently
traced the disputed source paths. It did not validate a final ledger or run any
Cargo or `mise` command.

## Paths inspected

- `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- `changes/active/oidc-production-readiness/spec.md`
- `changes/active/oidc-production-readiness/review/TASK-004-r2/{lead-direction-routing.md,TASK-004-R2-production-readiness-fixes.md,findings-validation.md}`
- `crates/shared/wyrd-client/src/{auth.rs,config.rs,saved_login.rs,client.rs,lib.rs}`
- `crates/shared/wyrd-client/src/transport/{config.rs,http.rs}`
- `crates/shared/wyrd-client/src/platform/handle.rs`
- `crates/shared/wyrd-client/tests/transport/http.rs`
- `crates/wyrd/wyrd-cli/src/auth/{login.rs,refresh.rs}`
- `crates/wyrd/wyrd-cli/src/client.rs`
- `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs`
- `crates/wyrd/wyrd-testing/src/human_login.rs`
- `crates/wyrd-spec/src/{auth/token.rs,operator_connection.rs}`
- resolved dependency source `webbrowser-1.2.4/src/{lib.rs,unix.rs,windows.rs}`

## 1. Scope of the retained form exchange

**Resolution: the invariant and maintainer reports are correct. This is
reachable task drift, not merely test exposure.**

`TokenExchange::exchange` is a public method on a public type in the public
`wyrd_client::auth` module (`auth.rs:178,261-275`; `lib.rs:12`). Its parameter is
the full public `TokenRequest` enum. That enum includes `DeviceCode` and
`RefreshToken`, so those variants reach the hand-written `grant` and
`post_form` implementation at `auth.rs:394-438` even though the same owner now
provides the `oauth2`-backed `device_access_token` and `refresh` methods at
`auth.rs:325-361`.

The in-repository production callers do not currently choose those two broad
variants. `AuthMiddleware` constructs only RFC 8693 `TokenExchange` and RFC
7523 `JwtBearer` requests before calling the generic method
(`auth.rs:949-1027`), and `platform_session` constructs its own RFC 8693 request
(`auth.rs:278-303`). The CLI journey nevertheless proves that the public path
is executable rather than dormant: it constructs `TokenRequest::DeviceCode`
and calls `exchange` for pending, wrong, and replayed codes
(`cli_login_journey.rs:283-304,341-346`). An external Rust consumer can make
the same production call without a test-only feature or private escape hatch.

TASK-012 is explicit that device and refresh move to `oauth2`, that only RFC
8693 and RFC 7523 retain the form POST, and that no hand `TokenExchange`
POST/decode remains for the replaced grants (task lines 24-37, 43-44, 74-78,
129-140). Therefore the defect is the production API's ability to produce an
invalid ownership choice, not the journey's desire to inspect one server
response. The correction boundary is to constrain the existing form-exchange
owner to the two unsupported standard grants and move the journey's one-shot
server assertions off that production bypass. No new grant abstraction,
protocol option, compatibility path, or source check is warranted. The
lead-approved RFC 7009 method at `auth.rs:364-392` is separate and remains
unchanged.

This conflict contributes no new proposed finding; it supports
`INV-012-001`/`MNT-TASK-012-1` as one shared-cause proposal.

## 2. Parsed target, canonical origin, and explicit callers

**Resolution: the durability report is correct only for the saved-login
identity boundary; `SEC-1` is correct for the wider shared-client target
boundary and is production-reachable.**

`canonical_origin` parses the URL and delegates to
`HttpsOrigin::of_url`, which rejects userinfo and returns only the normalized
origin (`saved_login.rs:108-130`; `operator_connection.rs:79-124`). Login and
logout use that result before storing, selecting, removing, or printing saved
records (`login.rs:65-80,155-163`). The saved-login requirement from prior
`FIND-TASK-004-9` is therefore closed for its durable store and summaries.

That normalization is not shared by the transport and token owners.
`HttpConfig::validate` parses only for scheme and loopback checks and returns
`Result<(), _>`; it neither rejects URL username/password nor returns the
parsed effective target (`transport/config.rs:201-241`). `TokenExchange::new`
then discards that parse, retains the original string after only trimming
trailing `/`, concatenates auth paths onto it, and exposes the raw value in
`Debug` (`auth.rs:187-194,216-252,428-433`). `HttpTransport::new` likewise
retains the raw string, prints it in `Debug`, and later appends request paths as
text (`transport/http.rs:119-165,741-770`). `ClientConfig` derives the raw
`HttpConfig` into its own `Debug` output (`config.rs:60-71`). Consequently a
value such as `https://alice:secret@wyrd.example/path?x=1#fragment` passes the
shared validation: its userinfo remains printable and its path/query/fragment
change or corrupt the endpoint strings instead of all consumers using the one
parsed origin decision.

This is reachable outside saved-login construction. `ClientConfig` returns an
explicit or ambient bearer/API-key/workload credential before calling
`canonical_origin` (`config.rs:185-203`), after which `AuthMiddleware::new`
constructs `TokenExchange` from the raw `http.base_url`
(`auth.rs:606-645`). `WyrdClient::with_config` then constructs the raw
`HttpTransport` too (`client.rs:70-91`). Direct entry points also pass raw
caller-controlled targets to `TokenExchange`: `wyrd auth refresh`
(`refresh.rs:36-45`) and `Platform::connect` (`platform/handle.rs:61-98`). The
CLI's ordinary assembled client calls `HttpConfig::validate`, but that check
has the same discard/userinfo behavior (`wyrd-cli/src/client.rs:50-63`). These
paths carry API keys, workload assertions, platform credentials, refresh
tokens, or access tokens and are not test-only.

TASK-012 owns this gap directly: Scenario 3 requires every shared caller to use
one parsed decision, userinfo to be refused, and path/query/fragment variants
to denote one origin (task lines 95-115). The root cause is the validation-only
API followed by reuse of the unnormalized input, not the saved-login store.
The standard correction boundary is the existing URL/`HttpsOrigin` owner: one
parsed, userinfo-free origin result must supply both `TokenExchange` and
`HttpTransport`, instead of adding a second parser, allowlist, setting, or
downstream redaction guard. Focused closure evidence belongs at those
constructors and should prove rejection of a userinfo sentinel and identical
root endpoint construction from case/default-port/path/query/fragment
spellings. No full journey suite or aggregate is appropriate here.

This conflict contributes no new proposed finding; it supports `SEC-1`,
refined to distinguish the closed saved-record identity from the still-open
shared effective-target boundary.

## 3. WSL and the browser-launch obligation

**Resolution: the dependency observation is factually correct, but it is not a
material TASK-012 implementation finding under the governing lead decisions.**

The resolved `webbrowser` source does invoke `cmd.exe /c start` and then
PowerShell for HTTP(S) URLs when its Unix desktop detection selects WSL
(`webbrowser-1.2.4/src/unix.rs:177-188,232-261`). Thus the categorical sentence
in `LoginFlow::run`'s rustdoc that the library uses "never a command
interpreter" (`login.rs:87-90`) is not literally true for that dependency-owned
WSL fallback.

The approved obligation routed into TASK-012 is narrower and already decided.
Prior `FIND-TASK-004-14` diagnosed Wyrd's native Windows `cmd /C start`
launcher, prescribed removal of that Wyrd-owned implementation, and routed
"Windows launcher without a command interpreter" to TASK-012 specifically via
`webbrowser` (`TASK-004-R2-production-readiness-fixes.md:148-159`;
`lead-direction-routing.md:20-22`). TASK-012 simultaneously requires exactly
`webbrowser = 1.2.4`, prohibits shell escaping and configurable browser
commands, and defines GREEN as replacing the launcher with
`webbrowser::open` (task lines 24-38, 117-128). The human's standing decision
accepts the native-Windows target check as the proof for that closure. The
candidate deletes Wyrd's per-platform launcher and calls the exact required
library (`login.rs:107-121`); the dependency's native Windows implementation
uses the Windows association API rather than `cmd.exe`.

Treating WSL as a new implementation failure would reopen that locked
dependency/platform decision. No correction remains inside the approved task:
a Wyrd WSL branch, escaping layer, feature/setting, or custom launcher would be
the expressly prohibited invented complexity; changing the exact dependency
or broadening platform policy would require new authority. Comparable projects
normally delegate browser opening to the vetted platform library in precisely
this way. Accordingly `PLAT-001` should not enter the task finding ledger. The
overbroad rustdoc wording may be deleted or narrowed while another material
remediation already touches `login.rs`, but wording alone has no behavioral
consequence and must not create a style-only round.

## New proposed findings

None. The first two conflicts consolidate existing proposals at their shared
causes. The third proposal is out of the locked task boundary.

## Overall result

**RESOLVED**

The candidate remained `29f7ae0ce8580cafc4873705b4c913e93bf7464f` at the end
of inspection.
