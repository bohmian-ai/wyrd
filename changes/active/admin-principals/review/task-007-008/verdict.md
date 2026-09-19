# TASK-007 / TASK-008 — review verdict

## Verdict

`FIX_REQUIRED`

## Immutable subject

| Item | Value |
|---|---|
| Repository root | `/home/user/wyrd` |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` (`git merge-base HEAD origin/change/surfaces-oracle-integration`) |
| Candidate | `4225069` `feat(client): project principal and credential administration to the SDKs` |
| Working tree | clean throughout the review; candidate did not change |
| Approved spec | `changes/active/admin-principals/spec.md` revision 6, status `approved` |
| Tasks reviewed | `tasks/TASK-007-platform-human-administration.md`, `tasks/TASK-008-sdk-and-mcp-projection.md` |
| Authority | `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` |
| Verification scope | spec `VER-001`..`VER-006` |

## Review topology deviation — disclosed

The `$wyrd-task-review` skill requires a two-wave multi-agent topology
(`task-rev`, `repo-rev`, one `domain-rev` per sensitive domain, then
`ponytail-rev`). **This harness exposes no agent-delegation capability**: there
is no `Task`/`Agent` tool, and the only spawn primitive
(`Claude_Code_Remote__create_session`) produces independent remote sessions in
separate containers that cannot write reports into this working tree or return
structured output to this agent.

The skill's literal response to that condition is `BLOCKED`. The invoking
instruction, however, explicitly directed this agent to perform the review and
return a verdict with an acceptance matrix and findings. This review was
therefore performed by a single reviewer applying each role's scope and the
Ponytail validation ladder in sequence against source, not summaries. The
resulting finding ledger is real and each entry is independently traceable to
the cited file and line, but the independence guarantee the skill's topology
provides was not obtained. Treat the verdict as sound on its evidence and
conservative on its completeness: a second independent pass could add findings,
and is unlikely to remove the blocking ones, which are absences confirmed by
`git diff` and `grep` rather than judgement calls.

## Wave results

| Role | Result | Report |
|---|---|---|
| `task-rev` (task acceptance) | `FAIL` | `task-review.md` |
| `repo-rev` (repository standards) | `FAIL` | folded into `task-review.md` and the ledger (topology deviation above) |
| `domain-rev` (security / identity / trust boundary) | `FAIL` | folded into the ledger (FIND-007-3, -4, -5, -6, -9) |
| `ponytail-rev` (validation) | completed, ledger non-empty | `findings-validation.md` |

## Acceptance summary

The full matrix is in `task-review.md`. In short:

- **TASK-007**: the *administration* half landed and is sound — platform
  authority is a grant, the connection is a singleton at platform scope, the
  pin is race-safe at the SQL level (`UPDATE ... WHERE subject IS NULL
  RETURNING` is atomic and one-time), login state is genuinely single-use and
  expiry-enforced (`DELETE ... RETURNING` plus an expiry filter), and the nonce
  is compared against the row this exchange consumed. The *human identity* half
  is unproved end to end and has two substantive defects (unverified-email
  pinning, missing SSRF screening) plus a non-atomic registration and no
  revocation path.
- **TASK-008**: largely undone. Only the Rust surface for *tenant* principal and
  credential administration exists. Python, TypeScript, MCP, the platform-plane
  client operations, the generated HTTP contract, and all documentation are
  absent.

Confirmed on the specific points the review was asked to attack:

- **First-login pinning is race-safe and one-time at the store**, and no second
  login can re-pin or steal a pinned principal. But an *unregistered subject can
  acquire platform authority* on the first login, because the match is on an
  unverified `email` claim — FIND-007-3.
- **No response, log, trace, error, or audit payload leaks the provider client
  secret today.** The type that carries it is nonetheless unprotected —
  FIND-007-9.
- **The nonce genuinely binds the token to this exchange**, and the login state
  is genuinely single-use and expiry-enforced. None of it is tested —
  FIND-007-2.
- **`cid: Option<String>` is not a weakening.** A credential-minted token cannot
  omit `cid`; the only cid-less minting path is federated login, and the
  federated anchor re-reads the principal on every request. The asymmetry that
  matters is that nothing can make a platform principal inactive — FIND-007-6.
- **The real-provider identity journey TASK-007 names as its primary proof was
  not written.** `identity_e2e.rs` is untouched and contains zero references to
  the platform plane — FIND-007-1.
- **TASK-008 is largely undone**, as expected — FIND-008-1 through -7.
- One further circular test was found beyond the two already caught:
  `platform_caller_carries_its_minting_credential` — FIND-007-7.

## Validated finding ledger

| ID | Class | Location | Summary |
|---|---|---|---|
| FIND-007-1 | MISSING | `crates/wyrd/wyrd-server/tests/identity_e2e.rs` (unchanged) | The real-provider identity journey named as the task's primary proof was never written; AC-015 and AC-016 have no evidence. |
| FIND-007-2 | MISSING | `crates/wyrd/wyrd-auth/src/platform_login.rs`; `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs` | Login, pinning, state consumption, and nonce binding have no test at any tier. |
| FIND-007-3 | INCORRECT | `crates/wyrd/wyrd-auth/src/platform_login.rs:282` | First-login pin matches on an unverified `email` claim, so a different subject at the same issuer can seize a pre-registered platform principal. |
| FIND-007-4 | VIOLATION | `crates/wyrd/wyrd-server/src/components/platform/identity.rs:129-178` | Connection configuration performs no URL validation and no SSRF screening and trusts a caller-supplied `jwks_uri`, against an explicit task constraint and an existing repository owner. |
| FIND-007-5 | INCORRECT | `crates/wyrd/wyrd-server/src/components/platform/identity.rs:100-126,283-292` | `register_admin` commits its allowance and then writes twice non-transactionally, orphaning a platform principal on the second failure. |
| FIND-007-6 | MISSING | `crates/wyrd/wyrd-server/src/components/platform/identity.rs:55-63` | Platform principal listing and revocation, named in the task's approach, do not exist, leaving the `is_active()` session guarantees unreachable. |
| FIND-007-7 | INCORRECT | `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:245-256` | Circular test asserts a fixture's own construction and a tautology while claiming to prove credential propagation into audit. |
| FIND-007-8 | MISSING | no location (not written) | AC-011 — same authenticated context shape for a federated human and a machine credential, and coexistence with the tenant administrative principal — has no evidence. |
| FIND-007-9 | VIOLATION | `crates/wyrd-spec/src/auth/platform_identity.rs:13-29` | The inbound provider client secret is a plain `String` with a derived `Debug`, bypassing the repository's `SecretBearer` owner. |
| FIND-008-1 | MISSING | `sdks/wyrd-sdk-python/` (unchanged) | No Python SDK projection of the administrative contract. |
| FIND-008-2 | MISSING | `sdks/wyrd-sdk-ts/` (unchanged) | No TypeScript SDK projection of the administrative contract. |
| FIND-008-3 | MISSING | `crates/wyrd/wyrd-server/src/mcp/` | No MCP administrative tools and no write-scope gating. |
| FIND-008-4 | MISSING | `crates/shared/wyrd-client/src/principals/handle.rs` | No platform-plane operation on any client surface, leaving the cross-plane refusal criterion unfalsifiable. |
| FIND-008-5 | MISSING | `crates/wyrd/wyrd-server/src/http/openapi.rs:14-33`; `openapi.yaml` | New administrative routes are absent from the generated HTTP contract; `codegen:check` passes vacuously. |
| FIND-008-6 | VIOLATION | `crates/wyrd/wyrd-server/src/boot/bootstrap.rs`; three `docs/src/content/docs/self-hosting/*` pages | `bootstrap-key`, `SYSTEM_OPERATOR_ID`, and the fabricated `bootstrap-admin` CardRef still exist in tree and in published documentation. |
| FIND-008-7 | MISSING | `docs/` (unchanged) | REQ-040 documentation was not written. |

Full diagnosis, evidence, falsifying scenarios, and decision-complete
corrections are in `findings-validation.md`.

## Verification limits

- `mise` is not installed in this environment and `cargo-nextest` is
  unavailable. `rustup run 1.97.1 cargo` and `cargo test` were substituted; the
  exact `mise exec -- cargo nextest run` forms `VER-002` prescribes could not be
  executed verbatim. This is an environment limitation, not a finding.
- Per `VER-003`, no broad aggregate was run and none is treated as missing.
- `mise run codegen:check`, `check:client-tier`, `check:sdk-client-tier`,
  `check:sdk-pyo3-scope`, `py:*`, and `docs:check` could not be run. Client-tier
  and PyO3-scope compliance was established by inspecting
  `crates/shared/wyrd-client/src/principals/` dependencies. `codegen:check` is
  believed to pass and is explicitly *not* evidence for FIND-008-5, for the
  reason recorded there.
- What was run passed: `cargo check` and `cargo clippy --all-targets` over
  `wyrd-auth-oidc`, `wyrd-auth`, `wyrd-server`, `wyrd-client`, `wyrd-sdk-rust`
  (clean); `platform_admin_e2e` 10/10; `wyrd-sdk-rust --test principals
  --ignored` 2/2.
- No source file was modified by this review. Only artifacts under
  `changes/active/admin-principals/review/task-007-008/` were written.

## Prior-finding closure

This is the first review of TASK-007 and TASK-008. Prior verdicts
(`review/task-001`, `review/task-002`, `review/task-003-006`) cover earlier
tasks and were not reassessed. FIND-007-5 is the same class of defect as the
atomicity finding remediated under
`review/task-003-006/TASK-003-006-R1-lifecycle-audit-and-atomicity.md`,
recurring in the one platform route family that remediation did not touch.

## Remediation

`TASK-007-008-R1-close-identity-and-projection-gaps.md`, in this directory.
Route it to a fresh `$wyrd-implement` agent.

## Concurrent working-tree mutation — recorded

At the close of the review `git status` showed three source files modified and a
new `changes/active/admin-principals/review/task-003-006-r1/` directory, none of
which existed when the review began. These are a concurrent session implementing
`TASK-003-006-R2-revocation-side-effects` in the same working tree, touching
`crates/wyrd/wyrd-auth/src/exchange_api_key.rs`,
`crates/wyrd/wyrd-server/src/components/principals/routes.rs`, and
`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs` (+133 lines).

This does **not** make the subject ambiguous and does not yield `BLOCKED`:

- `HEAD` remained `42250697e2ebc8073f76a61257fa1aaa862aa649` throughout. The
  reviewed subject is the commit range `c5c2075..HEAD`, which is unchanged.
- Every finding was established from committed content, via
  `git diff c5c2075..HEAD`, `git ls-tree -r HEAD`, and reads taken while the
  tree was clean. No finding rests on uncommitted state.
- The concurrent work targets TASK-003-006 remediation, outside this review's
  scope.

One consequence for a downstream reader: line numbers cited in
`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs` (FIND-007-5's falsifying
scenario, `:845-861`) are the committed positions at `4225069` and may have
shifted in the working tree. Resolve them against the commit, not the checkout.
The `an_operator_configures_and_removes_federated_platform_sign_in` test and its
duplicate-claim registration block are unchanged in substance.
