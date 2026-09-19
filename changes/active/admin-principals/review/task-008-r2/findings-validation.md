# TASK-008 Wave 2 findings validation — `task-008-r2`

| Item | Value |
|---|---|
| Reviewer | `ponytail-rev` (Wave 2 validation; independent; changed no source) |
| Task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Spec | `changes/active/admin-principals/spec.md`, revision 7 (approved) |
| Candidate HEAD | `4668d8d333ad043b4b2f8c258beb78eebb719466` |
| Closeout range | `289978fcc~1..4668d8d33` |
| Wave 1 inputs | `task-review.md` (TR-1..4), `standards-review.md` (RR-1..8), `domain-review-security.md` (DS-1..3), `domain-review-contract.md` (DC-1..7) |
| Working tree | clean at review start and end |

**Overall: the validated ledger is NON-EMPTY — 7 retained findings
(`FIND-008-8` .. `FIND-008-14`). `SPEC_REVISION_REQUIRED` does not apply.**

Of 22 proposed findings, 11 distinct defects collapse to 7 after dedupe, and 8
are rejected outright.

---

## 1. Per-proposed-finding validation

| Wave 1 ID(s) | My independent evidence | Verdict | Reason |
|---|---|---|---|
| `TR-1`, `RR-1`, `DC-3` | `grep -rn 'AuthFailed\|RevokeFailed\|AdminFailed\|IssueKeyFailed' crates/ sdks/ docs/ examples/` returns **only** `crates/wyrd/wyrd-cli/src/error.rs:227,242,257,272`. No constructor, no test, no generated artifact. | **CONFIRMED** (merged → `FIND-008-8`) | Exact set resolved: **all four**, not three. `RR-1` omitted `RevokeFailed` (the one the task names by name); `TR-1`/`DC-3` are right. Falsifies a stated acceptance criterion. |
| `TR-3`, `RR-3`, `DS-1` | `docs/src/content/docs/for-agents/workflow.svx:28,59,81` are the only `Authorization` instances under `docs/src/content/docs/`; the file contains zero `x-wyrd-access-token`. Sibling pages (`self-hosting/local-development.svx:41`, `concepts/authentication.svx:12`) state the correct rule. The routes take `Caller`, which reads `WYRD_ACCESS_TOKEN_HEADER` only (`token_extract.rs:16-17,22`). | **REVISED** (merged → `FIND-008-9`) | Defect confirmed; one owner, three lines. `DS-1`'s extra half — *add a new docs gate* — is **cut**: AGENTS.md §12 "Adding And Retiring Checks" makes a new permanent check pay a cost it has not earned here, and the three-line edit plus `docs:check` is the smallest credible proof. |
| `TR-4`, `RR-4`, `DS-2` | Two **live** mappers both emit `"authorization header malformed"`: `crates/wyrd/wyrd-server/src/http/error.rs:202` and `crates/wyrd/wyrd-auth/src/error.rs:35`; the `wyrd-spec` fixture at `crates/wyrd-spec/src/error.rs:4168` pins it. Both mappers have real callers (`http/error.rs:35`; `wyrd-auth/src/{callback,exchange_api_key,jwt_bearer,platform_login}.rs`). `AuthError::BadTokenFormat` is raised at `wyrd-auth-verify/src/lib.rs:472,599,606,609,685`. The same payload's `remediation` (rewritten by `21abea8c7`, `wyrd-spec/src/error.rs:543-548`) names `X-Wyrd-Access-Token`. | **REVISED** (merged → `FIND-008-10`) | Defect confirmed and reachable. `TR-4` cited only one of the two live sites; the correction must land at **both** plus the fixture, or the second copy keeps serving the wrong header. |
| `DS-3` | `crates/wyrd/wyrd-cli/src/auth/login.rs:99-103` — `value: input.to_owned()`. Rendered by `#[error("invalid {field} {value:?}: {expected}")]` (`error.rs:309`), printed twice on stderr by `print_cli_error` (`eval/output.rs:13-25`), reached from `lib.rs:61-66`. Pre-change (`289978fcc~1`) the branch returned `AuthFailed { status: 0, detail: "paste the full callback URL or …" }` — **no echoed value**. Input is the operator's pasted callback (`login.rs:44-49`). | **CONFIRMED** (`FIND-008-11`) | Introduced by this closeout. Violates the task invariant on credential plaintext and spec `INV-002` ("never … logged, traced, placed in an error … payload"). Reachable on any paste carrying `code` without `state`. |
| `RR-5` | Both cited items were materially modified by this diff, verified in `rtk proxy git diff 289978fcc~1..4668d8d33`: `parse_callback_input` (`login.rs:78`) had its error variant and payload replaced (`AuthFailed` → `InvalidArgument`) and carries **no rustdoc at all**; `check_lease` (`eval/routes.rs:255`) had the header it reads changed (`header::AUTHORIZATION` → `EVAL_LEASE_HEADER`), i.e. its failure condition, and carries one line of doc and no `# Errors`. Every other function touched in the same diff did receive `# Errors`. | **CONFIRMED** (`FIND-008-12`) | AGENTS.md §16 states this as a hard blocker; `architecture/agent-rules.md` as `BLOCK_BEFORE_MERGE`. Not "merely nearby" — the diff changed exactly the error behaviour these two fail to document. |
| `TR-2`, `DC-5` | `crates/wyrd/wyrd-cli/tests/cli.rs` registers six modules, none for `auth`. `grep -rn 'issue-key\|issue_key\|"auth"' crates/wyrd/wyrd-cli/tests/` → nothing. `principal_journey.rs` obtains every credential from the in-process harness (`server.bootstrap_service` + `exchange_api_key`, `:145-151`), never through the CLI. `issue_key.rs:87-160` is clap-parsing tests only. The cited "pre-existing `auth` journeys" do not exist. | **CONFIRMED / `DC-5` REVISED** (merged → `FIND-008-13`) | `TR-2` is the true gap. `DC-5`'s framing is **factually wrong**: `wyrd auth issue-key` (`auth/issue_key.rs:49-84`) *is* a CLI credential-creation command that prints plaintext once and already runs on `wyrd-client`. So the criterion is dischargeable with the surface that exists, and `DC-5`'s remedy — four new CLI commands, or escalation to spec authority — is rejected as unearned public-surface widening (a task non-goal) and as unnecessary. **No `SPEC_REVISION_REQUIRED`.** |
| `DC-6` | `mise.toml:1081-1083` runs `cargo run -p wyrd-cli -- dev bootstrap`. `cli.rs:55-84` declares no `Dev` variant; `lib.rs:104-107` pins `removed_dev_bootstrap_returns_usage_error` → exit 64. The task was added on this branch by `b36ad6e12`, and `rtk proxy git show b36ad6e12:…/cli.rs \| grep Dev` is empty — it was **born broken**. It is the only reference in the tree (`docs/`, `architecture/`, `scripts/` have none). | **REVISED** (`FIND-008-14`) | Defect confirmed; `mise.toml` is in TASK-008's declared write set and the acceptance criterion names "a removed bootstrap path" explicitly. `DC-6`'s alternative ("point it at the command that actually seeds") is cut in favour of the ladder's first rung: **delete it** — nothing references it and `cli:init` plus tenant provisioning is the `REQ-038` replacement. |
| `RR-2` | The two functions were **never one owner with one contract**. At `289978fcc~1`, `token_extract::bad_token_format` already read `"X-Wyrd-Access-Token is not a compact Wyrd JWT"` with `details: {"header": …}`, while `routes.rs::bad_subject_token_format` read `"subject_token is not a compact JWT"` with `details: {}`. `routes.rs:333` operates on a **request-body field** (`subject_token`) of the preview-gated `/auth/token` exchange, not a header credential; the caller supplies its own token, so no third party's token kind is disclosed. `domain-review-security` boundary 2 enumerated and rejected this independently. | **REJECTED** | Not a duplicate of one shared owner, not a regression (the messages and details already differed), no disclosure of a third party's credential, and no acceptance criterion of this task names `/auth/token`'s body-field error shape. |
| `RR-6` | Measured both ways. `env -u WYRD_CLI_E2E mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/principal_journey::/) + test(/card_lifecycle::pg_tests::/)'` → **6 passed in 0.016s**, and the four pre-existing `card_lifecycle::pg_tests::*` journeys report the *identical* hollow PASS as the two new tests (`card_lifecycle.rs:765,1393,1502,1909` all use the same `if WYRD_CLI_E2E != Ok("1") { return; }`). `scripts/run-family-tests.sh:25` is a bare `cargo nextest run --locked -p …` with **no `--skip pg_tests`**, so the `mod pg_tests` wrapper the finding proposes changes nothing about the behaviour it complains of. | **REJECTED** | The stated consequence is the repository's own established pattern in the same test binary, and the proposed correction demonstrably does not fix it. The residue is a naming convention (`agent-rules.md:16`) whose stated purpose — "the family lanes run `--skip pg_tests`" — is not how the lane actually runs, so the property is unreachable from this tree. Unrelated pre-existing convention debt, out of scope. |
| `RR-7` | `grep -n '\[`' openapi.yaml` returns **15 lines / 13 other sites** of Rust intra-doc links in the published contract, including Bifrost (`:1665,1676,1712,1853,2419,2432,2624,2637-2638,2808-2809,2963-2964,3046,4107`), all predating this diff. The claim "the first and only such leak" is false. | **REJECTED** | Matches an established, widespread repository pattern (AGENTS.md rule: read the surrounding code before flagging). The task's only openapi acceptance criterion is the authentication scheme. Correcting the pattern is a document-wide decision this task did not take on. |
| `RR-8` | `crates/wyrd/wyrd-server/src/components/eval/routes.rs:252` — `pub(crate)`, used once at `:259`, both in `routes.rs`. No behaviour effect; the reporter itself calls it "not a behaviour defect." | **REJECTED** | Taste-adjacent visibility width with zero observable consequence, required by no obligation. The subject file bans optional improvements and preferences. |
| `DC-1` | Fact confirmed: `sed -n '1039,1210p' openapi.yaml \| grep -c problem+json` → 0. But `problem+json` appears in the document **only** for Bifrost and Query paths (`:352`..`:1454`); **Cards (`:501-1038`), Principals, Platform and the `/auth/platform/*` routes all lack it equally**, and the Cards paths predate this change entirely. | **REJECTED** | Not specific to the administrative half — it is a document-wide pre-existing shape where Bifrost is the outlier. The task's acceptance criteria require only that the contract "declares the authentication scheme and regenerates cleanly"; item 4 is scoped in the task to *one* `securitySchemes` entry attached to every authenticated path. Publishing typed refusal bodies for administrative paths alone would make the document **more** inconsistent, and doing it document-wide is a public-contract change outside this task's write set. `AC-014`'s error-code clause is a change-level obligation — see §5. |
| `DC-2` | `crates/wyrd/wyrd-server/src/http/openapi.rs:79-114` publishes **every** administrative path this specification defines: `create_service_principal`, `issue_credential`, `list_credentials`, `revoke_credential`, `revoke_principal`, and the entire platform plane (token, tenants, recovery, OIDC connection, admins, login). The routes named as missing — `/auth/token`, `/auth/login`, `/auth/callback`, `/auth/issue-key`, `/v1/admin/trusted-issuers`, `/v1/admin/workload-bindings` — are pre-existing tenant auth/admin routes that predate this spec, appear nowhere in `spec.md`, and were never published. | **REJECTED** | The claimed "asymmetry" is between this spec's surface (published) and untouched pre-existing routes (never published). Annotating and publishing six new routes is a public-contract addition, not "declaring the scheme," and the task's non-goals forbid "widening beyond the callers named in the write set." Out of scope. |
| `DC-4` | Three-way mismatch confirmed: `openapi.yaml:1174-1206` declares no `requestBody`; `crates/wyrd/wyrd-server/src/auth/revoke.rs:41-45` takes `(State, Caller, Path)` with no `Json` extractor; `crates/shared/wyrd-client/src/principals/handle.rs:133-145` requires and sends `&RevokePrincipalRequest`; `crates/wyrd/wyrd-cli/src/principal/revoke.rs:31-37` makes `--kind`/`--reason` mandatory. **But** `rtk proxy git show 968c92641:…/principal/revoke.rs` shows `--kind` and `--reason` mandatory and the body already sent-and-ignored **before this branch existed**; the closeout changed only the transport. | **REJECTED** | Pre-existing debt the change neither introduced nor widened. Resolving it either removes required CLI flags (user-visible argument-surface change) or makes the server require and audit the body (public HTTP contract change) — a decision the task did not take on, and the implementor's deferral is therefore sound. (Its *stated premise* — "the body that the contract declares" — is inaccurate; that is a packet-text inaccuracy, noted in §5, not a finding against the implementation.) |
| `DC-7` | `sdks/wyrd-sdk-rust/src/lib.rs:4` is `pub use wyrd_client::*;`; `:6-20` is a `#[cfg(test)]` module whose doc claims it names "every composed client capability" and omits `eval::EvalProtocol` (added to `wyrd-client/src/lib.rs:18,28` by this closeout). `sdks/wyrd-sdk-rust/` is **not touched** by the closeout diff. | **REJECTED** | Test-only rustdoc in a package with no outside consumer of the list, and the task states plainly "**The Rust SDK is untouched** … must not be narrowed here." Editing it to satisfy a doc adjective would contradict the task's own prohibition. Dormant, zero-consumer surface the task does not require. |
| *(disclosed, unruled)* Co-author trailers | All nine closeout commits carry `Co-Authored-By: Claude Opus 5 (1M context)`; author and committer are `Thorrester <sjforrester32@gmail.com>` throughout. The eight commits **before** the branch base (`968c92641` and earlier) carry `Co-Authored-By: Claude Opus 5` too. | **REJECTED as a TASK-008 finding** | See §4. |

---

## 2. Final deduplicated ledger

Numbering continues from the prior review's `FIND-008-1..7`. None of these seven
is the same defect as a prior ID: prior `FIND-008-1/-2` were the Python and
TypeScript SDK administrative journeys that revision 7 deliberately removed, and
prior `FIND-008-5` (routes absent from the generated contract) has landed.

### `FIND-008-8` — MISSING — four unreachable per-command CLI error variants remain

- **Wave 1 sources.** `TR-1`, `RR-1`, `DC-3`. **Status:** CONFIRMED.
- **Violated obligation.** TASK-008 acceptance criterion "Unreachable per-command
  CLI error variants are deleted, not left in place"; Scenario 3 REFACTOR
  ("delete … the per-command error variants that become unreachable,
  `WyrdCliError::RevokeFailed` among them"); AGENTS.md §15.
- **Location.** `crates/wyrd/wyrd-cli/src/error.rs:227` (`AuthFailed`), `:242`
  (`RevokeFailed`), `:257` (`AdminFailed`), `:272` (`IssueKeyFailed`).
- **Evidence.** A tree-wide grep over `crates/`, `sdks/`, `docs/`, `examples/`
  returns only these four declaration sites. `3a71e0409` removed every
  constructor: `login.rs`/`refresh.rs`/`issue_key.rs` now raise
  `WyrdCliError::Server`, `trusted_issuer.rs`/`workload_binding.rs` raise
  `InvalidArgument`/`Io`/`Server`. The recorded evidence claims `RevokeFailed`
  was removed; it was not, and its three siblings were never enumerated. The
  cited proof (`mise run lints`, "dead-code clean") cannot see this: `dead_code`
  does not fire on variants of a `pub` enum.
- **Observable consequence.** Each variant carries a registered `#[wyrd_error]`
  code, status, title and remediation that no execution path can ever emit —
  including `WYRD_CLI_500_REVOKE_FAILED`, whose remediation contradicts the
  actual behaviour now recorded on `WyrdCliError::Server` (`error.rs:334-340`).
  Their `status: u16` fields are the shape of the per-command status mapping
  `REQ-047` deleted, standing as an invitation to reintroduce it. (They are *not*
  in `docs/src/content/docs/api/errors.md`, which lists only
  `WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED` and `WYRD_CLI_500_IO` — so this is
  dead in-crate catalog metadata, not published contract drift.)
- **Correction.** Delete the four variants with their `#[error]` and
  `#[wyrd_error]` attributes from `crates/wyrd/wyrd-cli/src/error.rs`. No new
  owner or mechanism: the existing `WyrdCliError::Server` already carries every
  server failure. Remove any import left unused.
- **Closure proof.** The compiler is the test — a variant that breaks the build
  on removal was reachable and stays:
  `mise exec -- cargo clippy --locked -p wyrd-cli --all-targets` and
  `mise run codegen:check`.

### `FIND-008-9` — INCORRECT — the agent-facing workflow page authenticates on `Authorization: Bearer`

- **Wave 1 sources.** `TR-3`, `RR-3`, `DS-1`. **Status:** REVISED (the proposed
  new docs gate is removed).
- **Violated obligation.** `INV-015`; `REQ-040` (a declared task obligation);
  `AC-013`; TASK-008 acceptance criteria "No Wyrd-owned surface reads the
  `Authorization` header; a tree-wide search returns nothing" and "No document,
  example, or surface still describes a second identity model …" — the row the
  implementor marked PASS on a grep that never left `crates/`.
  `architecture/wyrd-design.md:495-507` is the authority.
- **Location.** `docs/src/content/docs/for-agents/workflow.svx:28`, `:59`, `:81`.
- **Evidence.** Line 28 `-H "Authorization: Bearer $WYRD_ACCESS_TOKEN"` on
  `GET $WYRD_SERVER_URL/v1/cards/default/my-dataset/0.1.0`; line 59 the same on
  `POST $WYRD_SERVER_URL/v1/cards`; line 81 `"Authorization": f"Bearer {token}"`
  in the Python snippet. Both routes take the tenant `Caller` extractor, which
  reads `WYRD_ACCESS_TOKEN_HEADER` only
  (`crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:16-17,22`).
  The file contains no occurrence of `x-wyrd-access-token`. Every sibling page is
  already correct, and `self-hosting/local-development.svx:41` states the
  opposite rule outright — this page is the tree's lone contradiction.
- **Observable consequence.** An agent following its own dedicated workflow page
  verbatim sends no credential Wyrd reads and gets
  `WYRD_AUTH_401_UNAUTHENTICATED` on step 1 and step 3, on the surface AGENTS.md
  §2 declares primary.
- **Correction.** Replace all three with `X-Wyrd-Access-Token: Bearer
  $WYRD_ACCESS_TOKEN` (and the Python dict key with `"X-Wyrd-Access-Token"`),
  matching the existing wording at `concepts/authentication.svx:12` and
  `self-hosting/authentication.svx:11`. **Add no new check** — AGENTS.md §12
  makes a new permanent gate pay a cost it has not earned for a three-line prose
  fix, and `platform_admin_e2e` plus `principal_journey` already prove which
  header works.
- **Closure proof.** `grep -rn 'Authorization: Bearer' docs/src/content/docs/`
  returns no `$WYRD_SERVER_URL` example, and `mise run docs:check` exits 0.

### `FIND-008-10` — INCORRECT — the served `BadTokenFormat` detail names a header Wyrd never reads

- **Wave 1 sources.** `TR-4`, `RR-4`, `DS-2`. **Status:** REVISED (both live
  sites, not one).
- **Violated obligation.** `INV-015`; the same acceptance criterion as
  `FIND-008-9`; AGENTS.md §9. This closeout explicitly owns this code's prose —
  `21abea8c7` rewrote `WYRD_AUTH_400_BAD_TOKEN_FORMAT`'s title and remediation to
  name `X-Wyrd-Access-Token` (`crates/wyrd-spec/src/error.rs:543-548`).
- **Location.** `crates/wyrd/wyrd-server/src/http/error.rs:202` and
  `crates/wyrd/wyrd-auth/src/error.rs:35`; fixture
  `crates/wyrd-spec/src/error.rs:4168`.
- **Evidence.** Both live mappers emit `message: "authorization header
  malformed"`. Both are reached: `http/error.rs:35`
  (`From<AuthError> for WyrdErrorResponse`, so every tenant route) and
  `wyrd-auth/src/{callback.rs:180, exchange_api_key.rs:772, jwt_bearer.rs:111,
  platform_login.rs:243}`. `AuthError::BadTokenFormat` is raised at
  `wyrd-auth-verify/src/lib.rs:472,599,606,609,685` for an oversized token or a
  value that is not a compact JWT. The emitted problem document therefore reads
  `"detail": "authorization header malformed"` beside `"remediation": "Use
  \`X-Wyrd-Access-Token: Bearer <token>\` …"` — one payload, two headers.
- **Observable consequence.** A caller whose `X-Wyrd-Access-Token` is malformed is
  told in the human-readable field to fix a header it did not send and Wyrd does
  not read. The spec's target consumer is "a careful, literal interpreter that
  has no ability to ask for clarification"; one keying remediation off `detail`
  is sent to the wrong header.
- **Correction.** Change the message at **both** live sites to name the canonical
  header (e.g. `"X-Wyrd-Access-Token header malformed"`) and update the
  `wyrd-spec` fixture at `:4168`. Reuse the wording already established by
  `token_extract::bad_token_format` (`token_extract.rs:83-88`). Fixing only one
  site leaves the other serving the wrong header.
- **Closure proof.**
  `grep -rni '"authorization header' crates/` returns nothing, plus
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/http::error::tests::/)'`
  and
  `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/error::tests::/)'`.

### `FIND-008-11` — REGRESSION — `wyrd auth login` echoes the pasted OIDC callback, authorization code included, to stderr

- **Wave 1 sources.** `DS-3`. **Status:** CONFIRMED.
- **Violated obligation.** TASK-008 invariant "Credential plaintext crosses a
  client surface exactly once, in the response that created it, and is never
  persisted by a client"; spec `INV-002` ("Raw credential material is never
  persisted, recoverable, re-displayable, logged, traced, placed in an error or
  audit payload").
- **Location.** `crates/wyrd/wyrd-cli/src/auth/login.rs:99-103`, specifically
  `value: input.to_owned()` at `:101`.
- **Evidence.** `rtk proxy git diff 289978fcc~1..4668d8d33 --
  crates/wyrd/wyrd-cli/src/auth/login.rs` shows the pre-change arm returned
  `WyrdCliError::AuthFailed { status: 0, detail: "paste the full callback URL or
  \`code=<>&state=<>\` query string" }` — value-free. It now returns
  `InvalidArgument { field: "callback", value: input.to_owned(), … }`, rendered by
  `#[error("invalid {field} {value:?}: {expected}")]`
  (`crates/wyrd/wyrd-cli/src/error.rs:309`) and printed **twice** on stderr by
  `print_cli_error` (`crates/wyrd/wyrd-cli/src/eval/output.rs:13-25`, reached from
  `crates/wyrd/wyrd-cli/src/lib.rs:61-66`) — once as machine-shaped JSON
  `message`, once as `error: …` prose. `input` is the operator's pasted callback
  (`login.rs:44-49`). The arm is reached whenever `code` and `state` are not both
  present, so a truncated paste or an IdP returning `session_state` echoes a live
  `code`.
- **Observable consequence.** A single-use OIDC authorization code, still
  redeemable at `/auth/token` until used or expired, lands in terminal
  scrollback, redirected stderr, and — because the JSON line exists to be
  collected — in CI logs and log aggregators. An operator typo becomes credential
  disclosure on a durable surface, and it is the one place in this closeout where
  a credential reaches a surface it did not reach before.
- **Correction.** Keep `InvalidArgument` but stop carrying the raw input: pass a
  shape description (`value: "<callback>"`) or name only which of `code`/`state`
  was absent. No new mechanism — `InvalidArgument`'s `expected` field already
  carries the actionable guidance.
- **Closure proof.** A unit test in `login.rs`'s existing `mod tests` asserting
  that `parse_callback_input("?code=super-secret-code")`'s error `Display`
  contains neither `super-secret-code` nor `code=`, run with
  `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(/auth::login::tests::/)'`.

### `FIND-008-12` — VIOLATION — the two functions whose error behaviour this diff changed are the two that do not document it

- **Wave 1 sources.** `RR-5`. **Status:** CONFIRMED.
- **Violated obligation.** AGENTS.md §16 ("Every new or materially modified Rust
  item MUST have rustdoc … Every fallible Rust function or method MUST include a
  `# Errors` section … Missing or placeholder rustdoc on any touched Rust item is
  a hard blocker"); `architecture/agent-rules.md` (`BLOCK_BEFORE_MERGE`).
- **Location.** `crates/wyrd/wyrd-cli/src/auth/login.rs:78`
  (`parse_callback_input`) and
  `crates/wyrd/wyrd-server/src/components/eval/routes.rs:255` (`check_lease`).
- **Evidence.** Both are materially modified by this diff, not merely nearby.
  `parse_callback_input`: the diff replaced its failure branch's variant and
  payload (`AuthFailed` → `InvalidArgument` carrying the raw input); it has **no
  rustdoc at all** and no `# Errors`. `check_lease`: the diff changed
  `headers.get(header::AUTHORIZATION)` to `headers.get(EVAL_LEASE_HEADER)` — the
  precise condition under which it fails; it carries one line of doc and no
  `# Errors` for its three distinct refusal paths (absent header, no scheme
  separator, wrong scheme or empty token). Every other symbol touched in the same
  diff (`client.rs`, `eval/handle.rs`, `TokenExchange`, `login::dispatch`,
  `issue_key::dispatch`, `revoke::dispatch`) did receive `# Errors`.
- **Observable consequence.** `check_lease` is a security check whose refusal
  conditions are now undocumented at the moment they changed;
  `parse_callback_input` is the function an operator hits on a bad paste, and a
  maintainer cannot tell from its signature which of the CLI's error variants it
  produces — which is how `FIND-008-11` got in.
- **Correction.** Add rustdoc stating intent, the item's role in the surrounding
  workflow, and an `# Errors` section to both, following the pattern the same
  diff already applied to `login::dispatch` (`login.rs:21-27`). Fold the
  `parse_callback_input` doc into the same edit as `FIND-008-11`, which touches
  the same arm.
- **Closure proof.** Inspection against AGENTS.md §16, plus
  `mise exec -- cargo clippy --locked -p wyrd-cli -p wyrd-server --all-targets`
  (clippy's `missing_errors_doc` is public-API-only and does not substitute for
  the rule).

### `FIND-008-13` — MISSING — no lane receives a once-returned credential through the CLI and uses it

- **Wave 1 sources.** `TR-2` (confirmed), `DC-5` (revised into this). **Status:**
  CONFIRMED.
- **Violated obligation.** TASK-008 acceptance criterion "The CLI performs the
  administrative operations against a real server, **including receiving a
  once-returned credential and using it on a subsequent call**, through a lane
  that actually runs"; task invariant on single-crossing credential plaintext;
  spec `AC-014` ("The CLI and MCP exercise those operations against a real
  server"); `architecture/agent-rules.md` ("a unit test never substitutes for a
  missing journey").
- **Location.** `crates/wyrd/wyrd-cli/tests/cli.rs:1-13` (the lane's whole module
  list) and `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:87-160` (the command's
  only tests).
- **Evidence.** `tests/` holds `card_lifecycle.rs`, `eval_local_records.rs`,
  `eval_server_protocol.rs`, `eval_support/`, `loader.rs`, `principal_journey.rs`,
  `query_server_journey.rs` — no `auth` journey.
  `grep -rn 'issue-key\|issue_key\|"auth"' crates/wyrd/wyrd-cli/tests/` returns
  nothing, so the closeout's cited "pre-existing `auth` journeys through the same
  client" name tests that do not exist. `principal_journey.rs:145-151` draws every
  credential from the in-process harness (`bootstrap_service` +
  `exchange_api_key`), never through the CLI. `issue_key.rs:87-160` is
  clap-parsing only.
- **Observable consequence.** The one CLI flow that handles credential plaintext
  — `wyrd auth issue-key` printing a key that is never recoverable
  (`issue_key.rs:76-82`) — has no end-to-end coverage. A response field rename, an
  elided `key:` line, or an issued key rejected on its next use ships undetected,
  and the exact defect class this task exists to close (a CLI transport path with
  no test) survives on a command the task itself names as having carried the
  original header defect.
- **Correction.** Add one real-server CLI test in the existing `test:cli:journey`
  lane, following `principal_journey.rs`'s shape (it already owns `run_cli`,
  `start_served`, `stop_served`, `machine_token`): register a card, run
  `wyrd auth issue-key` against a bootstrapped tenant, capture the printed `key:`
  from stdout, exchange it for an access token, and assert that token reaches an
  authenticated `/v1` route. Register the module in
  `crates/wyrd/wyrd-cli/tests/cli.rs`. **No new public surface is needed** —
  `wyrd auth issue-key` already exists on `wyrd-client`
  (`auth/issue_key.rs:64-72`), which is why `DC-5`'s four new CLI commands are
  rejected.
- **Closure proof.** `WYRD_CLI_E2E=1 mise run test:cli:journey` plus the focused
  `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E
  'test(=<module>::<name>)'` under
  `scripts/postgres/with-test-postgres.sh`; confirm the selector first with
  `mise exec -- cargo nextest list -p wyrd-cli --test cli`.

### `FIND-008-14` — INCORRECT — a `mise` task invokes a CLI command that does not exist

- **Wave 1 sources.** `DC-6`. **Status:** REVISED (delete rather than repoint).
- **Violated obligation.** TASK-008 acceptance criterion "No document, example,
  or surface still describes a second identity model, **a removed bootstrap
  path**, or credential-keyed authorization"; `REQ-038`; `AC-013`. `mise.toml` is
  in TASK-008's declared test write set.
- **Location.** `mise.toml:1081-1083`.
- **Evidence.** The task runs `cargo run --locked -p wyrd-cli -- dev bootstrap`.
  `crates/wyrd/wyrd-cli/src/cli.rs:55-84` declares no `Dev` variant, and
  `crates/wyrd/wyrd-cli/src/lib.rs:104-107`
  (`removed_dev_bootstrap_returns_usage_error`) pins the removal at exit code 64.
  `rtk proxy git show b36ad6e12:crates/wyrd/wyrd-cli/src/cli.rs | grep Dev` is
  empty, so the task was born broken on this branch when it replaced the deleted
  `cli:bootstrap-key`. It is the only reference in the tree — `docs/`,
  `architecture/` and `scripts/` contain none. The sibling `cli:init` is correct.
- **Observable consequence.** The one seeding task an operator reaches for after
  `bootstrap-key` was removed exits 64 with a clap usage error, and the
  acceptance criterion forbidding a surface that describes a removed bootstrap
  path was recorded PASS.
- **Correction.** Delete the `[tasks."cli:dev-bootstrap"]` block. First rung of
  the ladder: nothing references it, and `REQ-038` names `cli:init` plus
  `REQ-020`/`REQ-025` tenant provisioning as the replacement — so there is no
  command to repoint it at.
- **Closure proof.** `grep -n 'dev-bootstrap' mise.toml` returns nothing and
  `mise tasks ls | grep dev-bootstrap` is empty.

---

## 3. Rejected findings — why each is omitted

- **`RR-2`** — omitted. The two `tenant_from_unverified_access_token` copies were
  never one owner with one contract: at `289978fcc~1` their messages and
  `details` already differed, and `routes.rs:333` validates a **request-body
  field** on the preview-gated `/auth/token` exchange rather than a header
  credential, so its `BadTokenFormat` discloses nothing about a third party's
  token. No regression, no shared root cause, no acceptance criterion.
- **`RR-6`** — omitted. Measured: the four pre-existing
  `card_lifecycle::pg_tests::*` journeys report the identical hollow PASS in
  0.016s without `WYRD_CLI_E2E`, and `scripts/run-family-tests.sh:25` carries no
  `--skip pg_tests`, so the proposed `mod pg_tests` wrapper would not change the
  behaviour complained of. The env-gate early return **is** the repository's
  pattern in the same binary; the residual is a naming convention whose stated
  purpose is not how the lane runs.
- **`RR-7`** — omitted. Rust intra-doc links in `openapi.yaml` descriptions are an
  established repository pattern at 13 other sites including Bifrost, all
  predating this diff; the "first and only leak" claim is false and the task's
  only openapi criterion is the authentication scheme.
- **`RR-8`** — omitted. A `pub(crate)` const used once in its own module: zero
  observable consequence, required by no obligation, and the reporter concedes it
  is not a behaviour defect.
- **`DC-1`** — omitted. Typed `problem+json` refusals are absent from Cards,
  Principals, Platform and `/auth/platform/*` alike — Bifrost and Query are the
  only paths that publish them, and the Cards paths predate this change. Fixing
  administrative paths alone would increase the document's inconsistency; fixing
  it document-wide is a public-contract change outside this task's write set and
  acceptance criteria.
- **`DC-2`** — omitted. Every administrative path this specification defines *is*
  published (`http/openapi.rs:79-114`). The routes named as missing are
  pre-existing tenant auth/admin routes absent from `spec.md` and never
  published; annotating and publishing them is a contract addition the task's
  non-goals forbid.
- **`DC-4`** — omitted. `--kind`/`--reason` were mandatory and the body was
  already sent-and-ignored before the branch base (`968c92641`); the closeout
  changed only the transport. Every resolution is a user-visible CLI-argument or
  public-HTTP-contract change the task did not take on, so the implementor's
  deferral stands.
- **`DC-7`** — omitted. A `#[cfg(test)]` rustdoc adjective in a package the
  closeout does not touch and the task explicitly forbids touching ("The Rust SDK
  is untouched … must not be narrowed here"); no outside consumer of the list.
- **Co-author trailers** — omitted as a TASK-008 finding. See §4.

---

## 4. Ruling on the disclosed `§13` contradiction

AGENTS.md §13 forbids AI co-author trailers; this session's harness attribution
instruction requires one while stating that the user's own instructions —
`CLAUDE.md`, which `@`-includes AGENTS.md — take precedence over it. Under the
approved authority, therefore, **AGENTS.md §13 wins and the trailers do
contravene it.**

It is nonetheless **not a finding of this review**, for three independent
reasons: the trailer is pre-existing repository-wide practice, present on the
eight commits before the branch base (`968c92641` and earlier) and so neither
introduced nor widened by TASK-008; it falls outside the task's write set,
acceptance criteria, and every obligation in its frontmatter; and its only remedy
is rewriting history, which this review is instructed not to propose. Author and
committer identity is correct on all nine commits
(`Thorrester <sjforrester32@gmail.com>`), which is what §13's substantive rule
protects. It is recorded here as a repository-policy item for the user to settle
between the harness instruction and AGENTS.md §13 — not as a defect in this
implementation.

---

## 5. New decisions required, verification limits, and blockers

**New product, public API, architecture, security, compatibility, cross-service,
concurrency-semantics, or persistent-data decision required by a retained
correction:** **none.** Every retained correction is a deletion, a prose
correction, a rustdoc addition, or a test in an existing lane over an existing
command. `SPEC_REVISION_REQUIRED` does **not** apply to any ledger entry. The two
Wave 1 findings that *would* have needed such a decision — `DC-2`/`DC-1`
(publishing new routes and typed refusal bodies) and `DC-4` (the revoke body) —
are rejected as outside the task rather than escalated, per the rule that a
finding the approved task does not require is out of scope. `DC-5`'s escalation
proposal is unnecessary: `wyrd auth issue-key` already exists, so `FIND-008-13`
closes with a test, not a new surface.

**Verification I performed.**

| Command | Result |
|---|---|
| `env -u WYRD_CLI_E2E mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/principal_journey::/) + test(/card_lifecycle::pg_tests::/)'` | 6 passed in 0.016s — the decisive evidence rejecting `RR-6` |
| `rtk proxy git diff 289978fcc~1..4668d8d33 -- <paths>` (real unified diff) | read for `login.rs`, `eval/routes.rs`, `openapi.yaml`, `token_extract.rs` |
| `rtk proxy git show 289978fcc~1:…/token_extract.rs`, `rtk proxy git show 968c92641:…/principal/revoke.rs`, `rtk proxy git show b36ad6e12:…/cli.rs` | pre-change baselines for `RR-2`, `DC-4`, `DC-6` |
| `rtk proxy git log --format=… 289978fcc~1..4668d8d33` and `-30 968c92641` | trailer and identity audit |
| tree-wide greps for the four CLI variants, `Authorization`, `"authorization header`, `WyrdError::`/`[\`` in `openapi.yaml`, `EVAL_LEASE_HEADER`, `problem+json`, `WYRD_CLI_`, `dev-bootstrap` | as cited per finding |

**Limits.**

1. I ran no journey lane requiring Postgres. Wave 1 ran `platform_admin_e2e`
   (17/17) and `test:cli:journey` (22 passed) under the repository wrapper and I
   accept those as executed evidence; none of my retained findings depends on
   re-running them. `mise run test:platform:journey` remains broken independently
   of this task (missing `setup:postgres`).
2. I ran no `lints`, `codegen:check`, `check:client-tier`, or `docs:check` lane.
   Wave 1 ran all four at exit 0, and `FIND-008-9` is consistent with a passing
   `docs:check` (that lane inspects build, links and contrast, not header prose).
3. `FIND-008-8`'s "removal compiles clean" claim rests on exhaustive grep, not on
   an executed build with the variants removed — I changed no source. The
   compiler is the stated closure proof precisely because of that.
4. `AC-014`'s "stable error codes" clause is genuinely unmet document-wide
   (Cards, Principals, Platform and `/auth/*` publish bare description strings).
   I reject it as a TASK-008 finding because the task's acceptance criteria scope
   the openapi obligation to the authentication scheme; it belongs to
   `$wyrd-change-review`, which maps spec obligations across the whole change,
   and to a task of its own if it is to be closed. Recorded here so the change
   reviewer does not mistake it for something Wave 2 missed.
5. The closeout's "Deliberately out of scope" note states the revoke route
   "ignores the `RevokePrincipalRequest` body **that the contract declares**".
   The generated contract declares no body (`openapi.yaml:1174-1206`). The
   deferral is sound; its recorded premise is inaccurate and should be corrected
   in the packet text. This is a record correction, not a finding.
6. MCP scope gating was not re-executed
   (`wyrd-mcp/tests/bifrost/mcp/principals.rs`, `#[ignore]`d, selected only by
   `test:bifrost:journey:mcp`). Prior-landed work (`128eb40`), untouched by this
   closeout; accepted on Wave 1's inspection.

**Blockers:** none. No source was missing, no caller chain was untraceable, and
every Wave 1 disagreement was resolved from source.

---

## 6. Overall

The validated ledger is **NON-EMPTY**: `FIND-008-8` through `FIND-008-14`.
`SPEC_REVISION_REQUIRED` does **not** apply.
