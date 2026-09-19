# Wave 2 validation — TASK-008 review `task-008-r4`

Reviewer: `ponytail-rev` (fresh, independent). Candidate `c34b9d1e0`, base
`f102e50ee` (round-2 candidate). No source file changed by the review. Working
tree at review end carries only the untracked `review/task-008-r4/` directory.

The reviewer's report write was blocked by the harness; this file is its report,
persisted by the orchestrator verbatim in substance, with the orchestrator's own
independent re-verification recorded in §1a.

## 1. Independent facts established before ruling

| Fact | Evidence |
|---|---|
| Byte identity of the SQL const | `rtk proxy git show <rev>:crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs \| awk '/^const SERVICE_ACCOUNT_BY_CARD_REF_SQL/,/^        "#;/' \| md5` → `2298c9e6b9cf3e40edb952818226b944` at **both** `f102e50ee` and `c34b9d1e0`. Claim holds. |
| No caller / write-side / migration / fixture change | `rtk proxy git diff --stat f102e50ee..c34b9d1e0` → one source file (`service_accounts.rs`, +42/−10); everything else is `review/task-008-r3/*.md`. |
| `FIND-008-15` closed | Selector confirmed via `cargo nextest list -p wyrd-sql --lib`; `PASS (1/1) queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`. The test asserts `card_ref @> $3`, `principal_kind = $2`, `ORDER BY created_at, id`, `LIMIT 1`, and `!card_ref::text` (`service_accounts.rs:448-454`) — strengthened 4 → 6 falsifiable assertions, nothing deleted or relaxed. |
| Rustdoc fact — `UNIQUE (data_tenant_id, name)` | `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:51` (live, unconditional). True. |
| Rustdoc fact — `name`-column coupling | `crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:58-75`: the same `card.metadata.name` is bound both into `card_ref.name` and into the `name` column. True. |
| Rustdoc fact — containment relaxes `space` | `CardRef.space: Option<SpaceName>` with `#[serde(default, skip_serializing_if = ...)]` (`crates/wyrd-spec/src/reference.rs:24-29`). True. |
| Authorship | `708f01ec9` and `c34b9d1e0` author **and** commit `Thorrester <sjforrester32@gmail.com>`; no AI co-author trailer. |
| No gate circumvention | No new `#[allow]`, no `#[ignore]`, no deleted test, no weakened check, no added repository check. |

### 1a. Orchestrator re-verification

Independently confirmed before persisting this ledger:

- The false clause is present verbatim at `service_accounts.rs:22-24`:
  `and every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)`,
  introduced by the sentence `What bounds this to one intended row is not the
  predicate but three facts outside it` at `:20-21`.
- `mise.toml:543-551` — `check:docs` sets
  `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links"` and covers
  only `wyrd-spec`, `wyrd-auth-issue`, `wyrd-auth-verify`. `wyrd-sql` is not in
  the lane, and `private_intra_doc_links` appears nowhere in `mise.toml` or
  `.github/workflows/`. The `RR4-1` gate claim is therefore not a repository
  obligation.
- The public function's own doc (`:175-183`) states intent, the JSONB binding,
  the durable key `(card_kind, card_uid)`, the GIN index, and `# Errors`.

## 2. Per-proposed-finding validation

| # | Wave 1 source(s) | Claim | Independent evidence | Ruling |
|---|---|---|---|---|
| A | `TR4-1` (DRIFT), `DA4-1` (INCORRECT) | The rustdoc's third bounding "fact" — *"every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)"* — is false | `service_accounts.rs:22-24`. `IssueKeyArgs` exists **only** at `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:18` (a `clap::Args` struct); `grep -rn IssueKeyArgs --include='*.rs'` returns that struct and this doc line and nothing else — it is not a caller of this query. The query's three real callers are `crates/wyrd/wyrd-auth/src/issue_api_key.rs:94`, `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:591`, and `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:155`. None requires a space: `IssueKeyRequest.card_ref` is a plain `CardRef` with no space requirement (`crates/wyrd-spec/src/auth/issue_key.rs:13-22`); the `/auth/issue-key` handler validates only RBAC and passes the body through (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:260-300`); `principal_kind_for_card` inspects `kind` only (`issue_api_key.rs:230-238`); `RequestedSubject::CardRef { card_ref: CardRef }` deserializes with no space check (`crates/wyrd-spec/src/auth/token.rs:69-73`); and `jwt_bearer` passes a `card_ref` read back out of `auth_workload_bindings`. **False at every one of the three paths**, not merely at some — the CLI's required `--space` constrains one optional client of one of the three routes and enforces nothing at the query. | **CONFIRMED (merged)** — one finding, `INCORRECT`, filed under reused ID `FIND-008-16`. |
| B | `RR4-1` (VIOLATION) | The public `service_account_by_card_ref` intra-doc-links the private const, emitting `rustdoc::private_intra_doc_links`, leaving the invariant out of public docs | The warning is real and new: `mise exec -- cargo doc -p wyrd-sql --no-deps` → *"public documentation for `service_account_by_card_ref` links to private item `SERVICE_ACCOUNT_BY_CARD_REF_SQL`"* at `:177:43`; `708f01ec9` replaced a link to the **public** `tenant_admin_principal_id` with this one, so it did not exist at `f102e50ee`. But it is neither gated nor a repository defect pattern: `mise.toml:543-551` (`check:docs`) denies only `missing_docs` and `broken_intra_doc_links` and covers only `wyrd-spec`, `wyrd-auth-issue`, `wyrd-auth-verify` — `wyrd-sql` is not in the lane; nothing in `mise.toml`, `Cargo.toml`, or `.github/workflows` mentions `private_intra_doc_links` or sets `RUSTDOCFLAGS` elsewhere. Precedent: the same run shows a *stronger* pre-existing `broken_intra_doc_links` warning in this crate (`crates/wyrd/wyrd-sql/src/error.rs:204`, unresolved `IdError`), and `cargo doc -p wyrd-client --no-deps` emits **two** public→private link warnings (`with_sink` → `WriterPool::new`; `http` → `HttpTransport::authenticated_url`) in the crate AGENTS.md §5 names *"the canonical Wyrd pattern"*. Substance: the public function's doc (`:175-183`) states intent, mechanism, durable key, index, and `# Errors`, and round 2's remediation criterion 3 **prescribed** putting the predicate explanation on the const; §16 nowhere requires an invariant to be reachable from generated *public* rustdoc. | **REJECTED** — no violated obligation; a lint preference, not a defect. |

### Dedupe and contradiction resolution

`TR4-1` and `DA4-1` name identical text at an identical location; merged.
The classification contradiction resolves in favour of **`INCORRECT`**: round 2's
prescription named three facts (the relaxation, the `UNIQUE`+projection bound,
and the ordered `LIMIT 1`) and did not forbid additional *accurate* context — so
the harm is not the deviation but that the added sentence is falsified by the
tree, on a credential-issuing path. `DRIFT` describes the shape of the edit and
understates the defect.

A and B are **not** one edit. A is closed inside the const's doc; B concerns a
link in a different doc block and is rejected outright. One correction only.

## 3. Final deduplicated ledger

### `FIND-008-16` — INCORRECT (reused ID, **not** closed)

- **Wave 1 sources:** `TR4-1`, `DA4-1`. **CONFIRMED** (merged).
- **ID choice:** reused rather than minting `FIND-008-17`. `FIND-008-16` was
  *"the rustdoc justifies single-row resolution with a bound that does not
  exist"*. The same doc comment, moved onto the const, still justifies
  single-row resolution with a bound that does not exist — different false
  bound, same defect, same location, same obligation. Round 2's `FIND-008-16`
  is therefore **not closed**; its classification is corrected from `DRIFT` to
  `INCORRECT` per §2.
- **Violated obligation:** AGENTS.md §16 (*"Rustdoc MUST explain intent … and
  relevant invariants or side effects"*; *"Documentation is part of
  implementation correctness"*) and §12 Completion Standard.
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:22-24`
  (within the const rustdoc at `:10-29`); the *"three facts"* count at `:20-21`
  is part of the same sentence.
- **Evidence:** §2 row A. `IssueKeyArgs` is a `wyrd-cli` clap struct, not a
  caller; all three real callers accept a `CardRef` whose `space` is an
  unvalidated `#[serde(default)] Option` on every path reaching this query.
- **Observable consequence:** the doc's stated purpose is to tell the next
  maintainer exactly what holds this credential-issuing lookup to one row. Two
  of its three named facts are enforced in the tree; the third is enforced
  nowhere, so a maintainer counts three guards where two exist and may relax one
  of the real two believing a caller-side guarantee still covers it. The
  adjacent sentence — *"none of that chain is enforced here"* — is what makes
  the invented link load-bearing rather than incidental.
- **Correction (decision-complete, minimum):** edit only that sentence in the
  existing doc comment. Delete the clause
  ``, and every caller passing a fully qualified ref (`IssueKeyArgs::space` is required)``
  and change `three facts outside it` to `two facts outside it`. Per the ladder,
  deleting a false clause is smaller than writing a true one, and the two
  surviving facts are independently verified true (§1) and complete — they are
  what actually bounds the query, exactly what round 2's prescription named. Do
  not substitute a "callers usually pass a space" hedge: the CLI's required
  `--space` is a property of one client, not of this query, and stating it here
  is what produced the error. Add no test, file, assertion, or predicate change.
- **Closure proof (smallest credible; no new dependency or harness):**

  ```bash
  grep -n 'IssueKeyArgs\|three facts' crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs   # must print nothing
  mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
    -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
  mise run fmt
  mise run lints
  ```

  (Selector confirmed present via `mise exec -- cargo nextest list -p wyrd-sql --lib`.)

## 4. Rejected findings and why they are omitted

- **`RR4-1` (private intra-doc link on a public credential-path function).** The
  warning is real and new, but violates no repository obligation: `check:docs`
  (`mise.toml:543-551`) neither denies `rustdoc::private_intra_doc_links` nor
  includes `wyrd-sql`; no other lane, `RUSTDOCFLAGS`, or CI path surfaces it;
  `wyrd-sql` already carries an ungated *stronger* rustdoc warning at
  `error.rs:204`, and `wyrd-client` carries two public→private link warnings of
  exactly this shape. Treating one new instance as a `VIOLATION` invents a check
  the repository has not adopted, which AGENTS.md §12's adding-and-retiring-checks
  reasoning weighs against. On substance, the public function's doc already
  carries intent, mechanism, durable key, index, and `# Errors`, and round 2's
  criterion 3 explicitly placed the predicate explanation on the const;
  following an approved prescription is not a §16 defect.
- **The two limits `domain-rev-auth-sql` recorded as non-findings — both
  agreed.** (1) The corrected test asserting nothing about `data_tenant_id = $1`
  is not a finding: criterion 1 enumerated the required assertions, and tenant
  scoping is a `TenantConn`/RLS property pinned in
  `crates/wyrd/wyrd-sql/tests/pg_migration.rs`, not a property of this const's
  text; the non-goals forbid adding coverage here. (2) The space relaxation
  being measured nowhere in the repository's own tests is not a finding: round 2
  rejected `DD3-4` because the CLI journey pins the semantics at the tier
  AGENTS.md §11 ranks highest, and the round-3 remediation task's non-goals
  explicitly prohibit adding a Postgres-backed `wyrd-sql` test for the matching
  semantics. Neither is promoted; no new evidence, no obligation in the approved
  task.

## 5. Prior-finding closure

- `FIND-008-1` .. `FIND-008-7` (prior `task-007-008` review): closed by spec
  revision 7 or by landed work; unchanged.
- `FIND-008-8` .. `FIND-008-14`: **closed.** Unchanged since round 2; the
  `f102e50ee..c34b9d1e0` write set touches only `service_accounts.rs`, so
  nothing could have regressed them.
- `FIND-008-15`: **closed.** Verified by run (§1).
- `FIND-008-16`: **NOT closed.** Criterion 3's three literal requirements were
  met, but the same doc comment now carries a different nonexistent bound; the
  defect the finding named survives. Retained above under its original ID.

## 6. Spec revision

**Not required.** The single retained correction deletes one clause of a doc
comment. No product behavior, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, or persistent-data decision is touched.
No `SPEC_REVISION_REQUIRED`.

## 7. Verification limits

- `mise run test:sql` was not re-run in Wave 2 (all three Wave 1 reviewers ran
  it, green); the focused selector above is the check that fails if the retained
  defect's file stops compiling. Per the subject's rules no broad aggregate was
  relied on as evidence.
- `708f01ec9`'s **commit message body** repeats a weakened form of the false
  clause (*"and callers passing a qualified ref"*). History rewriting is
  prohibited by the remediation task's constraints, so this is a recorded limit,
  not a finding; the correction removes the claim from the durable artifact (the
  rustdoc).
- `wyrd-server::auth_e2e::cache_ttl_path_also_flips_verdict` remains the known
  pre-existing red, reproduced by the implementor against a change-free commit;
  not exercised in this round and not attributed to this work.
- The only thing that blocked the reviewer was the harness refusing its report
  write; the orchestrator persisted the report here.

## 8. Overall

**Ledger non-empty:** one retained finding (`FIND-008-16`, `INCORRECT`), one
rejected Wave 1 finding (`RR4-1`).
