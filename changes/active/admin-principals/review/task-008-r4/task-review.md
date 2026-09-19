# TASK-008 — cumulative task acceptance review (`task-008-r4`)

Reviewer: `task-rev`, fresh and independent. Reviewed the complete cumulative
candidate at `c34b9d1e0` against the **original** task
(`changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`),
spec revision 7, and the two round-2 findings.

| Item | Value |
|---|---|
| Candidate HEAD | `c34b9d1e0` |
| Previously reviewed candidate | `f102e50ee` |
| This round's source commit | `708f01ec9` — one file, `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` (+42/-10) |
| Working tree | clean at start and end (`git status --porcelain` empty after `mise run fmt`, which rewrote nothing) |

## 1. Prior-finding closure

| ID | Disposition | Source evidence | Proof run |
|---|---|---|---|
| `FIND-008-8` | CLOSED (spot-check) | `crates/wyrd/wyrd-cli/src/error.rs` — no `RevokeFailed`, `HttpBuild`, `Http`, `UrlJoin` variant | `grep -nE 'RevokeFailed\|HttpBuild\|UrlJoin' …/error.rs` → none; `cargo clippy -p wyrd-sql --all-targets` clean (unrelated crate; CLI clippy carried from round 2) |
| `FIND-008-9` | CLOSED (spot-check) | `docs/src/content/docs/for-agents/workflow.svx` — no `authorization` occurrence | case-insensitive grep → none |
| `FIND-008-10` | CLOSED (spot-check) | `crates/wyrd/wyrd-server/src/http/error.rs:203` names `X-Wyrd-Access-Token` | grep at HEAD |
| `FIND-008-11` | CLOSED (spot-check) | `crates/wyrd/wyrd-cli/src/auth/login.rs:114-123` emits only `<missing code>` / `<missing state>` / `<missing code and state>`; no operator input reaches the refusal on any branch | source read at HEAD |
| `FIND-008-12` | CLOSED (spot-check) | `login.rs` `parse_callback_input` and `components/eval/routes.rs` carry intent-bearing rustdoc | source read; unchanged since `f102e50ee` (file absent from this round's diff) |
| `FIND-008-13` | CLOSED (spot-check) | `crates/wyrd/wyrd-cli/tests/cli.rs:1-2` registers `auth_issue_key_journey` in the `test:cli:journey` binary | grep at HEAD; lane itself carried from round 2 (see Verification limits) |
| `FIND-008-14` | CLOSED (spot-check) | `mise.toml` has no `dev-bootstrap` task | grep → none |
| `FIND-008-15` | **CLOSED** | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:451-455`: asserts `contains("card_ref @> $3")`, `contains("principal_kind = $2")`, `contains("ORDER BY created_at, id")`, `contains("LIMIT 1")`, `!contains("card_ref::text")`. Same test, in place — no new file, fixture, dependency, or second test; nothing deleted or `#[ignore]`d | selector confirmed with `cargo nextest list -p wyrd-sql --lib`; `cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` → **1 passed**. `mise run test:sql` → exit 0, 120 + 4 + 113 + 2 passed, 0 failed. `cargo clippy --locked -p wyrd-sql --all-targets` clean |
| `FIND-008-16` | **CLOSED WITH DEVIATION** | See §1.1 | `test:sql` green; `mise run fmt` no-op; all doc claims checked against schema and source individually |

### 1.1 `FIND-008-16` in full — every factual claim checked against source

The rustdoc now sits on `SERVICE_ACCOUNT_BY_CARD_REF_SQL`
(`service_accounts.rs:10-29`) and the function doc (`:174-178`) points at it.

| Claim in the new rustdoc | Checked against | Verdict |
|---|---|---|
| The projection writes `space: Some(..)` and `uid: Some(..)`, so `card_ref = $3` matched no registered principal | `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-63` constructs exactly that `CardRef`; `CardRef.uid` is `skip_serializing_if = "Option::is_none"` (`crates/wyrd-spec/src/reference.rs:36-37`) | TRUE |
| Containment relaxes every optional `CardRef` field, `space` included; a space-less ref matches a row in any space | `CardRef.space`/`uid` both `Option` with `skip_serializing_if` (`reference.rs:28-37`); `@>` is JSONB containment | TRUE |
| `UNIQUE (data_tenant_id, name)` on the table | `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:85`; `20260601000020_admin_principals.sql` alters nullability and the CHECK constraints only — it drops no unique constraint | TRUE |
| `auth_projection` keeps the `name` column equal to `card_ref->>'name'` | `auth_projection.rs:60` (`name: card.metadata.name.clone()` inside the ref) and `:71` (`.bind(card.metadata.name.as_str())` for the `name` column); the `ON CONFLICT` update keeps both in step | TRUE |
| GIN index on `card_ref` serves the lookup (retained in the function doc) | `20260601000001_auth.sql:92-93` `auth_service_accounts_card_ref_gin` | TRUE |
| `ORDER BY created_at, id LIMIT 1` is the deterministic guard, and the chain is not enforced at the query | const text at `:30-38`; no space or uid predicate | TRUE |
| The old "two Cards with the same identity" bound | removed (diff `-154,15 +174,10`) | GONE |
| **"every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)"** — asserted as one of the three facts that bound resolution to one intended row | Checked at all three callers: **`issue_key`** takes the ref straight from `IssueKeyRequest.card_ref` (`crates/wyrd-spec/src/auth/issue_key.rs:13-15`), whose `space` is optional on the wire and is validated nowhere — `crates/wyrd/wyrd-server/src/components/auth/routes.rs:260-300` and `crates/wyrd/wyrd-auth/src/issue_api_key.rs:86-97` never read `space`; the CLI's `--space` (`wyrd-cli/src/auth/issue_key.rs:28-30`) binds only the CLI. **`exchange_api_key`** takes it from `RequestedSubject::CardRef` (`crates/wyrd-spec/src/auth/token.rs:70-73`), also client-supplied and unvalidated (`exchange_api_key.rs:580-595`). **`jwt_bearer`** takes it from the stored workload binding (`jwt_bearer.rs:120-155` → `pg_resolvers.rs:221`), an operator-configured `CardRef` whose `space` is likewise optional | **FALSE as written** — see `TR4-1` |

Two of the three named bounds are exactly right, the removed claim is gone, and
the paragraph states the space relaxation plainly, so the finding's substance is
addressed. But the third "fact" is an enforcement guarantee no caller provides,
which is the same class of defect `FIND-008-16` was raised about — hence
CLOSED WITH DEVIATION plus one new finding.

**Criterion 3 and the doc's move onto the const.** Criterion 3 named
`SERVICE_ACCOUNT_BY_CARD_REF_SQL`, and that is where the doc now is, so the move
satisfies the criterion rather than evading it; the const is also the item the
claims are about. The function's own AGENTS.md §16 obligation still holds: it
keeps a one-line summary, an intent-bearing body, and its `# Errors` section
(`service_accounts.rs:179-180`) — the move deleted no required section.

## 2. The byte-identity claim — verified independently

I extracted the const from both revisions and compared the bytes, not the
implementor's command:

```
rtk proxy git show f102e50ee:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \
  | sed -n '/SERVICE_ACCOUNT_BY_CARD_REF_SQL: &str/,/"#;/p'   -> md5 2298c9e6b9cf3e40edb952818226b944
<HEAD file> | sed -n '…same range…'                            -> md5 2298c9e6b9cf3e40edb952818226b944
diff <f102e50ee extract> <HEAD extract>                         -> no differences
```

The claim **holds**. The predicate, its `ORDER BY created_at, id` and `LIMIT 1`
are unchanged. `rtk proxy git diff --stat f102e50ee..c34b9d1e0` shows exactly one
source file changed (`…/service_accounts.rs`, 42 changed lines) plus seven
round-2 review documents; no caller file (`issue_api_key.rs`,
`exchange_api_key.rs`, `jwt_bearer.rs`) and no write-side file
(`cards/auth_projection.rs`, any migration) appears. Criterion 4 holds for the
predicate, the `ORDER BY`/`LIMIT`, every caller, and the write side.

**Whole-delta accounting** (+42/-10, read line by line from the raw diff): 20
added lines of const rustdoc; 9 removed / 4 added lines of function rustdoc; 5
added lines of test rustdoc; 3 assertion lines added / 1 corrected. Nothing else.
The test rustdoc is required by AGENTS.md §16 for a materially modified test
function, so it is obligation, not scope creep. No added line is outside doc and
the prescribed assertions.

## 3. Acceptance matrix — original task

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No Wyrd-owned surface reads `Authorization`; tree-wide search returns nothing | `platform_extractor.rs` binds `token_extract::WYRD_ACCESS_TOKEN_HEADER` | Tree-wide sweep of `crates/`, `docs/`, `examples/`, `sdks/`, `openapi.yaml` for `header::AUTHORIZATION`, `from_static("authorization")`, `bearer_auth`, `Authorization: Bearer`. Eleven hits, each inspected: skald `google_oauth.rs:354` (outbound provider), vala `webhook.rs` (redaction), `platform_extractor.rs:247/265` + `wyrd-mcp/src/client.rs:438` + `wyrd-client/tests/transport/http.rs:750` (assert it is *not* read), `wyrd-client/tests/pg_auth_e2e_against_fixture.rs:96` (negative control proving the server ignores it), `wyrd-testing/src/oidc_fixture.rs:326` (Keycloak admin API), `pg_grpc_ingest_smoke.rs:465` (gRPC negative expecting `Unauthenticated`) | PASS |
| Platform session authenticates on `X-Wyrd-Access-Token`; tenant token on a platform route refused by scope | unchanged from `f102e50ee` (absent from this round's diff) | carried from round 2 (`platform_admin_e2e` 17/17); not re-run — see limits | PASS (carried) |
| An application's own `Authorization` carried through untouched | `platform_extractor.rs` unit tests, unchanged | carried from round 2 | PASS (carried) |
| Signing-key resolution exists once | `crates/shared/wyrd-auth-verify/src/lib.rs`, unchanged | carried from round 2 | PASS (carried) |
| No `reqwest::Client` in `crates/wyrd/wyrd-cli/src` | single construction point in `wyrd-cli/src/client.rs` over `wyrd-client` | `grep -rn 'reqwest::Client\|reqwest::ClientBuilder' crates/wyrd/wyrd-cli/src/` → none. Residual `reqwest` use is `Method` and `Body` values passed to `wyrd-client` (`auth/issue_key.rs:4`, `auth/workload_binding.rs:4`, `auth/trusted_issuer.rs:6`, `eval/agent.rs:123-125`) — types, not a client | PASS |
| Every CLI command authenticates on the canonical header, proven by a test that would have caught the defect | every command on `wyrd-client`; `principal_journey.rs`, `auth_issue_key_journey.rs` | registration verified at `wyrd-cli/tests/cli.rs:1-2`; lane result carried from round 2 (23 passed) | PASS (carried) |
| CLI receives a once-returned credential and uses it on a later call | `auth_issue_key_journey.rs` in `test:cli:journey` | carried from round 2 | PASS (carried) |
| `eval/*` on the shared client, no hand-rolled client left | `wyrd_client::eval`; `eval/agent.rs` on `request_external_stream` | no `reqwest::Client` in `wyrd-cli/src` | PASS |
| Unreachable per-command CLI error variants deleted | `wyrd-cli/src/error.rs` | grep → no `RevokeFailed`/`HttpBuild`/`Http`/`UrlJoin` | PASS |
| Generated contract declares the authentication scheme; regenerates cleanly, no hand edits | `openapi.yaml:4532-4539` — `securitySchemes.wyrdAccessToken` (`apiKey`, `in: header`, `name: X-Wyrd-Access-Token`) plus a document-level `security: [wyrdAccessToken: []]`; produced by the `SecurityAddon` generator modifier | `mise run codegen:check` → **exit 0**, tree clean afterwards | PASS |
| Tenant-scope caller cannot invoke a platform-plane operation via CLI or MCP | structural: `crates/wyrd/wyrd-server/src/mcp/` contains `bifrost.rs`, `principals.rs`, `probe.rs` — no platform tool; no platform CLI command | re-enumerated at HEAD; HTTP side carried from round 2 | PASS |
| No Python or TypeScript administrative binding; no unrun administrative journey | cumulative `sdks/` diff (`968c92641..c34b9d1e0`) is `sdks/wyrd-sdk-rust/src/lib.rs` only — a doc line and two compile-time symbol references in an existing `--lib` test; no Python or TypeScript file touched; `sdks/wyrd-sdk-rust/tests/` holds only `cards_state.rs`, so the two deleted admin journeys stay deleted; the `wyrd-client` re-export is not narrowed | `rtk proxy git diff --name-only 968c92641..c34b9d1e0 -- sdks/`; `ls sdks/wyrd-sdk-rust/tests/`; `codegen:check` (no stub drift) | PASS |
| MCP administrative writes scope-gated, reads always available | `crates/wyrd/wyrd-server/src/mcp/principals.rs:43-57` — `descriptors_unscoped()` = `list_credentials`; `write_descriptors()` = `revoke_credential`, with the operation re-authorizing and auditing behind the tool; issuance deliberately absent (`:14`) | source read at HEAD; unchanged this round | PASS |
| Platform revocation unchanged; rustdoc states the caches-nothing invariant; no epoch added | `crates/wyrd/wyrd-auth/src/platform_credentials.rs`, unchanged since `f102e50ee` | carried from round 2 | PASS (carried) |
| Credential plaintext crosses a client surface once and never reaches a log, trace, or error payload | `login.rs:114-123` refusal carries no operator input; `IssueKeyResponse.key` is `SecretBearer` | source read; focused `auth::login` tests carried from round 2 | PASS |
| No surface describes a second identity model, a removed bootstrap path, or credential-keyed authorization | `workflow.svx` clean; no `dev-bootstrap` in `mise.toml`; error-catalog remediations name the canonical header | greps at HEAD; `codegen:check` exit 0 | PASS |
| Every new or materially modified Rust item has rustdoc (§16), `# Errors` where fallible | const, function and test all documented; the function keeps `# Errors` after the doc move | `test:sql` green; `clippy -p wyrd-sql --all-targets` clean | PASS with deviation — `TR4-1` (a doc statement that is not true of the source) |
| Format, lints and the targeted tests for the touched surface pass (§12) | — | `mise run fmt` (no-op, tree stayed clean); `cargo clippy --locked -p wyrd-sql --all-targets` clean; focused test PASS; `mise run test:sql` exit 0 | PASS |
| Prohibited changes: no `AuthenticatedPrincipal` on the platform plane, no compatibility route/alias, no hand-edited generated artifact, no revocation epoch, no reinstated SDK journey | this round touched one `wyrd-sql` file; nothing else moved | diff inspection; `codegen:check` | PASS |
| Remediation non-goals: predicate not reverted or narrowed, no `space` clause, no Postgres-backed `wyrd-sql` semantics test, `wyrd-testing/src/server.rs` untouched, `UNIQUE (data_tenant_id, name)` and the `name`-column projection untouched, no attempt at the pre-existing `auth_e2e` red | const byte-identical (§2); no `space` predicate in the const; no test file added (`test:sql` count unchanged in kind); `server.rs`, migrations and `auth_projection.rs` absent from the diff; `tests/auth_e2e.rs` absent | §2 byte comparison; `git diff --name-only f102e50ee..c34b9d1e0` | PASS |
| Round-2 out-of-scope handoffs deliberately not folded in (same-name-across-spaces projection; stale `server.rs:2670-2672` comment) | both left alone | diff inspection — correct, per the non-goals; not a gap | PASS |

## 4. Proposed findings

### `TR4-1` — DRIFT — the new rustdoc names a bound no caller enforces

- **Violated obligation.** AGENTS.md §16 — "Rustdoc MUST explain intent … and
  relevant invariants or side effects"; §12 — "Documentation is part of
  implementation correctness". Same obligation `FIND-008-16` cited.
- **Location.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:21-23`
  ("… and every caller passing a fully qualified ref (`IssueKeyArgs::space` is
  required)").
- **Evidence.** The doc lists that as the third of "three facts outside it" that
  bound the query to one intended row. It is not a fact about the callers:
  `IssueKeyRequest.card_ref.space` (`crates/wyrd-spec/src/auth/issue_key.rs:13-15`,
  `reference.rs:28`) is optional on the wire and is read nowhere on the issue path
  (`components/auth/routes.rs:260-300`, `issue_api_key.rs:86-97`,
  `principal_kind_for_card` at `issue_api_key.rs:230-238`);
  `RequestedSubject::CardRef` (`auth/token.rs:70-73`) is equally unvalidated at
  `exchange_api_key.rs:580-595`; and `jwt_bearer.rs:120-155` uses whatever the
  stored workload binding holds (`pg_resolvers.rs:221`). Only the CLI's `--space`
  argument is required, and the CLI is one client of three call paths. The doc
  itself states, two sentences earlier, that a space-less ref matches any space —
  so the two statements do not agree.
- **Observable consequence.** The maintainer this doc was rewritten for reads an
  enforcement guarantee the tree does not provide, on a credential-issuing path.
  Because the clause is listed as load-bearing, it invites the same wrong
  inference `FIND-008-16` set out to remove: that the space relaxation cannot be
  reached, therefore the remaining bounds (`UNIQUE (data_tenant_id, name)`, the
  `name`-column coupling) are safe to relax. The first two named facts already
  bound the query on their own, so the clause adds no correctness argument — only
  a false one.
- **Required testable correction.** Amend that clause only; change no code and no
  test. Either delete it, or state what is actually true: `space` is optional on
  the wire (`IssueKeyRequest.card_ref` and `RequestedSubject::CardRef` accept a
  space-less ref; only the CLI's `--space` is required), so the unique constraint
  plus the `name`-column coupling are the whole bound. Proof is the same as
  `FIND-008-16`'s: `mise run fmt`, `cargo clippy --locked -p wyrd-sql
  --all-targets`, and
  `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E
  'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`
  plus `mise run test:sql`.

No other finding. Nothing was reopened, no gate was weakened, no new scope was
entered.

## 5. Verification limits

- Lanes run this round: `mise exec -- cargo nextest list -p wyrd-sql --lib`
  (selector confirmed), the focused `service_account_by_card_ref_…` expression,
  `mise run test:sql` (exit 0), `mise run codegen:check` (exit 0),
  `cargo clippy --locked -p wyrd-sql --all-targets`, `mise run fmt`. No broad
  aggregate was run or accepted: no `gate`, no `test:rust`, no whole-crate or
  family lane, no storage matrix, no `--all-features` workspace lane. No
  positional test filter was used.
- **Not re-run this round:** `WYRD_CLI_E2E=1 test:cli:journey`,
  `platform_admin_e2e`, `check:client-tier`, `check:sdk-client-tier`,
  `check:pyo3-scope`, `docs:check`, `mise run lints`, and the MCP catalog tests.
  Justification: every file those lanes cover is byte-identical to `f102e50ee`,
  which round 2 verified green against them (`rtk proxy git diff --name-only
  f102e50ee..c34b9d1e0` lists one `wyrd-sql` file plus review documents). Their
  rows above are marked "carried".
- `mise run fmt` runs `cargo fmt --all`, i.e. it writes. It rewrote nothing here;
  `git status --porcelain` was empty afterwards.
- The space-relaxation reachability in `TR4-1` was established from the wire
  types and the handler source, not by issuing a space-less request against a live
  server. Round 2 already measured the containment semantics in Postgres.
- The `@> $3` predicate itself was **not** re-litigated: round 2's ruling accepts
  it and forbids changing it. `TR4-1` is about prose only.
- Pre-existing red not attributed to this candidate:
  `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict`. Not re-run.
- The `Co-Authored-By: Claude` trailers on earlier branch commits (AGENTS.md §13)
  remain, as disclosed in the two prior verdicts. Carried forward, not a finding
  of this round; still the change owner's decision.

## 6. Result

`FAIL` — one finding, `TR4-1` (DRIFT, doc prose in
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:21-23`). `FIND-008-15`
is fully closed and `FIND-008-16` is closed with deviation; the byte-identity
claim holds; nothing was reopened and no new scope was entered. The remaining
correction is one clause of one doc comment.
