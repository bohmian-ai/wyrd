---
id: TASK-003
status: complete
approved_spec_revision: 5
requirements: [REQ-002, REQ-003, REQ-016]
acceptance: [AC-002, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-005, INV-007]
depends_on: [TASK-002]
---

# Declare and select the native Repository policy

Carry the six rules of **approved-declaration.md** through the existing declaration
workflow. Public shape is **contracts.md → Repository and declaration**; the old
non-Card proposal in repository-declaration.md is superseded. Successful declare
means a ready exact graph and selected Repository policy, not merely inserted rows.

## Input → owner → selected implementation

The caller supplies `wyrd declare wyrd.toml`, normal YAML Repository or existing
YAML/directory declaration. Minimal TOML selects Verifier paths/registered refs;
origin remote is only a hint, and metadata/spec overrides use the approved tables.
Task 002's real connection supplies host/repository-ID/access verification and
provider name. Outside a checkout, explicit identity supplies the hint instead.

Today `wyrd-config::WyrdConfig` is defaults-only; `wyrd-loader::load` runs local
parse→config→resolve→validate→order→diagnose, and CLI `card.rs::declare_tree`
builds DeclarationInput before invoking `Cards::declare`. Extend that workflow
for an inferred native Repository root and only the selected Verifiers plus
transitive graph. Complete local validation before registration. Server-owned
identity verification resolves the hint to canonical host/positive repository ID;
first registration infers Card name, verified overrides win and provider renames
preserve the established family. Do not move provider credentials/network IO into
the offline loader. `--check` remains local and labels remote checks deferred.

Add Repository to `wyrd-spec`'s ordinary envelope/catalog/spec/reference/binding
owners, using the closed shape in contracts, not a wrapper root or Service
alias. TOML verifier arrays expand into singular VerificationBinding entries.
Read/write/generated projections must accept the same kind. Normal YAML
Repository goes through the same lifecycle and selected-policy semantics.

Reuse the entire shared client `cards/saga` prepare/hash/submit/upload/complete
and server `components/cards/service.rs`, SQL cards/version resolution and
`queries/verification.rs::project_bindings` lifecycle. Preserve normal hashing,
auto-version/reuse and exact-pin conflict. Extend its readiness/activation
coordination: select only after all required graph/artifacts are ready, preserve
the previous selection on failure, and keep registered dependencies. Tie
completion/replay to its declaration attempt and expected prior selection:
identical target may reuse; distinct races conflict; delayed old completion cannot
undo a newer selection. A deliberately new declaration of an old exact version
can roll back. Do not conflate Card Active state with selected policy or redesign
the saga into another transaction system. Exact private coordination layout stays
at existing owners with no new PostgreSQL tables.

A binding list is complete desired state. Empty `verifier = []` selects no future
automatic reviews without disconnecting access; already accepted executions keep
their policy. Amend **wyrd-design.md → Kind catalog / Per-kind specs / Registry
lifecycle / Spec-file authoring / Workspace config / Reference-direction** and
root catalog guidance to authorize Repository, TOML inference and readiness
selection. This task introduces the closed `pull_request` binding/Trigger shape;
task 006 implements its event producer, without arbitrary event-name activation.

## Output and consumer

Return normal declaration receipt with exact Repository root/outcomes and
server-managed selected-version/binding status. Task 006 must resolve these
selected registered bindings, not TOML fetched from a PR. Task 007 establishes
deferred subscriptions from this active binding owner once deferred slots exist.
A success receipt must follow graph readiness and committed selection; earlier
registration/upload acknowledgement alone does not promise activation.

Project Repository/declaration/binding types and operations in shared Rust client,
all three SDKs, CLI, HTTP/MCP and generated wire/error outputs here. Ship authoring,
identity override, offline-check and rollback/empty-list guidance here.

## Proof

**AC-002:** Start with task 002's public connection, declare the minimal TOML via
real SDK/CLI into WyrdTestServer/Postgres, and inspect exact root/outcomes/selected
binding status. Include local and exact registered refs, unrelated YAML, offline
check without a server, unchanged/changed/exact-pin content, failed upload/readiness,
racing distinct/identical activations, delayed prior completion, intentional
rollback and empty desired list. Assert previous selection and successful
dependencies survive failed activation. Ordinary YAML/directory regression and
normal YAML Repository must use the same lifecycle, not a separate fixture path.

**AC-011 (declaration):** A reader follows authoring docs and sees matching typed
receipts/errors/served schemas across SDK, CLI and HTTP/MCP.

Tier 1: corresponding checked-in Card stories in Rust/Python/TypeScript SDKs and
MCP journeys using existing runners in README; tier 2: existing SQL Card/binding
and server declaration route `integration` owners via
`mise run test:cards:integration`; tier 3: loader/config/spec graph and validation
in owning modules via affected shared/Wyrd lanes. These describe new tests,
not invented existing names. Run README contract/codegen/type checks plus
`mise run check:tenant-isolation`. Task 006's seam proof must actually admit
using this receipt's selected binding; a manually seeded binding is insufficient.

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O-01 | Input → owner, ¶1 | `wyrd declare wyrd.toml` with minimal `[repository.verified_by]` declares an inferred Repository root | CLI/Rust SDK journey declares minimal TOML and receives Repository root receipt |
| O-02 | ¶1 | Normal YAML Repository Card declares through the same lifecycle | journey declares YAML Repository; same selection/binding status |
| O-03 | ¶1 | Existing YAML/directory declaration still works unchanged | existing loader/journey regression plus a YAML Service declare in the Repository journey |
| O-04 | ¶1 | Origin remote is only a hint; server verifies host/repository_id against the tenant connection | integration: hint for an unselected repository is refused `WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE` |
| O-05 | ¶1 | Metadata/spec overrides use `[repository.metadata]`/`[repository.spec]` and win (still verified) | loader unit + journey with explicit name/repository_id |
| O-06 | ¶1 | Outside a checkout explicit identity supplies the hint | loader unit: `--repository` remote outside a checkout loads; missing identity refuses |
| O-07 | ¶2 | Missing/ambiguous identity refuses clearly | loader unit: no origin/no override → typed diagnostic |
| O-08 | ¶2 | TOML resolves only selected Verifiers plus transitive graph; unrelated YAML not declared | loader unit + journey: unrelated YAML beside wyrd.toml absent from outcomes |
| O-09 | ¶2 | Local and exact registered Verifier refs both supported | journey: path ref and registered CardRef in `verifier` array |
| O-10 | ¶2 | Local validation completes before remote registration | CLI test: invalid local Verifier fails with loader error and no server call |
| O-11 | ¶2 | First registration infers Card name from repository name | journey: root name equals normalized repository name |
| O-12 | ¶2 | Provider rename preserves the established family | integration: after connection full_name changes, inferred declare keeps family name |
| O-13 | ¶2 | Explicit name conflicting with the established family / identity refuses (no silent drift) | integration: 409 identity conflict |
| O-14 | ¶2 | Provider credentials/network IO stay out of the offline loader | source proof: loader reads only `.git/config`; check:deps |
| O-15 | ¶2 | `--check` stays local and labels remote checks deferred | CLI unit: `--check` on TOML without server lists `deferred` checks |
| O-16 | ¶3 | Repository is an ordinary Card kind in envelope/catalog/spec/reference/binding owners with the closed contract shape | wyrd-spec unit: decode/round trip, unknown field refused, kind catalog |
| O-17 | ¶3 | TOML `verifier` array expands into singular VerificationBinding entries | loader unit: two verifiers → two bindings sharing runs_on |
| O-18 | ¶3 | Read/write/generated projections accept Repository (Rust/Python/TS kind, schemas) | codegen:check, Python/TS kind tests, served schema test |
| O-19 | ¶4 | Unchanged content reuses version; changed content auto-versions; changed content at an exact pin conflicts | journey: three declares with outcome/version assertions |
| O-20 | ¶4 | Selection only after all required graph/artifacts ready | integration/journey: artifact-bearing dependency pending → Repository not selected until completion |
| O-21 | ¶4 | Failed upload/readiness preserves previous selection; registered dependencies survive | journey: failed declare leaves prior selected version and dependency Cards readable |
| O-22 | ¶4 | Distinct racing activations from the same prior selection: one selects, other conflicts | integration: two attempts same expected prior, second → 409 selection conflict |
| O-23 | ¶4 | Identical racing target reuses the selected result | integration: two attempts same target both succeed |
| O-24 | ¶4 | Delayed old completion/replay cannot undo a newer selection | integration: replay of an earlier attempt after a newer selection does not change selection |
| O-25 | ¶4 | Deliberate new declaration of an old exact version rolls back | journey: re-declare old exact version → selected again |
| O-26 | ¶4 | Card Active state is not selected policy; no new PostgreSQL tables | integration: Active non-selected version reports not selected; migration adds no table |
| O-27 | ¶5 | Binding list is complete desired state; `verifier = []` selects no automatic reviews without disconnecting access | journey: empty list → selected version has no bindings; connection still active |
| O-28 | ¶5 | Closed `pull_request` binding/Trigger shape; no arbitrary event names | spec unit: `kind = "pull_request"` decodes, unknown event refused; server refuses pull_request off a Repository |
| O-29 | ¶5 | Amend wyrd-design.md sections and root catalog guidance | docs diff + docs:check |
| O-30 | Output ¶1 | Receipt returns exact Repository root/outcomes; server-managed selected-version and binding status on the Card | journey: receipt root + `status.verification` binding ids + selection |
| O-31 | Output ¶1 | Success receipt follows graph readiness and committed selection | journey: success only after selection committed (status read after receipt shows selected) |
| O-32 | Output ¶2 | Types/operations projected in shared Rust client, Rust/Python/TS SDKs, CLI, HTTP/MCP, generated wire/error outputs | per-surface journeys; codegen:check; MCP catalog |
| O-33 | Output ¶2 | Authoring, identity override, offline-check, rollback and empty-list guidance | docs guide + docs:check |
| O-34 | Proof AC-011 | Typed receipts/errors/served schemas match across SDK, CLI, HTTP/MCP | journeys assert same codes; served schema contains Repository |
| O-35 | Proof | tenant isolation and contract checks pass | check:tenant-isolation, codegen:check |

## Evidence

Journeys: `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs` (Rust SDK and CLI), `sdks/wyrd-sdk-python/tests/integration/test_repository_declaration.py`,
`sdks/wyrd-sdk-ts/wyrd/tests/integration/repository-declaration.test.ts` (same four
scenarios and codes), MCP `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/github.rs::an_agent_reads_the_selected_repository_policy`.
Story fixtures: `fixtures/cards/repository_declaration/`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| O-01 minimal TOML declares inferred Repository root | `wyrd-config/src/config.rs::RepositoryConfig`, `wyrd-loader/src/repository.rs`, `wyrd-cli/src/card.rs::declare_tree` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:171`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:355` | Rust journey `toml_policy_is_selected_only_once_declared`, `cli_checks_offline_and_declares_the_policy` | PASS |
| O-02 YAML Repository same lifecycle | `Spec::Repository` through `cards/saga` and `components/cards/service.rs` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:254-259` | `yaml_repository_declares_through_the_same_lifecycle` (Rust/Python/TS) | PASS |
| O-03 ordinary YAML declaration unchanged | `wyrd-sql/src/queries/cards/repository.rs` `COALESCE(ready)` for non-Repository roots | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:269-280` (YAML Service declares, carries no Repository status), `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:162`, `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1374` | Rust journey; `declare_and_pull::*` 15/15 in `mise run test:cards:integration` | PASS |
| O-04 remote is a hint; unselected repo refused | `components/cards/service.rs::resolve_repositories` via `GithubConnections::verify_repository` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:285` | `repository_identity_is_verified_by_the_connection` (Rust/Python/TS) | PASS |
| O-05 metadata/spec overrides win, still verified | `RepositoryMetadata`/`RepositoryIdentity`; `resolve_repositories` prefers `repository_id` over the remote | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:312` (name override wins), `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:317` (`repository_id = "1"` from an `acme/web` checkout verifies to `acme/api`), `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:323` (later inferred declare joins the family), `crates/shared/wyrd-loader/src/lib.rs:555` | Rust journey; wyrd-loader lib | PASS |
| O-06 explicit identity outside a checkout | `--repository` → `load_with_repository` | `crates/shared/wyrd-loader/src/lib.rs:555` | `cargo nextest run -p wyrd-loader --lib` | PASS |
| O-07 missing/ambiguous identity refuses clearly | loader diagnostics | `crates/shared/wyrd-loader/src/lib.rs:615` (missing → `REPOSITORY_IDENTITY_MISSING`), `:621` (remote naming no repository), `:632` (`--repository` without a `[repository]` table); `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:378-381`, Python `test_repository_declaration.py:168`, TS `repository-declaration.test.ts:140` (CLI surfaces carry the diagnostic) | wyrd-loader lib 56/56; Rust/Python/TS journeys | PASS |
| O-08 only selected Verifiers; unrelated YAML absent | loader builds root from `[repository.verified_by]` only | `crates/shared/wyrd-loader/src/lib.rs:502`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:178` | loader lib; Rust journey | PASS |
| O-09 local and registered refs | `RepositoryBindings::verifier: Vec<Ref>` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:193` (`changed.toml` = path + registered answer-check → 2 bindings, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:196`) | Rust/Python/TS journeys | PASS |
| O-10 local validation before registration | loader validates listed Verifiers before `Cards::declare` | `crates/shared/wyrd-loader/src/lib.rs:635`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:348` (no client resolved) | loader lib; CLI journey | PASS |
| O-11 first registration infers name | `wyrd-spec/src/card/repository.rs::infer_card_name` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:172` | Rust journey | PASS |
| O-12 rename preserves family | `repository_family_name` lookup in `resolve_repositories` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:307` | Rust journey (rename observed through reconnect) | PASS |
| O-13 explicit conflicting name refused | `WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:298` | Rust/Python/TS journeys | PASS |
| O-14 no provider IO in loader | `wyrd-loader/src/repository.rs:128` reads only `.git/config` | `crates/shared/wyrd-loader/src/repository.rs:175`; dependency proof: `cargo tree --locked -p wyrd-loader -e normal` lists 247 crates, none matching `reqwest`, `hyper`, `tokio`, `http`, `octocrab` or `ureq` | wyrd-loader lib; cargo tree | PASS |
| O-15 `--check` local, deferred labels | `wyrd-cli/src/card.rs::check_tree` | `crates/wyrd/wyrd-cli/src/card.rs:1154`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:335` | wyrd-cli lib; CLI journey (Rust/Python/TS) | PASS |
| O-16 Repository ordinary kind, closed shape | `wyrd-spec/src/card/repository.rs`, `envelope.rs` | `crates/wyrd-spec/src/card/mod.rs:2591`, `crates/wyrd-spec/src/card/repository.rs:221` | wyrd-spec lib | PASS |
| O-17 verifier array → singular bindings | loader expansion | `crates/shared/wyrd-loader/src/lib.rs:502`, `crates/wyrd/wyrd-config/src/config.rs:305` | loader/config lib | PASS |
| O-18 projections accept Repository | `gen_schemas.rs`, `gen_ts_types.py`, Python `Kind::Repository`, TS exports | Python `first.root.kind == CardKind.Repository`; TS `toMatchObject({kind:"Repository"})`; `pg_openapi_contract.rs:1537-1549` | `codegen:check` rc=0; py/ts journeys; openapi 24/24 | PASS |
| O-19 reuse / auto-version / pin conflict | normal saga hashing; `seed_version` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:188`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:193`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:210` | Rust/Python/TS journeys | PASS |
| O-20 select only after graph ready | `settle_repository_selection` waits for every owned Card Active | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1388` (pending root), `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1503` (Active root waits for a pending dependency with an artifact hash), `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1522` (selects once it activates) | `pg_cards_declare::repository_selection_settles_once_against_the_expected_prior` | PASS |
| O-21 failed declaration keeps selection; deps survive | settle not reached / `Pending` | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1426` and `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1503-1504` (unready graph keeps previous selection), `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:210-219` (refused declarations keep selection; registered dependency readable) | SQL integration; Rust/Python/TS journeys | PASS |
| O-22 distinct race: one selects, other conflicts | CAS on expected prior; `settle_selection` maps `Conflict` | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1441`; `crates/wyrd/wyrd-server/src/components/cards/service.rs:3453` (`WYRD_REGISTRY_409_REPOSITORY_SELECTION_CONFLICT`, on every retry) | SQL integration; `wyrd-server` `components::cards::service::pg_tests::a_raced_repository_declaration_answers_a_selection_conflict` | PASS |
| O-23 identical target reuses | target equality | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1407` | SQL integration | PASS |
| O-24 delayed completion cannot undo newer | settled outcome kept | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1441-1448` | SQL integration | PASS |
| O-25 rollback by old exact version | new operation with old target | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:224-226`, `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1461` | Rust journey; SQL integration | PASS |
| O-26 Active ≠ selected; no new tables | `selected_at` column + partial unique index | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:200`, `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1449`; migration has 0 `CREATE TABLE` | journey; `grep -ci "create table"` = 0 | PASS |
| O-27 empty list, connection stays active | complete desired state | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:234-237` | Rust/Python/TS journeys | PASS |
| O-28 closed pull_request; refused off Repository | `TriggerActivation::PullRequest {}`; `resolve.rs:376-388` | `crates/wyrd-spec/src/card/mod.rs:2617-2618`, `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:264` | wyrd-spec lib; Rust journey | PASS |
| O-29 design + root catalog amended | `architecture/wyrd-design.md` (Kind catalog, Repository per-kind, Trigger, Repository selection, Pre-declaration, Reference slots, Workspace config, Reference-direction); `AGENTS.md` 13 kinds | docs diff | `mise run docs:check` rc=0 | PASS |
| O-30 receipt root/outcomes + status | `RepositoryStatus` on `status.repository`; `status.verification` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:171-183` | journeys | PASS |
| O-31 receipt follows committed selection | settle at end of `initialize_uploads`/`complete_card` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:180` (status read right after receipt is selected) | journeys | PASS |
| O-32 projections on every surface | Rust client/SDK, CLI, Python, TS, HTTP route (`POST /v1/cards` driven by every SDK journey), MCP read tools (`cards.get`/`cards.list`; MCP has no declare tool) | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:171`; Python `test_repository_declaration.py:87`; TS `repository-declaration.test.ts:70`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/github.rs:253`; `pg_openapi_contract.rs:1537-1549` | journeys per surface; `codegen:check` | PASS |
| O-33 authoring guide | `docs/src/content/docs/build-and-verify/declare/declare-a-repository.md` embeds the journey fixtures | docs page | `mise run docs:check` rc=0 | PASS |
| O-34 same receipts/errors/served schemas | identical codes asserted in the three SDK journeys | Rust/Python/TS each assert `403_REPOSITORY_INACCESSIBLE`, `409_REPOSITORY_IDENTITY_CONFLICT`, `409_SPEC_DRIFT`, `422_UNRESOLVED_DEPENDENCY`, `400_INVALID_ENVELOPE` with `REPOSITORY_IDENTITY_MISSING`; served `Spec`/`CardKind` at `pg_openapi_contract.rs:1537-1549`; error catalog page regenerated by `docs:generate` | journeys; openapi 24/24; `docs:check` | PASS |
| O-35 tenant isolation + contract checks | raw-query justification in `queries/cards/repository.rs` | — | `check:tenant-isolation` rc=0, `codegen:check` rc=0 | PASS |
| O-36 (unledgered, task ¶5) already accepted executions keep their policy | no Repository execution exists before task 006 admits one | — | — | OPEN: not observable in this task; route to task 006's admission proof |
| O-37 (unledgered, Output ¶1) task 006 resolves selected registered bindings, not PR TOML | selected bindings served on `status.repository`/`status.verification` | `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs:180-183` | Rust journey | PASS for the producer side; consumer proof belongs to task 006 |
| O-38 (unledgered, Proof AC-002) failed artifact upload in a Repository graph keeps the previous selection | an unverified upload leaves its Card pending, which `settle_repository_selection` never selects | `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1503-1504` models the pending upload at the owner | SQL integration | OPEN: a journey cannot fail an upload server-side. The client verifies each artifact's size and SHA-256 before submitting (`crates/shared/wyrd-client/src/cards/saga/hash_artifacts.rs:46`), and no storage-fault harness exists; adding one is a new test mechanism, routed to `$wyrd-plan` |

Non-goals stayed excluded: no PR event producer (task 006), no deferred
subscriptions (task 007), no new PostgreSQL tables, no provider IO in the loader.

### Diagnoses

- **Symptom:** every plain YAML declare returned `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`.
  **Evidence:** trace `decoding column "ready": unexpected null`. **Cause:** the
  selection settle's LEFT JOIN produced a NULL `ready` when the operation has no
  Repository target. **Fix site:** `wyrd-sql/src/queries/cards/repository.rs`
  (`COALESCE(..., false)`), the single owner every `complete_card` routes through;
  regression `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs:1374`.
- **Symptom:** `WYRD_LOADER_400_PATH_ESCAPE` for `../code-review.yaml`. **Cause:**
  the loader workspace is the `wyrd.toml` directory. **Fix site:** fixture layout
  (`policies/*.toml` copied to the checkout root), no loader change.
- **Symptom:** remote host did not match the configured mock host. **Cause:**
  `RepositoryRemote::parse` dropped a non-default web port. **Fix site:**
  `wyrd-spec/src/card/repository.rs` (RED `web_remote_keeps_its_port`).
- **Symptom:** rename via `installation_repositories added` not followed.
  **Cause:** task 002 follows only removals. **Fix site:** test observes a rename
  by disconnect + reconnect, which re-reads repositories; no production change.

### Process exceptions

- Spec, loader, config, SQL, server and projection code were written before
  their tests. RED was observed only for `web_remote_keeps_its_port` and the
  OpenAPI `Spec` `oneOf` count (14 → 15). The SQL, CLI, Python, TypeScript and
  MCP journeys, the unready-selection case and the off-Repository refusal passed
  on first run against existing behavior.

### Commands

- `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run check:tenant-isolation`, `mise run check:deps`, `mise run docs:check`: rc=0.
- `mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-loader -p wyrd-config -p wyrd-cli -p wyrd-client --lib`: 1215 passed.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server -p wyrd-sql --lib'`: 693 passed.
- `mise run test:cards:integration` (includes `pg_cards_declare`, `declare_and_pull`, `repository_declaration::`): rc=0.
- Python `test_repository_declaration.py` 4 passed; TS `repository-declaration.test.ts` 4 passed; MCP `an_agent_reads_the_selected_repository_policy` passed; `pg_openapi_contract` 24 passed.
- `git diff --check`: clean.

### Checker round 1

A fresh read-only checker named O-05, O-07, O-14, O-20, O-21, O-22, O-31, O-32,
O-34 and three unledgered requirements. These are closed by:

- the override, CLI-surface identity diagnostic and YAML Service regression
  assertions in `sdks/wyrd-sdk-rust/tests/integration/repository_declaration.rs`;
- the pending-dependency graph case in `crates/wyrd/wyrd-sql/tests/integration/pg_cards_declare.rs`;
- the server-owner selection-conflict test in `crates/wyrd/wyrd-server/src/components/cards/service.rs::pg_tests`;
- the loader dependency proof.

All of these passed on first run against existing behavior, which is a process
exception.

Mechanical deviations:

- The tier-2 "server declaration route" proof runs through the `POST /v1/cards`
  call in every SDK journey.
- MCP exposes Cards read tools only, so MCP proves the read projection.

O-31 rests on two proofs: the journey's status read after the receipt, and the
owner-tier proof that a pending graph never selects. O-36 and O-38 stay open.

### Status

`BLOCKED` on two items:

- **O-36** needs Repository executions, which task 006 admits.
- **O-38** needs a server-side upload-fault harness that does not exist.

Route both to `$wyrd-plan` for task correction. Every other ledger row is
`PASS`.

## Completion

Complete following the Task 003 PASS verdict
(`review/003-repository-declaration-r3-e8839b80e/verdict.md`) for Wyrd candidate
`e8839b80e62c24ad78d28a29f4b905191a35ac8d`, which carries the TASK-003-R1 and
TASK-003-R2 remediations. FIND-003-1 through FIND-003-8 are closed; no findings
remain.
