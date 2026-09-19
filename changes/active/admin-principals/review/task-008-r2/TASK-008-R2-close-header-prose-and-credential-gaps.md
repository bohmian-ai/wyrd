---
task: TASK-008-R2
title: Close the header-prose, dead-variant, and credential-disclosure gaps in the administrative projection
spec: SPEC-admin-principals
spec_revision: 7
reviews: changes/active/admin-principals/review/task-008-r2/
obligations: [REQ-036, REQ-038, REQ-040, REQ-047, INV-002, INV-015, AC-013, AC-014]
findings: [FIND-008-8, FIND-008-9, FIND-008-10, FIND-008-11, FIND-008-12, FIND-008-13, FIND-008-14]
status: open
---

## Subject

| Item | Value |
|---|---|
| Approved spec | `changes/active/admin-principals/spec.md`, revision 7 |
| Original task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Reviewed candidate | branch `claude/admin-principals-spec-qfsmjc`, HEAD `4668d8d33` |
| Review verdict | `changes/active/admin-principals/review/task-008-r2/verdict.md` — `FIX_REQUIRED` |
| Validated ledger | `changes/active/admin-principals/review/task-008-r2/findings-validation.md` |

TASK-008's substance landed and holds: the canonical header, the one signing-key
resolution, the CLI on `wyrd-client`, the published security scheme, the platform
revocation rustdoc. Seven bounded defects remain. Six are deletions, prose, or
rustdoc; one is a missing journey in a lane that already exists. Nothing here
needs a new capability, dependency, abstraction, check, or contract decision.

The recurring cause is worth naming, because it shapes every diagnosis below: the
closeout's evidence was recorded against greps and lanes that could not see the
gap. `mise run lints` cannot see an unreachable variant of a `pub` enum; a grep
scoped to `crates/` cannot see `docs/`; `docs:check` does not assert which header
an example sends; nothing asserts a served `detail` string or what an error
message omits. **Do not close a finding below by re-running the check that
already passed.** Each finding names a proof that fails today.

---

## FIND-008-8 — MISSING — four unreachable per-command CLI error variants remain

**Violated obligation.** TASK-008 acceptance criterion "Unreachable per-command
CLI error variants are deleted, not left in place"; Scenario 3 REFACTOR
("`WyrdCliError::RevokeFailed` among them"); AGENTS.md §15.

**Current behavior.** `crates/wyrd/wyrd-cli/src/error.rs:227` (`AuthFailed`),
`:242` (`RevokeFailed`), `:257` (`AdminFailed`), `:272` (`IssueKeyFailed`) are
declared and have no constructor anywhere in `crates/`, `sdks/`, `docs/`, or
`examples/`. `3a71e0409` removed every construction site — `login.rs`,
`refresh.rs`, `issue_key.rs` now raise `WyrdCliError::Server`;
`trusted_issuer.rs` and `workload_binding.rs` raise
`InvalidArgument`/`Io`/`Server`.

**Why the candidate's proof falls short.** The closeout claims `RevokeFailed` was
removed; it was not, and its three siblings were never enumerated. The cited
evidence (`mise run lints`, "dead-code and unused-import clean") cannot detect
this: `dead_code` does not fire on variants of a `pub` enum.

**Observable consequence.** Four registered `#[wyrd_error]` codes, statuses,
titles and remediations no execution path can emit — including
`WYRD_CLI_500_REVOKE_FAILED`, whose remediation now contradicts what
`WyrdCliError::Server` actually records (`error.rs:334-340`). Their `status: u16`
fields are the shape of the per-command status mapping `REQ-047` deleted, left in
place as an invitation to reintroduce it.

**Correction.** Delete the four variants with their `#[error]` and
`#[wyrd_error]` attributes. Reuse no new owner: `WyrdCliError::Server` already
carries every server failure. Remove any import the deletion leaves unused. Do
not repoint anything at them and do not keep one "for symmetry" — a variant that
breaks the build on removal was reachable and stays.

**Proof.** `mise exec -- cargo clippy --locked -p wyrd-cli --all-targets` and
`mise run codegen:check`. The compiler is the test here.

---

## FIND-008-9 — INCORRECT — the agent-facing workflow page authenticates on `Authorization: Bearer`

**Violated obligation.** `INV-015`; `REQ-040`; `AC-013`; the acceptance criteria
"No Wyrd-owned surface reads the `Authorization` header; a tree-wide search
returns nothing" and "No document, example, or surface still describes a second
identity model…". `architecture/wyrd-design.md` lines 495-507 are the authority.

**Current behavior.** `docs/src/content/docs/for-agents/workflow.svx:28` sends
`-H "Authorization: Bearer $WYRD_ACCESS_TOKEN"` to
`GET $WYRD_SERVER_URL/v1/cards/...`; `:59` does the same on `POST /v1/cards`;
`:81` sets `"Authorization": f"Bearer {token}"` in the Python snippet. Both routes
take the tenant `Caller` extractor, which reads `WYRD_ACCESS_TOKEN_HEADER` only
(`crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:16-17,22`). The
file contains no occurrence of `x-wyrd-access-token`.

**Why the candidate's proof falls short.** The recorded search was
`grep -rn ... crates/`. It could not reach `docs/`. Every sibling docs page is
already correct, and `self-hosting/local-development.svx:41` states the opposite
rule outright — this page is the tree's lone contradiction.

**Observable consequence.** An agent following its own dedicated workflow page
verbatim sends no credential Wyrd reads and receives
`WYRD_AUTH_401_UNAUTHENTICATED` on steps 1 and 3 — on the surface AGENTS.md §2
declares primary.

**Correction.** Replace all three with `X-Wyrd-Access-Token: Bearer
$WYRD_ACCESS_TOKEN` (and the Python dict key with `"X-Wyrd-Access-Token"`),
matching the wording already established at `concepts/authentication.svx:12` and
`self-hosting/authentication.svx:11`. **Add no new docs check.** AGENTS.md §12
makes a permanent gate pay a cost on every run by every contributor; a three-line
prose fix has not earned one, and `platform_admin_e2e` plus `principal_journey`
already prove which header works.

**Proof.** `grep -rn 'Authorization: Bearer' docs/src/content/docs/` returns no
`$WYRD_SERVER_URL` example, and `mise run docs:check` exits 0.

---

## FIND-008-10 — INCORRECT — the served `BadTokenFormat` detail names a header Wyrd never reads

**Violated obligation.** `INV-015`; the same acceptance criterion as
`FIND-008-9`; AGENTS.md §9. This closeout owns the code's prose: `21abea8c7`
rewrote `WYRD_AUTH_400_BAD_TOKEN_FORMAT`'s title and remediation to name
`X-Wyrd-Access-Token` (`crates/wyrd-spec/src/error.rs:543-548`) and stopped
there.

**Current behavior.** Two live mappers emit `message: "authorization header
malformed"` — `crates/wyrd/wyrd-server/src/http/error.rs:202` (reached from
`From<AuthError> for WyrdErrorResponse` at `:35`, so every tenant route) and
`crates/wyrd/wyrd-auth/src/error.rs:35` (reached from `callback.rs:180`,
`exchange_api_key.rs:772`, `jwt_bearer.rs:111`, `platform_login.rs:243`). A
fixture at `crates/wyrd-spec/src/error.rs:4168` repeats the string.
`AuthError::BadTokenFormat` is raised at `wyrd-auth-verify/src/lib.rs:472,599,
606,609,685` for an oversized token or a value that is not a compact JWT.

**Why the candidate's proof falls short.** No test asserts the served `detail`
string, so `codegen:check` and `lints` both pass while the emitted problem
document reads `"detail": "authorization header malformed"` beside
`"remediation": "Use \`X-Wyrd-Access-Token: Bearer <token>\` …"` — one payload,
two headers.

**Observable consequence.** A caller whose `X-Wyrd-Access-Token` is malformed is
told in the human-readable field to fix a header it did not send and Wyrd does
not read. The spec's target consumer is a literal interpreter that cannot ask for
clarification.

**Correction.** Change the message at **both** live sites to name the canonical
header, reusing the wording `token_extract::bad_token_format` already established
(`token_extract.rs:83-88`), and update the `wyrd-spec` fixture at `:4168`.
Correcting one site leaves the other serving the wrong header — this is one
defect with two mappers, not a choice between them. Change no code, status, or
remediation; only the `detail`/`message` string.

**Proof.** `grep -rni '"authorization header' crates/` returns nothing, plus
`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/http::error::tests::/)'`
and
`mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/error::tests::/)'`.

---

## FIND-008-11 — REGRESSION — `wyrd auth login` echoes the pasted OIDC callback, live authorization code included

**Violated obligation.** TASK-008 invariant "Credential plaintext crosses a
client surface exactly once, in the response that created it, and is never
persisted by a client"; spec `INV-002` (raw credential material is never logged,
traced, or placed in an error payload).

**Current behavior.** `crates/wyrd/wyrd-cli/src/auth/login.rs:99-103` returns
`InvalidArgument { field: "callback", value: input.to_owned(), … }`. `input` is
the operator's pasted callback URL or query string (`login.rs:44-49`). The
variant renders as `#[error("invalid {field} {value:?}: {expected}")]`
(`crates/wyrd/wyrd-cli/src/error.rs:309`) and `print_cli_error`
(`eval/output.rs:13-25`, reached from `lib.rs:61-66`) writes it to stderr
**twice** — once as a machine-shaped JSON `message`, once as `error: …` prose.
The arm is reached whenever `code` and `state` are not both present, so a
truncated paste or an IdP returning `session_state` echoes a live `code`.

**Why the candidate's proof falls short.** This is new in the closeout. The
pre-change arm returned `WyrdCliError::AuthFailed { status: 0, detail: "paste the
full callback URL or \`code=<>&state=<>\` query string" }` — value-free.
`3a71e0409` replaced it while migrating away from `AuthFailed`, and nothing
asserts what an error message does *not* contain.

**Observable consequence.** A single-use OIDC authorization code, still
redeemable at `/auth/token` until used or expired, lands in terminal scrollback,
redirected stderr, and — because the JSON line exists to be collected — in CI
logs and log aggregators. An operator typo becomes credential disclosure on a
durable surface. It is the one place in this closeout where a credential reaches
a surface it did not reach before.

**Correction.** Keep `InvalidArgument` — it is the right variant and its
`expected` field already carries the actionable guidance. Stop carrying the raw
input: give `value` a shape description (`"<callback>"`) or name only which of
`code`/`state` was absent. Do not reinstate `AuthFailed` (see `FIND-008-8`), do
not add redaction machinery, and do not suppress the error.

**Proof.** A unit test in `login.rs`'s existing `mod tests` asserting that the
error `Display` for an input such as `"?code=super-secret-code"` contains neither
`super-secret-code` nor `code=`:
`mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(/auth::login::tests::/)'`.

---

## FIND-008-12 — VIOLATION — the two functions whose error behavior this diff changed are the two that do not document it

**Violated obligation.** AGENTS.md §16 — every new or materially modified Rust
item must have rustdoc explaining intent and its role in the surrounding
workflow, and every fallible function must have a `# Errors` section; missing
rustdoc on a touched item is a hard blocker. `architecture/agent-rules.md`
classes it `BLOCK_BEFORE_MERGE`.

**Current behavior.** `crates/wyrd/wyrd-cli/src/auth/login.rs:78`
(`parse_callback_input`) has no rustdoc at all and no `# Errors`, yet this diff
replaced its failure branch's variant and payload.
`crates/wyrd/wyrd-server/src/components/eval/routes.rs:255` (`check_lease`)
carries one line of doc and no `# Errors` for its three distinct refusal paths
(absent header, no scheme separator, wrong scheme or empty token), yet this diff
changed `headers.get(header::AUTHORIZATION)` to `headers.get(EVAL_LEASE_HEADER)`
— the precise condition under which it fails.

**Why the candidate's proof falls short.** `clippy::missing_errors_doc` is
public-API-only, so `lints` passes. Every other symbol the same diff touched
(`client.rs`, `eval/handle.rs`, `TokenExchange`, `login::dispatch`,
`issue_key::dispatch`, `revoke::dispatch`) did receive `# Errors` — these two
were missed, not exempted.

**Observable consequence.** `check_lease` is a security check whose refusal
conditions became undocumented at the moment they changed. `parse_callback_input`
is the function an operator hits on a bad paste and a maintainer cannot tell from
its signature which error variant it produces — which is how `FIND-008-11` got
in.

**Correction.** Add rustdoc stating intent, the item's role in the surrounding
workflow, and an `# Errors` section to both, following the pattern the same diff
already applied at `login.rs:21-27`. Fold the `parse_callback_input` documentation
into the same edit as `FIND-008-11` — it is the same arm. Document only these two
items; do not add rustdoc to untouched code (AGENTS.md §16 forbids it).

**Proof.** Inspection against AGENTS.md §16 plus
`mise exec -- cargo clippy --locked -p wyrd-cli -p wyrd-server --all-targets`.

---

## FIND-008-13 — MISSING — no lane receives a once-returned credential through the CLI and uses it

**Violated obligation.** TASK-008 acceptance criterion "The CLI performs the
administrative operations against a real server, **including receiving a
once-returned credential and using it on a subsequent call**, through a lane that
actually runs"; spec `AC-014`; `architecture/agent-rules.md` ("a unit test never
substitutes for a missing journey").

**Current behavior.** `crates/wyrd/wyrd-cli/tests/cli.rs:1-13` registers
`card_lifecycle`, `eval_local_records`, `eval_server_protocol`, `eval_support`,
`loader`, `principal_journey`, `query_server_journey` — no `auth` journey.
`grep -rn 'issue-key\|issue_key' crates/wyrd/wyrd-cli/tests/` returns nothing.
`principal_journey.rs:145-151` draws every credential from the in-process harness
(`bootstrap_service` + `exchange_api_key`), never through the CLI.
`issue_key.rs:87-160` is clap-parsing only.

**Why the candidate's proof falls short.** The closeout cites "the pre-existing
`auth` journeys through the same client". Those tests do not exist. The one CLI
flow that handles credential plaintext — `wyrd auth issue-key` printing a key
that is never recoverable (`issue_key.rs:76-82`) — has no end-to-end coverage, so
a response-field rename, an elided `key:` line, or an issued key rejected on its
next use ships undetected. That is the same defect class this task exists to
close, on a command the task itself names as having carried the original header
defect.

**Observable consequence.** The acceptance criterion's second clause has no
evidence at any tier.

**Correction.** Add one real-server CLI test to the existing `test:cli:journey`
lane and register its module in `crates/wyrd/wyrd-cli/tests/cli.rs`. Follow
`principal_journey.rs`'s shape — it already owns `run_cli`, `start_served`,
`stop_served`, and `machine_token`, so reuse those rather than building a new
harness. The path: bootstrap a tenant, run `wyrd auth issue-key`, capture the
printed `key:` from stdout, exchange it for an access token, and assert that
token reaches an authenticated `/v1` route. **No new public surface is needed** —
`wyrd auth issue-key` already exists over `wyrd-client`
(`crates/wyrd/wyrd-cli/src/auth/issue_key.rs:64-72`). Do not add CLI commands for
credential creation; the review rejected that proposal for exactly this reason.
Gate the test the way the lane's existing journeys are gated; do not introduce a
new gating convention.

**Proof.** `WYRD_CLI_E2E=1 mise run test:cli:journey`, plus the focused
`mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=<module>::<name>)'`
under `scripts/postgres/with-test-postgres.sh`. Confirm the selector first with
`mise exec -- cargo nextest list -p wyrd-cli --test cli`, and verify the test
fails when the credential is not actually reused.

---

## FIND-008-14 — INCORRECT — a `mise` task invokes a CLI command that does not exist

**Violated obligation.** TASK-008 acceptance criterion "No document, example, or
surface still describes a second identity model, **a removed bootstrap path**, or
credential-keyed authorization"; `REQ-038`; `AC-013`. `mise.toml` is in TASK-008's
declared test write set.

**Current behavior.** `mise.toml:1081-1083` (`cli:dev-bootstrap`) runs
`cargo run --locked -p wyrd-cli -- dev bootstrap`.
`crates/wyrd/wyrd-cli/src/cli.rs:55-84` declares no `Dev` variant, and
`crates/wyrd/wyrd-cli/src/lib.rs:104-107`
(`removed_dev_bootstrap_returns_usage_error`) pins the removal at exit code 64.
`rtk proxy git show b36ad6e12:crates/wyrd/wyrd-cli/src/cli.rs | grep Dev` is
empty, so the task was born broken on this branch when it replaced the deleted
`cli:bootstrap-key`.

**Why the candidate's proof falls short.** No lane invokes the task, so nothing
fails. It is the only reference in the tree — `docs/`, `architecture/`, and
`scripts/` contain none — and the sibling `cli:init` is correct.

**Observable consequence.** The one seeding task an operator reaches for after
`bootstrap-key` was removed exits 64 with a clap usage error, while the
acceptance criterion forbidding a surface describing a removed bootstrap path was
recorded PASS.

**Correction.** Delete the `[tasks."cli:dev-bootstrap"]` block. First rung of the
ladder: nothing references it, and `REQ-038` names `cli:init` plus
`REQ-020`/`REQ-025` tenant provisioning as the replacement, so there is no
command to repoint it at. Do not recreate a `dev bootstrap` command.

**Proof.** `grep -n 'dev-bootstrap' mise.toml` returns nothing and
`mise tasks ls | grep dev-bootstrap` is empty.

---

## Constraints, preserved behavior, non-goals

**Preserve unchanged.** Everything TASK-008 landed and this review passed: the
`platform_extractor` canonical-header binding and its *indistinguishable*
rejection; `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check; the single
`TokenVerifier` signing-key resolver and `verify_external_against` left alone;
the CLI's single `wyrd-client` construction point (`wyrd-cli/src/client.rs`);
`wyrd_client::eval::{EvalProtocol, EvalRun}` and `request_external_stream`; the
`SecurityAddon` generator and the regenerated `openapi.yaml`; the platform
revocation mechanism and its corrected rustdoc; `principal_journey.rs`.

**Constraints.**

- The original task's **Prohibited changes** and **Material Stop Conditions**
  remain in force verbatim: no Python or TypeScript administrative binding, no
  reinstated Rust SDK administrative journeys, the platform plane is not routed
  through `AuthenticatedPrincipal`, the indistinguishable rejection is not
  weakened, no revocation epoch on the platform plane, no compatibility route or
  alias, no hand-edited generated artifacts.
- Add **no** new repository check, dependency, abstraction, trait, helper type,
  or test harness. Every correction above is a deletion, a string, rustdoc, or
  one test in an existing lane with an existing harness.
- Do not circumvent a gate: no `#[allow]`, no `#[ignore]`, no deleted or weakened
  test, no broadened boundary glob.
- Do not rewrite git history. The `Co-Authored-By: Claude` trailers the verdict
  discloses are outside this task's write set.

**Non-goals — do not do these, all rejected by validation.**

- Do not publish error bodies or additional routes in `openapi.yaml` (`DC-1`,
  `DC-2`): both exceed item 4's obligation, which was one `securitySchemes`
  entry attached to every authenticated path.
- Do not resolve the `POST /v1/principals/{principal_id}/revoke` body
  disagreement (`DC-4`). It predates the branch base and needs a public-contract
  decision this task declined; leave the recorded deferral standing.
- Do not add CLI or MCP credential-creation commands (`DC-5`): `wyrd auth
  issue-key` already exists.
- Do not change the journey gating convention (`RR-6`), the intra-doc links in
  generated output (`RR-7`), the `pub(crate)` const visibility in
  `components/eval/routes.rs` (`RR-8`), the second
  `tenant_from_unverified_access_token` site (`RR-2`), or any
  `sdks/wyrd-sdk-rust` rustdoc (`DC-7`).
- Do not repair `mise run test:platform:journey`'s missing `setup:postgres`
  dependency. It is broken independently of this change and outside the write
  set; use
  `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'`.

## Acceptance criteria

| # | Criterion | Finding |
|---|---|---|
| 1 | `AuthFailed`, `RevokeFailed`, `AdminFailed`, and `IssueKeyFailed` no longer exist in `crates/wyrd/wyrd-cli/src/error.rs`, and `wyrd-cli` builds and lints clean | `FIND-008-8` |
| 2 | `docs/src/content/docs/for-agents/workflow.svx` authenticates every `$WYRD_SERVER_URL` example on `X-Wyrd-Access-Token`, and no docs page sends `Authorization: Bearer` to a Wyrd route; no new docs check was added | `FIND-008-9` |
| 3 | Both live `BadTokenFormat` mappers and the `wyrd-spec` fixture name the canonical header; `grep -rni '"authorization header' crates/` returns nothing; status, code, and remediation are unchanged | `FIND-008-10` |
| 4 | `wyrd auth login`'s invalid-callback error contains no part of the pasted input; a unit test asserts the absence of both the code value and `code=` from the rendered error | `FIND-008-11` |
| 5 | `parse_callback_input` and `check_lease` each carry rustdoc stating intent and workflow role plus an `# Errors` section naming every refusal path; no untouched item gained documentation | `FIND-008-12` |
| 6 | A test in the `test:cli:journey` lane runs `wyrd auth issue-key` against a real server, captures the once-printed key, and proves it authenticates a subsequent `/v1` call; the module is registered in `tests/cli.rs` and the test fails if the credential is not reused | `FIND-008-13` |
| 7 | `mise.toml` declares no `cli:dev-bootstrap` task and no surface invokes `wyrd dev bootstrap` | `FIND-008-14` |
| 8 | No preserved behavior above changed, no prohibited change or non-goal was entered, and no gate was weakened | all |

## Verification

Focused proof per finding (each fails at `4668d8d33` and passes after the
correction):

```bash
mise exec -- cargo clippy --locked -p wyrd-cli --all-targets            # 1, 5
grep -rn 'Authorization: Bearer' docs/src/content/docs/                 # 2
grep -rni '"authorization header' crates/                               # 3
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/http::error::tests::/)'                                     # 3
mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(/error::tests::/)'                                           # 3
mise exec -- cargo nextest run --locked -p wyrd-cli --lib \
  -E 'test(/auth::login::tests::/)'                                     # 4
grep -n 'dev-bootstrap' mise.toml                                       # 7
```

Broader verification, matching the original task's scope:

```bash
mise run fmt
mise run lints
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-auth-verify \
  -p wyrd-client -p wyrd-cli -p wyrd-mcp --all-targets
mise run check:client-tier
mise run check:sdk-client-tier
mise run check:unwrap-audit
mise run codegen:check
mise run docs:check
WYRD_CLI_E2E=1 mise run test:cli:journey
WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'
```

Confirm every named selector with `mise exec -- cargo nextest list` against the
owning package before relying on it. Do not offer a broad aggregate
(`mise run gate`, `test:rust`, a whole-crate or family lane, the storage matrix,
any `--all-features` workspace lane) as evidence for any criterion above.

## Authority links

- Approved spec: `changes/active/admin-principals/spec.md`, revision 7
- Original task: `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`
- Review verdict and ledger: `changes/active/admin-principals/review/task-008-r2/`
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md` (lines 495-507 for the canonical header)
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
