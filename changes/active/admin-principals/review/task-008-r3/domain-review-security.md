# Domain review — security / identity / trust boundaries (`task-008-r3`)

Reviewer: `domain-rev-security`. Candidate `f102e50ee`. Control commits used:
`4668d8d33` (previously reviewed candidate) and `968c92641` (branch base), both
checked out in a throwaway detached worktree under the scratchpad and removed
afterwards. Working tree clean at start and at end; no source file changed.

**Overall result: FAIL** — one `REGRESSION` (a red fast-lane test on the exact
surface the remediation changed) and one `DRIFT`.

---

## 1. Reviewed boundary and how it was traced

Authentication and principal resolution, the two control planes, credential
plaintext handling, revocation timeliness, scope/RBAC gating.

- **Principal resolution.** Read `SERVICE_ACCOUNT_BY_CARD_REF_SQL`
  (`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:10-19`) and its
  binder (`:169-180`), then all three callers:
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:591`
  (`resolve_requested_subject`, the delegation path), and
  `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:155`
  (`load_service_account_subject`, fed by the stored workload binding).
  Traced the wire shape of `$3` through `CardRef`'s serde
  (`crates/wyrd-spec/src/reference.rs:14-38`) and `VersionBlock`
  (`crates/shared/wyrd-semver/src/block.rs:15`), and the durable write shape
  through `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:14-24,56-62`
  and `insert_service_account` (`service_accounts.rs:82-131`). Read every
  constraint on the table
  (`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:68-100`,
  `20260601000011_auth_relax_created_by_fk.sql:21-36`,
  `20260601000020_admin_principals.sql:85-145`) to establish what can match.
- **Credential plaintext.** Read `crates/wyrd/wyrd-cli/src/auth/login.rs` end to
  end (every path that touches the pasted callback), the `InvalidArgument`
  rendering (`crates/wyrd/wyrd-cli/src/error.rs:249-263`),
  `crates/wyrd/wyrd-cli/src/auth/issue_key.rs`, the new journey
  `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs`, and the harness
  scrubbing in `crates/wyrd/wyrd-cli/tests/principal_journey.rs:20-58` against
  the client credential chain
  (`crates/shared/wyrd-client/src/transport/credential.rs:241-275`).
- **Planes.** Re-read `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs`
  (whole file), `crates/shared/wyrd-auth-verify/src/lib.rs:409-500` and `:817`,
  `crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:18-90`.
- **Error prose.** All three `BadTokenFormat` sites named by `FIND-008-10` plus
  every remaining producer of that variant.

## 2. Authority and source coverage

Read: spec revision 7 (`Current baseline`, REQ-036/038/040/047,
INV-002/013/014/015, AC-013/014), `TASK-008-sdk-and-mcp-projection.md`,
`review/task-008-r2/{verdict,findings-validation,TASK-008-R2-*}.md`,
`architecture/wyrd-design.md` 495-507, `AGENTS.md` §2/§9/§11/§12/§16,
`architecture/agent-rules.md`. Diffs read as real unified diffs via
`rtk proxy git …` and the pre-built `r2.diff` / `closeout-cumulative.diff`.

## 3. Verification performed, and its limits

Ran (all narrow, no aggregates):

| Lane | Result |
|---|---|
| `WYRD_AUTH_E2E=1 … --test platform_admin_e2e` (subject's working wrapper) | 17/17 PASS |
| `WYRD_AUTH_E2E=1 … --test auth_e2e` | 7 PASS, 1 FAIL (`cache_ttl_path_also_flips_verdict`) |
| `WYRD_CLI_E2E=1 … -p wyrd-cli --test cli -E 'test(=auth_issue_key_journey::auth_issue_key_cli_journey) or test(=principal_journey::principal_revoke_cli_journey) or test(=…refuses_an_unprivileged_caller)'` | 3/3 PASS |
| `-p wyrd-cli --lib -E 'test(=auth::login::tests::the_refusal_does_not_echo_the_pasted_callback) or test(=cli::tests::the_shipped_command_tree_is_consistent) or …'` | 4/4 PASS |
| `-p wyrd-sql --lib -E 'test(/queries::auth::/)'` | 31 PASS, **1 FAIL** → `DS3-1` |

Every selector was confirmed with `cargo nextest list` first; no positional
filters were used.

**Limits.**
- I did not mutate the issued key inside the new journey to confirm the
  implementor's `WYRD_AUTH_401_API_KEY_INVALID` claim — a reviewer changes no
  source. I verified the equivalent property structurally instead (§5, item 4).
- I did not exercise the `jwt_bearer` containment path end to end; no journey
  seeds a workload binding whose stored `card_ref` shape differs from the
  principal's, so that caller's behavior is established by reading only.
- No MCP, TypeScript, or Python surface was run; those belong to other Wave 1
  boundaries.

### The disclosed red is genuinely pre-existing

`wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` fails identically at
`f102e50ee`, at `4668d8d33`, **and at the branch base `968c92641`**. Captured
message (`--no-capture`): the failure is at `auth_e2e.rs:51`, on
`srv.delegate(...)`, with
`503 WYRD_AUTH_503_VERIFY_UNAVAILABLE / "auth backend unavailable"`.

That is **not** a verification-cache defect. `verify`/`verify_platform` never
produce that message. It is
`impl From<DelegateError> for WyrdError`'s `DelegateError::Database(_)` arm at
`crates/wyrd/wyrd-auth/src/exchange_api_key.rs:791-794`: a swallowed `sqlx`
error on the delegation path, rendered as an infrastructure 503 with the
underlying error discarded from the response. Because it reproduces at the
branch base it is unrelated pre-existing debt and I do not raise it as a
finding — but the mapping is worth a separate ticket: the remediation text
("Retry with backoff. Do not re-authenticate") is actively wrong guidance for
whatever this actually is, and the discarded `sqlx` error makes it
undiagnosable from the wire.

## 4. Direct ruling on `card_ref @> $3`

**Safe, in-scope bug fix. It does not need an approved product or security
decision.** Evidence, in the order it matters:

1. **`$3` cannot degenerate.** `CardRef` serializes `kind`, `name` and
   `version` unconditionally (`reference.rs:18-24`); only `space` and `uid`
   carry `skip_serializing_if = "Option::is_none"` (`:29,37`). `VersionBlock` is
   a newtype over `String` (`block.rs:15`), so `version` is an exact scalar
   match, not a recursively-contained sub-object. `$3` is therefore never `{}`
   and never `null`, and containment is never vacuous.
2. **Tenant isolation is untouched.** `data_tenant_id = $1` is bound from
   `conn.data_tenant_id()`, and every caller holds a `TenantConn`, which binds
   `app.current_tenant` via `set_config(..., true)`
   (`crates/wyrd/wyrd-sql/src/tenant_conn.rs:16-23`) under `FORCE ROW LEVEL
   SECURITY` (`20260601000001_auth.sql:97-101`). No caller reaches this query
   through `OperatorPool`.
3. **The old predicate could not match a real row.** The production writer
   stores `card_ref` with `uid: Some(card_uid)` and a resolved `space`
   (`auth_projection.rs:56-62`), while every client-expressible ref omits `uid`
   — the CLI builds `space/Kind/name@version` and nothing else
   (`issue_key.rs:56-59`). Whole-document equality could never succeed for a
   registered principal. This is a defect fix, not a policy relaxation.
4. **The widened key still resolves at most one row.**
   `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts`
   (`20260601000001_auth.sql:83`) survives every later migration — 11 drops only
   `contype='f'` FKs to `auth_users`, 20 drops only `contype='c'` checks matching
   `%principal_kind%`. `name` is the Card name for every card-bound principal
   (`auth_projection.rs:70`). So a candidate carrying `kind`+`name`+`version`
   can satisfy containment for at most one active row per tenant, with or
   without `space`. The widening therefore **cannot** resolve a credential to a
   different principal, a different version, a different space's principal, or
   another tenant's row: naming `space` narrows further (containment requires
   equality on any key present), and omitting it cannot broaden past the unique
   name.
5. **`ORDER BY created_at, id LIMIT 1` is defensive, not semantic.** Given (4)
   no reachable state produces two matches, so nothing is silently chosen.

The change is privilege-relevant in the sense that it alters *which* stored rows
a lookup can reach, but it is not privilege-*granting*: the set it can reach has
cardinality ≤ 1 and is the row the caller named. The one problem is that the
rustdoc justifies the safety with the wrong invariant — see `DS3-2`.

## 5. Per-item findings on the remediation's own claims

**1. Shared resolution query** — ruled above. One finding (`DS3-2`) on its
documentation, one (`DS3-1`) on its guard test.

**2. `FIND-008-11` closure — PASS.** `login.rs` has exactly one error
construction site (`:115-123`) and `input` reaches no error, no `tracing`
call, no `Debug`/`Display`, no file, on any branch; the `Url::parse` branch
falls through to the same site. `value` is one of three fixed shape
descriptions. `WyrdCliError::InvalidArgument`'s `#[error("invalid {field}
{value:?}: {expected}")]` (`error.rs:249`) renders all three fields, so the
Display assertion covers the whole rendering. On the weakened assertion: the
load-bearing one is `!rendered.contains("super-secret-code")`, and that still
fails for a full echo, a URL-encoded echo, an echo relocated into `field` or
`expected`, and any truncation that keeps ≥ ~10 characters. Only a truncation
to fewer than ten characters slips past both assertions, which is not a
credible reintroduction. Property holds.

**3. `FIND-008-10` closure — PASS.** All three sites carry the new wording:
`crates/wyrd/wyrd-server/src/http/error.rs:202-204`,
`crates/wyrd/wyrd-auth/src/error.rs:34-36`,
`crates/wyrd-spec/src/error.rs:4167-4169`. `code`, `status`, `title` and
`remediation` are unchanged at the catalog (`wyrd-spec/src/error.rs:543-556`).
No disclosure regression: absent-vs-malformed was already distinguished by
status and code before this change (`401 WYRD_AUTH_401_UNAUTHENTICATED` /
"missing X-Wyrd-Access-Token header" at `token_extract.rs:24-29` vs
`400 WYRD_AUTH_400_BAD_TOKEN_FORMAT`), and wrong-scope is 403; naming a format
instead of a header adds no new oracle. The plane whose rejection must be
indistinguishable — platform — never routes through this mapper:
`extract_platform_token` collapses every header shape to `None`
(`platform_extractor.rs:75-82`) and `unauthenticated()` is the single rendering
(`:89-94`).

**4. New credential journey — PASS, with a structural sensitivity argument.**
`issue_key.rs:84` is the only print of the plaintext (`key_id` and `prefix` are
not the secret, and `strip_prefix("key:")` cannot match `key_id:`). The CLI
persists nothing. The second command can only succeed on the printed key:
`run_cli_with_credential` removes `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`,
`WYRD_TENANT` and `WYRD_API_KEY` before setting the one named variable
(`principal_journey.rs:39-48`), and `WYRD_CONFIG_HOME` is pointed at a
nonexistent temp dir, which defeats the `credentials.toml` file floor —
`WYRD_CONFIG_HOME` is the highest-precedence config-dir source
(`credential.rs:267-275`). So a wrong or unaccepted key exits non-zero and the
assertion fires. The journey is also genuine coverage *of the `@>` fix*: the
fixture stores a uid-bearing `card_ref` (`wyrd-testing/src/server.rs:4619-4629`
→ `provision_tenant_service_principal`) while the CLI sends a uid-less one, so
this test fails under `card_ref = $3`. The `--kind` help change altered no
validation: `pub kind: String` has no `value_parser`, and parsing still happens
in `dispatch` (`issue_key.rs:56-67`).

**5. `disable_version_flag` — PASS.** `issue_key.rs:17` matches the existing
precedent at `card.rs:96,176,194`. It suppresses only the flag
`propagate_version = true` (`cli.rs:13`) would inject; it adds no argument,
alias or subcommand and cannot change how `--token`, `--server` or any other
credential-bearing argument parses. `cli::tests::the_shipped_command_tree_is_consistent`
runs `Cli::command().debug_assert()` on the real tree and passes.

**6. Properties re-verified after the adjacent edits — all PASS.**
canonical-header binding and single indistinguishable rejection
(`platform_extractor.rs:66-94,140-143,157-166`); plane separation in both
directions — `verify_platform` refuses any non-`platform` scope marker
(`wyrd-auth-verify/src/lib.rs:447-449`) and a platform session yields no tenant
so it cannot satisfy `Caller`; one signing-key resolver shared by both planes
(`lib.rs:413-425`, the only `decoding_keys` lookup); no revocation epoch, cache
or memoization on the platform plane (`verify_platform` caches nothing and
consults no `self.revocation`); no `AuthenticatedPrincipal` on the platform
plane (`PlatformCaller` carries `AuthContext::from(PlatformPrincipal…)`); no
nullable tenant (`VerifiedToken.principal: Principal`, `Principal.tenant_id:
DataTenantId`). The remediation touched no authorization or audit code path:
`components/auth/routes.rs` is unmodified across `968c92641..f102e50ee`, and the
`/auth/issue-key` handler's `authorize_service_accounts_write` +
`record_audit` + in-transaction `service.audit(...)` + `conn.commit()` sequence
(`routes.rs:278-306`) is the same established pattern used by 14 call sites.
The four unreachable CLI error codes the prior round flagged are fully gone —
no reference to `WYRD_CLI_401_AUTH_FAILED`,
`WYRD_CLI_500_{REVOKE,ADMIN,ISSUE_KEY}_FAILED` survives outside review prose.

**7. Tree-wide `Authorization` sweep — PASS.** Redone independently across
`crates/`, `sdks/`, `docs/`, `examples/`, `scripts/` and the generated
contract, excluding build output. Every surviving hit is benign: two negative
tests asserting the SDK never writes the header
(`crates/shared/wyrd-client/tests/transport/http.rs:750-751`,
`crates/wyrd/wyrd-mcp/src/client.rs:439`), one test asserting a valid JWT in
`Authorization` is rejected
(`crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs:96-104`), an
outbound third-party webhook header and its redaction tests
(`crates/vala/vala-core/src/alert_router/webhook.rs`,
`crates/skald/skald-observer/src/redaction.rs:154`), a check script comment,
and OIDC `Authorization code`/`URL` prose. No Wyrd-inbound `Authorization`
remains in any doc, example, SDK or schema; `docs/` now teaches
`X-Wyrd-Access-Token` including the three `workflow.svx` snippets this
remediation fixed. The one stale string
("Present a valid Wyrd token in the Authorization header.") lives only in
untracked `wyrd-ui/build/` output.

**8. Disclosed red** — verified pre-existing at the branch base; see §3.

---

## 6. Material findings

### `DS3-1` — `REGRESSION`

- **Obligation violated:** `AGENTS.md` §12 ("Format, lints, and the targeted
  tests/checks for the touched surface pass"); the remediation task's own
  verification obligation for the surface it changed.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`
  (asserting against `:15`).
- **Evidence:** line 15 is now `AND card_ref @> $3`, but the guard test still
  asserts `SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")`. The
  credential-free fast lane is red:
  `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`
  → `FAIL`, `assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")`
  at `service_accounts.rs:431:9`. `-E 'test(/queries::auth::/)'` → 31 passed,
  1 failed. The same test passes at `4668d8d33`, so the remediation introduced
  the break.
- **Observable consequence:** any contributor running the narrowest lane for
  this crate gets a red suite, and — worse for this boundary — the repository
  has *no* surviving assertion pinning the match operator of the query that
  decides which principal a credential resolves to. The one test whose entire
  job is to pin that operator now pins the operator the code no longer uses, so
  a future edit back to `=` (or onward to something looser) is caught by
  nothing in the fast lane.
- **Testable correction (bounded, no decision needed):** update the guard to
  assert the operator the query actually uses and to forbid the one it must not
  regress to — `assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref @> $3"))`,
  `assert!(!SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3"))`, plus
  `assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("LIMIT 1"))` so the
  single-row guarantee is pinned too. Then the focused selector above must pass.

### `DS3-2` — `DRIFT`

- **Obligation violated:** `AGENTS.md` §16 — rustdoc on a materially modified
  item "MUST explain intent … and relevant invariants". The invariant this
  rustdoc names is not the one the code's safety depends on.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:155-165`.
- **Evidence:** the doc argues *"Two active principals sharing one Card identity
  would require two Cards with the same identity, so the oldest wins"*. That
  claim does not cover the case the widening actually creates. Because `space`
  is `skip_serializing_if = "Option::is_none"` (`reference.rs:29`), a candidate
  that omits `space` constrains only `kind`+`name`+`version`, and two Cards in
  *different* spaces have different identities while producing identical
  `kind`+`name`+`version` — exactly the shape the stated invariant says cannot
  arise. What actually guarantees a single match is a constraint the rustdoc
  never mentions: `UNIQUE (data_tenant_id, name)`
  (`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:83`), combined with
  `name` being the Card name (`queries/cards/auth_projection.rs:70`).
- **Observable consequence:** the documented rationale misdirects the next
  maintainer away from the constraint that makes a credential-resolution lookup
  unambiguous. That constraint is under independent pressure — it currently also
  prevents two versions of one Service card from both owning a principal, since
  `auth_projection`'s `ON CONFLICT` targets `(data_tenant_id, principal_kind,
  card_kind, card_uid)` and a second version would insert a second row with the
  same `name`. Anyone who relaxes `UNIQUE (data_tenant_id, name)` to fix that,
  having read this rustdoc, will believe ambiguity is impossible while
  `ORDER BY created_at, id LIMIT 1` quietly begins resolving a presented
  credential to the *oldest* of several eligible principals.
- **Testable correction (bounded, no decision needed):** state the real
  invariant in the rustdoc — that a space-less candidate is unambiguous only
  because `UNIQUE (data_tenant_id, name)` admits one active row per Card name
  per tenant — and name that constraint so a migration touching it surfaces this
  caller. A `#[test]` asserting `LIMIT 1` is present (folded into `DS3-1`'s
  correction) pins the mechanical half. *Changing the behavior* from
  "oldest wins" to "refuse an ambiguous resolution" would be a security-behavior
  decision, not a bounded fix; I am **not** asking for it, and the current
  `LIMIT 1` is correct under today's schema.

---

## 7. Result

**FAIL.** `DS3-1` is a red test in the credential-free fast lane, on the exact
query this remediation changed, and it leaves the resolution operator
unguarded. `DS3-2` is a documentation defect on the same item. Everything else
in this boundary — the `@>` semantics themselves, both closed findings, the new
journey, the clap change, plane separation, and the `Authorization` sweep —
verifies clean.
