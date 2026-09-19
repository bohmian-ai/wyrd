# Repository-standards review — TASK-008 round 5 (`task-008-r6`)

Reviewer: `repo-rev` (fresh, independent), repository-standards axis only.
Candidate `b98ac9fa9`, base `ba223a4db`. No source file changed.
Report returned as text by the reviewer; persisted here by the orchestrator.

## Preflight

| Check | Result |
|---|---|
| HEAD | `b98ac9fa91670456b87dfbc82059038f47a0627e` = expected |
| Tree | `git status --porcelain` empty |
| Source write set (verified from the raw diff, not taken on trust) | exactly `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, +5/−5, all `///` lines; the other 6 paths are `changes/active/admin-principals/review/task-008-r5/*.md` |

The delta is five `///` lines inside one sentence of one doc comment on a private
const. That is the entire authority surface, and it is narrow. It is not inflated
below.

## 1. Authority-coverage table

| Changed surface | Applicable authority | Verdict |
|---|---|---|
| Doc comment on the private const `SERVICE_ACCOUNT_BY_CARD_REF_SQL` (`service_accounts.rs:19-28`) | AGENTS.md §16 (rustdoc explains intent, workflow role, invariants; hard blocker) + `agent-rules.md` rustdoc rule | PASS |
| Doc comment on public `service_account_by_card_ref` (`:174-182`) — read as it stands, since the const text is what it defers to | AGENTS.md §16 `# Errors` requirement | PASS |
| Hand rewrap (`cargo fmt` does not reflow doc comments) | AGENTS.md §4 formatting lane; file-local `///` width precedent | PASS |
| Whole range (2 commits) | AGENTS.md §12 completion standard + "do not circumvent a gate" | PASS |
| Whole range (2 commits) | AGENTS.md §13 git identity | PASS |
| Verification evidence claimed by the implementor | AGENTS.md §11 verification scope | PASS |
| Code content | `agent-rules.md`: never mention plans, tasks, or agents in the codebase | PASS — the doc text names no plan, task, review round, or agent |

### Explicitly not applicable

- `architecture/wyrd-design.md`, `architecture/bifrost-design.md` — the delta
  touches no Card kind, envelope, contract, route, header, SDK surface, MCP tool,
  or Bifrost path. Not reachable; no coverage claimed.
- §3 / §7 / §8 — no crate boundary crossed, no PyO3, no Python-visible symbol.
- §5 / §6 / §9 / §10 — no Rust item added, removed, renamed, or re-shaped;
  signatures byte-identical.
- §11 journey-test requirement — no user- or agent-facing behavior changed.
- §12 "public contracts regenerate cleanly when touched" — none touched, so
  `codegen:check` is not owed.

## 2. Per-rule result and evidence

**§16 / `agent-rules.md` rustdoc — PASS.** Every surviving factual claim verified
against the tree:

- `UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` —
  `migrations/20260601000001_auth.sql:85`. Retained: the two later name-agnostic
  drop loops against that table filter `contype = 'c'`
  (`20260601000020_admin_principals.sql:113-128`) and `contype = 'f'`
  (`20260601000011_auth_relax_created_by_fk.sql:25-36`); neither can drop a `'u'`
  constraint. Confirmed by reading every `DROP CONSTRAINT` hit in the migrations.
- Name projection equality — `queries/cards/auth_projection.rs`:
  `name: card.metadata.name.clone()` into the `CardRef`, and
  `.bind(card.metadata.name.as_str())` into the `name` column, in the same
  `UPSERT_SQL` call. Same value, both places. The claim is exact.
- `ORDER BY created_at, id` / `LIMIT 1` present in the const and pinned by
  `service_account_by_card_ref_uses_jsonb_card_ref_binding`.
- GIN index on `card_ref` (cited by the public function doc) —
  `20260601000001_auth.sql:93`. Durable key `(card_kind, card_uid)` — `:84`.

**Argument completeness after the deletion — intact.** The sentence enumerates
break-ways for the chain it just named, and the chain has exactly two links. Two
links, two break-ways. The deleted third was the one with no link behind it.
Nothing dangles: the colon-clause still has its antecedent ("none of *that
chain*"), and both following sentences still resolve.

**`# Errors` — PASS.** The const is not fallible and owes none.
`service_account_by_card_ref` retains `/// # Errors` / "Returns the database error
when the read fails." Pre-existing `# Errors` gaps on untouched siblings
(`service_account_by_id`, `api_key_by_prefix`, `touch_api_key_last_used`,
`insert_refresh_token`, `service_account_roles`) are outside the write set and
unchanged by this delta — disclosed, not filed.

**Rewrap quality — PASS.**

- Longest `///` line in the file is 82 chars at line 14, pre-existing and
  untouched. The five rewrapped lines are 79, 81, 71, 74, 48 — all at or under the
  file's existing maximum. No over-length regression.
- Backtick parity: 28 backticks in the block, and every individual line has an
  even count, so no inline-code span is split across a `///` boundary. The five
  rewrapped lines contain zero backticks, so no code span was reachable by the
  rewrap at all.
- Adjacent-duplicate scan over the joined paragraph returned `[]`; the reflowed
  paragraph reads grammatically.

**§12 / gate-circumvention — PASS.** `git diff --name-only ba223a4db..b98ac9fa9`
lists one `.rs` and six `.md`. No `mise.toml`, `.github`, `scripts/`, or
`Cargo.toml`. Grepping the range's added and removed lines for `allow(`,
`#[ignore`, `ignore]`, `deny(`, `expect(` → none. No test deleted or ignored, no
boundary glob touched, no check added or retired.

**§13 git identity — PASS.** Both commits, author *and* committer
`Thorrester <sjforrester32@gmail.com>`; `%(trailers)` empty on both.

**§11 verification scope — PASS, and the claimed set is right.** For a doc-only
edit inside one `wyrd-sql` file, §11 owes `mise run fmt`, `mise run lints`, and
the narrowest test lane for the touched surface. No Python, no generated contract,
no `docs/`, no boundary-sensitive move, and §11's explicit instruction is not to
reach for `mise run gate`. The reported focused selector plus `test:sql` plus
`fmt` plus `lints` is the correct narrow set; nothing §11 requires for this
surface is missing.

## 3. Commands run

| Command | Result |
|---|---|
| `git rev-parse HEAD` | `b98ac9fa9167…` |
| `rtk proxy git status --porcelain` | empty |
| `rtk proxy git diff ba223a4db..b98ac9fa9 --stat` | 7 files, +1161/−5 |
| `rtk proxy git diff ba223a4db..b98ac9fa9 -- <file>` | raw: 5 `///` lines out, 5 in; nothing else |
| `rtk proxy git diff --name-only ba223a4db..b98ac9fa9` | 1 `.rs` + 6 `.md` |
| range diff grepped for `allow(` / `#[ignore` / `deny(` / `expect(` | none |
| `rtk proxy git log --format='… %(trailers)'` | both `Thorrester <sjforrester32@gmail.com>`, trailers empty |
| line-length + backtick-parity + doubled-word scan | max `///` 82 (pre-existing line 14); all lines even backticks; doubled words `[]` |
| `mise exec -- cargo fmt -p wyrd-sql -- --check` | clean |
| `mise run lints` (clippy `--workspace --all-features --all-targets`) | finished, no diagnostics |
| focused `cargo nextest run` on the pinning selector | 1 passed, 73 skipped |
| `grep -rn "UNIQUE (data_tenant_id, name)\|card_kind, card_uid\|USING GIN" migrations/` | `:85`, `:84`, `:93` confirmed |
| `grep -rn "DROP CONSTRAINT" migrations/` + read each hit | drops filter `contype='c'` / `'f'` only; the `'u'` name constraint survives |

## 4. Findings

**None. The finding list is empty, and that is the complete result.**

The failure modes the round could plausibly have produced were each checked: a
false surviving claim on a credential-issuing path; a rewrap that split a backtick
span or produced an over-length `///` line; a dangling clause left by the
mid-sentence deletion; a doubled or dropped word; an argument left incomplete by
removing one of three enumerated items; a silently weakened gate; a bad commit
identity. Each came back clean.

Nothing in the remaining text would cause a maintainer to **do** the wrong thing.
The two claims the text makes load-bearing are load-bearing, so a maintainer is
correctly warned off relaxing either; and the text no longer warns them off a
change that is in fact safe, which was the defect `FIND-008-16` named. Per the
round's materiality bar nothing read-only was filed, and nothing
action-changing was found.

## 5. Result

**PASS** — repository-standards axis only.

## 6. Verification limits

- No Postgres-backed lane run. `mise run test:sql` was not re-run by this
  reviewer; the bounding claims were verified by reading the migrations and the
  projection source rather than by executing them.
- `mise run lints` finished from cache in 1.59s. It re-fingerprints on source
  change, and the fmt check and nextest run both compiled the touched crate, so
  the clean result is treated as real for this file; no from-scratch rebuild was
  forced.
- The rustdoc rendering of the block was not built. Per the subject, the
  `private_intra_doc_links` warning and the pre-existing `[IdError]`
  `broken_intra_doc_links` warning at `error.rs:204` are settled and out of scope.
- Standards axis only: no task-acceptance judgment, no Ponytail ladder audit, no
  domain review of SQL semantics beyond verifying the factual claims the doc makes.
