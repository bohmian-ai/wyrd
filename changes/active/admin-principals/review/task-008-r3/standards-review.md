# TASK-008 review `task-008-r3` — repository-standards review

Reviewer: `repo-rev` (independent repository-standards reviewer).
Scope: whether every changed surface in the **cumulative** candidate complies
with the applicable repository authority. Not task acceptance, not the Ponytail
audit, not optional improvements.

| Item | Value |
|---|---|
| Candidate HEAD | `f102e50eea437ff4ba29571412197a1c4923cbbc` |
| Remediation commits | `9fe02aa2e`, `67c9d2df6`, `f102e50ee` |
| Cumulative closeout range | `289978fcc~1..f102e50ee` |
| Branch base | `968c92641` |
| Working tree at report time | clean apart from this review directory |

**Overall result: FAIL** — one confirmed red test the candidate itself broke
(`RR3-1`), and one security-relevant behavior widening introduced by the
remediation with no covering decision or test (`RR3-2`).

---

## 1. Authority coverage

Authority selected through `architecture/references/README.md`'s authority
hierarchy and canonical routes. Read in full: `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/references/README.md`, `TESTING.md`,
`mise.toml` (task lanes for the touched surfaces),
`architecture/references/languages/errors.md` route target (stable errors and
CLI/HTTP mapping), `architecture/references/languages/testing-workflows.md`
route target (journey/unit tiers and gates).

| Changed surface | Applicable authority |
|---|---|
| `crates/wyrd/wyrd-cli/src/cli.rs` (new `mod tests`) | AGENTS.md §5, §11 (tier 3), §12, §16; agent-rules Rust-documentation and struct-centered rules |
| `crates/wyrd/wyrd-cli/src/error.rs` (4 variants deleted) | AGENTS.md §4 (`WyrdError` derive catalog, no hand-written `code()`/`status()`/`remediation()`/problem-json), §9, §12; agent-rules generated-artifact rule; router → `languages/errors.md` |
| `crates/wyrd/wyrd-cli/src/auth/login.rs` | AGENTS.md §4 (secret handling, `thiserror`), §11 (tier 3), §16 (`# Errors`) |
| `crates/wyrd/wyrd-cli/src/auth/issue_key.rs` | AGENTS.md §4, §16; agent-rules Rust-documentation rule |
| `crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs` (new) | AGENTS.md §11 (tier 1, lane registration), §16; `TESTING.md` "The three tiers", "Layout", "Writing a test"; agent-rules external-`tests/`-file and family-lane rules |
| `crates/wyrd/wyrd-cli/tests/cli.rs` | AGENTS.md §11; `TESTING.md` "Layout" |
| `crates/wyrd/wyrd-cli/tests/principal_journey.rs` | AGENTS.md §4 (`.clone()`), §5, §6, §16 |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs` | AGENTS.md §9 (tenant isolation on every path), §11, §12, §15 (`wyrd-sql` is the durable Postgres layer), §16; agent-rules `TenantConn`/RLS boundary rule |
| `crates/wyrd/wyrd-server/src/http/error.rs` | AGENTS.md §4, §9; router → `languages/errors.md` |
| `crates/wyrd/wyrd-server/src/components/eval/routes.rs` (`check_lease` doc) | AGENTS.md §6, §16; agent-rules Rust-documentation rule |
| `crates/wyrd/wyrd-auth/src/error.rs`, `crates/wyrd-spec/src/error.rs` | AGENTS.md §4, §8, §9; agent-rules generated-artifact rule |
| `openapi.yaml`, `crates/wyrd-spec/{schemas,tests/schemas}/ui_problem_examples.json`, `docs/src/content/docs/api/openapi.md` (cumulative range) | agent-rules generated-artifact rule; AGENTS.md §11 (`codegen:check`) |
| `docs/src/content/docs/for-agents/workflow.svx` | AGENTS.md §11 (`docs:check`), §15 (fix the root cause once) |
| `docs/scripts/generate_api_docs.py` (cumulative range) | AGENTS.md §11 (`py:format`, `py:lints`), §16 |
| `mise.toml` (`cli:dev-bootstrap` deleted) | AGENTS.md §12 "Adding And Retiring Checks", §11 |
| `changes/active/admin-principals/review/task-008-r2/*` | AGENTS.md §14 (packet location); no code authority |
| Commits `9fe02aa2e`, `67c9d2df6`, `f102e50ee` | AGENTS.md §13 |

No applicable authority was unavailable. Coverage is complete.

---

## 2. Per-rule result

### AGENTS.md §4 Rust Core Rules

| Rule | Result | Evidence |
|---|---|---|
| Domain newtypes for durable identifiers | PASS | `service_accounts.rs:174-176` takes `&CardRef`, `conn.data_tenant_id()`; the journey carries `CardName`/`VersionBlock`/`SpaceName` through `card.name`/`card.version`/`card.space` (`auth_issue_key_journey.rs:74-86`) |
| Borrowed params where ownership not needed | PASS | `run_cli_with_credential(&[&str], &str, &str)` (`principal_journey.rs:26-30`); `issued_key(&str)` (`auth_issue_key_journey.rs:27`) |
| `.clone()` is a design question | PASS | `base_url.clone()` (`auth_issue_key_journey.rs:88`, `:112`) and `key.clone()` (`:119`) are small boundary values crossing into `spawn_blocking`, which needs `'static` ownership; `card_ref().expect(..).clone()` (`:60`) takes ownership of a borrowed fixture value. The `run_cli` → `run_cli_with_credential` generalization added no clone: the variable is `&str` and the credential still moves once per call (`principal_journey.rs:26-45`) |
| `thiserror` in libraries / `anyhow` only in binaries | PASS | `WyrdCliError` remains a `thiserror` enum; no `anyhow` introduced (cumulative diff has no `anyhow` addition) |
| `WyrdError` derive catalog, no hand-written `code()`/`status()`/`remediation()`/problem-json | PASS | The four deleted variants took their `#[wyrd_error(...)]` metadata with them; nothing hand-written was left. `WyrdCliError::{code,status,remediation}` are derive-generated. `CliBoundaryError::{code,status,title,remediation,details}` (`error.rs:395-451`) are two-arm dispatchers that call the derive-generated method on the local arm and `as_problem_json()` on the catalog arm — not a parallel table. No reference to the four removed codes survives anywhere in `crates`, `sdks`, or `docs` |
| `tracing` with structured fields | PASS | no diagnostics added or changed; `IssueApiKey::execute` retains `#[tracing::instrument(..., fields(card_ref = %request.card_ref, actor = %actor.id), err)]` (`issue_api_key.rs:80-86`) |
| `secrecy::SecretString` / redacted `Debug` for secret-bearing types | PASS | `login.rs` now refuses without the paste in the payload (`login.rs:115-121`); the journey handles the plaintext key as a local `String` inside one test function, adds no secret-bearing struct, and the response field is still behind `expose()` (`issue_key.rs:83`) |
| No `unwrap()` in non-test code on env/fs/network/parsing/input/db/storage/external paths | PASS | `mise run check:unwrap-audit` → exit 0. Non-test additions use `?` (`login.rs:115`, `service_accounts.rs:178-182`) |
| `expect()` only for named invariants | PASS | every new `expect` is in test code and names the invariant (`auth_issue_key_journey.rs:52`, `:57`, `:60`, `:84`, `:124`) |
| No wildcard deps, no per-crate profile blocks | PASS | no manifest changed in the remediation range |
| No `#[allow(clippy::...)]` without a `// justification:` line (agent-rules) | PASS | `mise run check:clippy-allow-audit` → exit 0; the cumulative diff adds no `#[allow]` at all |

### AGENTS.md §5 Abstraction Rules / required struct-centered style

| Rule | Result | Evidence |
|---|---|---|
| Concrete types; no single-impl trait | PASS | no trait added anywhere in the remediation |
| No zero-sized utility struct wrapping unrelated functions | PASS | `run_cli_with_credential`, `run_cli_async_with_credential`, `issued_key` are stateless deterministic helpers with no owned dependencies — the §5 free-function exemption, and the shape the crate already used (`principal_journey.rs` had exactly this before) |
| Stateful capability has one owning concrete struct | PASS (N/A) | nothing stateful added; `IssueKeyArgs` remains a clap value struct and `IssueApiKey` remains the owning service |
| Materially changed Rust must not preserve functional drift (agent-rules) | PASS | the generalization threads no shared dependency or context: one `&str` variable name and one credential, both per-call arguments |

### AGENTS.md §6 Async and Runtime Rules

| Rule | Result | Evidence |
|---|---|---|
| Every `async fn` earns its state machine | PASS | `run_cli_async_with_credential` awaits a real `spawn_blocking` join (`principal_journey.rs:49-57`); `run_cli_async` is a one-line composition over it (`principal_journey.rs:60-62`); the new journey awaits IO throughout. `issued_key` is correctly synchronous (`auth_issue_key_journey.rs:27`) |
| The deleted sync wrapper | PASS | no dead sync wrapper remains: `run_cli_with_credential` is the one synchronous entry and it is live, called from `run_cli_async_with_credential:53` |
| No ad hoc Tokio runtimes in library code | PASS | none added; the journey uses `#[tokio::test]` |

### AGENTS.md §9 Server and Contract Rules

| Rule | Result | Evidence |
|---|---|---|
| Typed request/response bodies | PASS | `issue_key` still takes `Json<IssueKeyRequest>` and returns `Json<IssueKeyResponse>` (`wyrd-server/src/components/auth/routes.rs:260-265`) |
| `WyrdError`-derived structured errors on public handlers | PASS | `auth_error_to_wyrd` still maps through the catalog; only the message literal changed (`http/error.rs:202-204`) |
| `#[tracing::instrument]` with scrubbed args on write handlers | PASS | unchanged; `IssueApiKey::execute` skips `conn` and `request` |
| Audit/request context on durable writes | PASS for the mechanism, see `RR3-2` for its resource string | `routes.rs:274-290` authorizes then records before opening the tenant transaction |
| No compatibility routes or aliases | PASS | no route added; one broken `mise` task removed |
| Tenant isolation preserved on every path | PASS | `service_account_by_card_ref` still runs on `&mut TenantConn<'_>` with `data_tenant_id = $1` under RLS, and still commits nothing itself (`service_accounts.rs:169-183`). The explicit tenant predicate is pre-existing and unchanged. No `OperatorPool` or `PgPool` was introduced; `mise run check:client-tier` and `check:sdk-client-tier` → exit 0 |
| Intra-tenant *space* scoping of the lookup | **FAIL** | `RR3-2` |

### AGENTS.md §11 Testing Workflow and `TESTING.md`

| Rule | Result | Evidence |
|---|---|---|
| Every user-facing capability ships a tier-1 journey | PASS | `auth_issue_key_cli_journey` drives the shipped binary twice against a real server, issuing then spending the credential (`auth_issue_key_journey.rs:36-129`) |
| The journey is registered in a lane that actually runs it | PASS | `cli.rs:1-2` adds the module to the `--test cli` target; `mise.toml:146-151` `test:cli:journey:inner` runs that target under `WYRD_CLI_E2E=1`. Observed: `mise run test:cli:journey` → exit 0, `test auth_issue_key_journey::auth_issue_key_cli_journey ... ok`, `23 passed; 0 failed; 5 ignored; ... finished in 18.17s` — the test ran, it did not early-return |
| Not a selector that passes after selecting nothing | PASS | the lane selects a target, not a filter; the named test appears in the run list above |
| Gating follows the lane's convention | PASS | `WYRD_CLI_E2E` early return at `auth_issue_key_journey.rs:42`, identical to `principal_journey.rs:152`, `:211` and `card_lifecycle.rs:765` etc. |
| External `tests/` file earns its place (agent-rules) | PASS | it spawns the compiled binary and boots a real HTTP server; and it is a module of the existing `cli` target, so it adds no second test binary |
| SQL-backed tests go through the repository-managed environment | PASS | every Postgres-backed run in this review used `scripts/postgres/with-test-postgres.sh` or `mise run test:cli:journey` |
| `TESTING.md` "Layout: no `#[path]` attributes" | PASS, scoped | the rule is stated for the two capability-directory trees it names (`wyrd-testing/tests`, `vala-bifrost-redux/tests`). `wyrd-cli/tests/cli.rs` is a flat single-target tree whose six existing modules are all `#[path]` lines; the new line follows the file's pattern (§16 "follow existing code style") rather than restructuring an unrelated crate |
| "A test must assert" | PASS | four assertions plus the credential-reuse assertion (`auth_issue_key_journey.rs:76-128`) |
| "A test must not name a plan" | PASS | no task or plan identifier in any new test name or comment |
| Verification scope run for the touched surface | **FAIL** | `RR3-1`: the `wyrd-sql` change ships a red unit test in that crate's own family lane |

### AGENTS.md §12 Completion Standard

| Rule | Result | Evidence |
|---|---|---|
| Format, lints, targeted tests/checks pass | **FAIL** | `RR3-1`. Everything else passed: `mise run fmt` 0, `mise run lints` 0, `check:client-tier` 0, `check:sdk-client-tier` 0, `check:pyo3-scope` 0, `check:unwrap-audit` 0, `check:clippy-allow-audit` 0, `codegen:check` 0, `docs:check` 0, `py:format` 0, `py:lints` 0, `test:cli:journey` 0, `wyrd-cli --lib` focused 5/5 PASS |
| Do not circumvent a gate | PASS | the cumulative diff (`289978fcc~1..f102e50ee`, filtered to `crates sdks scripts mise.toml Cargo.toml`) adds no `#[allow]`, `#![allow]`, or `#[ignore]` line; `scripts/` is untouched in the whole range; no test was deleted; no boundary glob broadened; the only `mise` task removed is `cli:dev-bootstrap` |
| Substituted proof 1 — `test(/http::error::tests::/)` replaced by grep + journeys | PASS, did not weaken a check | the claim is true: the crate has no such module; `mise exec -- cargo nextest list -p wyrd-server` lists no `http::error::tests::` entry. The property — no mapper still says the old text — is proven by absence repo-wide: `grep -rn "authorization header malformed"` over the tree excluding `changes/` returns nothing, and the header the docs and remediation now name is the one the transport actually sends (`wyrd-client/tests/transport/http.rs:743-747`) |
| Substituted proof 2 — `!contains("code=super")` instead of `!contains("code=")` | PASS, did not weaken the property | asserting `!contains("code=")` is impossible without also removing the variant's legitimate `expected` guidance `code=<>&state=<>` (`login.rs:120`), i.e. the prescribed assertion would have forced a weaker error message. The pair actually asserted — `!contains("super-secret-code")` and `!contains("code=super")` — covers both the credential value and the raw paste prefix, which is the property (`login.rs:160-176`). Verified green |
| §12 "Adding And Retiring Checks" — added `cli::tests::the_shipped_command_tree_is_consistent` | PASS | this is an ordinary test, which the rule names as the preferred alternative to a repository check. The property it protects is real and unreachable by the compiler or type system: `propagate_version` pushes `--version` into every subcommand at *runtime* command construction, so `IssueKeyArgs`'s own `--version` panicked only when that subcommand was parsed. It restates no existing rule — no other test builds the real `Cli` tree, which is exactly the gap the doc comment names (`cli.rs:87-103`). Verified green |
| §12 "Adding And Retiring Checks" — deleted the `cli:dev-bootstrap` `mise` task | PASS | the failure it could catch is unreachable: `wyrd dev bootstrap` no longer exists — `cli.rs` declares no `dev` command and no `Bootstrap` command type exists in `wyrd-cli/src`. The task could only exit non-zero. No dangling reference to the task name remains anywhere outside `changes/` |

### AGENTS.md §15 Implementation Rules

| Rule | Result | Evidence |
|---|---|---|
| `wyrd-sql` owns the durable Postgres layer | PASS | the query change is in `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs`, the owner |
| Client-tier crates depend on no `sqlx`/cloud SDK/`datafusion`/`deltalake` | PASS | `check:client-tier` and `check:sdk-client-tier` → exit 0 |
| Fix the root cause once at the shared owner | PASS | the card-ref lookup was fixed once in the shared query, so `issue_api_key.rs:94`, `jwt_bearer.rs:155` and `exchange_api_key.rs:591` all moved together; and the docs header fix left no sibling wrong — no `Authorization`-authenticated Wyrd example survives in `docs/src/content` |
| No scaffolding for hypothetical reuse | PASS | `run_cli_with_credential` was generalized because the new journey needs `WYRD_API_KEY`, not speculatively; `issued_key` has exactly one caller |
| The smallest incomplete solution is still wrong | **FAIL** | `RR3-2` |

### AGENTS.md §16 General Code Rules

Every new or materially modified Rust item, audited individually:

| Item | `file:line` | Rustdoc | `# Errors` | `# Panics` | Result |
|---|---|---|---|---|---|
| module doc, new journey file | `auth_issue_key_journey.rs:1-8` | yes, names the regression it exists to catch | n/a | n/a | PASS |
| `fn issued_key` | `auth_issue_key_journey.rs:27` | yes | n/a (infallible) | documented in prose — "Panics with the whole output when the label is absent" — matching the established in-tree style for test support helpers (`wyrd-testing/tests/bifrost/forge/public_support.rs:76`, `:115`, `:145`) | PASS |
| `fn auth_issue_key_cli_journey` | `auth_issue_key_journey.rs:36` | yes, explains why it is a credential handoff | n/a | n/a | PASS |
| `fn run_cli_with_credential` | `principal_journey.rs:20-25` | yes, updated for the generalization and still names the process-table and ambient-login invariants | n/a | n/a | PASS |
| `fn run_cli_async_with_credential` | `principal_journey.rs:47-48` | yes | n/a | n/a | PASS |
| `fn run_cli_async` | `principal_journey.rs:59-60` | yes | n/a | n/a | PASS |
| `start_served`, `stop_served`, `v1_status`, `machine_token` visibility widened | `principal_journey.rs:89`, `:104`, `:119`, `:134` | pre-existing docs retained and still accurate | n/a | n/a | PASS |
| `mod tests` in `cli.rs` | `cli.rs:87-88` | undocumented, matching every other `#[cfg(test)] mod tests` in the crate (`login.rs:128`, `error.rs:471`, `registration.rs:287`, …) | n/a | n/a | PASS |
| `fn the_shipped_command_tree_is_consistent` | `cli.rs:96-103` | yes, explains why per-command tests cannot see the defect | n/a | n/a | PASS |
| `fn the_refusal_does_not_echo_the_pasted_callback` | `login.rs:152-158` | yes, names the credential and the two print sites | n/a | n/a | PASS |
| `fn parse_callback_input` | `login.rs:78-92` | yes, explains the operator hand-carry and the non-echo invariant | yes | n/a | PASS |
| `struct IssueKeyArgs` | `issue_key.rs:11-17` | yes, explains why `disable_version_flag` is required | n/a | n/a | PASS |
| `fn check_lease` | `eval/routes.rs:254-268` | yes, explains the lease-vs-token split and constant-time comparison | yes, naming both codes and every branch that yields the first | n/a | PASS |
| `fn service_account_by_card_ref` | `service_accounts.rs:155-168` | present, and the GIN-index claim is true (`wyrd-sql/migrations/20260601000001_auth.sql:92-93`) and `created_at` exists (`:81`) | yes | n/a | **FAIL** — the stated invariant is false; see `RR3-2` |
| `SERVICE_ACCOUNT_BY_CARD_REF_SQL` | `service_accounts.rs:10-19` | n/a (private const, pre-existing, undocumented before and after) | n/a | n/a | PASS |

| Rule | Result | Evidence |
|---|---|---|
| No comments/docstrings/annotations added to untouched code | PASS | every doc added sits on an item the same commit changed |
| Single responsibility | PASS | `issued_key` parses; `auth_issue_key_cli_journey` drives; `run_cli_with_credential` runs one process |
| Follow existing style | PASS | gating, `#[path]` registration, helper shape, and assertion style all match the crate's existing journeys |
| Python tests use top-level `def test_*` only | PASS (N/A) | the only Python touched in the cumulative range is `docs/scripts/generate_api_docs.py`, not a test; `py:format` and `py:lints` → exit 0 |

### AGENTS.md §13 Git Identity Rules

| Rule | Result | Evidence |
|---|---|---|
| Author and committer are `Thorrester <sjforrester32@gmail.com>` | PASS | all three remediation commits: `9fe02aa2e`, `67c9d2df6`, `f102e50ee` — `an=` and `cn=` both `Thorrester <sjforrester32@gmail.com>`, `%G?` = `G` (good signature) |
| No AI co-author trailer | PASS | the implementor's claim is verified — the full `%B` of all three commits contains no `Co-Authored-By` line. The trailers the prior verdict disclosed remain only on earlier commits in the range; no history rewrite is proposed |

### Generated-artifact rules (agent-rules)

| Rule | Result | Evidence |
|---|---|---|
| `openapi.yaml`, schemas, `.pyi`, `.d.ts` not hand-edited | PASS | the cumulative range changes `openapi.yaml`, `crates/wyrd-spec/schemas/ui_problem_examples.json`, `crates/wyrd-spec/tests/schemas/ui_problem_examples.json`, and `docs/src/content/docs/api/openapi.md`. `mise run codegen:check` → exit 0 and left the tree clean, which is the sanctioned proof that all four match what their generators produce from the current sources (`wyrd-server/src/http/openapi.rs`, `crates/wyrd-spec/src/error.rs`, `docs/scripts/generate_api_docs.py`). The remediation's own `crates/wyrd-spec/src/error.rs:4168` edit is inside `mod tests`, a source fixture, not a generated file |
| No `.pyi`/`.d.ts` touched | PASS | `git diff --stat` over `'*.pyi' '*.d.ts'` for the range lists none |

### Claimed pre-existing red — independently verified, not a candidate regression

`wyrd-server::auth_e2e cache_ttl_path_also_flips_verdict` fails at the candidate:

```
FAIL [ 10.613s] wyrd-server::auth_e2e cache_ttl_path_also_flips_verdict
panicked at crates/wyrd/wyrd-server/tests/auth_e2e.rs:51:10:
delegate: Http { status: 503, code: "WYRD_AUTH_503_VERIFY_UNAVAILABLE", ... }
```

The delegate path does reach the changed query (`exchange_api_key.rs:585-593`
→ `service_account_by_card_ref`, and `DelegateError::Database` maps to
`AuthVerifyUnavailable` at `exchange_api_key.rs:792`), so the claim needed
falsifying rather than accepting. I ran the same test in a detached worktree at
the branch base `968c92641`, with the candidate's change absent, under the same
Postgres wrapper:

```
panicked at crates/wyrd/wyrd-server/tests/auth_e2e.rs:51:10:
delegate: Http { status: 503, code: "WYRD_AUTH_503_VERIFY_UNAVAILABLE", ... }
```

Byte-identical failure at the base. Additionally, `auth_issue_key_cli_journey`
passing proves the `@>` query executes and resolves a row, so the 503 is not the
new SQL erroring. Pre-existing, outside this candidate, not reported as a
finding. The worktree was removed and the tree left clean.

---

## 3. Findings

### `RR3-1` — REGRESSION

- **Violated rule:** `AGENTS.md` §12 Completion Standard ("Format, lints, and
  the targeted tests/checks for the touched surface pass") and §11 Verification
  Scope ("Rust crate change: prefer the nearest crate-specific `mise run ...`
  task"). `architecture/agent-rules.md`: "Run whole-crate tests through the
  crate's `mise run` task."
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431`
  (the assertion), broken by `:15` (the SQL change).
- **Observable consequence:** the unit test pinning this query's shape still
  asserts the operator the remediation replaced, so it fails:

  ```
  $ mise exec -- cargo nextest run --locked -p wyrd-sql --lib \
      -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'
  thread '...' panicked at crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:431:9:
  assertion failed: SERVICE_ACCOUNT_BY_CARD_REF_SQL.contains("card_ref = $3")
  Summary [0.136s] 1 test run: 0 passed, 1 failed, 73 skipped
  ```

  `wyrd-sql` is in the `wyrd` family package list
  (`scripts/test-families.sh:16`), so `mise run test:wyrd` — and every aggregate
  above it — is red on the candidate. `mise run lints` does not catch it:
  clippy compiles the assertion without evaluating it. The owning crate's lane
  was never run for a change to that crate.
- **Testable correction:** update the assertion to the operator the query now
  uses and to the determinism the change added — assert
  `contains("card_ref @> $3")`, `contains("ORDER BY created_at, id")` and
  `contains("LIMIT 1")`, and keep the existing `principal_kind = $2` and
  `!contains("card_ref::text")` assertions. Then
  `mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E 'test(=queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding)'`
  passes, and `mise run test:wyrd` no longer carries this failure.

### `RR3-2` — INCORRECT

- **Violated rule:** `AGENTS.md` §9 Server and Contract Rules ("Preserve tenant
  isolation across every public and internal server path" — the intra-tenant
  space scoping of a credential-issuance and delegation lookup),
  §15 Implementation Rules ("The smallest incomplete solution is still wrong";
  "Simplicity never overrides explicit product behavior, architecture,
  validation, security, tenancy … requirements"), and §16 ("Documentation is
  part of implementation correctness" — the rustdoc states an invariant that is
  now false).
- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:15`
  (`AND card_ref @> $3`), with the false invariant at `:161-165` and the
  unvalidated wire inputs at `crates/wyrd-spec/src/auth/issue_key.rs:15` and
  `crates/wyrd-spec/src/auth/token.rs:70-72`.
- **Observable consequence:** blanket JSONB containment makes **every** absent
  `CardRef` field optional, not just `uid`. `CardRef.space` is
  `#[serde(default, skip_serializing_if = "Option::is_none")]`
  (`crates/wyrd-spec/src/reference.rs:28-29`), so a request that omits `space`
  serializes to a three-key document that a stored ref in *any* space contains.
  Confirmed against Postgres:

  ```
  select '{"kind":"Service","name":"x","version":"1.0.0","space":"prod","uid":"u1"}'::jsonb
      @> '{"kind":"Service","name":"x","version":"1.0.0"}'::jsonb;
   t
  ```

  Neither wire surface requires `space`: `issue_key` deserializes
  `IssueKeyRequest` and passes `request.card_ref` straight through with no
  space validation (`wyrd-server/src/components/auth/routes.rs:260-300`), and
  `RequestedSubject::CardRef` reaches the same query on the delegate path
  (`wyrd-auth/src/exchange_api_key.rs:585-593`). So a caller POSTing
  `{"card_ref":{"kind":"Service","name":"api","version":"1.0.0"}}` to
  `/auth/issue-key` is minted a live API key bound to whichever space's
  principal happens to be oldest — `ORDER BY created_at, id LIMIT 1` makes that
  silent and deterministic rather than an error — and the same omission on
  `/auth/token` delegates to it. Under the previous equality predicate such a
  request resolved nothing. The audit row cannot distinguish the outcome either:
  the resource is `format!("card:{}", request.card_ref.name)`
  (`routes.rs:284`), which carries no space. The CLI happens not to expose this
  (`--space` is a required `String`, `wyrd-cli/src/auth/issue_key.rs:28-30`), so
  the new journey cannot see it — the reachable surface is HTTP and MCP.

  The rustdoc's stated invariant is false for the same reason: "Two active
  principals sharing one Card identity would require two Cards with the same
  identity" (`:161-163`) holds only if `space` is part of the compared identity,
  which containment makes optional. Two principals in different spaces are two
  legitimate Card identities that this predicate now collapses to one.

  This also answers the question the implementor raised: uid-insensitivity is a
  bug fix, because `uid` is server-derived and no client can express it. Making
  `space` — which the client *can* express and which pins Card identity together
  with `name` and `version` (`crates/wyrd-spec/src/reference.rs:24-27`) —
  optional on a credential-issuance path is a behavior change, and it needs
  either a narrowed predicate or an explicit spec decision, not a bug fix.
- **Testable correction:** keep containment but make it insensitive only to the
  server-derived field. Either (a) bind an explicitly constructed identity
  document — `kind`, `space`, `name`, `version` — and refuse a `card_ref` whose
  `space` is `None` at the wire boundary with a stable `WyrdError`, or
  (b) compare the identity keys explicitly
  (`card_ref->>'kind'`, `->>'space'`, `->>'name'`, `->>'version'`) and drop
  containment. Either way correct the rustdoc invariant to say the comparison is
  over `(kind, space, name, version)` and ignores `uid`.
  Testable: a tier-2 Postgres test in `wyrd-sql` that stores one active
  principal in space `prod`, then asserts `service_account_by_card_ref` with a
  `CardRef` whose `space` is `None` returns `None`, and with `space: Some("prod")`
  returns the row; plus a tier-1 HTTP journey that POSTs `/auth/issue-key` with
  a space-less `card_ref` and asserts a refusal rather than an issued key.
