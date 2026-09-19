# TASK-008 — review verdict (`task-008-r2`)

## Verdict

`FIX_REQUIRED`

Seven validated findings, `FIND-008-8` through `FIND-008-14`. All are bounded
implementation defects inside the approved behavior: four deletions or prose
corrections, one rustdoc obligation, one credential-disclosure regression, and
one missing journey in an existing lane. None requires a new product, public
API, architecture, security, compatibility, cross-service, concurrency, or
persistent-data decision, so `SPEC_REVISION_REQUIRED` does not apply.

## Immutable subject

| Item | Value |
|---|---|
| Repository root | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` (git worktree) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `4668d8d333ad043b4b2f8c258beb78eebb719466` |
| Branch base (fork point) | `968c92641` |
| Closeout range reviewed | `289978fcc~1..4668d8d33` — `289978fcc`, `3a71e0409`, `40daeee97`, `73aaa0616`, `074a40e87`, `343cda711`, `21abea8c7`, `fac7888dc`, `4668d8d33`; cumulative branch range `968c92641..4668d8d33` (163 files) reviewed as context |
| Approved spec | `changes/active/admin-principals/spec.md` revision 7, status `approved` |
| Task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Authority | `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` (lines 495-507), `architecture/references/` router |
| Verification scope | `VER-001`..`VER-006` |
| Working tree | clean at review start and end apart from this review directory; candidate did not change during either wave (`git rev-parse HEAD` re-checked between waves) |
| Prior review | `changes/active/admin-principals/review/task-007-008/` (spec revision 6; finding IDs `FIND-008-1`..`FIND-008-7`) |

## Review topology

Executed as the skill requires: two waves of fresh independent subagents, the
orchestrator reviewing nothing itself.

| Wave | Role | Result | Report |
|---|---|---|---|
| 1 | `task-rev` — task acceptance | `FAIL` | `task-review.md` |
| 1 | `repo-rev` — repository standards | `FAIL` | `standards-review.md` |
| 1 | `domain-rev` — security / identity / trust boundary | `FAIL` | `domain-review-security.md` |
| 1 | `domain-rev` — public contract / client boundary | `FAIL` | `domain-review-contract.md` |
| 2 | `ponytail-rev` — validation | ledger non-empty (7 retained, 8 rejected) | `findings-validation.md` |

Wave 1 reviewers received the immutable subject and their own scope only — no
peer conclusions and no intended verdict. `ponytail-rev` received the full diff,
the authorities, and all four Wave 1 reports, and no intended verdict.

## Acceptance matrix

Consolidated from `task-review.md` and the validated ledger. `FAIL` rows name
the finding that falsifies them.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No Wyrd-owned surface reads the `Authorization` header; a tree-wide search returns nothing | `components/auth/platform_extractor.rs` binds `token_extract::WYRD_ACCESS_TOKEN_HEADER` (`289978fcc`) — but the implementor's grep never left `crates/` | `docs/src/content/docs/for-agents/workflow.svx:28,59,81` | **FAIL** — `FIND-008-9` |
| A platform session token authenticates on `X-Wyrd-Access-Token`; a tenant token on a platform route is still refused by the scope marker | `platform_extractor.rs`; `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check unchanged | `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other`, 17/17 PASS via the Postgres wrapper | PASS |
| A platform session on a tenant route is unauthenticated | `343cda711`, at the shared `token_extract` owner | `wyrd-server --lib` auth module units, 6/6 | PASS |
| An application's own `Authorization` header is carried through untouched and never consulted | `platform_extractor.rs` both-headers and Authorization-only unit tests | `test(/components::auth::platform_extractor::tests::/)` PASS | PASS |
| Signing-key resolution exists once and both internal verify paths call it; `verify_external_against` untouched | one private `TokenVerifier` resolver, `crates/shared/wyrd-auth-verify/src/lib.rs` | `wyrd-auth-verify --lib`, 41/41 PASS; refactor confirmed behavior-preserving by `domain-rev-security` | PASS |
| No `reqwest::Client` constructed anywhere in `crates/wyrd/wyrd-cli/src` | single construction point `crates/wyrd/wyrd-cli/src/client.rs` | `grep -rn 'reqwest::Client' crates/wyrd/wyrd-cli/src/` → no matches; `check:client-tier` exit 0 | PASS |
| Every CLI command that calls a Wyrd route authenticates on the canonical header, proven by a test that would have caught the original defect | `client.rs` + `card.rs`, `query/mod.rs`, `principal/`, `auth/`, `eval/` (`3a71e0409`) | `crates/wyrd/wyrd-cli/tests/principal_journey.rs` runs in `test:cli:journey` (22 passed, `WYRD_CLI_E2E=1`) | PASS |
| `eval/*` moved onto the shared client or removed, reason recorded; no hand-rolled client left behind | `wyrd_client::eval::{EvalProtocol, EvalRun}`; `eval/agent.rs` on the pre-existing `request_external_stream` for its third-party endpoint | `eval_server_protocol::server_protocol_carries_lease_after_open`; `request_external_stream` confirmed pre-existing and Wyrd-credential-free | PASS |
| Unreachable per-command CLI error variants are deleted, not left in place | none — `AuthFailed`, `RevokeFailed`, `AdminFailed`, `IssueKeyFailed` all remain | `crates/wyrd/wyrd-cli/src/error.rs:227,242,257,272`; tree-wide grep finds no constructor | **FAIL** — `FIND-008-8` |
| The generated contract declares the authentication scheme and regenerates cleanly with no hand edits | `SecurityAddon` in `crates/wyrd/wyrd-server/src/http/openapi.rs`; regenerated `openapi.yaml` | `codegen:check` exit 0, tree clean after regeneration; `openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme` | PASS |
| The CLI performs the administrative operations against a real server, **including receiving a once-returned credential and using it on a subsequent call**, through a lane that actually runs | `principal_journey.rs` proves revoke; the cited "pre-existing `auth` journeys" do not exist | `grep -rn 'issue-key\|issue_key' crates/wyrd/wyrd-cli/tests/` → nothing | **FAIL** — `FIND-008-13` |
| A tenant-scope caller cannot invoke a platform-plane operation through the CLI or an MCP tool; the refusal is the stable contract error | structural — MCP catalog exposes no platform tool, CLI declares no platform command (both enumerated at HEAD) | `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other`; `principal_revoke_cli_journey_refuses_an_unprivileged_caller` asserts `WYRD_PERMISSION_403_DENIED_RBAC` | PASS |
| No Python or TypeScript administrative binding exists, and no unrun administrative journey remains | nothing added under `sdks/`; the two `#[ignore]`d Rust SDK journeys stay deleted | `check:sdk-client-tier`, `check:pyo3-scope`, `codegen:check` (no stub drift) | PASS |
| MCP administrative write tools unavailable without their scope; read tools always available | unchanged from `128eb40` | MCP tool-catalog tests unchanged and passing | PASS |
| Platform revocation behavior unchanged; rustdoc states the caches-nothing invariant | `crates/wyrd/wyrd-auth/src/platform_credentials.rs` | `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` | PASS |
| No revocation epoch added to the platform plane (**stop condition**) | no epoch, cache, or memoization introduced on the platform path | confirmed by `domain-rev-security` tracing `confirm_credential_session`, the federated path, and `resolve_grant` | PASS |
| No document, example, or surface still describes a second identity model, a removed bootstrap path, or credential-keyed authorization | — | `docs/.../for-agents/workflow.svx:28,59,81`; `mise.toml:1081` runs the removed `wyrd dev bootstrap` | **FAIL** — `FIND-008-9`, `FIND-008-14` |
| Credential plaintext crosses a client surface exactly once and is never logged, traced, or placed in an error payload (task invariant; spec `INV-002`) | `login.rs:101` now carries the operator's pasted OIDC callback — live authorization code included — into `InvalidArgument`, printed twice on stderr | `crates/wyrd/wyrd-cli/src/auth/login.rs:99-103`; pre-change arm was value-free | **FAIL** — `FIND-008-11` |
| Served error prose names the header a caller actually sends | `crates/wyrd-spec/src/error.rs` remediations corrected (`21abea8c7`) — but both live `BadTokenFormat` mappers still say `authorization` | `crates/wyrd/wyrd-server/src/http/error.rs:202`; `crates/wyrd/wyrd-auth/src/error.rs:35` | **FAIL** — `FIND-008-10` |
| Every new or materially modified Rust item has rustdoc with `# Errors` (AGENTS.md §16, hard blocker) | most touched symbols comply | `crates/wyrd/wyrd-cli/src/auth/login.rs:78`; `crates/wyrd/wyrd-server/src/components/eval/routes.rs:255` — the two functions whose error behavior this diff changed | **FAIL** — `FIND-008-12` |
| Platform plane not routed through `AuthenticatedPrincipal`; no nullable tenant on `VerifiedToken`/`Principal` (**prohibited change**) | two extractors and two principal stores retained | traced by `domain-rev-security` | PASS |
| Indistinguishable platform rejection not weakened (**prohibited change**) | `platform_extractor.rs` keeps its own rejection rather than `extract_wyrd_access_token`'s informative one | unit tests; no new distinguishable observable | PASS |
| No compatibility route, alias, or transitional flag (**prohibited change**) | none in the diff | grep over the closeout range | PASS |
| Generated stubs, schemas, and `openapi.yaml` not hand-edited (**prohibited change**) | generator changed, artifacts regenerated | `codegen:check` exit 0 with a clean tree | PASS |
| Non-goals held: no UI, no platform operation exposed to tenant-scope clients, no widening beyond the named callers | no UI file changed; no Python or TypeScript file changed | diff inspection | PASS |
| Verification scope `VER-001`..`VER-006` | — | `fmt`, `lints`, `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `codegen:check`, `docs:check` all exit 0; focused nextest expressions and both journey lanes pass | PASS, but see verification limits |

## Validated finding ledger

Numbering continues from the prior review's `FIND-008-1`..`FIND-008-7`; none of
these seven is the same defect as a prior ID. Full evidence and
decision-complete corrections are in `findings-validation.md`.

| ID | Wave 1 sources | Status | Class | Location | Violated obligation |
|---|---|---|---|---|---|
| `FIND-008-8` | `TR-1`, `RR-1`, `DC-3` | CONFIRMED | MISSING | `crates/wyrd/wyrd-cli/src/error.rs:227,242,257,272` | AC "unreachable per-command CLI error variants are deleted"; AGENTS.md §15 |
| `FIND-008-9` | `TR-3`, `RR-3`, `DS-1` | REVISED | INCORRECT | `docs/src/content/docs/for-agents/workflow.svx:28,59,81` | `INV-015`, `REQ-040`, `AC-013`; AC "a tree-wide search returns nothing" |
| `FIND-008-10` | `TR-4`, `RR-4`, `DS-2` | REVISED | INCORRECT | `crates/wyrd/wyrd-server/src/http/error.rs:202`; `crates/wyrd/wyrd-auth/src/error.rs:35`; fixture `crates/wyrd-spec/src/error.rs:4168` | `INV-015`; AGENTS.md §9 |
| `FIND-008-11` | `DS-3` | CONFIRMED | REGRESSION | `crates/wyrd/wyrd-cli/src/auth/login.rs:99-103` | task invariant on single-crossing credential plaintext; spec `INV-002` |
| `FIND-008-12` | `RR-5` | CONFIRMED | VIOLATION | `crates/wyrd/wyrd-cli/src/auth/login.rs:78`; `crates/wyrd/wyrd-server/src/components/eval/routes.rs:255` | AGENTS.md §16; `agent-rules.md` `BLOCK_BEFORE_MERGE` |
| `FIND-008-13` | `TR-2`, `DC-5` | CONFIRMED | MISSING | `crates/wyrd/wyrd-cli/tests/cli.rs:1-13` | AC "including receiving a once-returned credential and using it on a subsequent call"; `AC-014` |
| `FIND-008-14` | `DC-6` | REVISED | INCORRECT | `mise.toml:1081-1083` | AC "no surface still describes a removed bootstrap path"; `REQ-038` |

### Rejected proposals — omitted, not softened

| Wave 1 ID | Reason for rejection |
|---|---|
| `RR-2` | The two `tenant_from_unverified_access_token` sites were never one shared owner, and the disclosure alleged is a body field, not a header. |
| `RR-6` | Measured both ways: the pre-existing `card_lifecycle::pg_tests` gate identically, so the proposed change alters nothing the repository does not already do. |
| `RR-7` | Rust intra-doc links in generated output are an established repository pattern (13 other sites); zero consequence for a contract consumer. |
| `RR-8` | A `pub(crate)` const with no outside consumer is a visibility preference with no observable consequence. |
| `DC-1` | Absent error bodies are a document-wide pre-existing shape — Cards paths lack them too; not a defect this task introduced or took on. |
| `DC-2` | Publishing additional pre-existing non-spec routes is a contract addition beyond item 4's obligation (one `securitySchemes` entry attached to every authenticated path). |
| `DC-4` | The three-way revoke-body disagreement predates the branch base and needs a public-contract decision the task explicitly declined; out of scope rather than remediation. |
| `DC-7` | Test-only rustdoc in `sdks/wyrd-sdk-rust`, which the task forbids touching. |

`DC-3` and `TR-1` merged into `FIND-008-8`; `DC-5` merged into `FIND-008-13`.

## Verification limits

- The implementor's verification reproduces. Every narrow lane run during this
  review exits 0: `fmt`, `lints`, `check:client-tier`, `check:sdk-client-tier`,
  `check:pyo3-scope`, `check:unwrap-audit`, `codegen:check`, `docs:check`, and
  a five-package `cargo clippy --all-targets`. Focused nextest expressions pass:
  `wyrd-server --lib` platform-extractor units 6/6, `wyrd-auth-verify --lib`
  41/41, `platform_admin_e2e` 17/17, `test:cli:journey` 22 passed with
  `WYRD_CLI_E2E=1`.
- **Green checks did not catch five of the seven findings.** `mise run lints`
  cannot see `FIND-008-8` (`dead_code` does not fire on variants of a `pub`
  enum); `docs:check` does not assert which header an example sends
  (`FIND-008-9`); no test asserts the served `detail` string (`FIND-008-10`);
  no test asserts what `login`'s error does not contain (`FIND-008-11`); and
  `clippy::missing_errors_doc` is public-API-only (`FIND-008-12`). Treat the
  green aggregate as necessary, not sufficient.
- `mise run test:platform:journey` is broken independently of this task (it
  depends on a `setup:postgres` task that no longer exists). The platform suite
  was run through `scripts/postgres/with-test-postgres.sh` instead. Repairing
  that lane is outside this task's write set, and this review did not treat the
  broken lane as a finding.
- No broad aggregate (`gate`, `test:rust`, whole-crate or family lanes, storage
  matrix, any `--all-features` workspace lane) was accepted as evidence.
- Timing-channel differences in the platform plane's indistinguishable rejection
  were reasoned about from source, not measured.

## Prior-finding closure

Prior review `task-007-008` (spec revision 6) recorded `FIND-008-1`..`FIND-008-7`.
Their disposition at this candidate:

| Prior ID | Disposition |
|---|---|
| `FIND-008-1`, `FIND-008-2` | Closed by specification: revision 7 removed the Python and TypeScript administrative bindings from scope and forbids adding them. Verified absent. |
| `FIND-008-3` | Closed — MCP credential observation and revocation landed with write-scope gating (`128eb40`); catalog tests pass. |
| `FIND-008-4` | Closed — `wyrd_client::Platform` / `PlatformSession` exist (`7ecd356`), and the cross-plane refusal is proven by `platform_admin_e2e`. |
| `FIND-008-5` | Closed — administrative paths are published in the generated contract (`384e167`) and `codegen:check` is no longer vacuous. |
| `FIND-008-6` | Closed in tree for the code path; residue remains as `FIND-008-14` (a `mise` task still invoking a removed bootstrap command). |
| `FIND-008-7` | Closed — REQ-040 documentation landed; `docs:check` passes. One page contradicts the canonical header, carried forward as `FIND-008-9`. |

## Disclosed policy item — not a TASK-008 finding

All nine closeout commits are authored and committed by
`Thorrester <sjforrester32@gmail.com>`, which AGENTS.md §13 requires. Each also
carries a `Co-Authored-By: Claude` trailer. AGENTS.md §13 forbids AI co-author
trailers and takes precedence over the harness's attribution instruction, so the
trailers contravene repository policy. It is **not** recorded as a finding: the
trailers are present on commits predating the branch base, they are outside the
task's write set, and the only remedy is rewriting history. Surfaced for the
change owner to decide.

## Remediation

`changes/active/admin-principals/review/task-008-r2/TASK-008-R2-close-header-prose-and-credential-gaps.md`

Route it to a fresh `$wyrd-implement` agent. A later review reassesses the
complete cumulative candidate against the original task.
