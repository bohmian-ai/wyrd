# Repository-standards review — TASK-008 round 4 (`task-008-r5`)

Reviewer: `repo-rev` (fresh, independent). Candidate `ba223a4db`, base `c34b9d1e0`.
Report returned as text by the reviewer (the Write tool refuses report files);
persisted here by the orchestrator.

## 0. Subject verification

| Check | Result |
|---|---|
| `git rev-parse HEAD` | `ba223a4db0445d44a6f57eee21115da0b77adb48` — matches |
| `git status --porcelain` | clean |
| Source delta (`rtk proxy git diff c34b9d1e0..ba223a4db -- crates/`) | exactly one file, `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, +3/−4, all inside the `SERVICE_ACCOUNT_BY_CARD_REF_SQL` doc comment |
| Rest of range | 6 markdown files, all under `changes/active/admin-principals/review/task-008-r4/` |

`mise.toml`, every `check:*` script, every test file, every `Cargo.toml` and
migration are untouched.

## 1. Authority-coverage table

The authority surface is genuinely narrow, stated rather than manufactured. The
delta adds no code, no item, no dependency, no feature, no SQL, no async, no
test, no generated artifact, and no Python/TS surface.

| Changed surface | Applicable authority | Why |
|---|---|---|
| `service_accounts.rs:18-20` (doc comment on the private const) | AGENTS.md §16 (rustdoc as hard blocker); `agent-rules.md` rustdoc rule (line 35); `agent-rules.md:33` (no plan/task/agent references in code) | The only substantive rule set a doc-comment edit can violate |
| Same, as documentation of a durable Postgres query | AGENTS.md §15 (`wyrd-sql` is the durable Postgres layer; stop at the first correct option); §4 | Doc must describe durable behavior accurately |
| Whole delta | AGENTS.md §12 (completion standard; do not circumvent a gate; adding/retiring checks); `agent-rules.md:19` | Verified negatively from the diff |
| Both commits | AGENTS.md §13 (git identity) | Author, committer, trailers |
| Verification evidence | AGENTS.md §11 (verification scope, focused selectors, `mise` lanes) | Right narrow set for a doc-only `wyrd-sql` change |
| Markdown packet (`review/task-008-r4/`) | AGENTS.md §14 | Paths and location |

Not applicable and deliberately not stretched: §2, §3, §5, §6, §7, §8, §9, §10,
§11's codegen/boundary/docs-site/example lanes, and `agent-rules.md`'s
`TenantConn`/`OperatorPool`, RLS, audit, SSRF, `build.rs`, positional-mapping,
and clippy-allow rules — no code changed.

## 2. Pass/fail per applicable rule

**§16 rustdoc + `agent-rules.md:35` — PASS.** Every surviving factual claim
verified against the tree:

| Claim | Evidence | Verdict |
|---|---|---|
| `UNIQUE (data_tenant_id, name)` on the table | `migrations/20260601000001_auth.sql:85`; `20260601000020_admin_principals.sql:96-97` explicitly retains it and drops only the `principal_kind`/card checks | accurate |
| `auth_projection` keeps `name` = `card_ref->>'name'` | `queries/cards/auth_projection.rs:57-63` builds `card_ref.name` from `card.metadata.name`; `:71` binds the same value to the `name` column; `UPSERT_SQL:21-23` keeps both in sync on conflict | accurate |
| projection writes `space: Some(..)`, `uid: Some(..)` | `auth_projection.rs:61-62` | accurate |
| containment relaxes every optional `CardRef` field, `space` included | `wyrd-spec/src/reference.rs:28-37` — `space`/`uid` are the only `Option` fields, both `skip_serializing_if` | accurate |
| GIN index on `card_ref` serves the lookup | `migrations/20260601000001_auth.sql:92-93` | accurate |
| the deleted claim was false | `IssueKeyArgs` exists only at `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:18` — a clap struct, not a caller. The three real callers (`wyrd-auth/src/issue_api_key.rs:94`, `exchange_api_key.rs:591`, `jwt_bearer.rs:155`) pass a `CardRef` with no space enforcement. Deletion warranted | accurate |

No dangling reference (`grep -rn "three facts\|fully qualified ref" crates/`
returns only two unrelated Vala hits; `IssueKeyArgs` appears nowhere outside
`wyrd-cli`), no broken sentence, and "two facts" matches the two items listed.

On the later clause "relax the unique constraint, decouple the name projection,
or add an optional `CardRef` field": this reviewer read it as naming three ways
to *widen*, two of which break the two named facts and the third of which widens
containment itself, already established in the paragraph's opening sentence, and
judged it loose phrasing rather than an inaccuracy — **not filed**. Wave 2
overruled that judgement; see `findings-validation.md`.

`# Errors` present on the public fallible `service_account_by_card_ref`
(`:180-181`). No plan, task, or agent reference in the prose.

**§12 "do not circumvent a gate" — PASS.** No `#[allow]`, no `#[ignore]`, no
test deleted or weakened, no boundary glob broadened, no `mise.toml` or
`scripts/` edit, no check added or retired.

**§13 git identity — PASS.** Both commits, author *and* committer
`Thorrester <sjforrester32@gmail.com>`; no `Co-Authored-By` or AI trailer in
either body. No `git config` run, no identity env var set.

**§11/§12 verification scope — PASS.** For a doc-only change confined to one
`wyrd-sql` file the required narrow set is Rust formatting, clippy, and the
owning crate's lane plus the focused selector — exactly what the implementor
reports. Nothing else in §11 is triggered. `mise run gate` is correctly absent.

**§15 / §4 — PASS.** No code path, type, dependency, file, or configuration
option added; the ladder's first rung (delete rather than add) is what was taken.

**§14 planning artifacts — PASS.** The six markdown files land under
`changes/active/admin-principals/review/task-008-r4/` on the change branch.

## 3. Commands run

| Command | Result |
|---|---|
| `git rev-parse HEAD` | `ba223a4db…` |
| `git status --porcelain` (×3) | clean each time |
| `rtk proxy git diff --stat c34b9d1e0..ba223a4db` | 7 files, +1260/−4 |
| `rtk proxy git diff c34b9d1e0..ba223a4db -- crates/` | the +3/−4 doc hunk only |
| `git log -2 --format=… ba223a4db` | identities and bodies as above |
| `mise run fmt` | ran `cargo fmt --all`; tree still clean → compliant |
| `mise run lints` | clippy workspace-wide, finished in 36.55s, zero warnings, zero errors |
| `mise exec -- cargo doc -p wyrd-sql --no-deps` | success; 2 warnings (below) |
| focused `cargo nextest run` on the pinning selector | `1 passed, 73 skipped` |
| `sed -n '535,560p' mise.toml` | `check:docs` denies `missing_docs` + `rustdoc::broken_intra_doc_links` for `wyrd-spec`, `wyrd-auth-issue`, `wyrd-auth-verify` only |

`cargo doc -p wyrd-sql` warnings, reported as asked, neither filed:

1. `rustdoc::private_intra_doc_links` at `service_accounts.rs:176` — the settled,
   rejected `RR4-1`. Not re-filed; no relocation proposed.
2. `rustdoc::broken_intra_doc_links` — unresolved `[IdError]` at
   `crates/wyrd/wyrd-sql/src/error.rs:204`. Pre-existing, outside every write set
   in this delta, and `check:docs` does not cover `wyrd-sql`. Unrelated
   pre-existing debt, which the classification rules exclude. Disclosed only so
   the parent knows `cargo doc` is not silent on this crate and that neither
   warning is a gate failure.

## 4. Findings

**None.** No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION`
against any applicable repository authority. `RR5-<n>` IDs unassigned.

## 5. Result

**PASS** — repository-standards axis only.

## 6. Verification limits

- Repository-standards compliance only: no task-acceptance assessment, no spec
  obligation mapping, no Ponytail audit.
- No Postgres-backed lane run. A doc-comment edit cannot change runtime
  behavior, so the risk of that omission is nil.
- `mise run lints` is the workspace clippy lane and is not `--all-features`; the
  subject's binding rules forbid an `--all-features` workspace lane as evidence,
  and a comment edit cannot introduce a feature-gated lint.
- `mise run fmt` mutates rather than checking; compliance was inferred from the
  tree remaining clean afterwards rather than from a `--check` exit code.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` were not
  re-read end to end: the delta touches no contract, kind, header, route, SDK
  surface, or Bifrost path. Stated as a limit rather than claimed as coverage.
- CodeGraph not used; no `.codegraph/` directory at this worktree root, so
  AGENTS.md §18 says to skip it.
