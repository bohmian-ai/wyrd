# TASK-008 round 4 — repository-standards review (`repo-rev`)

Reviewer: `repo-rev`, independent repository-standards pass. Scope: repository-rule
compliance of the cumulative candidate, with effort concentrated on this round's one
source file. Not task acceptance, not the Ponytail audit, not optional improvements.

| Item | Value |
|---|---|
| Candidate HEAD | `c34b9d1e01c2c29ec8056e4d5ed220d61f9a6793` |
| This round's source commit | `708f01ec9` (one source file, +42/-10) |
| This round's evidence commit | `c34b9d1e0` (no code) |
| Cumulative closeout range | `289978fcc~1..c34b9d1e0` |
| Branch base | `968c92641` |
| Working tree | clean at start and at end (verified after `mise run fmt` and `codegen:check`) |
| Overall result | **FAIL** — one material finding (`RR4-1`) |

## 1. Authority coverage

Authorities selected through `architecture/references/README.md` §"Authority
hierarchy" and §"Canonical routes". The round-4 write set is one private SQL const,
one public tenant-scoped read function, and one inlined unit test in the durable
Postgres layer — a Rust/SQL/testing concern with no Card-contract, PyO3, TypeScript,
Vala, Bifrost, OLAP, or docs-site surface, so the router's `domain/` slices are
deliberately not loaded (README §"Compound routing examples", final row).

| Changed surface | Applicable authority (file · section) |
|---|---|
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:10-29` — rustdoc newly added to `SERVICE_ACCOUNT_BY_CARD_REF_SQL` | `AGENTS.md` §16 (rustdoc on every new/materially modified item, incl. private) · §15 (`wyrd-sql` is the durable Postgres layer) · `architecture/agent-rules.md` "Rust documentation follows AGENTS.md §16 as a hard acceptance criterion" · "Never mention references to plans, tasks, or other agents in the codebase" |
| same file `:30-38` — the SQL const text itself | `AGENTS.md` §9 (tenant isolation on every server path) · `architecture/agent-rules.md` "TenantConn (Postgres RLS) is the load-bearing tenant boundary" · `architecture/wyrd-security-posture.md` (credential-issuing path) |
| same file `:175-195` — `service_account_by_card_ref` doc + body | `AGENTS.md` §16 (rustdoc, `# Errors`) · §4 (domain types, no `unwrap`, `.clone()` as design question) · §5 (struct-centered style) · §6 (async earns its IO) · §9 (typed contracts, tenant isolation) · `agent-rules.md` "raw `sqlx::PgPool` banned / only `&mut TenantConn<'_>` or `&OperatorPool`", "a function accepting `&mut TenantConn<'_>` MUST NOT commit/rollback", "bring types in with `use`", "all `use` at top of module" |
| same file `:434-456` — `service_account_by_card_ref_uses_jsonb_card_ref_binding` | `AGENTS.md` §11 (tier taxonomy, journey rule, selector discipline) · §16 (rustdoc on test functions; Python-test shape n/a) · §12 (do not circumvent a gate) · `TESTING.md` §"The three tiers", §"Layout" · `agent-rules.md` "tests are inlined into the owning `src/` module by default", "tests needing Postgres … go in `mod pg_tests`" |
| `mise.toml`, `scripts/checks/` (cumulative) | `AGENTS.md` §12 "Adding And Retiring Checks" · `agent-rules.md` "Cargo features must be earned" |
| generated artifacts (`openapi.yaml`, `.pyi`, `crates/wyrd-spec/**/schemas/*.json`) (cumulative) | `AGENTS.md` §8, §12 · `agent-rules.md` "Never hand-edit generated artifacts" |
| commits `708f01ec9`, `c34b9d1e0` | `AGENTS.md` §13 Git Identity Rules |

Coverage is complete for the round-4 write set; no authority needed for this write set
was unavailable. Rounds 2 and 3 covered the earlier cumulative surfaces; §5 confirms
they did not regress.

## 2. Rule-by-rule result

### AGENTS.md §16 — General Code Rules

| Rule | Result | Evidence |
|---|---|---|
| Rustdoc on every new/materially modified item, incl. private | **PASS** for the const | `service_accounts.rs:10-29`. Nineteen lines of substantive prose, not a placeholder: it states the mechanism (containment, not equality, because `auth_projection` writes `space: Some(..)`/`uid: Some(..)` while a caller names only `space/Kind/name@version`), the workflow role (lookup for the client-expressible identity on a credential-issuing path), and the real invariants. At the branch base `968c92641:…service_accounts.rs:10-17` this const carried **no** rustdoc while this branch materially changed its text — so this round closes a latent §16 gap rather than merely relocating prose. |
| Rustdoc invariants must be factually correct | **PASS** | Each of the four claims verified against source, not prose: `UNIQUE (data_tenant_id, name)` exists at `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:84` and is **not** among the constraints this branch's own `20260601000020_admin_principals.sql:102-151` drops or replaces; `auth_projection` sets `space: Some(space.clone())` and `uid: Some(card_uid.clone())` at `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:62-63` and binds the `name` column from `card.metadata.name` (`:60`, `:72`) — the same value the `card_ref` carries; `IssueKeyArgs::space` is a required `pub space: String` at `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:30`. The round-3 finding that the old doc named a bound that does not exist ("two Cards with the same identity") is closed: that sentence is gone (`708f01ec9` diff, `-` lines at old `:160-163`). |
| Rustdoc on the modified public function, incl. workflow role and invariants | **FAIL** | `service_accounts.rs:175-183` — see `RR4-1`. |
| `# Errors` on every fallible function | **PASS** | `service_accounts.rs:181-182` retains `## Errors` / "Returns the database error when the read fails." after the doc move. |
| `# Panics` where a panic remains possible | **PASS (n/a)** | The function body `:184-195` is `bind`/`fetch_optional` with `?`-free `.await` returning `Result`; no panic path. The test's `.expect("static name is valid")` calls (`:442-446`) are test-local invariants with messages naming the invariant, permitted by §4. |
| Rustdoc on test functions | **PASS** | `service_accounts.rs:434-438`. Substantive rather than a restatement: it names *why* text-pinning is the mechanism ("the widening this guards is invisible at the call site") and what each half prevents (`=` matches no registered principal; dropping the ordered `LIMIT 1` makes a multi-row match arbitrary rather than bounded). It explains the test's reason to exist, which the assertions cannot. |
| Do not add comments/docstrings/annotations to untouched code | **PASS** | The `708f01ec9` diff touches exactly three hunks (`@@ -7,6`, `@@ -154,15`, `@@ -416,6`). Neighbouring undocumented items are correctly left alone: `service_account_by_id` (`:197-215`) is a fallible `pub async fn` with a one-line doc and no `# Errors`, and `refresh_token_insert_is_principal_generic` (`:458`) has no rustdoc — both pre-existing at the branch base and both outside this write set. Adding docs there would have violated this rule. |
| Single responsibility | **PASS** | No function split or merged. |

### AGENTS.md §4 — Rust Core Rules

| Rule | Result | Evidence |
|---|---|---|
| No new `unwrap()`/`expect()` in non-test code | **PASS** | `rtk proxy git diff f102e50ee..c34b9d1e0 -- crates/ \| grep -E '^[+-].*(unwrap\(\|expect\()'` → no match. `mise run check:unwrap-audit` → `unwrap/expect audit passed`, exit 0. |
| No new `.clone()` | **PASS** | Same grep, no match. The pre-existing `Json(card_ref.clone())` at `:447` is an unchanged context line in the diff and is test-local. |
| No hand-written `code()`/`status()`/`remediation()`/problem-json | **PASS** | No error surface touched; the function returns `sqlx::Error` as before (`:178`). |
| Domain types for durable identifiers | **PASS** | Unchanged: `conn.data_tenant_id().as_uuid()` (`:186`), `&CardRef` (`:177`). |
| No wildcard dependency version, no per-crate profile block | **PASS** | No manifest in the round-4 write set (`rtk proxy git diff --name-only f102e50ee..c34b9d1e0 \| grep -v '^changes/'` → the one `.rs` file only). The `[profile.release]` block at `sdks/wyrd-sdk-python/Cargo.toml:67` that `cargo` warns about on every lane is from `35ba64fe3`, confirmed an ancestor of the branch base `968c92641` — pre-existing, not attributable, and out of scope. |
| Secret handling unchanged | **PASS** | No `SecretString`, `Debug` impl, or secret-bearing struct in the diff. |
| `tracing` with structured fields | **PASS (n/a)** | No instrumentation added or removed. |

### AGENTS.md §5 / §6 — structure and async

| Rule | Result | Evidence |
|---|---|---|
| Struct-centered style; free functions only for stateless work | **PASS (unchanged)** | `service_account_by_card_ref` remains a module-level query function taking its connection explicitly — the established `wyrd-sql` query-module shape (`queries/auth/mod.rs:46` re-export). No new symbol, owner, trait, or helper introduced. |
| `async fn` earns its IO | **PASS** | The function directly awaits `fetch_optional` (`:194`). No new `async`. |
| No ad hoc Tokio runtime | **PASS** | None present. |

### AGENTS.md §9 — Server and contract rules · agent-rules tenancy

| Rule | Result | Evidence |
|---|---|---|
| Tenant isolation preserved | **PASS** | The predicate is byte-identical to the previously reviewed candidate. Extracted independently: the const body at `c34b9d1e0` and at `f102e50ee` `diff` clean and both md5 to `2298c9e6b9cf3e40edb952818226b944`, matching the implementor's claim. `data_tenant_id = $1` (`:33`), `status = 'active'` (`:36`) intact. |
| `TenantConn`/`OperatorPool` boundary untouched | **PASS** | Signature `conn: &mut TenantConn<'_>` (`:185`); no `&sqlx::PgPool`, no `Transaction` parameter, no `OperatorPool` widening. |
| A `&mut TenantConn<'_>` callee must not commit/rollback | **PASS** | Body `:184-195` contains neither; it uses `&mut **conn.transaction()`. |
| No new manual tenant filter added on a TenantConn path | **PASS** | The `data_tenant_id = $1` clause is present verbatim at the branch base (`968c92641:…:13`) and in every sibling query in the module. Pre-existing module-wide convention, not introduced or widened here; flagged as context only, not a finding. |
| Typed request/response, versioned contracts, no compatibility alias | **PASS (n/a)** | No wire type, route, or handler touched. |
| `use` statements at top of module; bare names in signatures | **PASS** | `:4-8` unchanged; no function-scoped `use` added. |

### AGENTS.md §11 · TESTING.md — testing workflow

| Rule | Result | Evidence |
|---|---|---|
| Correct tier home and directory placement | **PASS** | `TESTING.md:16` puts tier 3 in `src/**/mod tests`; `agent-rules.md` requires tests inlined into the owning `src/` module unless an external file is earned. The test lives in `#[cfg(test)] mod tests` inside the owning source file, asserting only on in-crate `&str` consts — no external target earned or created. |
| Credential-free, Docker-free, fast lane | **PASS** | The body (`:439-456`) constructs a `CardRef` and does string `contains` assertions. No Postgres, no env var, no `mod pg_tests` requirement. `mise exec -- cargo nextest run -p wyrd-sql --lib -E 'test(=…)'` ran it in **0.007 s** with no database present. |
| Journey rule — a unit test does not substitute for a missing tier-1 | **PASS** | The user-facing capability behind this predicate ships a tier-1 journey: `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs:41` `auth_issue_key_cli_journey` drives the real CLI against a booted server, issues a card-bound key from a `card_ref()` and spends it (`:57`, `:91`, `:115`), gated on `WYRD_CLI_E2E=1` (`:42`). The three call sites of the query (`wyrd-auth/src/issue_api_key.rs:94`, `jwt_bearer.rs:155`, `exchange_api_key.rs:591`) are reached through that path. The unit test is a supporting text pin, not a substitute. |
| The selector exists and selects a test (no pass-after-selecting-nothing) | **PASS** | `mise exec -- cargo nextest list --locked -p wyrd-sql --lib` line 37 → `wyrd-sql queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`. The focused run reported `Starting 1 test across 1 binary (73 tests skipped)` → `1 test run: 1 passed` — a nonzero selection, and an exact `test(=…)` expression rather than a positional filter. |
| `mise run test:sql` genuinely selects this test | **PASS** | `mise.toml:1463-1473` `test:sql:inner` runs `cargo nextest run --locked -p wyrd-sql --test-threads=1` with **no `--test`/`--lib` restriction**, so the lib target's unit tests are in the selection set. Lane run green end to end (exit 0), final segments `113 tests run: 113 passed` (`vala-sql`) and `2 tests run: 2 passed` (`wyrd-storage::pg_sweeper`). This closes the round-3 gap where the verification set had no SQL lane to catch the red assertion. |

### AGENTS.md §12 — Completion Standard

| Rule | Result | Evidence |
|---|---|---|
| Format, lints, targeted tests/checks pass | **PASS** | `mise run fmt` exit 0 with the working tree still clean afterwards (`git status --porcelain` empty). `mise run lints` (`cargo clippy --locked --workspace --all-features --all-targets -- -D …`) exit 0. `mise run test:sql` exit 0. `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `codegen:check`, `docs:check` — all exit 0. |
| Do not circumvent a gate | **PASS** | Round 4: `rtk proxy git diff f102e50ee..c34b9d1e0 -- crates/ \| grep -E '^[+-].*(#\[allow\|#\[ignore)'` → no match. Cumulative `289978fcc~1..c34b9d1e0` over `crates/`: **0** removed `#[test]`/`#[tokio::test]` attribute lines against **8** added, and **0** added `#[allow`/`#[ignore`. |
| The round-3 red test was fixed at the root, not papered over | **PASS** | The test was neither deleted, `#[ignore]`d, nor relaxed. The assertion was **corrected to the shipped predicate** (`card_ref = $3` → `card_ref @> $3`, `:449`) and **strengthened** with two new assertions pinning `ORDER BY created_at, id` and `LIMIT 1` (`:451-452`), while retaining `principal_kind = $2` (`:450`) and the negative `!…contains("card_ref::text")` (`:453`). Net assertion count 4 → 6; every one is falsifiable against the const text. |
| No new repository check added | **PASS** | `rtk proxy git diff --stat 289978fcc~1..c34b9d1e0 -- mise.toml scripts/checks/` → `mise.toml \| 4 ----`, one file, deletions only. The deletion is the `cli:dev-bootstrap` **dev task** for a removed CLI command (`mise.toml` diff, `-[tasks."cli:dev-bootstrap"]`), not a check. `scripts/checks/` untouched. No check added, and no live-boundary check retired. |

### AGENTS.md §15 — Implementation Rules

| Rule | Result | Evidence |
|---|---|---|
| Correct owner | **PASS** | A tenant-scoped SQL query const, its query function, and its text pin all live in `crates/wyrd/wyrd-sql` — §15 "`wyrd-sql` is the durable Postgres layer". |
| Client-tier / SDK-tier dependency bans unaffected | **PASS** | `mise run check:client-tier` and `check:sdk-client-tier` exit 0; no manifest touched. |
| No scaffolding for hypothetical reuse; stop at the first correct option | **PASS** | No file, type, trait, helper, dependency, config option, or fixture added. The change is prose plus three assertion lines on an existing test. |

### AGENTS.md §13 — Git Identity Rules

| Rule | Result | Evidence |
|---|---|---|
| Author and committer are the repository contributor | **PASS** | `708f01ec9` — author `Thorrester <sjforrester32@gmail.com>`, committer `Thorrester <sjforrester32@gmail.com>`. `c34b9d1e0` — author `Thorrester <sjforrester32@gmail.com>`, committer `Thorrester <sjforrester32@gmail.com>`. |
| No AI co-author trailer | **PASS** | Full `%B` of both commits inspected: **no** `Co-Authored-By:`, `Co-authored-by:`, or any AI attribution trailer on either. Correct per §13, which overrides the harness's default attribution guidance. No history rewrite proposed. |

### Generated artifacts

| Rule | Result | Evidence |
|---|---|---|
| No generated artifact changed this round | **PASS** | Round-4 write set is one `.rs` file plus `changes/**`; `openapi.yaml`, `.pyi`, and `tests/schemas/*.json` are absent from `rtk proxy git diff --name-only f102e50ee..c34b9d1e0`. |
| Generated artifacts regenerate cleanly | **PASS** | `mise run codegen:check` → schemas, OpenAPI, and 13 stub modules regenerated, `All checks passed!`, exit 0, and `git status --porcelain` empty immediately afterwards (no drift written). |
| Docs site regenerates cleanly | **PASS** | `mise run docs:check` → 60 pages indexed, `Docs a11y OK — 60 pages checked, contrast AA verified (light + dark)`, exit 0. |

## 3. Findings

### `RR4-1` — VIOLATION — the public function's rustdoc delegates its caller-relevant invariant to a private item, and the link does not resolve

- **Rule**: `AGENTS.md` §16 — "Every new or materially modified Rust item MUST have
  rustdoc… Rustdoc MUST explain intent, how the item participates in the surrounding
  workflow, and relevant invariants or side effects." Reinforced by
  `architecture/agent-rules.md`: "Rust documentation follows `AGENTS.md` §16 as a hard
  acceptance criterion… Missing or placeholder rustdoc is `BLOCK_BEFORE_MERGE`."
- **Location**: `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:175-183`
  (the doc block on `pub async fn service_account_by_card_ref`), specifically the
  delegation at `:177-178`:
  `/// Binds the caller's ref as JSONB for [`SERVICE_ACCOUNT_BY_CARD_REF_SQL`],`
  `/// whose documentation carries what the predicate does and does not bound.`
  The link target `SERVICE_ACCOUNT_BY_CARD_REF_SQL` is **private** — declared
  `const`, not `pub const`, at `:30`.
- **Observable consequence**: two concrete effects, both new in `708f01ec9`.
  1. `mise exec -- cargo doc --locked -p wyrd-sql --no-deps` now emits
     `warning: public documentation for `service_account_by_card_ref` links to
     private item `SERVICE_ACCOUNT_BY_CARD_REF_SQL` … this item is private`
     (`rustdoc::private_intra_doc_links`, on by default). This warning did not exist
     at `f102e50ee`, where the only intra-doc link in this block pointed at
     `tenant_admin_principal_id` — a `pub async fn` at `:226` — and resolved.
     `wyrd-sql` is outside `check:docs` (`mise.toml:544-551` covers only `wyrd-spec`,
     `wyrd-auth-issue`, `wyrd-auth-verify`), so no gate catches it; this is a new
     compiler-emitted diagnostic, not a gate break, and it is not a gate
     circumvention.
  2. More materially: in generated docs the public function now carries **no**
     statement of the invariant a caller most needs on a credential-issuing path —
     that containment relaxes `space`, so a `CardRef` with `space: None` matches a
     registered principal in *any* space — and the pointer offered in its place is
     unrenderable. The three callers
     (`crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`,
     `jwt_bearer.rs:155`, `exchange_api_key.rs:591`) construct that `CardRef`
     themselves; `CardRef::space` is `Option`, so a caller can pass `None`, and the
     only place that consequence is now written down is invisible from the public API
     surface. The previous revision stated the containment rationale inline on the
     function; moving the prose onto the const (correct in itself, and what the
     round-3 remediation task asked for) removed it from the public item without
     leaving the caller-facing half behind.
- **Testable correction**: make the public function's doc stand alone, and keep the
  const's doc as the detailed home. Concretely: (a) add one sentence to
  `service_accounts.rs:175-183` naming the caller-relevant invariant — that the match
  is by containment, so a `CardRef` whose `space` is `None` is not constrained to one
  space and callers must pass a fully qualified ref; and (b) de-link the reference to
  the private const, i.e. write `` `SERVICE_ACCOUNT_BY_CARD_REF_SQL` `` in plain
  backticks instead of `` [`SERVICE_ACCOUNT_BY_CARD_REF_SQL`] ``.
  Verify with `mise exec -- cargo doc --locked -p wyrd-sql --no-deps 2>&1 | grep
  private_intra_doc_links` → no match, and the single pre-existing unrelated warning
  (`unresolved link to `IdError``, `crates/wyrd/wyrd-sql/src/error.rs:204`, introduced
  by `dee00f3e8`, file untouched by this branch) remaining the only warning.
  Neither part changes SQL, any caller, or the test.

No other material repository-rule finding. Specifically **not** reported, as out of
scope or not attributable: the `[profile.release]` block in
`sdks/wyrd-sdk-python/Cargo.toml:67` (pre-existing at the branch base); the
`IdError` rustdoc warning in `wyrd-sql/src/error.rs:204` (pre-existing, file
untouched); the missing `# Errors` on `service_account_by_id` (`:197`, pre-existing
and outside this write set — documenting it would itself violate §16's
"do not add comments… to code you did not touch"); the manual `data_tenant_id = $1`
predicate on a `TenantConn` path (pre-existing module-wide convention, present at the
branch base); and the two out-of-scope handoffs recorded in round 2 (the
same-name-across-spaces projection failure and the stale `wyrd-testing`
`server.rs:2670-2672` comment), confirmed left alone in this round's write set, which
is consistent with round 2's non-goals and not a gap.

## 4. Verification run

Every command below was run from the worktree root. No broad aggregate was used as
evidence: no `gate`, no `test:rust`, no crate/family aggregate, no storage matrix, no
`--all-features` workspace test lane. `test:sql` is in scope as the narrow owning lane
for `wyrd-sql`. Every selector confirmed with `cargo nextest list` before use.

| Command | Result |
|---|---|
| `mise exec -- cargo nextest list --locked -p wyrd-sql --lib` | selector present (line 37) |
| `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'` | `1 test run: 1 passed, 73 skipped` |
| `mise run fmt` | exit 0; tree clean after |
| `mise run lints` | exit 0 |
| `mise run test:sql` | exit 0; all four segments green |
| `mise run check:client-tier` | exit 0 |
| `mise run check:sdk-client-tier` | exit 0 |
| `mise run check:pyo3-scope` | exit 0 |
| `mise run check:unwrap-audit` | exit 0 — `unwrap/expect audit passed` |
| `mise run codegen:check` | exit 0 — `All checks passed!`; tree clean after |
| `mise run docs:check` | exit 0 — `Docs a11y OK` |
| `mise exec -- cargo doc --locked -p wyrd-sql --no-deps` | **2 warnings** — one pre-existing (`IdError`), one new and attributable (`RR4-1`) |
| SQL const md5 at `c34b9d1e0` vs `f102e50ee` | both `2298c9e6b9cf3e40edb952818226b944`; bodies `diff`-identical — the no-behavior-change claim independently verified |

Working tree clean at review end. No source file changed by this review.

## 5. Cumulative non-regression

Confirmed against the cumulative closeout range `289978fcc~1..c34b9d1e0`, whose
non-`changes/` write set spans 52 files (`Cargo.lock`, `crates/shared/**`,
`crates/wyrd-spec/**`, `crates/wyrd/**`, `docs/**`, `mise.toml`, `openapi.yaml`):

- 0 added `#[allow`/`#[ignore` attributes; 0 removed test attributes against 8 added.
- `mise.toml` changed by deletions only — the `cli:dev-bootstrap` dev task for a
  removed CLI command; no check added or retired, `scripts/checks/` untouched.
- All six boundary/contract checks and both doc lanes green, with no generated-artifact
  drift written to the tree.

Rounds 2 and 3 found these surfaces compliant apart from findings now closed; nothing
in the evidence above indicates a regression.

## Overall result

**FAIL** — one material finding, `RR4-1` (`AGENTS.md` §16 ·
`architecture/agent-rules.md` rustdoc criterion), at
`crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:175-183`. Authority
coverage is complete; the review is not blocked. Everything else this round checked —
including the round-3 red assertion, the const's substantive and factually verified
rustdoc, the test's rustdoc, the unchanged SQL predicate, the tenancy boundary, the
test's tier home and lane selection, the absence of any gate circumvention, and both
commits' git identity — passes.
