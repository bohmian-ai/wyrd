# TASK-004 round-2 focused follow-up review

## Immutable subject and conflicts

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Authority: approved `changes/active/oidc-production-readiness/spec.md`
  revisions 8 and 9, plus every decision and addendum in
  `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`; those authorities
  supersede conflicting TASK-004 prose.
- Exclusion: revision 10's REQ-021 wire encoding belongs to TASK-008 and was
  not inspected or raised here.

The candidate remained at the stated object throughout this pass. The
repository has no `.codegraph/` directory, so navigation used the cumulative
diff, `rg`, and direct source/caller inspection. This follow-up investigated
only three material disagreements among the round-2 discovery reports; it does
not cast an overall acceptance vote.

## Source paths inspected

- Device authorization and browser callback:
  `crates/wyrd/wyrd-auth/src/{cli_logins,login,callback,issuance}.rs`,
  `crates/wyrd/wyrd-server/src/auth/cli_login.rs`,
  `crates/wyrd/wyrd-sql/src/queries/auth/{device_authorizations,login_state,refresh_tokens}.rs`,
  the device/login-state/refresh migrations, and the focused auth and CLI
  journey tests.
- Saved-login identity and output:
  `crates/shared/wyrd-client/src/{saved_login,config,auth}.rs`,
  `crates/shared/wyrd-client/src/transport/{config,http}.rs`,
  `crates/wyrd/wyrd-cli/src/auth/login.rs`, and the existing
  `wyrd_spec::operator_connection::HttpsOrigin` pattern.
- Credential-file protection:
  `crates/shared/wyrd-client/src/{credentials_file,saved_login}.rs`, every
  read/write/renew/logout/API-key-cache caller, and their Unix tests.
- Standards and comparable behavior: [RFC 8628 sections
  3.3-3.5](https://www.rfc-editor.org/rfc/rfc8628), Keycloak's
  [device-grant design](https://github.com/keycloak/keycloak-community/blob/main/design/oauth2-device-authorization-grant.md)
  and [device endpoint](https://github.com/keycloak/keycloak/blob/main/services/src/main/java/org/keycloak/protocol/oidc/grants/device/endpoints/DeviceEndpoint.java),
  and GitHub's [documented device-token
  polling](https://docs.github.com/en/apps/creating-github-apps/writing-code-for-a-github-app/building-a-cli-with-a-github-app).
  These all keep the device authorization as the grant authority and return
  credentials through the token endpoint; none uses a second independently
  expiring login as the authority for a deleted grant.

## Conflict 1 — device approval, terminal state, and credential issuance

### Exact reachable ordering

`CliLogins::approve` reads an unexpired, undenied device id through
`pending_device_authorization` and commits that tenant transaction before it
calls `HumanConnections::begin_bound`
(`cli_logins.rs:194-218`; `device_authorizations.rs:35-42,128-141`).
`begin_bound` performs provider discovery and then opens a different tenant
transaction to insert a five-minute `auth_login_state` row containing only the
bare `device_id` (`login.rs:100-158`; `login_state.rs:40-48,219-258`). The
login-state table has a unique index on `device_id`, but no foreign key to
`auth_device_authorizations` and no live-device predicate
(`20260925000001_auth_login_state_binding.sql:23-45`).

That split admits two terminal interleavings:

1. After approval's first commit, a token poll can observe expiry and run
   `delete_device_authorization`, which deletes the device row and only login
   state already present at that statement (`cli_logins.rs:327-350`;
   `device_authorizations.rs:71-81`). `begin_bound` can then insert a new orphan
   login state for the deleted id.
2. After approval's first commit, another verification form can call
   `CliLogins::deny`; denial marks the still-live device row terminal but does
   not prevent the later `begin_bound` insert (`cli_logins.rs:228-250`;
   `device_authorizations.rs:44-50`). The callback can complete before the
   next poll processes that denial.

The callback consumes the orphan or denied binding without consulting the
device table (`callback.rs:114-215`; `login_state.rs:50-65`). In its final
tenant transaction it can provision/update the User and roles, call
`issue_human_session`, insert `auth_refresh_tokens`, seal the raw token pair,
and complete the login state (`callback.rs:260-344`; `issuance.rs:661-716`). A
later denied/expired poll deletes the device and login-state rows and returns
`access_denied` or `expired_token`; it does not revoke or delete the refresh
row. `auth_refresh_tokens` has no device id or relationship to either device
table (`20260601000001_auth.sql:183-205`;
`refresh_tokens.rs:46-53,242-274`). The durable active row therefore survives
until ordinary refresh expiry. Its raw token normally becomes unreachable
when the sealed completion is deleted, but the session authority was still
minted and durable identity/role side effects may already have committed.

The existing sequential tests do not exercise either gap. They deny only
after the login state already exists and poll immediately, or expire a device
that has no in-flight approval (`cli_logins.rs:684-783`). The CLI journey also
tests denial and expiry only as sequential terminal cases
(`cli_login_journey.rs:287-380`).

### Standard and owner assessment

The shared browser-login owner explains why the implementation issues first
and redeems a sealed completion later: ordinary browser login already uses
that callback/completion pattern. It does not authorize that ordering for the
new device grant. Revision 8 specifically replaces the custom CLI handoff with
RFC 8628, REQ-011 says the CLI polls the token endpoint for its credential,
and AC-007 says an expired or denied device code yields no Wyrd credential or
browser session. RFC 8628 describes the token endpoint validating the device
code and returning the token only after the user approved; Keycloak and
GitHub's documented flows do the same. Reusing the browser callback is valid
for provider authentication, but an independent browser-completion lifetime
cannot outlive or override the device grant.

A foreign key alone would close only deletion-before-insert. A callback-time
live-row check alone would still permit the code to expire after callback
issuance but before token polling. Extending the device expiry or redeeming a
completed token after the advertised device expiry would instead weaken the
approved RFC terminal behavior. The correction boundary is therefore the
existing device-grant authority and token-endpoint redemption, not another
lease, cleanup worker, replay protocol, or compatibility state.

### Proposed finding: FOLLOWUP-R2-001

- **Status:** confirms `DATA-TEN-R2-002` and disproves the contrary invariant,
  concurrency, and system lifecycle conclusions.
- **Classification:** INCORRECT.
- **Violated obligation:** REQ-011 and AC-007 require conventional RFC 8628
  behavior and no Wyrd credential/session for an expired, denied, or ended
  device code.
- **Observable consequence:** a terminal grant can still drive User/role
  mutation and commit an active refresh row; the CLI receives an error while
  durable session authority survives without any remaining device linkage.
- **Smallest standard correction:** keep the device authorization authoritative
  through one-use token-endpoint redemption. Provider callback completion may
  record only what that live grant needs; Wyrd access/refresh authority must be
  issued only while atomically redeeming the still-live, approved device grant.
  Reuse the current tenant transaction, verified-login state, User/role owner,
  and human-session issuer. Add no parallel grant, cleanup service, or custom
  recovery mechanism.
- **Focused proof:** pause approval after its initial device lookup; in separate
  cases deny the code and expire/poll-delete it, then resume approval and drive
  the provider callback. Both cases must produce no completion, access token,
  or refresh row. Retain a normal approval that redeems exactly once at the
  token endpoint.

**Resolution: RESOLVED.** The data/tenancy report identified a real reachable
lifecycle break. The common callback transaction is internally atomic, but it
is not atomic with, or constrained by, the device authorization whose terminal
state governs the grant.

## Conflict 2 — saved-login `canonical_origin`

`canonical_origin` parses with `reqwest::Url`, clears only query and fragment,
and serializes the remaining URL (`saved_login.rs:107-121`). Consequently
`https://user:password@wyrd.example.com/prefix/?x=1` becomes
`https://user:password@wyrd.example.com/prefix`, not the URL origin. This is
not a theoretical deserialization-only value:

- CLI `LoginArgs.server` and `LogoutArgs.server` are `Url` values and permit
  userinfo/path; `LoginFlow::new` passes the same value to `TokenExchange` and
  `canonical_origin` (`wyrd-cli/src/auth/login.rs:27-83`).
- A successful device login persists that returned string, then login, logout,
  and status print it through `SavedLoginSummary`
  (`login.rs:150-167,189-220,223-271`).
- Every Rust/Python/TypeScript saved-login consumer computes and selects on the
  same key through `ClientConfig::resolve_credential`
  (`wyrd-client/src/config.rs:161-203`). Paths therefore split one standards
  origin into different saved-login buckets, while embedded URL credentials
  enter the credential file, source identity/debug text, and CLI output.

The native URL operation already expresses the approved key:
`Url::origin().ascii_serialization()`. Wyrd already uses exactly that operation
for `HttpsOrigin`, while refusing userinfo and treating paths as outside the
origin (`wyrd-spec/src/operator_connection.rs:85-127`). Comparable clients
key host/login state by normalized host or origin, not by a URL containing
userinfo. No second identifier or compatibility store is justified.

### Proposed finding: FOLLOWUP-R2-002

- **Status:** confirms `INV-R2-001`.
- **Classification:** INCORRECT.
- **Violated obligation:** REQ-012 and TASK-004 key records by canonical server
  origin; login/status/logout must expose only safe non-secret identity data.
- **Observable consequence:** URL userinfo can be persisted and printed, and
  path spelling can make the same origin's saved credential undiscoverable or
  unremovable from another equivalent URL.
- **Smallest standard correction:** derive the saved-login key with the URL
  library's native origin serialization, following the existing `HttpsOrigin`
  pattern. Do not add an alias, migration map, or path-specific login identity.
  Broader transport-URL policy is outside this focused conflict.
- **Focused proof:** show that case/default-port/path/query/fragment variants
  of one origin produce one key and select/remove the same record, and that
  userinfo is absent from the persisted key and CLI status/login/logout output.

**Resolution: RESOLVED.** The invariant report is correct. The data/security
acceptance of saved-login origin exposure did not inspect the helper's actual
serialization or its direct CLI output consumers.

## Conflict 3 — group-writable credential directory and path race

`CredentialsFile::check_dir` requires ownership but forbids only world-write
(`0o002`), explicitly accepting an owner directory with mode `0775`
(`credentials_file.rs:182-201`). The Unix test pins that acceptance
(`saved_login.rs:562-610`). In a group containing another local account, that
account has write+execute permission on the directory and can rename or
replace `credentials.toml` despite the legitimate file's `0600` mode.

`read_text` first validates `symlink_metadata(path)` and the returned inode's
owner/type/mode, then separately reopens the path with `read_to_string`
(`credentials_file.rs:242-265`). A group member can rename the checked entry
and substitute a file or symlink between those operations; the opened inode is
not the one whose metadata passed. Every saved-login select, status, renewal,
logout, write/document read, and API-key cache operation shares this path. The
directory lock does not exclude an uncooperative group member.

The precondition is concrete but bounded: the directory must be owned by the
Wyrd user, group-writable and searchable, and the attacker must be another
member of that group. A usual user-private group has no such second member,
but the mode check cannot establish that. Under those conditions the attacker
can race injected credential/config content into a victim operation or cause
unsafe replacement/denial, contradicting the approved user-only, fail-closed
store boundary.

The ordinary minimal correction is to reject group-write as well as
world-write (`0o022`) on the configuration directory. Owner-only write with
group/other read/search still supports conventional `0755` XDG layouts while
removing the different-UID path replacer. Once that boundary holds, a process
able to perform the remaining metadata/open race is the same UID (already able
to read and replace the credential) or privileged; a new `openat`/inode-pinning
abstraction is not required for this task and would exceed comparable-project
practice.

### Proposed finding: FOLLOWUP-R2-003

- **Status:** confirms `INV-R2-002`, narrowed to the real cross-account
  precondition and the minimum conventional correction.
- **Classification:** INCORRECT.
- **Violated obligation:** the binding human direction requires the one
  `credentials.toml` to be user-only and fail closed when unsafe; the file
  owner's own module promises a directory only that user can change.
- **Observable consequence:** another member of a writable group can bypass
  the checked inode through directory-entry replacement and make Wyrd consume
  attacker-selected credential/config content.
- **Smallest standard correction:** change the existing directory permission
  boundary to reject `0o022`. Do not add a second store, lock protocol,
  deadline, ACL framework, or new path-open abstraction.
- **Focused proof:** update the Unix permission test so `0775` is refused,
  while an owner-controlled directory (including conventional non-writable
  group/other search permissions) and a private regular credential file still
  support save, renewal, and logout.

**Resolution: RESOLVED.** The invariant report found a real local trust-boundary
gap. The other reports' fail-closed conclusion relied on the file mode without
accounting for the directory entry that selects which inode is actually read.

## Verification and stability

- `git diff --check` passed for the complete base-to-candidate range.
- This pass statically inspected the complete cumulative diff, the full bodies
  and callers named above, and the recorded focused/journey tests. It did not
  rerun the long Postgres/Keycloak lanes.
- Existing tests do not schedule the device lookup/binding interleaving, cover
  URL userinfo or path-equivalent origin selection, or reject a `0775`
  credential directory; the last case is currently asserted as accepted.
- `HEAD` was rechecked after writing this report and remained
  `362878494ed80ca5c5533a4364f744bf92dd1e06`.

## Final resolution

**RESOLVED.** All three discovery conflicts are source-resolvable. The device
lifecycle finding, canonical-origin finding, and group-writable-directory
finding are each reachable, bounded defects with conventional corrections;
none requires or permits a Wyrd-specific mechanism beyond the established
standard or existing repository owners.
