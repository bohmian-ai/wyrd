# Domain review — public contract and client boundary (`domain-rev-contract`)

Subject: TASK-008 closeout, candidate HEAD `4668d8d33`, spec revision 7.

## 1. Reviewed boundary and how I traced it

The language-agnostic wire contract and the single-client-implementation
boundary:

- **Generated contract.** Read `crates/wyrd/wyrd-server/src/http/openapi.rs` in
  full, then enumerated the served routes myself from
  `crates/wyrd/wyrd-server/src/http/router.rs` and each merged router
  (`components/auth/routes.rs:42-47`, `components/admin/routes.rs:57-70`,
  `components/principals/routes.rs:48-58`, `components/platform/routes.rs:37-52`,
  `components/platform/identity.rs:59-86`) and compared that set against the 25
  paths in `openapi.yaml`. Checked every handler signature for its auth
  extractor to decide, independently of `ANONYMOUS_PATHS`, which published paths
  authenticate.
- **The new unit test** `openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme`
  passes and is *partly* a tautology. It asserts the scheme's shape and the
  document-level requirement — real value — but its per-path half reads the same
  `super::ANONYMOUS_PATHS` the modifier writes from, so it can only fail if the
  modifier is broken or someone adds a per-route `security(...)`. It cannot
  detect a route wrongly listed as anonymous or an anonymous route missing from
  the list. I verified that property by hand instead: all three entries
  (`/auth/platform/token`, `/auth/platform/login`, `/auth/platform/callback`)
  take no auth extractor and are merged outside `require_authenticated`, and
  every other published path takes `Caller` or `PlatformCaller`. The scheme
  attachment is therefore **truthful today**, on evidence the test does not
  supply.
- **Regeneration.** `mise run codegen:check` passes, so `openapi.yaml`, the
  `.pyi` stubs, and both `ui_problem_examples.json` copies are genuinely
  generated, not hand-edited. The two JSON files are generated artifacts that
  regenerate consistently with the `wyrd-spec` catalog prose change.
- **REQ-036 concretely.** Walked credential creation
  (`POST /v1/principals/{id}/credentials`) and principal revocation
  (`POST /v1/principals/{id}/revoke`) as an independent implementor would: path,
  body, headers, scopes, error codes. Findings DC-1, DC-2, DC-4 come from that
  walk.
- **Published docs.** Read the `generate_api_docs.py` diff, `api/errors.md`,
  `api/openapi.md`, and searched `docs/src` and `examples/` for the removed
  bootstrap path and for the wrong identity model. Finding DC-6 comes from that
  search; the `bootstrap-key` / `SYSTEM_OPERATOR_ID` / `bootstrap-admin` search
  is clean outside the `docs/.svelte-kit` build output.
- **Single client implementation.** Read `wyrd-cli/src/client.rs`,
  `principal/revoke.rs`, `auth/{login,refresh,issue_key,trusted_issuer,workload_binding}.rs`,
  `card.rs`, `query/mod.rs`, `eval/{server.rs,agent.rs}`, and `error.rs` in
  full. No HTTP client is constructed anywhere under `wyrd-cli/src`
  (`grep -rn "reqwest::Client"` → no matches); every caller routes through
  `crate::client`. `auth login`/`auth refresh` fold onto
  `wyrd_client::auth::TokenExchange`, not a re-posted exchange. The new
  `wyrd-cli` manifest line is `wyrd-auth-verify` as a **dev**-dependency for the
  journey test — earned and outside the production cone.
- **New `wyrd-client` capability.** `request_external_stream` was **not** added
  by this change: it is the pre-existing credential-free presigned-storage seam
  (`transport/http.rs:287`, used by five storage callers). `eval/agent.rs`
  reuses it for the operator-supplied agent endpoint, which is exactly a
  cross-origin, credential-free call — correct owner, correct rung of the
  ladder, no non-Wyrd transport added. The only new transport code is
  `request_json_with_headers`, `pub(crate)`, which exists because the eval lease
  is a second credential `request_json` cannot carry; it is the minimum.
  `EvalProtocol`/`EvalRun` follow the `cards/handle.rs::Cards` pattern
  (`Arc<WyrdClient>` field, inherent methods, redacted `Debug`, one private
  `leased` helper owning the header). No finding on items 5.
- **SDK scope.** No Python or TypeScript administrative binding exists; no
  `.pyi`/`.d.ts` administrative surface; `sdks/wyrd-sdk-rust/tests/` holds only
  `cards_state.rs`, so the deleted admin journeys stayed deleted; the wholesale
  `pub use wyrd_client::*;` re-export was not narrowed. One residue: DC-7.
- **No compatibility surface.** Scanned the cumulative diff for added
  `deprecated`/`compat`/`legacy`/`alias`/transitional constructs; every hit is
  prose in the change packet asserting their absence. Clean.
- **Lane reality.** `wyrd-cli` sets `autotests = false` and declares one test
  target, so I checked `tests/cli.rs`: it `#[path]`-includes both
  `principal_journey.rs` and `eval_server_protocol.rs`, and
  `test:cli:journey` → `:inner` runs `--test cli` with `WYRD_CLI_E2E=1`, which
  is the env gate both new tests read. The lane really selects them; I ran it
  and both executed.

## 2. Authority and source coverage

- `changes/active/admin-principals/spec.md` revision 7: REQ-036, REQ-040,
  REQ-047, INV-013, INV-015, AC-013, AC-014, VER-001..VER-006.
- `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`:
  Owners/Scope/Prohibited changes, "Scope removed at revision 7", "Proof
  Strategy For The Non-Behavioral Remainder" item 4, Acceptance Criteria,
  Implementation Evidence.
- `AGENTS.md` §2, §3, §9, §12, §15, §16.
- Source read: the files named in §1 plus `crates/wyrd-spec/src/error.rs` diff,
  `crates/shared/wyrd-client/src/{lib.rs,client.rs,transport/http.rs,principals/handle.rs,platform/mod.rs,eval/*}`,
  `crates/wyrd/wyrd-server/src/{auth/revoke.rs,mcp/principals.rs}`,
  `mise.toml`, `scripts/checks/client-tier.sh`, `openapi.yaml`.

## 3. Verification limits

Commands actually run, from the repository root, with real results:

| Command | Result |
|---|---|
| `mise run codegen:check` | PASS (`All checks passed!`) |
| `mise run docs:check` | PASS (60 pages, a11y AA) |
| `mise run check:client-tier` | PASS |
| `mise run check:sdk-client-tier` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme)'` | PASS (1 passed, 384 skipped) |
| `mise run test:cli:journey` (sets `WYRD_CLI_E2E=1`) | PASS — 22 passed, 5 ignored; includes `principal_journey::principal_revoke_cli_journey`, `..._refuses_an_unprivileged_caller`, `eval_server_protocol::server_protocol_carries_lease_after_open` |
| `grep -rn "reqwest::Client" crates/wyrd/wyrd-cli/src/` | no matches |

Selectors confirmed with `mise exec -- cargo nextest list --locked -p wyrd-server --lib`.

Limits:

- **`check:client-tier` and `check:sdk-client-tier` do not cover the CLI files
  this task changed.** `scripts/checks/client-tier.sh` inspects the dependency
  cones of `wyrd-spec`, `wyrd-auth-verify`, `wyrd-client`, `wyrd-mcp`, and the
  Skald crates; `check:sdk-client-tier` inspects only `wyrd-sdk-rust`. Neither
  looks at `crates/wyrd/wyrd-cli` and neither can observe a constructed HTTP
  client. The implementation evidence cites `check:client-tier` as proof that no
  `reqwest::Client` exists in `wyrd-cli/src`; that attribution is unsupported.
  The property itself holds today (grep above), and it is unprotected going
  forward. I am recording this as a limit rather than a finding because the task
  requires no check.
- I did not run the platform journey (`platform_admin_e2e`); it is another
  reviewer's boundary and the lane is independently broken per the subject file.
- I did not run `mise run lints`; the dead-variant question in DC-3 was settled
  by reading, and `dead_code` does not fire on public enum variants anyway.
- `docs/.svelte-kit/output/**` is committed build output that still contains
  vendored source strings mentioning `bootstrap-key`. I treated it as generated
  noise, not a documentation surface, and excluded it. If the repository
  considers it authored content, that judgement changes.

## 4. Findings

### DC-1 — INCORRECT — administrative paths publish no stable error codes and no problem body

**Obligation.** AC-014: "the generated OpenAPI document declares every
administrative path, its typed bodies, **its stable error codes**, and the
authentication scheme those paths require." REQ-036: implementable from the
artifact alone.

**Location.** `openapi.yaml:1062`, `1092`, `1122`, `1168-1172`, `1201-1205`
(tenant principal surface) and `openapi.yaml:296-300`, `325-327`, `247-249`,
`155-157` (platform surface). Source of the omission: the `responses(...)` lists
in `crates/wyrd/wyrd-server/src/components/principals/routes.rs:205-215`,
`286-296`, `322-331`, `402-412`, `crates/wyrd/wyrd-server/src/auth/revoke.rs:32-38`,
`crates/wyrd/wyrd-server/src/components/platform/routes.rs:68-72`.

**Evidence.** Every refusal on every administrative operation is published as a
bare description string with no `content` and no `code`:

```yaml
        '401':
          description: Authentication required
        '403':
          description: Tenant principal administration required
```

`awk 'NR>=1039 && NR<=1206' openapi.yaml | grep -c "problem+json"` → `0`. The
Bifrost and Card operations in the same document *do* publish
`application/problem+json` with `$ref: '#/components/schemas/WyrdProblem'`, and
`openapi::tests::bifrost_operations_publish_typed_problem_refusals` pins that for
them — so the document is internally inconsistent about how a refusal is
described, and the newly added administrative half is the weaker one.

**Observable consequence.** An independent client implementing credential
creation or principal revocation from the artifact cannot learn that refusals
arrive as `application/problem+json`, and cannot learn a single stable
`WYRD_*` code to branch on. It must guess from the status code, which is exactly
the credential-agnostic string matching `WyrdError` exists to remove. The
authentication scheme was published while the other half of the same acceptance
criterion was left unpublished.

**Testable correction.** Add the common refusals to each administrative
`#[utoipa::path]` as `WyrdProblem` `application/problem+json` responses, the way
the Bifrost routes already do, and regenerate. Extend
`openapi::tests::bifrost_operations_publish_typed_problem_refusals` (or add a
sibling) to cover `/v1/principals*`, `/v1/principals/{principal_id}/revoke`,
`/platform/tenants`, `/platform/tenants/admin/credentials`, `/platform/admins*`,
and `/platform/oidc/connection`. The test fails before the change and passes
after.

### DC-2 — MISSING — the artifact declares the scheme but not how to obtain the credential it requires

**Obligation.** REQ-036: "The generated contract MUST declare the authentication
scheme its administrative paths require, so an independent client can implement
them **from the artifact alone**." AC-014: "declares **every** administrative
path".

**Location.** `openapi.yaml` publishes 25 paths (`grep -n "^  /" openapi.yaml`);
the `paths(...)` list in
`crates/wyrd/wyrd-server/src/http/openapi.rs:79-114` omits every route in
`crates/wyrd/wyrd-server/src/components/auth/routes.rs:42-47` and
`crates/wyrd/wyrd-server/src/components/admin/routes.rs:57-70`.

**Evidence.** Unpublished served routes:

- `POST /auth/token`, `GET /auth/login`, `GET /auth/callback` — the tenant
  plane's only credential-to-access-token exchange.
- `POST /auth/issue-key` — issues a card-bound credential, plaintext once;
  called by `wyrd auth issue-key` (`crates/wyrd/wyrd-cli/src/auth/issue_key.rs:65-72`).
- `/v1/admin/trusted-issuers` and `/v1/admin/workload-bindings` (POST/GET/DELETE)
  — tenant administrative CRUD, called by
  `crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:16,132-202` and
  `auth/workload_binding.rs:103-168`.

The platform plane's equivalent exchange *is* published
(`/auth/platform/token`, `openapi.yaml:68`), so the omission is an asymmetry
within the same document, not a uniform scoping decision.

**Observable consequence.** The contract now says every administrative path
needs `X-Wyrd-Access-Token: Bearer <token>` and gives a tenant-plane client no
documented way to obtain that token. Two administrative surfaces the Wyrd CLI
itself calls are absent entirely. REQ-036's stated premise — the reason item 4
was worth doing — is still unmet for the tenant plane.

**Testable correction.** Annotate the four `/auth/*` handlers and the six
`/v1/admin/*` handlers with `#[utoipa::path]`, add them to
`WyrdApiDoc`'s `paths(...)`, list the three anonymous ones in `ANONYMOUS_PATHS`,
and regenerate. Then strengthen
`every_authenticated_path_declares_the_one_wyrd_scheme` into a non-tautological
check: assert the published path set equals an explicitly enumerated expected
set, so a served-but-unpublished administrative route fails the test rather than
being invisible to it.

### DC-3 — MISSING — the unreachable per-command CLI error variants were not deleted

**Obligation.** Task acceptance criterion: "Unreachable per-command CLI error
variants are deleted, not left in place." Task Scenario 3 REFACTOR names
`WyrdCliError::RevokeFailed` explicitly. Implementation Evidence claims
`RevokeFailed` was removed.

**Location.** `crates/wyrd/wyrd-cli/src/error.rs:227` (`AuthFailed`), `:242`
(`RevokeFailed`), `:257` (`AdminFailed`), `:272` (`IssueKeyFailed`).

**Evidence.** All four are still declared, each with a registered public error
code — `WYRD_CLI_401_AUTH_FAILED`, `WYRD_CLI_500_REVOKE_FAILED`,
`WYRD_CLI_500_ADMIN_FAILED`, `WYRD_CLI_500_ISSUE_KEY_FAILED` — and none is
constructed anywhere: `grep -rn "RevokeFailed\|AuthFailed\|AdminFailed\|IssueKeyFailed" crates/wyrd/wyrd-cli/src/`
returns matches only inside `error.rs` itself. Their fields (`status: u16`,
`detail: String`) are the shape of the per-command status-code mapping the move
to `wyrd-client` replaced. The evidence table's proof — `mise run lints`
"dead-code clean" — cannot see this: `dead_code` does not fire on variants of a
public enum.

**Observable consequence.** The CLI's derive-backed error catalog publishes four
stable codes that no code path can emit, including one whose remediation text
("Check the principal ID, kind, access token, and server connectivity") contradicts
the real behavior, where a revoke failure now surfaces as
`WyrdCliError::Server { source: WyrdError }`. An agent or operator handling
`WYRD_CLI_500_REVOKE_FAILED` is handling a code that will never arrive; a future
contributor reading `error.rs` sees a per-command mapping layer that REQ-047
deleted.

**Testable correction.** Delete the four variants and any now-unused imports.
`mise exec -- cargo clippy --locked -p wyrd-cli --all-targets` must stay clean,
and `mise run test:cli:journey` must still pass.

### DC-4 — INCORRECT — contract, server, and shared client disagree on the revoke request body

**Obligation.** REQ-036 (contract implementable from the artifact alone; no
surface introduces a second model), REQ-047 (the shared client is the one
projection of the contract), AC-014.

**Location.** Contract: `openapi.yaml:1174-1206` — `post` with `parameters` only
and **no `requestBody`**. Server:
`crates/wyrd/wyrd-server/src/auth/revoke.rs:41-44` — `(State, Caller, Path)`, no
`Json` extractor, so any body is discarded. Client:
`crates/shared/wyrd-client/src/principals/handle.rs:133-145` — requires
`&RevokePrincipalRequest` and sends it. CLI:
`crates/wyrd/wyrd-cli/src/principal/revoke.rs:37-42, 57-66` — `--kind` and
`--reason` are both **required** flags feeding that body.

**Evidence.** Three surfaces, three different contracts for one operation. The
recorded "Deliberately out of scope" note states that the route "ignores the
`RevokePrincipalRequest` body **that the contract declares**"; the generated
contract declares no body at all, so the recorded contract fact is wrong and the
deferral rests on it.

**Observable consequence.** An operator runs
`wyrd principal revoke <id> --kind service --reason "compromised"`, is *required*
to supply a kind and a reason, and neither reaches the server or the audit row —
`revoke_principal` audits `"revoke principals"` with no reason and derives the
kind itself. The CLI's help text ("Audit reason for the revocation") is false. An
independent client built from the artifact sends no body, which works, so the
only surface behaving inconsistently with the published contract is Wyrd's own
sole client.

**Testable correction.** Pick one contract and make all three agree. The
smallest version consistent with the server: drop the `RevokePrincipalRequest`
parameter from `Principals::revoke_principal` and drop `--kind`/`--reason` from
the CLI command, with `principal_journey::principal_revoke_cli_journey` updated
to the argument list an operator actually needs. If the reason must be audited
instead, that is the public contract change the note describes and needs the
body declared in the contract, extracted by the handler, and asserted in the
audit row.

### DC-5 — MISSING — no operator or agent surface creates a credential, and no lane proves one

**Obligation.** Task acceptance criterion: "The CLI performs the administrative
operations against a real server, **including receiving a once-returned
credential and using it on a subsequent call**, through a lane that actually
runs." AC-014: "The CLI and MCP exercise those operations against a real
server."

**Location.** `crates/wyrd/wyrd-cli/src/cli.rs:56-84` — the whole command tree;
`Principal` has one subcommand, `revoke`
(`crates/wyrd/wyrd-cli/src/principal/mod.rs`). MCP:
`crates/wyrd/wyrd-server/src/mcp/principals.rs:43-56` — `list_credentials` read,
`revoke_credential` write.

**Evidence.** Coverage of the administrative surface across the two surfaces the
revision-7 spec designates as *the* administrative clients:

| Operation | CLI | MCP |
|---|---|---|
| `POST /v1/principals` (create service principal) | — | — |
| `POST /v1/principals/{id}/credentials` (issue) | — | — |
| `GET /v1/principals/{id}/credentials` | — | read tool |
| `DELETE .../credentials/{cid}` | — | write tool |
| `POST /v1/principals/{id}/revoke` | `principal revoke` | — |
| Whole platform plane (token, tenants, recovery, OIDC, admins) | — | — |

No test anywhere receives a once-returned credential through the CLI and uses it
on a later call: `grep -rn "issue-key\|issue_key" crates/wyrd/wyrd-cli/tests/`
returns nothing, and `grep -rn '"auth"' crates/wyrd/wyrd-cli/tests/` returns
nothing — the "pre-existing `auth` journeys" the evidence cites for this
criterion do not exist. `auth trusted-issuer`'s only coverage is a `wiremock`
unit test (`auth/trusted_issuer.rs:613-616`), not a real server.

**Observable consequence.** The credential-issuance path — the one operation
whose whole design point is that plaintext crosses a client surface exactly once
— is reachable from no Wyrd-owned operator or agent surface and is proven by no
journey. That also falsifies the premise revision 7 used to drop the Python and
TypeScript bindings ("The Python package already reaches every administrative
operation through `wyrd.cli.run_wyrd_cli`"): the in-process CLI reaches neither
principal creation nor credential issuance nor any platform operation.

**Testable correction.** Either add the missing CLI commands (`principal create`,
`credential issue`, `credential list`, `credential revoke`) over the existing
`wyrd_client::Principals` handle and extend `principal_journey.rs` to create a
principal, capture the once-returned credential from stdout, and authenticate a
subsequent call with it — or escalate to specification authority to narrow this
acceptance criterion, because it cannot be discharged by the surfaces that
exist. Claiming PASS on evidence that does not exist is the part that must not
stand.

### DC-6 — INCORRECT — a repository task still invokes the removed bootstrap command

**Obligation.** Task acceptance criterion: "No document, example, or surface
still describes a second identity model, **a removed bootstrap path**, or
credential-keyed authorization." REQ-038 / REQ-040 / AC-013.

**Location.** `mise.toml:1081-1083`.

**Evidence.**

```toml
[tasks."cli:dev-bootstrap"]
description = "Seed the local Wyrd tenant and service credentials"
run = "cargo run --locked -p wyrd-cli -- dev bootstrap"
```

`wyrd dev bootstrap` does not exist: `crates/wyrd/wyrd-cli/src/cli.rs:56-84`
declares no `Dev` variant, and
`crates/wyrd/wyrd-cli/src/lib.rs:104-107` pins its removal —
`removed_dev_bootstrap_returns_usage_error` asserts exit code `64`. The task was
added on this branch (commit `b36ad6e12`) as the replacement for the deleted
`cli:bootstrap-key`, and `mise.toml` is in TASK-008's expected write set. The
sibling `cli:init` is correct (`wyrd-server` does have an `Init` subcommand,
`crates/wyrd/wyrd-server/src/main.rs:42`).

**Observable consequence.** The one seeding task an operator or contributor
reaches for after `bootstrap-key` was removed exits with a clap usage error. A
surface describing a removed bootstrap path is exactly what this acceptance
criterion forbids, and the criterion was recorded PASS.

**Testable correction.** Point `cli:dev-bootstrap` at the command that actually
seeds a local deployment, or delete the task if `cli:init` plus tenant
provisioning replaces it. Either way the operator path must be executable; a
`mise run <task>` that exits 64 is not a documented journey.

### DC-7 — DRIFT — the Rust SDK root advertises and pins the administrative handles, and its completeness claim is now stale

**Obligation.** Revision 7 / TASK-008 "Scope removed at revision 7": "**The Rust
SDK is untouched.**" AC-014: the Rust SDK's re-export "carries no separate
administrative surface of its own." AGENTS.md §16: documentation is part of
implementation correctness.

**Location.** `sdks/wyrd-sdk-rust/src/lib.rs:4-5` (module doc) and `:23-24`
(unit test).

**Evidence.** The branch modified the SDK root to name `principals::Principals`
and `platform::Platform` as SDK-projected capabilities and to pin both in
`sdk_root_projects_the_composed_client_capabilities`, whose own rustdoc claims it
names "**every** composed client capability from `wyrd-client`". The closeout
then added a new composed capability, `eval::EvalProtocol`
(`crates/shared/wyrd-client/src/lib.rs:18,24`), and did not add it to that list —
so the test's stated invariant is false at HEAD while still passing.

**Observable consequence.** The package the spec describes as carrying no
administrative surface of its own now documents one and asserts it in a test,
while the list that claims completeness is incomplete. A maintainer adding the
next `wyrd-client` capability has no failing signal, and revision 7's statement
that the SDK is untouched does not match the tree.

**Testable correction.** Decide which it is. If the SDK root is to enumerate
capabilities, add `eval::EvalProtocol` and make the test's completeness claim
enforceable (iterate a declared list, or drop the word "every"). If revision 7's
"untouched" is authoritative, revert the module-doc and test additions from
`4225069`/`7ecd356` so the SDK is the wholesale re-export the spec describes.

## 5. Result

**FAIL**

DC-1, DC-2, and DC-5 leave acceptance obligations this task owns unmet
(AC-014's error codes and path completeness; the CLI credential journey). DC-3
and DC-6 are required deletions recorded as done that were not done. DC-4 is a
contract fact recorded incorrectly, and the deferral rests on it. The parts of
the boundary that are sound — regeneration is genuine, the scheme is truthfully
attached, the CLI owns no transport, `wyrd-client` gained only the minimum, no
Python/TypeScript administrative binding exists, no compatibility surface was
added, and the new tests really run — are recorded in §1 and are not in dispute.
