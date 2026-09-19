# TASK-008 — task acceptance review (`task-008-r3`, remediation round)

| Item | Value |
|---|---|
| Reviewer | `task-rev` (fresh, independent; implemented nothing; changed no source outside this file) |
| Candidate HEAD | `f102e50eea437ff4ba29571412197a1c4923cbbc` |
| Previously reviewed candidate | `4668d8d33` (verdict `FIX_REQUIRED`, `FIND-008-8`..`FIND-008-14`) |
| Remediation commits | `9fe02aa2e`, `67c9d2df6`, `f102e50ee` |
| Original task (acceptance standard) | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Approved spec | `changes/active/admin-principals/spec.md` revision 7 |
| Remediation task | `changes/active/admin-principals/review/task-008-r2/TASK-008-R2-close-header-prose-and-credential-gaps.md` |
| Working tree | clean at review start and end apart from this file; `HEAD` re-checked `f102e50ee` |

**Overall result: `FAIL`** — two findings, both in the one file the remediation
touched outside its prescribed corrections
(`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`). All seven prior
findings are closed.

---

## 1. Prior-finding closure

| ID | Result | Source evidence | Proof I ran |
|---|---|---|---|
| `FIND-008-8` | **CLOSED** | `crates/wyrd/wyrd-cli/src/error.rs` — `AuthFailed`, `RevokeFailed`, `AdminFailed`, `IssueKeyFailed` deleted with their `#[error]`/`#[wyrd_error]` attributes (`9fe02aa2e`, −60 lines). | `git grep -n 'AuthFailed\|RevokeFailed\|AdminFailed\|IssueKeyFailed\|WYRD_CLI_401_AUTH_FAILED\|WYRD_CLI_500_REVOKE_FAILED\|WYRD_CLI_500_ADMIN_FAILED\|WYRD_CLI_500_ISSUE_KEY_FAILED'` over `crates/ sdks/ docs/ examples/ architecture/` → no matches; `cargo clippy --locked -p wyrd-cli -p wyrd-client -p wyrd-sql --all-targets` exit 0; `mise run codegen:check` exit 0 with a clean tree. Correction boundary exact: four variants, nothing else. |
| `FIND-008-9` | **CLOSED** | `docs/src/content/docs/for-agents/workflow.svx:28,59,81` now send `X-Wyrd-Access-Token: Bearer $WYRD_ACCESS_TOKEN` (and the Python dict key). | `grep -rn 'Authorization' docs/src/content examples/` → only prose that *denies* the header (`self-hosting/local-development.svx:41`, `bifrost/reading-data.svx:162`), page titles, and internal `/concepts/authorization/` links. No `$WYRD_SERVER_URL` example remains. `mise run docs:check` exit 0. **No new docs check was added** (prescribed). |
| `FIND-008-10` | **CLOSED WITH DEVIATION** | Both live mappers and the fixture now read `"X-Wyrd-Access-Token is not a compact Wyrd JWT"`: `crates/wyrd/wyrd-server/src/http/error.rs:203`, `crates/wyrd/wyrd-auth/src/error.rs:35`, `crates/wyrd-spec/src/error.rs:4168`. Wording reused from `token_extract.rs:86`. Code, status and remediation untouched. | `grep -rni '"authorization header' crates/` → no matches. The prescribed selector claim is **true**: `cargo nextest list -p wyrd-server --lib \| grep -c 'http::error::tests'` → `0`. But the implementor's stronger claim ("no test exists for either mapper") is **false** — the module is `http::error::error_mapper_tests` and holds nine tests including `auth_error_mapping_uses_locked_codes` and `jwt_error_kind_mapping_classifies_bad_format`. I ran the correct selector: `cargo nextest run -p wyrd-server --lib -E 'test(/http::error::error_mapper_tests::/)'` → 9/9 PASS. `-p wyrd-spec --lib -E 'test(/error::tests::/)'` also passes. The deviation is in the implementor's search, not in the outcome: the property holds and is grep-enforced, which is what the prescribed criterion asked for. |
| `FIND-008-11` | **CLOSED** | `crates/wyrd/wyrd-cli/src/auth/login.rs:112-122` — `value` is now one of `"<missing code>"` / `"<missing state>"` / `"<missing code and state>"`; the raw `input` never enters the error. `InvalidArgument` retained, no redaction machinery, `AuthFailed` not reinstated. | `cargo nextest run -p wyrd-cli --lib -E 'test(/auth::login::tests::/)'` → 4/4 PASS including the new `the_refusal_does_not_echo_the_pasted_callback`. **On the substituted assertion:** the prescribed `code=` clause was genuinely impossible — the arm's own `expected` guidance is `"the full callback URL, or a \`code=<>&state=<>\` query string"`. The substitute drops to `!contains("code=super")`, but the *load-bearing* clause is unchanged: `!rendered.contains("super-secret-code")`. Any reintroduction that echoes the pasted input — whole, truncated, re-quoted, or interpolated into another field — necessarily carries the code value and trips that assertion. The property is proven. |
| `FIND-008-12` | **CLOSED** | `login.rs:78-92` (`parse_callback_input`) — intent, workflow role, the credential-echo rationale, and `# Errors`. `crates/wyrd/wyrd-server/src/components/eval/routes.rs:254-267` (`check_lease`) — role beneath principal identity, the constant-time rationale, and `# Errors` naming all five refusal conditions and the invalid-lease case. | Inspection against AGENTS.md §16; `cargo clippy --locked -p wyrd-cli … --all-targets` exit 0. Diff confirms **no untouched item gained documentation** — the only other rustdoc added in the remediation is on symbols the remediation itself changed (`IssueKeyArgs`, `service_account_by_card_ref`, the new test fns). |
| `FIND-008-13` | **CLOSED** | `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs` (new, 130 lines), registered at `crates/wyrd/wyrd-cli/tests/cli.rs:1-2`. Reuses `principal_journey`'s existing harness (`start_served`, `stop_served`, `machine_token`, `v1_status`) — no new harness. Issues the key through the shipped binary, parses the once-printed `key:` line, and spends it on a second `wyrd list` invocation whose only credential is `WYRD_API_KEY`. | `WYRD_CLI_E2E=1 mise run test:cli:journey` → **23 passed, 0 failed, 5 ignored**; log line 91 `test auth_issue_key_journey::auth_issue_key_cli_journey ... ok`, so it is not gate-skipped. Reuse is structurally forced: `run_cli_with_credential` `env_remove`s `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`, `WYRD_TENANT`, `WYRD_API_KEY` and points `WYRD_CONFIG_HOME` at an empty directory before setting the one named variable, and `principal_journey`'s sibling test proves the server enforces authorization. I did not re-run the implementor's tamper check (see §4). |
| `FIND-008-14` | **CLOSED** | `[tasks."cli:dev-bootstrap"]` deleted from `mise.toml` (`9fe02aa2e`, −4 lines). No command recreated. | `grep -rn 'dev-bootstrap\|dev bootstrap' mise.toml scripts/ docs/ architecture/` → no matches. |

---

## 2. Original-task acceptance matrix (re-verified at `f102e50ee`)

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No Wyrd-owned surface reads the `Authorization` header; a tree-wide search returns nothing | `components/auth/platform_extractor.rs:33,75` binds `token_extract::WYRD_ACCESS_TOKEN_HEADER` | `git grep -I -E 'AUTHORIZATION\|"authorization"\|Authorization: Bearer\|authorization header' -- crates sdks examples` (node_modules/.svelte-kit excluded) leaves only: skald provider **outbound** calls (`skald-providers/src/auth/{openai.rs,google_oauth.rs}`), vala webhook **redaction** (`alert_router/webhook.rs:176,568`), skald redaction (`loop_runtime.rs:893`, `redaction.rs:34`), and three tests asserting the header is *not* read (`platform_extractor.rs:247,265`; `wyrd-mcp/src/client.rs:438`) plus one gRPC negative fixture (`pg_grpc_ingest_smoke.rs:465`). `docs/` and `examples/` now clean (`FIND-008-9`). | PASS |
| Platform session authenticates on `X-Wyrd-Access-Token`; a tenant token on a platform route still refused by the scope marker | `platform_extractor.rs`; `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check untouched by the remediation | `cargo nextest run -p wyrd-server --lib -E 'test(/components::auth::platform_extractor::tests::/)'` PASS; carried forward from `task-008-r2` (`platform_admin_e2e` 17/17) — the remediation changes no platform file | PASS |
| An application's own `Authorization` is carried through untouched and never consulted | `platform_extractor.rs:243-270` — `an_applications_own_authorization_header_is_never_read`, `authorization_alone_yields_no_token` | focused nextest above | PASS |
| Signing-key resolution exists once and both internal verify paths call it; `verify_external_against` untouched | one private `TokenVerifier::signing_key` at `crates/shared/wyrd-auth-verify/src/lib.rs:413`, called only at `:444` and `:512`; `verify_external_against` (`:679`) keeps its own JWKS resolution | `grep -n 'self.signing_key('` → exactly two call sites; file unchanged by the remediation | PASS |
| No `reqwest::Client` constructed anywhere in `crates/wyrd/wyrd-cli/src` | single construction point `crates/wyrd/wyrd-cli/src/client.rs` over `wyrd-client`; `eval/agent.rs:122` uses `request_external_stream` | `grep -rn 'reqwest::Client' crates/wyrd/wyrd-cli/src/` → no matches; `mise run check:client-tier` exit 0 | PASS |
| Every CLI command that calls a Wyrd route authenticates on the canonical header, proven by a test that would have caught the original defect | `client.rs` + `card.rs`, `query/`, `principal/`, `auth/`, `eval/` all on `wyrd-client` | `WYRD_CLI_E2E=1 mise run test:cli:journey` 23 passed — `principal_journey` and now `auth_issue_key_journey` both drive the shipped binary against a real server | PASS |
| `eval/*` moved onto the shared client or removed, reason recorded; no hand-rolled client left behind | `wyrd_client::eval::{EvalProtocol, EvalRun}`; `eval/agent.rs` on the pre-existing credential-free `request_external_stream` for its third-party endpoint | `eval_server_protocol` journeys pass in the lane run above | PASS |
| Unreachable per-command CLI error variants deleted, not left in place | `FIND-008-8` closure | tree-wide grep empty; clippy exit 0 | PASS |
| Generated contract declares the authentication scheme and regenerates cleanly with no hand edits | `openapi.yaml:4532-4537` — `securitySchemes.wyrdAccessToken`, `type: apiKey`, `in: header`, `name: X-Wyrd-Access-Token`, produced by the `SecurityAddon` modifier | `mise run codegen:check` exit 0, `git status --porcelain` empty afterwards | PASS |
| CLI performs the administrative operations against a real server, **including receiving a once-returned credential and using it on a subsequent call**, through a lane that actually runs | `auth_issue_key_journey.rs` | `FIND-008-13` closure row | PASS |
| A tenant-scope caller cannot invoke a platform-plane operation through the CLI or an MCP tool | structural: `git grep -l 'platform' -- crates/wyrd/wyrd-mcp/src` → no file; `crates/wyrd/wyrd-cli/src/cli.rs:56-84` declares no platform verb | enumerated at HEAD; `principal_revoke_cli_journey_refuses_an_unprivileged_caller` (in the lane run) asserts `WYRD_PERMISSION_403_DENIED_RBAC` | PASS |
| No Python or TypeScript administrative binding; no unrun administrative journey remains | remediation touched no `sdks/`, `.py`, `.pyi`, `.ts`, or `.d.ts` file (`git diff --stat 4668d8d33..f102e50ee`) | `mise run check:sdk-client-tier`, `check:pyo3-scope` exit 0; `codegen:check` exit 0 (no stub drift) | PASS |
| MCP administrative writes scope-gated; read tools always available | unchanged from `128eb40`; no MCP file in the remediation diff | clippy exit 0; MCP catalog tests unchanged | PASS |
| Platform revocation behavior unchanged; rustdoc states the caches-nothing invariant | `crates/wyrd/wyrd-auth/src/platform_credentials.rs` — untouched by the remediation | carried forward from `task-008-r2` | PASS |
| No revocation epoch on the platform plane (**stop condition**) | no epoch, cache, or memoization in the remediation diff | diff inspection | PASS |
| Platform plane not routed through `AuthenticatedPrincipal` (**prohibited change**) | two extractors retained; no platform file changed | diff inspection | PASS |
| Indistinguishable platform rejection not weakened (**prohibited change**) | `platform_extractor.rs` unchanged | diff inspection | PASS |
| No compatibility route, alias, or transitional flag (**prohibited change**) | none | diff inspection | PASS |
| Generated stubs, schemas, `openapi.yaml` not hand-edited (**prohibited change**) | generator unchanged in the remediation; artifacts already regenerated | `codegen:check` exit 0 with a clean tree | PASS |
| No document, example, or surface still describes a second identity model, a removed bootstrap path, or credential-keyed authorization | `FIND-008-9` and `FIND-008-14` closures | greps above; `docs:check` exit 0 | PASS |
| Credential plaintext crosses a client surface exactly once, never logged/traced/in an error payload (task invariant; `INV-002`) | `login.rs:112-122`; `auth_issue_key_journey.rs` prints the key only to the captured stdout it consumes | `FIND-008-11` closure | PASS |
| Every new or materially modified Rust item has rustdoc with `# Errors` (AGENTS.md §16) | `login.rs:78`, `routes.rs:254`, `IssueKeyArgs`, `service_account_by_card_ref`, `cli::tests`, the new journey's items all documented | inspection of the full remediation diff | PASS |
| Non-goals held: no UI; no platform operation exposed to tenant-scope clients; no widening beyond the named callers | no UI file changed | diff inspection | PASS |
| Remediation-task constraint: **no new repository check, dependency, abstraction, trait, helper type, or test harness** | no new `mise` task, no `Cargo.toml` change, no new trait or helper type; `principal_journey`'s existing harness reused | `git diff --stat` — no manifest or `mise.toml` addition (only a deletion) | PASS |
| Remediation-task constraint: no gate circumvented — no `#[allow]`, `#[ignore]`, deleted or weakened test, broadened glob | no `#[allow]` or `#[ignore]` added; `principal_journey`'s two journeys and every assertion retained verbatim (`run_cli` → `run_cli_with_credential` renames the function and *adds* `env_remove("WYRD_ACCESS_TOKEN")`, strengthening the scrub; `run_cli_async` retained as a shim) | diff inspection; `4668d8d33` vs HEAD test-function lists identical | PASS |
| Remediation-task non-goals (`DC-1`, `DC-2`, `DC-4`, `DC-5`, `RR-2`, `RR-6`, `RR-7`, `RR-8`, `DC-7`, `test:platform:journey` repair) | none entered: no error bodies or routes added to `openapi.yaml`, revoke-body disagreement untouched, no new CLI/MCP credential-creation command, gating convention unchanged, `pub(crate)` const unchanged, no `sdks/wyrd-sdk-rust` rustdoc, `test:platform:journey` left broken | diff inspection | PASS |
| Verification scope `VER-001`..`VER-006` | — | `fmt`, `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `codegen:check`, `docs:check` all exit 0; `cargo clippy -p wyrd-cli -p wyrd-client -p wyrd-sql --all-targets` exit 0; focused nextest selectors pass; `test:cli:journey` 23 passed | **FAIL** — `TR3-1`: `cargo nextest run -p wyrd-sql --lib` fails |
| Unreached scope: **the shared authentication query changed outside every prescribed correction** | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15` | see `TR3-1`, `TR3-2` | **FAIL** |

---

## 3. Proposed findings

### `TR3-1` — `REGRESSION` — the remediation leaves a failing unit test in the tree

- **Violated obligation.** AGENTS.md §12 ("Format, lints, and the targeted
  tests/checks for the touched surface pass"); the remediation task's constraint
  "Do not circumvent a gate: no … deleted or weakened test"; `VER-001`.
- **Location.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`,
  asserting against the constant at `:15`.
- **Evidence.** `67c9d2df6` changed `SERVICE_ACCOUNT_BY_CARD_REF_SQL` from
  `card_ref = $3` to `card_ref @> $3` and did not touch the pre-existing test
  `service_account_by_card_ref_uses_jsonb_card_ref_binding`, whose line 431 still
  reads `assert!(SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3"));`.
  Reproduced:

  ```
  mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
    -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
  → FAILED. 0 passed; 1 failed
    panicked at crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431:9:
    assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")
  ```

  It passed at `4668d8d33` (`git show 4668d8d33:…/service_accounts.rs` carries
  both the old predicate and the same assertion). The implementor's recorded
  "Commands run" list contains no `-p wyrd-sql` invocation and no `test:sql`
  lane, which is why the red was not seen; `mise run lints` cannot see it.
- **Observable consequence.** `mise run test:sql` and every aggregate that
  includes `wyrd-sql` are red on the candidate. The test is also the only
  artifact that records what this query's predicate is contracted to be, so the
  tree currently states one contract and executes another.
- **Required testable correction.** Update the assertion to express the
  predicate the query now uses — the containment operator and the
  `ORDER BY created_at, id` / `LIMIT 1` determinism guard — so the test fails if
  either is dropped. Closure proof: the focused selector above passes, and
  `mise exec -- cargo nextest run --locked -p wyrd-sql --lib
  -E 'test(/queries::auth::service_accounts::tests::/)'` is green.

### `TR3-2` — `INCORRECT` — containment resolves a principal the caller never named when `space` is omitted

- **Violated obligation.** TASK-008's owner boundary and `REQ-036`/`INV-014`
  (one identity model, resolved from the identity a client can express);
  AGENTS.md §11 (a new user-observable behavior on a durable seam ships with a
  test); AGENTS.md §15 ("stop at the first correct option … reuse the
  repository's current owner or pattern").
- **Location.** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15`
  (`AND card_ref @> $3`), reached from
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:591`, and
  `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:155`.
- **Evidence.** `CardRef` declares both `space` and `uid` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`
  (`crates/wyrd-spec/src/reference.rs:28,36`), and carries no
  `deny_unknown_fields`/required-space validation. `Json(card_ref)` therefore
  binds `{"kind":…,"name":…,"version":…}` — with no `space` key — whenever the
  caller omits it, and JSONB `@>` is subset containment, so that document is
  contained by a stored `card_ref` in **any** space. The two caller-facing
  entry points both accept a caller-authored `CardRef` off the wire:
  `IssueKeyRequest.card_ref` (`crates/wyrd-spec/src/auth/issue_key.rs:15`, route
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:260`) and
  `RequestedSubject::CardRef` on the `/auth/token` delegation exchange
  (`crates/wyrd-spec/src/auth/token.rs:70`). Neither handler checks `space`
  before the lookup. The CLI is safe only incidentally — `issue_key.rs:56-59`
  builds the ref through `CardRef::from_str`, which always sets
  `space: Some(_)` (`reference.rs:515-537`) — so the exposure is the HTTP surface
  and the first-class SDKs that construct `CardRef` directly.
  The table already carries the typed identity the caller *can* express as
  `NOT NULL` columns — `card_kind`, `space`, `name`, `version`
  (`crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:72-77`) — which is
  the repository's existing owner for this lookup and is fail-closed on a
  missing space. Nothing tests either behavior: no unit, integration, or journey
  test exercises `service_account_by_card_ref`'s resolution semantics (the only
  test was the SQL-string assertion of `TR3-1`), and
  `auth_issue_key_journey.rs` exercises exactly one principal in one space.
- **Observable consequence.** `POST /auth/issue-key` with
  `{"card_ref":{"kind":"Service","name":"billing","version":"1.0.0"}}` mints a
  once-returned API key bound to whichever active `Service` principal named
  `billing@1.0.0` sorts oldest by `(created_at, id)` — in a space the caller
  never named — and the response's `card_ref` echo does not disclose the
  substitution. The same omission on the delegation exchange yields a delegated
  access token for a subject the caller did not name. Before `67c9d2df6` both
  requests refused (`ServiceAccountNotFound` / `SubjectNotFound`). This is a
  fail-open widening on the credential-issuance and delegation paths.
- **Required testable correction.** Resolve on the identity a client can
  express, exactly: replace the containment predicate with the table's typed
  columns (`card_kind = $`, `space = $`, `name = $`, `version = $`), so a
  `CardRef` with `space: None` binds NULL and matches nothing; or, if the JSONB
  predicate is kept, refuse a `card_ref` carrying no `space` at the request
  boundary with the stable validation error. Either way the correction ships a
  test that a space-less `CardRef` does not resolve a principal in another
  space, and one that two same-identity principals in different spaces each
  resolve only from their own space — the integration tier
  (`crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs` already owns the fixture
  shape) is the cheapest home. The `ORDER BY created_at, id LIMIT 1` guard may
  stay; with an exact predicate it is inert.

**Ruling on the implementor's escalation question** — *does relaxing principal
resolution from exact match to containment need an approved decision rather than
a bug fix?*

The **outcome** needs no spec revision, and the escalation is correctly
declined. The stored `card_ref` carries a server-resolved `uid` that no client
can name (the fixture at `crates/wyrd/wyrd-testing/src/server.rs:4619-4629`
invents one; the production shape is the same), while `CardRef`'s string form —
the only form a CLI, MCP, or HTTP caller can spell — is
`space/Kind/name@version`. Whole-document equality against a uid-bearing
document was therefore unsatisfiable from any client, which made the FIND-008-13
journey unprovable rather than merely unproven. The durable key
`(card_kind, card_uid)` and its uniqueness constraint are unchanged, no principal
becomes reachable that was not intended to be, and no product, compatibility,
cross-service, or persistent-data decision is at stake. `SPEC_REVISION_REQUIRED`
does **not** apply, and changing the query at its shared owner rather than at one
caller is the right root-cause repair — the implementor was right on both counts.

What is wrong is the **mechanism**, not the intent: containment relaxes more than
uid-insensitivity, because the two other optional fields ride the same
`skip_serializing_if`. That is a bounded implementation defect with a smaller,
already-present alternative, so it is `INCORRECT` rather than `DRIFT` — the edit
is in scope, its shape is not.

### Judged and **not** reported as findings

- **`cli.rs` — `disable_version_flag = true` plus
  `cli::tests::the_shipped_command_tree_is_consistent`.** The defect was real
  and blocking: built at `4668d8d33`,
  `cargo run -p wyrd-cli -- auth issue-key --help` aborts with
  `clap_builder-4.6.0/src/builder/debug_asserts.rs:99` —
  *"Command issue-key: Argument names must be unique, but 'version' is in use by
  more than one argument or group (call `cmd.disable_version_flag(true)`…)"*.
  The shipped command was unusable, so this is a necessary precondition of
  `FIND-008-13`'s journey, and the fix is the mechanism clap itself names and
  `card.rs` already uses. On AGENTS.md §12: I tested whether the new permanent
  test duplicates existing coverage by running the three
  `auth::issue_key::tests::*` (which do call the real `Cli::try_parse_from`) at
  `4668d8d33` — **all three PASS** against the panicking binary. So no existing
  check catches this class; the new three-line test is the only thing that does,
  it protects a property the compiler cannot, and it earns its cost. Its stated
  rationale ("the command's own unit tests build a bare `Parser` wrapper") is
  inaccurate — they use `Cli`, clap just does not assert the subtree on that
  path — but the conclusion it supports is empirically correct.
- **`principal_journey.rs` — `run_cli` → `run_cli_with_credential`.** Both
  journeys and every assertion survive verbatim; the generalization *adds*
  `env_remove("WYRD_ACCESS_TOKEN")`, so the scrub is stronger, not weaker. The
  "deleted sync wrapper" is the renamed function itself; `run_cli_async` remains
  as a thin shim. Nothing proved before is unproven now.
- **`issue_key.rs` `--kind` help text.** The old `e.g. service, agent` named
  values `parse_card_kind` rejects — it matches `CardKind::wire_name()` exactly
  (`crates/wyrd-spec/src/reference.rs:543-548`) — and the journey passes
  `wire_name()`. A one-line correction to the help of a flag the remediation's
  own journey exercises; in scope, no obligation violated.
- **`auth_e2e::cache_ttl_path_also_flips_verdict`.** Verified pre-existing: see
  §4. Not a `REGRESSION`.
- **`Co-Authored-By: Claude` trailers** on the pre-remediation commits. Already
  disclosed by the `task-008-r2` verdict as a policy item outside this task's
  write set; the three remediation commits carry none, and history was correctly
  not rewritten. Not re-raised.

---

## 4. Verification limits

- **The disclosed pre-existing failure is confirmed pre-existing.** I built a
  detached worktree at `4668d8d33` under a separate `CARGO_TARGET_DIR` in the
  scratchpad (the reviewed tree was never modified; `git status --porcelain`
  empty throughout) and ran
  `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- cargo nextest run
  --locked -p wyrd-server --test auth_e2e -E "test(=cache_ttl_path_also_flips_verdict)"
  --no-capture`. It fails there with the identical cause:
  `delegate: Http { status: 503, code: "WYRD_AUTH_503_VERIFY_UNAVAILABLE", … }`
  at `crates/wyrd/wyrd-server/tests/auth_e2e.rs:51`. The same test fails at
  `f102e50ee` (captured run reports it as `SIGABRT`, the abort behind the same
  panic). The failure therefore **predates the remediation** and is not a
  `REGRESSION`. One correction to the record: the implementor's evidence says
  they reproduced it at `HEAD~1`, but `HEAD~1` is `67c9d2df6`, the commit that
  *contains* the containment change; the change-free commit is `HEAD~2`
  (`9fe02aa2e`). The conclusion survives the imprecision — I reproduced it two
  commits earlier still. Note also that this test's `srv.delegate(…, callee.card_ref())`
  path does run through `service_account_by_card_ref`, so the coincidence was
  worth eliminating rather than assuming.
- The scratchpad worktree and its target directory are removed at the end of
  this review; they never touched the reviewed tree's index, stash, or `target/`.
- I did **not** re-run the implementor's `FIND-008-13` sensitivity check
  (tampering the captured key to confirm the reuse assertion fails), because it
  requires editing the tree, which this review may not do. Reuse is instead
  argued structurally from the harness's credential scrubbing plus the lane's
  own authorization-enforcement evidence.
- No broad aggregate was accepted or run as evidence: no `mise run gate`, no
  `test:rust`, no `test:sql`, no whole-crate or family lane, no storage matrix,
  no `--all-features` workspace lane. Every named selector was confirmed with
  `mise exec -- cargo nextest list` before use, and no positional filter was
  used.
- `TR3-2`'s consequence is reasoned from the serde attributes, the two wire
  shapes, and the JSONB containment semantics; I did not construct the
  cross-space HTTP request against a live server, because doing so requires
  adding a test to the tree.
- `mise run lints` (workspace-wide) was not run; I ran the narrower
  `cargo clippy --locked -p wyrd-cli -p wyrd-client -p wyrd-sql --all-targets`
  (exit 0) plus `mise run fmt`. Platform-plane rows unchanged by the remediation
  are carried forward from `task-008-r2`'s verified evidence rather than re-run.

---

## 5. Result

`FAIL` — every prior finding (`FIND-008-8`..`FIND-008-14`) is closed, but the
remediation introduced two defects in
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`: a failing unit test
(`TR3-1`, `REGRESSION`) and a fail-open widening of principal resolution on the
credential-issuance and delegation paths (`TR3-2`, `INCORRECT`). Both are bounded
implementation defects with decision-complete corrections inside the approved
behavior; neither needs a specification revision.
