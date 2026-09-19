# TASK-008 — review verdict (`task-008-r3`, remediation round)

## Verdict

`FIX_REQUIRED`

Two validated findings, `FIND-008-15` and `FIND-008-16`, both in one file and
both closable by one edit. Every prior finding (`FIND-008-8`..`FIND-008-14`) is
closed. `SPEC_REVISION_REQUIRED` does **not** apply — the escalation the
implementor raised was ruled on and rejected on evidence.

## Immutable subject

| Item | Value |
|---|---|
| Repository root | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` (git worktree) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `f102e50eea437ff4ba29571412197a1c4923cbbc` |
| Previously reviewed candidate | `4668d8d33` (verdict `FIX_REQUIRED`, review `task-008-r2`) |
| Remediation commits | `9fe02aa2e`, `67c9d2df6`, `f102e50ee` |
| Cumulative closeout range reviewed | `289978fcc~1..f102e50ee`; branch base `968c92641` reviewed as context |
| Approved spec | `changes/active/admin-principals/spec.md` revision 7, status `approved` |
| Original task (the acceptance standard) | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Remediation task implemented | `changes/active/admin-principals/review/task-008-r2/TASK-008-R2-close-header-prose-and-credential-gaps.md` |
| Authority | `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` (lines 495-507), `architecture/references/` router |
| Verification scope | `VER-001`..`VER-006` |
| Working tree | clean at review start and end apart from this review directory; candidate did not change during either wave (`git rev-parse HEAD` re-checked between waves) |

## Review topology

| Wave | Role | Result | Report |
|---|---|---|---|
| 1 | `task-rev` — cumulative task acceptance and prior-finding closure | `FAIL` | `task-review.md` |
| 1 | `repo-rev` — repository standards | `FAIL` | `standards-review.md` |
| 1 | `domain-rev` — security / identity / trust boundary | `FAIL` | `domain-review-security.md` |
| 1 | `domain-rev` — persistent data / query semantics | `FAIL` | `domain-review-data.md` |
| 2 | `ponytail-rev` — validation | ledger non-empty (2 retained, 3 rejected after dedupe from 11 proposals) | `findings-validation.md` |

Wave 1 reviewers received the immutable subject and their own scope only — no peer
conclusions and no intended verdict. `ponytail-rev` received the full diffs, the
authorities, all four Wave 1 reports, and no intended verdict. The four reviewers
reached **incompatible** conclusions about the one substantive change in the
remediation, so resolving that contradiction was assigned to Wave 2 as a blocking
condition.

## Prior-finding closure — all seven closed

| Prior ID | Disposition | Evidence |
|---|---|---|
| `FIND-008-8` | CLOSED | all four variants gone from `crates/wyrd/wyrd-cli/src/error.rs`; nothing repointed, no unused import |
| `FIND-008-9` | CLOSED | `docs/src/content/docs/for-agents/workflow.svx` has no `authorization` occurrence; no new docs check added |
| `FIND-008-10` | CLOSED WITH IMMATERIAL DEVIATION | both live mappers and the `wyrd-spec` fixture name the canonical header. The prescribed selector `test(/http::error::tests::/)` does select nothing, as the implementor said; `task-rev` found the real module is `http::error::error_mapper_tests` (9 tests, run green), so the implementor's "those mappers have no tests" claim was wrong while the substituted grep proof is sound |
| `FIND-008-11` | CLOSED | `crates/wyrd/wyrd-cli/src/auth/login.rs:114-123` carries only `<missing code>` / `<missing state>` / `<missing code and state>`; no operator input reaches the error on any branch. The weakened `code=super` assertion is adequate — the variant's own `expected` guidance legitimately contains `code=<>&state=<>`, so asserting absence of bare `code=` would have forced degrading the guidance |
| `FIND-008-12` | CLOSED | `login.rs:90-93` and `crates/wyrd/wyrd-server/src/components/eval/routes.rs:248-262` both carry intent-bearing rustdoc with `# Errors` |
| `FIND-008-13` | CLOSED | `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs` registered in `tests/cli.rs`, runs in `test:cli:journey`, `PASS [3.863s]`; the CLI issues a credential and spends it on a later call |
| `FIND-008-14` | CLOSED | no `dev bootstrap` reference remains in `mise.toml` |

## The contradiction, and the ruling that decided the verdict

The remediation changed one production surface beyond the prescribed corrections:
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` moved
`service_account_by_card_ref` from `card_ref = $3` to `card_ref @> $3` with
`ORDER BY created_at, id LIMIT 1`. Three callers sit on the authentication path
(`issue_key`, `exchange_api_key`, `jwt_bearer`). The implementor asked explicitly
whether uid-insensitive principal resolution needs a spec decision rather than a
bug fix.

Wave 1 split: `domain-rev-security` ruled it safe because
`UNIQUE (data_tenant_id, name)` bounds the match; `domain-rev-data` reported
measuring two matching rows across spaces and argued the true root cause was a
*test-fixture* divergence, with "no production writer of a Card-bound `card_ref`
exists" as its load-bearing fact, escalating to `SPEC_REVISION_REQUIRED`;
`task-rev` and `repo-rev` filed the space fail-open as `INCORRECT`.

`ponytail-rev` settled it against the live schema and Postgres, and the
orchestrator independently re-verified the two facts that carry the ruling:

- **The "no production writer" claim is false.**
  `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-63` writes
  `card_ref` with `space: Some(..)` and `uid: Some(..)`, reached unconditionally
  from card registration at
  `crates/wyrd/wyrd-server/src/components/cards/service.rs:1072-1074`. Registering
  any Service or Agent Card stores a uid-bearing `card_ref`.
- **Whole-document equality could therefore never match.** `CardRef.uid` is
  `skip_serializing_if = Option::is_none` and `FromStr` never sets it, so no
  client-expressible ref could resolve a registered principal under `= $3`.
  Measured: `stored = {…,"uid":"1111…"}` against a uid-free document is `f`.

So the `@>` change repairs a real production bug on the correct shared owner, and
the ladder's competing option — fix the fixture and revert the predicate — would
have papered over that bug to suit a test. The match is bounded to one candidate
row in production, though not for the reason `domain-rev-security` gave: the
operative chain is `auth_projection.rs` keeping the `name` column equal to
`card_ref->>'name'` plus the unconditional `UNIQUE (data_tenant_id, name)`.

**Ruling:** the change is in-scope, correct, and minimal. `REQ-004` and the
`wyrd apply` non-goal govern the write side, which is untouched. The space
fail-open is real at the predicate level but not production-reachable — the CLI's
`space` is a required argument, a *wrong* space still refuses, and the route's
authorization is a tenant-wide permission with an already space-insensitive
resource label — so it is rejected as a finding and its residual risk is captured
as the rustdoc finding instead.

## Acceptance matrix

Every row the prior review passed was re-verified against the cumulative
candidate, because remediation touched `cli.rs`, a shared SQL query,
`issue_key.rs`, `login.rs`, `principal_journey.rs`, `components/eval/routes.rs`,
and `mise.toml`. Full matrix in `task-review.md`.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No Wyrd-owned surface reads `Authorization`; a tree-wide search returns nothing | `platform_extractor.rs` on `WYRD_ACCESS_TOKEN_HEADER`; `workflow.svx` corrected; both `BadTokenFormat` mappers corrected | tree-wide sweep across `crates/`, `sdks/`, `docs/`, `examples/`, `scripts/` and the generated contract | PASS |
| A platform session authenticates on `X-Wyrd-Access-Token`; a tenant token on a platform route is refused by the scope marker; a platform session on a tenant route is unauthenticated | unchanged from the prior candidate | `platform_admin_e2e` 17/17 | PASS |
| An application's own `Authorization` is carried through untouched | `platform_extractor.rs` unit tests | focused `test(/components::auth::platform_extractor::tests::/)` | PASS |
| Signing-key resolution exists once; `verify_external_against` untouched | one private `TokenVerifier` resolver | `wyrd-auth-verify --lib` | PASS |
| No `reqwest::Client` in `crates/wyrd/wyrd-cli/src` | single construction point `wyrd-cli/src/client.rs` | grep → no matches; `check:client-tier` | PASS |
| Every CLI command authenticates on the canonical header, proven by a test that would have caught the original defect | `client.rs` and every command on `wyrd-client` | `principal_journey.rs` and `auth_issue_key_journey.rs` in `test:cli:journey` | PASS |
| `eval/*` on the shared client, reason recorded; no hand-rolled client left | `wyrd_client::eval`; `eval/agent.rs` on `request_external_stream` | `eval_server_protocol` | PASS |
| Unreachable per-command CLI error variants deleted | four variants removed (`9fe02aa2e`) | `cargo clippy -p wyrd-cli --all-targets` | PASS |
| The generated contract declares the authentication scheme and regenerates cleanly | `SecurityAddon` in `http/openapi.rs`; `openapi.yaml` | `codegen:check` exit 0, clean tree | PASS |
| The CLI performs administrative operations against a real server, including receiving a once-returned credential and using it on a subsequent call, through a lane that runs | `auth_issue_key_journey.rs` (`67c9d2df6`), registered in `tests/cli.rs` | `WYRD_CLI_E2E=1 test:cli:journey`, 23 passed; tamper-sensitivity confirmed (`WYRD_AUTH_401_API_KEY_INVALID`) | PASS |
| A tenant-scope caller cannot invoke a platform-plane operation through CLI or MCP | structural, re-enumerated at HEAD | `platform_admin_e2e`; `principal_revoke_cli_journey_refuses_an_unprivileged_caller` | PASS |
| No Python or TypeScript administrative binding; no unrun administrative journey | nothing under `sdks/` changed | `check:sdk-client-tier`, `check:pyo3-scope`, `codegen:check` | PASS |
| MCP administrative writes scope-gated; reads always available | unchanged | MCP catalog tests | PASS |
| Platform revocation unchanged; rustdoc states the caches-nothing invariant; no epoch added | unchanged | `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` | PASS |
| Credential plaintext crosses a client surface exactly once and never reaches a log, trace, or error payload | `login.rs:114-123`; `auth_issue_key_journey.rs` | focused `test(/auth::login::tests::/)` | PASS |
| No document, example, or surface describes a second identity model, a removed bootstrap path, or credential-keyed authorization | `workflow.svx`; `cli:dev-bootstrap` deleted | `docs:check`; greps empty | PASS |
| Every new or materially modified Rust item has rustdoc with `# Errors` | most items comply | `service_accounts.rs:162-165` documents an invariant the schema does not provide and omits the one it relies on | **FAIL** — `FIND-008-16` |
| Format, lints, and the targeted tests for the touched surface pass (AGENTS.md §12) | — | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431` asserts `card_ref = $3` against the shipped `@> $3`; `-p wyrd-sql --lib` is red | **FAIL** — `FIND-008-15` |
| Prohibited changes held: no `AuthenticatedPrincipal` on the platform plane, no nullable tenant, indistinguishable rejection intact, no compatibility route or alias, no hand-edited generated artifacts | traced | `codegen:check`; source inspection | PASS |
| Non-goals held: no UI, no platform operation exposed to tenant scope, no widening beyond the named callers | no UI or Python or TypeScript file changed | diff inspection | PASS |
| Remediation-task constraints: no new dependency, abstraction, trait, helper type, or test harness; explicit non-goals untouched | the clap fix, harness generalization, and `--kind` help correction were judged earned and in scope by all four reviewers; the SQL change was ruled in-scope root-cause repair | see the ruling above | PASS |
| Tenant isolation preserved on every path, including the changed query | tenant predicate plus `TenantConn`'s `app.current_tenant` under `FORCE ROW LEVEL SECURITY` intact on all three callers | traced by both domain reviewers | PASS |

## Validated finding ledger

| ID | Wave 1 sources | Status | Class | Location | Violated obligation |
|---|---|---|---|---|---|
| `FIND-008-15` | `TR3-1`, `RR3-1`, `DS3-1`, `DD3-1` | CONFIRMED | REGRESSION | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431` | AGENTS.md §12 (targeted tests for the touched surface pass), §11 (run the owning crate's lane); `agent-rules.md` `BLOCK_BEFORE_MERGE` |
| `FIND-008-16` | `DS3-2`, `DD3-3` | REVISED | DRIFT | `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:162-165` | AGENTS.md §16 (rustdoc states relevant invariants), §12 (documentation is part of implementation correctness) |

### Rejected proposals — omitted, not softened

| Wave 1 ID | Reason for rejection |
|---|---|
| `TR3-2`, `RR3-2`, `DD3-2` | The space fail-open is real in the predicate and not production-reachable: `IssueKeyArgs.space` is required, `CardRef::FromStr` always sets `space`, a *wrong* space still refuses, at most one candidate row per tenant can exist, and the route's authorization is a tenant-wide permission with an already space-insensitive resource label. The measured two-row state requires a test-only writer. The residual risk is retained as `FIND-008-16`. |
| `DD3-4` | Coverage exists at the tier the repository ranks highest: `auth_issue_key_journey.rs` pins the new matching semantics end to end and cannot pass under the old predicate. An additional SQL-tier pin is coverage the task never asked for. |
| `DD3-5` | Same defect as `DD3-2`, and its load-bearing premise ("no production writer of a Card-bound `card_ref`") is false. Its recommended correction would revert a real production fix to accommodate a test fixture. `REQ-004` and the `wyrd apply` non-goal govern the untouched write side. No spec revision required. |

## Verification limits

- No broad aggregate was accepted as evidence — no `mise run gate`, `test:rust`,
  whole-crate or family lane, storage matrix, or `--all-features` workspace lane.
  Every selector was confirmed with `cargo nextest list`; no positional filter was
  used.
- Lanes run green across the waves: `fmt`, `lints`, `check:client-tier`,
  `check:sdk-client-tier`, `check:pyo3-scope`, `check:unwrap-audit`,
  `check:clippy-allow-audit`, `codegen:check`, `docs:check`, `py:format`,
  `py:lints`, `WYRD_CLI_E2E=1 test:cli:journey` (23 passed), and
  `platform_admin_e2e` 17/17 through the Postgres wrapper.
- **`-p wyrd-sql --lib` is red at this candidate** (`FIND-008-15`), reproduced in
  0.6 s. The implementor's recorded command list contains no `wyrd-sql` lane,
  which is how it shipped.
- The disclosed pre-existing failure is confirmed pre-existing:
  `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` reproduces the
  identical `WYRD_AUTH_503_VERIFY_UNAVAILABLE` at `4668d8d33` and at the branch
  base; `tests/auth_e2e.rs` does not appear in the branch diff. One correction to
  the implementor's account: they cited `HEAD~1`, which *contains* the SQL change
  — the change-free commit is `HEAD~2`. `domain-rev-security` traced the 503 to
  `DelegateError::Database(_)` at `exchange_api_key.rs:791`, not a
  verification-cache defect. Not a finding; outside the write set.
- `mise run test:platform:journey` remains broken independently of this task
  (missing `setup:postgres` dependency); the platform suite was run through
  `scripts/postgres/with-test-postgres.sh`. Not treated as a finding.
- `ponytail-rev` did not re-run `platform_admin_e2e`, the MCP lane,
  `codegen:check`, or the Python and TypeScript lanes; Wave 1 covered those and
  its two findings do not touch them.
- Timing-channel differences in the platform plane's indistinguishable rejection
  were reasoned about from source, not measured.

## Out of scope — handoff, not findings

- `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` is
  unconditional while `auth_projection.rs:71` binds the `name` column to
  `card.metadata.name`, so registering two same-named Service or Agent Cards in
  different spaces within one tenant makes the second projection fail on that
  unique. Pre-existing, predates the branch, unrelated to TASK-008. For the spec
  owner.
- `crates/wyrd/wyrd-testing/src/server.rs:2670-2672` still comments that the
  principal keeps a uid-less `card_ref` "for the exact JSONB lookup", which the
  shipped predicate has made obsolete. Test-only prose on an item this candidate
  did not otherwise change.

## Disclosed policy item — carried forward, not a finding

History was not rewritten, so the `Co-Authored-By: Claude` trailers disclosed in
the `task-008-r2` verdict remain on the earlier commits; AGENTS.md §13 forbids
them and takes precedence over the harness attribution instruction. The three
remediation commits (`9fe02aa2e`, `67c9d2df6`, `f102e50ee`) are authored and
committed by `Thorrester <sjforrester32@gmail.com>` and carry no such trailer.
Still the change owner's decision.

## Remediation

`changes/active/admin-principals/review/task-008-r3/TASK-008-R3-pin-the-shipped-card-ref-predicate.md`

Route it to a fresh `$wyrd-implement` agent. Both findings are one edit in one
file. A later review reassesses the complete cumulative candidate against the
original task.
