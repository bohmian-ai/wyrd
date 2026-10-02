# TASK-004 round-2 structured Ponytail validation

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Reviewed range: the complete cumulative `base..candidate` range
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, with
  revisions 8 and 9 superseding conflicting TASK-004 text
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Prior review:
  `changes/active/oidc-production-readiness/review/TASK-004-r1/`
- Binding human direction:
  `changes/active/oidc-production-readiness/review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`,
  including every addendum

Revision 10's REQ-021 is owned by TASK-008. I did not treat JSON request
bodies, problem-json OAuth errors, or any other RFC 6749 wire-format work as a
TASK-004 defect.

The candidate remained the stated commit throughout validation. `.codegraph/`
is absent, so source tracing used the cumulative diff, `rg`, direct source and
caller inspection, the installed dependency source, and the supplied reports.
All required discovery and follow-up reports were present.

## Validation method and source coverage

I checked every proposed finding against the current candidate, not reviewer
agreement. The principal producer-to-consumer traces were:

| Boundary | Producer and owner | Callers, sibling consumers, and writers checked |
|---|---|---|
| Saved server identity | `canonical_origin`; `SavedLogin::{from_token,summary}` | `ClientConfig::resolve_credential`; `SavedLogins::{save,select,source,renew,remove}`; CLI login/logout/status; Rust, Python, and TypeScript saved-login journeys |
| Credential-file trust | `CredentialsFile::{check_dir,read_text,lock,document,replace}` | saved-login read/write/renew/remove; API-key token cache; CLI status/logout; every SDK resolver |
| Secret-bearing HTTP | `HttpConfig::validate`; `TokenExchange::new` and its one reqwest client | API-key, workload, platform, device, refresh, and revocation requests; shared client, platform handle, CLI login/logout/refresh, saved-login renewal, and test harness callers |
| Browser launch | `LoginFlow::run` -> `open_in_browser` | the default CLI device-login path and `--no-browser` journey path |
| Device grant | `CliLogins::{authorize,approve,deny,redeem}`; device SQL | `HumanConnections::begin_bound`; login-state insert/consume/complete/redeem; OIDC callback; human-session issuance; refresh-row writers; server routes and sequential tests |
| Migration evolution | the two base-registered auth migrations and new device migration | sqlx embedded migrator, checksum verifier, current login-state queries, clean-database migration tests |
| SDK documentation | shared-client, Python, and TypeScript source docs and generated declaration | actual newest-login selection; `HumanSso::{cli_login,save_login}` device-grant helper; codegen/type declaration owners |

The final `mise run test:identity:journey` rerun and `git diff --check` both
exited 0. Those results establish broad regression evidence but cannot close
the source-proven gaps below: the current tests either omit the edge or, for
the `0775` directory, assert the unsafe behavior.

## Proposed-claim validation

| Discovery claim | Disposition | Validation |
|---|---|---|
| Behavior review: no findings | **REJECTED as a complete acceptance conclusion** | Its positive path traces are credible, but it did not inspect the URL spelling, redirect, filesystem-directory, migration-upgrade, device interleaving, Windows launcher, or stale-doc paths below. This is not itself a finding. |
| `INV-R2-001`; `FOLLOWUP-R2-002` | **CONFIRMED** | `canonical_origin` removes only query, fragment, and a trailing slash. URL userinfo and path remain in the durable key and CLI summaries. The URL library's native origin operation and the existing `HttpsOrigin` pattern prove the simpler conventional boundary. |
| `INV-R2-002`; `FOLLOWUP-R2-003` | **CONFIRMED** | `check_dir` forbids only `0o002`; the test expressly accepts `0775`. A second group member can replace the pathname between `symlink_metadata` and `read_to_string`. Rejecting directory write bits `0o022` removes that different-UID replacer without a new open/inode abstraction. |
| `STD-R2-001`; `MAINT-TASK-004-R2-1` | **CONFIRMED and deduplicated** | Three credential-resolution docs still promise an ambiguity error that revision 8 removed, and the Python/TypeScript testing docs plus generated TypeScript declaration still name the deleted CLI handoff. Both are one stale-authority documentation defect. |
| System review: no findings | **REJECTED as a complete acceptance conclusion** | Its healthy and recovery traces are otherwise credible, but the device authorization is not authoritative across approval, callback issuance, and terminal polling. This is not a separate finding. |
| Concurrency/durability review: no findings | **REJECTED as a complete acceptance conclusion** | Row locking serializes token polls, but it does not close the transaction gap before `begin_bound` or prevent callback issuance after the device grant ends. This is not a separate finding. |
| `SEC-R2-001` | **CONFIRMED as prior `FIND-TASK-004-5`** | The raw, case-sensitive `strip_prefix("http://")` is not URL validation. A mixed-case HTTP scheme is standards-valid, normalized by the URL client, and bypasses the remote-cleartext refusal. The round-1 root defect therefore remains open rather than becoming a new finding. |
| `SEC-R2-002` | **CONFIRMED** | `TokenExchange` uses reqwest's default redirect policy. Its shared POST helpers carry reusable secrets in the body; 307/308 preserve the method and body, so a redirect can replay them at another origin. The repository already uses `Policy::none()` at auth trust boundaries. |
| `SEC-R2-003` | **CONFIRMED** | The Windows default path passes a server-returned URL to `cmd /C start`. `cmd.exe` treats `&`, `|`, `^`, and related bytes as command syntax; a URL query can contain those bytes. A native URL-opening API removes the interpreter boundary. |
| `DATA-TEN-R2-001` | **CONFIRMED** | Both edited files existed at the base. The embedded ledger checks checksums, and the new migration does not evolve `cli_handoff_id` into the schema current queries expect. A base deployment therefore cannot upgrade. |
| `DATA-TEN-R2-002`; `FOLLOWUP-R2-001` | **REVISED** | The race and orphan refresh authority are confirmed. The proposed consequence is narrowed: REQ-011/AC-007 require no Wyrd credential or browser session after terminal expiry/denial; they do not independently forbid all User/role bookkeeping after provider authentication. The correction must defer Wyrd session/refresh issuance until atomic token-endpoint redemption of a live approved grant. |

No proposal requires a new product, public API, compatibility, architecture,
security, concurrency, or persistent-data decision. Every retained correction
uses an existing owner and standard mechanism. Restoring the deleted custom
handoff, refresh-pending state, generation protocol, tombstone, lock deadline,
separate store, live-provider matrix, or REQ-021 work would be drift and is not
part of any correction.

## Final deduplicated finding ledger

### FIND-TASK-004-5 — REVISED — VIOLATION: remote cleartext refusal is bypassable by URL spelling

- **Discovery sources:** prior `FIND-TASK-004-5`; `SEC-R2-001`.
- **Violated obligation:** REQ-005/REQ-012 and the repository TLS boundary
  require secret-bearing client exchanges to refuse remote cleartext while
  preserving the standard loopback HTTP development exception.
- **Location:**
  `crates/shared/wyrd-client/src/transport/config.rs:201-250` and
  `crates/shared/wyrd-client/src/auth.rs:136-176`.
- **Evidence:** `HttpConfig::validate` calls `is_cleartext_remote`, which only
  recognizes the exact lowercase prefix `http://` and hand-parses the host.
  `TokenExchange::new` then builds reqwest from the unchanged value. All
  exchange, device, refresh, revoke, platform, CLI, and SDK callers route
  through this owner. Existing tests cover lowercase spellings only.
- **Observable consequence:** `HTTP://remote.example` passes validation and
  can carry API keys, workload assertions, device codes, refresh tokens, or
  revocation tokens in plaintext.
- **Decision-complete correction:** replace the raw spelling parser at the
  existing `HttpConfig` owner with the already-installed URL parser. Decide
  the scheme and loopback exception from the parsed scheme and host, reject
  malformed or unsupported targets, and make both authenticated transport and
  `TokenExchange` consume that one result. Do not add a spelling blacklist,
  auth-only parser, setting, or exception.
- **Focused closure proof:** extend the existing transport and
  `TokenExchange` tests with mixed/uppercase remote HTTP and malformed parser
  edge forms; prove they fail before a request, while parsed HTTPS and actual
  `localhost`, `127.0.0.1`, and `::1` HTTP remain accepted.

### FIND-TASK-004-8 — CONFIRMED — VIOLATION: changed SDK documentation retains revision-8-deleted behavior

- **Discovery sources:** `STD-R2-001`; `MAINT-TASK-004-R2-1`.
- **Violated obligation:** AGENTS.md section 16 and maintainer-style require
  materially changed Rust items and foreign-language declarations to describe
  their actual workflow. Revision 8 selects the newest server login when no
  tenant is supplied and replaces the custom CLI handoff with RFC 8628.
- **Location:**
  `crates/shared/wyrd-client/src/config.rs:164-183`,
  `sdks/wyrd-sdk-python/src/client.rs:36-46`,
  `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1076-1083`,
  `sdks/wyrd-sdk-python/src/testing.rs:800-808`,
  `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:587-594`, and generated
  `sdks/wyrd-sdk-ts/testing/index.d.ts:214-223`.
- **Evidence:** `SavedLogins::select` selects the newest candidate when no
  tenant is given; it has no ambiguity branch. `HumanSso::save_login` drives
  device authorization, browser approval, and device-code redemption. The
  cited docs instead promise an ambiguity refusal and a removed CLI handoff.
- **Observable consequence:** maintainers and SDK consumers are told to
  reason about two mechanisms that do not exist, while generated declarations
  publish one of those false contracts.
- **Decision-complete correction:** update only the five source documentation
  owners to state the newest-login default, selected-tenant mismatch, and RFC
  8628 device login; regenerate the TypeScript declaration through its
  existing owner. Add no compatibility vocabulary, check, file, or test.
- **Focused closure proof:** repository search finds no relevant saved-login
  ambiguity or CLI-handoff wording in current source/declarations, and the
  existing docs/codegen/TypeScript declaration lanes pass. Do not hand-edit
  the generated declaration.

### FIND-TASK-004-9 — CONFIRMED — INCORRECT: the saved-login key is not a canonical origin and can expose URL userinfo

- **Discovery sources:** `INV-R2-001`; `FOLLOWUP-R2-002`.
- **Violated obligation:** REQ-012 and the unsuperseded TASK-004 local-authority
  contract key a saved login by canonical server origin and permit only safe,
  token-free identity metadata in CLI output.
- **Location:** `crates/shared/wyrd-client/src/saved_login.rs:36-121`, with
  CLI output consumers at
  `crates/wyrd/wyrd-cli/src/auth/login.rs:96-110,156-167,189-220,268-271`
  and resolution at `crates/shared/wyrd-client/src/config.rs:193-200`.
- **Evidence:** the helper parses the URL but clears only query and fragment,
  then serializes username, password, and path. That string is persisted,
  compared by every saved-login path, included in source identity/debug text,
  and printed by login/logout/status. The existing `HttpsOrigin` owner and
  URL library already use `Url::origin().ascii_serialization()` and reject
  userinfo.
- **Observable consequence:** a URL such as
  `https://user:secret@wyrd.example/prefix` persists and prints its userinfo;
  path spelling also splits one standards origin into separate login buckets.
- **Decision-complete correction:** derive the key with the URL library's
  native origin serialization, following the existing `HttpsOrigin` pattern,
  and refuse userinfo before a saved credential is created or selected. There
  is no shipped base saved-login format to preserve, so add no alias,
  compatibility map, or second identifier.
- **Focused closure proof:** one shared-client test proves case, default-port,
  path, query, and fragment variants select/remove the same origin record and
  userinfo never enters the persisted summary; the CLI status/login/logout
  proof confirms it is absent from output.

### FIND-TASK-004-10 — CONFIRMED — INCORRECT: a group-writable configuration directory defeats the user-only credential boundary

- **Discovery sources:** `INV-R2-002`; `FOLLOWUP-R2-003`.
- **Violated obligation:** the binding human direction requires the one
  `credentials.toml` to be user-only and fail closed when unsafe. The
  credential-file owner itself promises a directory only the user can change.
- **Location:**
  `crates/shared/wyrd-client/src/credentials_file.rs:182-265` and the contrary
  test at `crates/shared/wyrd-client/src/saved_login.rs:562-610`.
- **Evidence:** `check_dir` passes only `0o002` to `check_owned`, so `0775` is
  accepted. Another member of that group can replace `credentials.toml`
  between the pathname metadata check and `read_to_string`. The cooperative
  directory lock does not constrain that account. Every saved-login operation
  and the API-key cache share this owner.
- **Observable consequence:** another local group member can cause Wyrd to
  consume attacker-selected credential/config content despite the checked
  file having been owner-owned `0600`.
- **Decision-complete correction:** change the existing directory mode floor
  to reject group- and world-write bits (`0o022`). Keep owner-controlled
  read/search permissions valid. Do not add another store, ACL framework,
  lock protocol, timeout, or inode-pinning abstraction.
- **Focused closure proof:** update the focused Unix permission test so `0775`
  is refused, while an owner-controlled non-group-writable directory and a
  private regular file still support save, selection, renewal, and logout.

### FIND-TASK-004-11 — CONFIRMED — VIOLATION: immutable registered migrations were rewritten

- **Discovery source:** `DATA-TEN-R2-001`.
- **Violated obligation:** the SQL foundation requires registered migrations
  to remain immutable, ordered, and checksum-verified so an existing base
  deployment can upgrade through later migrations.
- **Location:**
  `crates/wyrd/wyrd-sql/migrations/20260925000001_auth_login_state_binding.sql:9-45`
  and
  `crates/wyrd/wyrd-sql/migrations/20261001000002_auth_connection_test_state.sql:3-18`.
- **Evidence:** both files exist at the base. The candidate changes the first
  schema from `cli_handoff_id` to `device_id` and changes the later constraint
  to expect that rewritten name. The new
  `20261002000000_auth_device_authorizations.sql` creates only the device
  table. `wyrd-sql` embeds checksums and explicitly rejects applied checksum
  drift; without that rejection, current queries would still fail against the
  base column.
- **Observable consequence:** a deployment migrated through the base cannot
  start the candidate or serve the device flow. A clean-database green test
  does not exercise this upgrade.
- **Decision-complete correction:** restore both registered files byte-for-byte
  to their base contents. Put the handoff-to-device schema evolution in the
  new later device migration using ordinary PostgreSQL column/index/constraint
  evolution, ending with the schema current queries require. Preserve the one
  migration ledger and checksum enforcement.
- **Focused closure proof:** migrate a database through the base set, apply the
  candidate migrations, and verify both ledger/checksum acceptance and the
  final device/login-state schema. Retain the clean-database migration proof.

### FIND-TASK-004-12 — REVISED — INCORRECT: terminal device authorization can still mint renewable Wyrd authority

- **Discovery sources:** `DATA-TEN-R2-002`; `FOLLOWUP-R2-001`.
- **Violated obligation:** REQ-011 and AC-007 require the RFC 8628 device grant
  to return no Wyrd credential or browser session after expiry, denial, or
  another terminal end.
- **Location:**
  `crates/wyrd/wyrd-auth/src/cli_logins.rs:194-218,310-385`,
  `crates/wyrd/wyrd-auth/src/login.rs:100-158`,
  `crates/wyrd/wyrd-auth/src/callback.rs:260-344`,
  `crates/wyrd/wyrd-sql/src/queries/auth/device_authorizations.rs:35-81`, and
  `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:40-92,297-340`.
- **Evidence:** approval commits its live-device lookup before discovery and
  `begin_bound`, which later inserts an independent login-state row with no
  foreign key or live-device predicate. A denial or expiry poll can end and
  delete the grant in that gap, after which `begin_bound` can insert an orphan
  login state. Separately, callback completion re-bases the login-state expiry
  and calls `issue_human_session`, committing an active refresh row before the
  device-code token endpoint locks or redeems the device authorization. A
  later terminal poll deletes only device/login state, not that refresh row.
  RFC 8628 defines the token endpoint as the point that validates the device
  code and returns the token for an approved, unexpired grant.
- **Observable consequence:** the CLI receives `access_denied` or
  `expired_token`, yet callback processing can already have committed an
  active, unreachable refresh credential. The finding does not require
  suppressing all User/role bookkeeping not prohibited by AC-007; the retained
  defect is session and renewable-authority issuance.
- **Decision-complete correction:** keep the existing device authorization as
  the sole grant authority through token-endpoint redemption. The provider
  callback may record the verified login result needed by that grant, but it
  must not issue or persist Wyrd access/refresh authority for a Device
  initiation. The token endpoint must lock and revalidate the still-live,
  approved device row and atomically use the existing tenant transaction and
  human-session issuer to issue the Wyrd credential once, append its canonical
  audit, and consume the grant. Preserve ordinary browser-login issuance.
  Add no parallel grant, cleanup service, lease, retry journal, or compatibility
  route.
- **Focused closure proof:** pause approval after its initial lookup; in
  separate cases deny the code and expire/poll-delete it, then resume approval
  and complete provider authentication. Also expire an approved callback
  result before its next poll. Every terminal case returns no access/refresh
  token and leaves no `auth_refresh_tokens` row; a normal grant issues exactly
  once from the token endpoint.

### FIND-TASK-004-13 — CONFIRMED — VIOLATION: secret-bearing OAuth POSTs follow redirects across origin boundaries

- **Discovery source:** `SEC-R2-002`.
- **Violated obligation:** REQ-005, REQ-011, REQ-012, and the repository secret
  boundary require reusable credentials to reach only the configured Wyrd
  authority.
- **Location:** `crates/shared/wyrd-client/src/auth.rs:155-176,255-296`.
- **Evidence:** `TokenExchange` builds reqwest without a redirect policy;
  reqwest follows up to ten redirects by default. Its shared POST helpers put
  API keys, workload assertions, platform credentials, device codes, refresh
  tokens, and revocation tokens in request bodies. HTTP 307/308 preserve the
  POST and body. The repository already uses `reqwest::redirect::Policy::none()`
  at auth/network trust boundaries.
- **Observable consequence:** a compromised or misconfigured endpoint can
  redirect a reusable secret body to another HTTPS origin, which can reuse
  that authority or retain a revocation token.
- **Decision-complete correction:** set `Policy::none()` once on the existing
  `TokenExchange` reqwest client and surface every redirect through the normal
  non-success/error path. Add no redirect allowlist, per-route policy, setting,
  or replacement HTTP client.
- **Focused closure proof:** two local origins return 307 and 308 from the
  configured origin; representative token-exchange and revocation calls fail,
  and the second origin receives zero requests.

### FIND-TASK-004-14 — CONFIRMED — VIOLATION: Windows browser opening interprets the verification URL as shell syntax

- **Discovery source:** `SEC-R2-003`.
- **Violated obligation:** REQ-011 requires the CLI to open the server-provided
  verification URL safely. The URL is data from a network trust boundary, not
  a command string.
- **Location:** `crates/wyrd/wyrd-cli/src/auth/login.rs:96-110,281-301`.
- **Evidence:** the default Windows branch executes
  `cmd /C start "" <url>`. Microsoft documents `&`, `|`, `^`, parentheses,
  and related bytes as `cmd.exe` syntax. `AbsoluteUrl` validates an HTTP(S)
  URL but does not make query bytes safe for a command interpreter. The
  journey always uses `--no-browser`, so it cannot cover this sink.
- **Observable consequence:** a malicious or compromised configured server
  can return an otherwise valid verification URL containing command
  metacharacters and execute another command with the CLI user's privileges.
- **Decision-complete correction:** remove `cmd.exe` from the Windows path and
  pass the URL as one item to the native Windows URL-opening API (the existing
  platform shell `open` operation, such as `ShellExecuteW`). Keep the printed
  URL and failure fallback. Do not add shell escaping, a configurable browser
  command, or another launcher abstraction.
- **Focused closure proof:** Windows-target compilation plus inspection/test of
  the native launcher path proves URLs containing `&`, `|`, `^`, quotes, and
  whitespace are passed as one URL item and no command interpreter is invoked.
  Do not add a source-grep gate or a fake launcher framework solely for this
  proof.

## Prior-finding closure

| Prior finding | Round-2 disposition | Source-backed reason |
|---|---|---|
| `FIND-TASK-004-1` | **CLOSED under binding human direction** | `tenant` is a route-key selector; `ClientConfig::refuse_selector` rejects it beside self-naming bearer/API-key credentials, workload exchange uses it, and the shared/language journeys cover selection and mismatch. |
| `FIND-TASK-004-2` | **CLOSED by superseding revision 8** | Durable generation and per-request revalidation were explicitly removed; the candidate contains neither mechanism. Requiring them would be drift. |
| `FIND-TASK-004-3` | **CLOSED by superseding revision 8** | `RefreshPending`, custom lock timeout, and their proof were removed; ordinary blocking lock/reread and accepted server reuse containment remain. |
| `FIND-TASK-004-4` | **CLOSED by human direction and revision 8** | The one local store is `credentials.toml`, protected by ownership/mode with no local encryption, and pending state no longer exists. `FIND-TASK-004-10` is a new defect in the implementation of that approved boundary, not revival of the rejected sealing decision. |
| `FIND-TASK-004-5` | **OPEN — REVISED above** | The shared owner now rejects lowercase remote HTTP, but standards-valid mixed-case spellings bypass the raw prefix check. |
| `FIND-TASK-004-6` | **CLOSED** | Python public constructors, Bifrost wrappers, PyO3 owners, package declarations, and the remaining first-class surfaces accept and forward `tenant`; recorded typing/codegen/journey evidence agrees. |
| `FIND-TASK-004-7` | **CLOSED** | `CliLogins::end` appends `auth.cli_login.logout` in the refresh-chain revocation transaction, and the PostgreSQL test covers one event and audit-failure rollback. |

## Rejected or narrowed remediation

- Do not restore any revision-8-deleted handoff, pending marker, generation,
  tombstone, deadline, per-login version, extra credential file, or token
  cache. Those mechanisms are neither standard nor approved.
- Do not add an origin alias/migration map for the new saved-login format; no
  base deployment contains it.
- Do not add `openat` inode pinning after rejecting group/world-writable
  directories; the remaining same-UID/privileged actor already owns the
  credential boundary.
- Do not require device expiry/denial to undo every User or role record absent
  an approved obligation. It must prevent Wyrd session and refresh authority.
- Do not turn redirect handling into an allowlist or option; the standard
  minimum for these secret POSTs is no automatic redirect.
- Do not address revision 10 REQ-021 in TASK-004.

## Overall validation disposition

**FIX_REQUIRED**

The validated ledger contains eight retained findings:
`FIND-TASK-004-5` and `FIND-TASK-004-8` through `FIND-TASK-004-14`.
All are bounded corrections under approved authority. None requires a spec
revision, and none permits a new Wyrd-specific mechanism, setting, file,
option, compatibility path, or check.
