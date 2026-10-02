---
id: TASK-004-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 10
requirements: [REQ-005, REQ-011, REQ-012, AC-004, AC-007, AC-009]
depends_on: []
parent_task: TASK-004
remediates: [FIND-TASK-004-5, FIND-TASK-004-8, FIND-TASK-004-9, FIND-TASK-004-10, FIND-TASK-004-11, FIND-TASK-004-12, FIND-TASK-004-13, FIND-TASK-004-14]
---

# Close TASK-004 production-readiness gaps

## Authority and immutable review subject

- Approved spec:
  `changes/active/oidc-production-readiness/spec.md`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Review base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Reviewed candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-004-r2/findings-validation.md`

The binding human direction in the round-1 review and approved spec revisions
8 and 9 supersede conflicting original-task text. Revision 10's REQ-021 is
owned by TASK-008 and is not part of this remediation.

## Outcome

Make the existing RFC 8628 laptop login and shared saved-user credential path
safe at its actual trust and persistence boundaries: canonical server identity,
user-only local storage, parsed TLS validation, no secret redirects, safe
native browser opening, immutable migration upgrades, terminal device-grant
enforcement, and documentation that describes the shipped standard flow.

Keep the implementation conventional. Do not restore or replace the deleted
custom handoff, refresh-pending state, generation protocol, tombstone, lock
deadline, separate credential/token store, per-login format version, live
provider matrix, or any REQ-021 behavior.

## Issue diagnoses and required corrections

### FIND-TASK-004-5 — parsed target policy is still bypassable

`HttpConfig::validate` hand-parses only a lowercase `http://` prefix.
Reqwest accepts standards-valid mixed-case schemes, so a remote target such as
`HTTP://remote.example` reaches every shared exchange caller after passing the
cleartext check. API keys, workload assertions, device codes, refresh tokens,
and revocation tokens can therefore cross remote cleartext despite the TLS
boundary.

Correct the shared `HttpConfig` owner by using the already-installed URL parser
for scheme and host decisions. Reject malformed and unsupported targets and
remote HTTP; retain only parsed HTTPS and the established loopback HTTP
exception. Authenticated transport and `TokenExchange` must consume the same
decision. Do not add an auth-only parser, spelling blacklist, setting, or
exception list beyond actual loopback hosts.

### FIND-TASK-004-8 — changed documentation describes removed behavior

The shared-client, Python, and TypeScript configuration docs still promise an
ambiguity error when no tenant is selected, although revision 8 and
`SavedLogins::select` use the most recent server login. Python/TypeScript test
wrapper docs and the generated TypeScript declaration still name the deleted
CLI handoff, although `HumanSso` drives the RFC 8628 device grant.

Update the existing source documentation owners to describe the newest-login
default, selected-tenant mismatch behavior, and RFC 8628 device login. Regenerate
the TypeScript declaration through its existing generator. Do not hand-edit
generated output or add compatibility terminology or a prose-only check.

### FIND-TASK-004-9 — saved server identity is not an origin

`canonical_origin` removes query, fragment, and a trailing slash but preserves
URL userinfo and paths. The resulting value is persisted, compared by every
saved-login operation, and printed by CLI login/status/logout. This can expose
embedded URL credentials and split one deployment origin into multiple login
buckets.

Use the URL library's native origin serialization at the existing saved-login
owner, following the repository's `HttpsOrigin` pattern, and refuse userinfo
before a saved credential can be created or selected. There is no shipped base
saved-login format to migrate, so add no alias, compatibility map, or second
identifier.

### FIND-TASK-004-10 — the credential directory is writable by other users

The file itself is checked as owner-owned `0600`, but `check_dir` rejects only
world-write. A group-writable `0775` directory is accepted, allowing another
group member to replace the pathname between metadata validation and the
path-based read. The cooperative lock does not constrain that account.

At the existing credential-file owner, reject group- and world-write bits
(`0o022`) while preserving owner-controlled read/search permissions. This is
the minimum conventional directory boundary. Do not add another store, ACL
framework, lock protocol, timeout, or inode-pinning abstraction.

### FIND-TASK-004-11 — registered migrations were mutated

The candidate edits two migration files that already exist at the base,
changing `cli_handoff_id` to `device_id` and changing the later constraint.
The embedded migration ledger verifies checksums, so an existing deployment
cannot apply the candidate. Without checksum enforcement, it would retain the
old column while current queries expect the new one.

Restore both registered migration files byte-for-byte to their base contents.
Express the handoff-to-device schema evolution in the new later device
migration with ordinary PostgreSQL column/index/constraint evolution, ending
with the schema current queries require. Preserve the existing ledger and
checksum enforcement; do not add a parallel migration path or weaken
validation.

### FIND-TASK-004-12 — terminal device grants can leave renewable authority

Device approval checks the live row and commits, then performs discovery and
inserts bound login state in a later transaction without a live device-row
predicate or foreign key. Denial or expiry can delete the grant in that gap,
after which approval can insert orphan login state. The callback also issues
and commits a human refresh credential before the token endpoint locks and
redeems the still-live device authorization. A terminal poll removes the
device/login state but not that refresh row.

Keep the device authorization as the sole grant authority through token-endpoint
redemption. The provider callback may record the verified login result needed
by the grant, but for a Device initiation it must not issue or persist Wyrd
access/refresh authority. The token endpoint must lock and revalidate the live,
approved grant and atomically reuse the existing tenant transaction,
human-session issuer, canonical audit owner, and one-use consumption to issue
the credential exactly once. Preserve ordinary browser-login issuance. Do not
add a second grant, cleanup service, lease, retry journal, or compatibility
route. The correction need not undo User or role bookkeeping that AC-007 does
not prohibit; it must prevent session and renewable authority after a terminal
grant.

### FIND-TASK-004-13 — secret POST bodies follow redirects

`TokenExchange` uses reqwest's default redirect policy. Its shared POST helpers
carry reusable secrets, and HTTP 307/308 can replay the method and body at
another origin. The repository already uses reqwest's no-redirect policy at
auth/network trust boundaries.

Set `reqwest::redirect::Policy::none()` once on the existing `TokenExchange`
client and pass redirect responses through the normal non-success/error path.
Do not add a redirect allowlist, per-route option, or replacement client.

### FIND-TASK-004-14 — Windows browser launch crosses a command interpreter

The default Windows path passes the server-returned verification URL to
`cmd /C start`. Valid URL query data may contain `&`, `|`, `^`, quotes, or
whitespace that `cmd.exe` interprets as syntax, allowing a malicious configured
server to execute another command as the CLI user. The existing journey uses
`--no-browser` and does not cover this sink.

Remove `cmd.exe` from the Windows path and pass the URL as one item to the
native Windows shell URL-opening operation, such as `ShellExecuteW` with the
`open` verb. Keep the printed URL and failure fallback. Do not add shell
escaping, a configurable browser command, another launcher abstraction, or a
source-grep gate.

## Preserved behavior and non-goals

- Keep one `~/.config/wyrd/credentials.toml`, preserve unrelated TOML content,
  and retain the blocking file lock plus atomic replacement.
- Keep revision 8's ordinary refresh-under-lock behavior and its accepted
  crash-between-rotation-and-save consequence.
- Keep the most recent login as the no-selector default and the tenant route
  key as the only selector.
- Keep explicit/self-naming credential precedence and the shared Rust owner for
  Rust, Python, and TypeScript.
- Keep local-first, best-effort logout and per-login refresh-chain revocation.
- Keep transactional canonical audit and the existing server tenant/RLS
  boundaries.
- Keep ordinary browser OIDC session issuance unchanged.
- Do not implement REQ-021 or introduce any JSON/form compatibility path.
- Do not add a new product, public option, store, state machine, background
  service, compatibility layer, or repository check.

## Acceptance criteria and focused proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-004-5` | Parsed target validation rejects every remote HTTP spelling and malformed/unsupported target while accepting HTTPS and real loopback HTTP; all shared transport callers use the same result. | Extend existing transport and `TokenExchange` tests for mixed/uppercase schemes, malformed edge forms, HTTPS, `localhost`, `127.0.0.1`, and `::1`; prove refusal before any request. |
| `FIND-TASK-004-8` | Source and generated docs describe newest-login selection and RFC 8628 device login only. | Repository search plus existing docs, codegen, and TypeScript declaration lanes; generated files come only from the source owner. |
| `FIND-TASK-004-9` | Case/default-port/path/query/fragment variants resolve to one origin record, and userinfo is refused and absent from persisted/printed summaries. | Shared-client normalization/select/remove tests plus CLI output assertions. |
| `FIND-TASK-004-10` | Group- or world-writable configuration directories fail closed; owner-controlled directories with a private regular file continue to support the full saved-login lifecycle. | Update the focused Unix permission test so `0775` is refused and the safe modes still save, select, renew, and remove. |
| `FIND-TASK-004-11` | A database migrated through the immutable base ledger upgrades to the candidate schema without checksum drift; a clean database still reaches the same schema. | Base-set-then-candidate migration upgrade test plus the existing clean migration test. |
| `FIND-TASK-004-12` | Denied, expired, deleted, or already-redeemed device grants produce no access token, refresh token, browser session, or surviving refresh row; a live approved grant issues exactly once at token redemption. | Pause after approval lookup; deny or expire/poll-delete in another actor; resume provider completion and assert no authority. Also expire an approved callback result before polling and assert no refresh row. Retain normal one-use success. |
| `FIND-TASK-004-13` | 307/308 responses never replay token or revocation bodies to another origin. | Two local origins; representative exchange and revocation calls fail and the second origin observes zero requests. |
| `FIND-TASK-004-14` | The Windows path uses no command interpreter and passes metacharacter-bearing URLs as one URL item, while fallback reporting remains intact. | Windows-target compilation and the smallest native-boundary inspection/test; no fake launcher framework or grep gate. |

## Broader verification

Run every new or changed named test with its exact `mise exec --` selector and
the repository-managed setup it needs. Then run the smallest complete existing
lanes for the touched surfaces:

```text
mise run fmt
mise run lints
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:workspace-hack
mise run docs:check
mise run test:shared
mise run test:principals:integration
mise run test:cli:journey
mise run test:wyrd-sdk
mise run py:test:unit
mise run py:typecheck
mise run ts:test:unit
mise run ts:typecheck
mise run ts:napi:check
mise run test:identity:journey
```

Add Python/TypeScript integration lanes only if their runtime code or journeys
change beyond regenerated declarations. This remediation remains routed
directly to `$wyrd-implement`; it does not require another planning cycle.
