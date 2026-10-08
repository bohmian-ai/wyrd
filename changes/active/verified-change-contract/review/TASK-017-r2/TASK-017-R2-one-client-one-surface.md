---
id: TASK-017-R2
kind: remediation
status: implemented
spec: SPEC-verified-change-contract
spec_revision: 70
requirements: [REQ-192, REQ-193, REQ-195, REQ-199, REQ-208, REQ-209, REQ-210, REQ-211, REQ-212, REQ-213, REQ-214, AC-048, AC-049, AC-060, AC-061, AC-062, AC-063, AC-064, AC-065, AC-066, AC-067]
depends_on: [TASK-017-R1]
parent_task: TASK-017
remediates: [FIND-PERSONA-01, FIND-PERSONA-02, FIND-PERSONA-03, FIND-PERSONA-04, FIND-PERSONA-05, FIND-PERSONA-06, FIND-PERSONA-07, FIND-PERSONA-08, FIND-PERSONA-09, FIND-PERSONA-10, FIND-PERSONA-11, FIND-PERSONA-12, FIND-PERSONA-13, FIND-GATE-FLUSH]
---

# TASK-017 R2: One client, one surface, usable roles

## Authority and subject

- Approved authority: [spec revision 70](../../spec.md), approved by the user
  on 2026-10-07. Revision 68 adds the parity-boundary decision (D10);
  revision 69 adds three named idiom exceptions (D12); revision 70 names
  Python and TypeScript `to_arrow` an ecosystem adapter (D12).
- Original task: [TASK-017](../../tasks/TASK-017-sdk-test-standard-and-cleanup.md).
- Builds on the uncommitted TASK-017-R1 candidate
  ([evidence](../TASK-017-r1/TASK-017-R1-readable-sdk-journeys.md)). Its
  journeys, story names, and `fixtures/README.md` table stay the contract.
- Implementors receive `$wyrd-implement`; nothing is committed by this task.

## Why

A user-persona review read the R1 journeys as a Python data scientist would,
to learn the SDK from them. It found that the stories cannot be copied into
a notebook:

- A bundle comes only from the test-only CLI.
- A verify key comes only from a harness Role grant.
- Identity arrives three ways: `credential=`, `client=`, or the environment.
  `verify` silently runs as whichever client Bifrost was started with.

A cross-SDK inventory then found the same operations shaped differently in
each SDK, and most public parameters undocumented in all three. `test:bifrost`
is also red on a production Scribe flush race.

## User decisions (2026-10-07)

| # | Decision |
|---|---|
| D1 | Every server-facing public surface takes one optional `WyrdClient`. Omitted, it auto-resolves. The server URL, credential, and gRPC URL live on the `WyrdClient` constructor only (REQ-208). |
| D2 | A `WyrdState`'s identity is fixed when it is created. `start_bifrost` takes no identity, and `verify` uses the state's client (REQ-209). |
| D3 | All public surfaces have the same operations, argument names and kinds, and typed outputs in Rust, Python, and TypeScript, with named idiom exceptions only (REQ-210, REQ-211). |
| D4 | A tenant-admin CLI grants Roles: `wyrd auth grant-role` (REQ-212). |
| D5 | Services default to `wyrd_default`. The name uses `_` because role names allow only `[a-z0-9_]`. It grants emit and verify, not tenant-wide reads (REQ-213). |
| D6 | Python `cards.get` returns typed, attribute-access Cards (REQ-193). Python gains `cards.hydrate`, `CardRef` selectors, and `PathLike` paths. |
| D7 | Every public parameter is documented in every SDK (REQ-214). |
| D8 | The persona review's test-only fixes 7–13 are applied. Multi-step tests keep their `fixtures/README.md` names, so story names do not change. |
| D9 | `user-persona-review` is a global skill for repeating this review. |
| D10 | (revision 68) Parity is measured on `wyrd_sdk`, Python `wyrd`, and `@wyrd/sdk`. Names are unified: `root_ref`, `service`, `steps`. Synchronous twins (Rust and Python blocking Bifrost, Python sync `record`) and Rust/Python programmatic `register` are named exceptions. `wyrd_sdk` does not re-export `Workflows` or `PublicWyrdGatewayCaller`. Low-level methods on re-exported types sit behind a `wyrd-client` `internal` feature, enabled by the CLI, MCP, testing, and binding crates but never by `wyrd-sdk-rust` (enforced by `check:deps`). Python Workflow builders are removed; Workflows are YAML-authored (REQ-210, AC-067). |
| D11 | Every ambient resolution (an omitted client) goes through one door that reads the global config file, environment, and defaults, so `WyrdClient()` and `Cards()` cannot resolve to different servers (REQ-209). |
| D12 | (revision 69) Three more named REQ-210 exceptions: Python `to_pandas`/`to_polars`, `wyrd.otel.install_run_correlation`, and (revision 70) Python/TypeScript `to_arrow` (ecosystem adapters); Rust `ClientConfig`/`Environment`/`GlobalConfig` (the Rust spelling of constructor options); fixture-only test-harness CLI helpers such as Rust `cli::load(Option<&Path>)`. `parity.md` cites them instead of listing them as gaps (REQ-210, AC-067). |

## Workstreams

Write sets are disjoint so W1–W3 can run in parallel. W4–W6 start after W1
and W2, because they consume the new Rust surfaces and the CLI function.

### W1: Rust client surface (`crates/shared/wyrd-client`, `sdks/wyrd-sdk-rust/src`)

- **Identity (REQ-208).**
  - Remove `Cards::new(server_url, credential)` (`cards/handle.rs:161`) and
    route its internal callers (`handle.rs:633,809`,
    `workflow/mod.rs:130`) through `with_client` / `from_env`. Add
    `Cards::from_env` if it is absent.
  - `Gateway::new(client)` (`gateway.rs:58`) becomes `with_client` plus
    `from_env`.
  - `Platform::connect(base_url, &SecretString)` (`platform/handle.rs:71`)
    becomes `with_client`.
- **State identity (REQ-209).**
  - Add `WyrdState::from_path_with_client(path, client)`, which sets the
    state's client cell at creation.
  - `start_bifrost_with(&client, table)` and `start_bifrost_with_config`
    (`state.rs:488,511`) lose the client parameter and use `self.client()`.
  - `state.client()` (`state.rs:539`) keeps resolving the ambient chain
    once and caching it.
- **Parity (REQ-210).**
  - Add `Cards::hydrate(selector, destination, mode)`, a thin method over
    `CardGraphHydrator::new(self.registry_context())`
    (`cards/hydrate/mod.rs:56,74`).
  - Keep one describe name: `describe_table(namespace, name)`. Remove the
    duplicate `Bifrost::describe(fqn)` (`bifrost/facade.rs:826`) or make
    it the only one, whichever has fewer callers. Record the choice.
  - Unify `stream` on `stream(query, &params, deadline: Option<Duration>)`.
  - Add `Workflow::from_yaml` if it is missing.
- **Paths (REQ-211).** `WyrdState::from_path` accepts `impl AsRef<Path>`.
- **Re-exports.** `sdks/wyrd-sdk-rust/src/lib.rs` drops the glob
  `pub use wyrd_client::*` for explicit re-exports, so
  `gateway_credential` is no longer public. This matches the crate's own
  doc rule.
- **Docs (REQ-214).** Every public function with parameters in
  `wyrd-client` has an `# Arguments` section, plus `# Errors`. Apply this
  to every item W1 touches and every SDK-facing public item. There are
  about 109.
- **Tests.**
  - Update the Rust unit tests in `state.rs` (12 sites) and `cards`.
  - Add unit tests for `Cards::hydrate` and for the state-client binding:
    a state created with client A runs Bifrost and verify as A.

### W2: Roles (`wyrd-spec`, `wyrd-runtime`, `wyrd-sql`, `wyrd-server`, `wyrd-cli`)

Design from the RBAC inventory:

- **The `wyrd_default` role (REQ-213).**
  - Add a `wyrd_default` built-in to `BUILTIN_ROLES`
    (`crates/shared/wyrd-runtime/src/builtin_roles.rs:21-71`) with
    `bifrost_table:read`, `bifrost_record:write`, `evals:run`. Keep
    `workload` unchanged as the grantable tenant-wide Bifrost role.
  - Point the default constant (`WORKLOAD_ROLE`, `builtin_roles.rs:81`) at
    `wyrd_default` under a name that says what it is (`DEFAULT_CARD_ROLE`).
    `upsert_service_account_from_card` then `grant_*`
    (`crates/wyrd/wyrd-sql/src/queries/cards/auth_projection.rs:98-129`)
    grants it.
  - The migration `20261002000100_workload_role.sql` has not been released
    (absent from `main`). Edit it in place to seed both `workload` and
    `wyrd_default`, and to grant existing Card-bound principals
    `wyrd_default`. Seeds for new tenants go in
    `crates/wyrd/wyrd-auth/src/seed.rs`.
- **The grant route (REQ-212).**
  - Route: `POST /v1/auth/grant-role`, mounted next to issue-key in
    `crates/wyrd/wyrd-server/src/components/auth/routes.rs`.
  - Request and response: `GrantRoleRequest { card_ref: CardRef, role:
    String }` and `GrantRoleResponse { principal_id, card_ref, roles,
    granted }`, in `crates/wyrd-spec/src/auth/tenant_principals.rs`.
  - Gate: tenant administrator (`Permission::wildcard()`), not
    `service_accounts:write`.
  - Authorization pattern (follow `principals::authorize`, `routes.rs:84-104`):
    1. check permission;
    2. stage the allow or deny decision on the audit outbox
       (`auth.principal.role.grant`, resource `card:{ref}/role:{name}`);
    3. open a `TenantConn` for `caller.data_tenant_id`.
  - Lookup and errors: find the target principal with
    `service_account_by_card_ref`. An unknown or foreign Card gets
    `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`, and an unknown role gets
    `WYRD_SPEC_400_VALIDATION`. The grant is idempotent (`granted:
    false` when the role is already held). Write it through the existing
    SQL owner, `queries/auth/role_assignments.rs:84`.
  - Register the route in OpenAPI.
- **CLI and the test-only function.**
  - `wyrd auth grant-role --kind --name --version --space --role`,
    reusing issue-key's card-ref parsing.
  - `wyrd_cli::commands::grant_role(kind, name, version, space, role,
    client)`.
  - Python and TypeScript bindings follow in W4 and W5.
- **Docs.**
  - `docs/src/content/docs/concepts/authorization.svx` (the default role
    and grant-role)
  - `architecture/wyrd-security-posture.md:85`
- **Tests.**
  - Rust route tests: tenant isolation, non-enumerating 404, unknown role
    400, non-admin 403 (including `runtime_admin`), idempotency, and the
    staged audit decision.
  - Update the `seed.rs` and `builtin_roles` tests.
  - Every test relying on the old default needs updating: the
    `eval_verification.rs`, `observe_ingest.rs:89`, MCP `verification.rs:292`
    and `pg_card_registration_route.rs:3460` call sites. Where a test
    needed tenant-wide reads, it now grants `workload` explicitly.
- **Out of scope.** The `POST /v1/principals` escalation is recorded in the
  spec as a follow-up, and no revoke route is added.

### W3: Scribe flush race (`crates/vala/vala-bifrost-redux/src/scribe`)

- **Fault.** `flush_staged` → `publish_residue(Drain)` → `publish_claims`
  breaks when nothing is in flight (`persistence.rs:2247-2251`), even while
  the 1 s `publish_due` tick drives a claim over members the flush drained.
  It therefore returns before they publish. This failed
  `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass`.
- **Fix.** Snapshot the durable, unpublished member set at the flush and
  wait only for it.
  - Add `ScribeStagingRuntime::owned_members()` (a clone of
    `StagingAssembler.owned` under `lock_assembly`) and `owns_any(&snapshot)`.
  - In the worker's `publish_residue`, take the snapshot first, before
    `ready_keys()`.
  - `ClaimSlotWait::Await` carries `&snapshot`.
  - In the `in_flight.is_empty()` branch: break on failure. Keep the
    exhausted-budget branch unchanged. Otherwise, while
    `owns_any(snapshot)`, await `released` (already registered at the loop
    top, so no wakeup is lost) and `continue`; break when false.
  - Claims taken on newer data never enter the snapshot, so a flush ends
    under continuous ingest and other tenants' traffic does not block it.
- **Tests.**
  - Staging-runtime unit test: `owns_any` tracks only snapshot members.
  - Persistence tests:
    - `flush_waits_for_an_outside_claim_over_members_durable_at_drain`
    - `flush_ignores_an_outside_claim_taken_after_its_snapshot`
- **Regression.** Re-run the closeout test by exact selector, and
  `scribe/lifecycle.rs`.

### W4: Python SDK (`sdks/wyrd-sdk-python`)

- **Identity (REQ-208, REQ-209).**
  - `Cards(client=None)`, `Gateway(client=None)`,
    `OperatorConnections(client=None)`, `Bifrost(table, client=None,
    client_byte_limit_bytes=None)` and `TableConfig.describe(…,
    client=None)`: remove `server_url`, `credential` and `grpc_url`.
  - `WyrdState.from_path(path, client=None, …)`, and
    `start_bifrost(table=None, client_byte_limit_bytes=None)`.
  - The Rust wrapper `src/state/mod.rs:1611` stops calling
    `Cards::new(url, cred)`.
  - The `WyrdClient` docstring defines `credential`.
- **Parity (REQ-210, REQ-211).**
  - `cards.hydrate(ref, destination, metadata_only=False)`.
  - Root `cards.list`, and `cards.delete(ref)`.
  - Per-kind `get`, `delete` and `workflow.load` accept a `CardRef`.
  - `register_from_path(path: str | PathLike[str])`, and fix its docstring
    ("a Card file or a bundle directory").
  - `Bifrost.stream(query, params, deadline_ms=None)`, matching the agreed
    shape.
- **Typed Cards (REQ-193).** Switch `scripts/gen_card_types.py` from
  TypedDicts to frozen dataclasses with a kind-discriminated `from_wire`.
  `cards.get` returns them. `codegen:check` proves they don't drift.
- **Testing CLI.** Add `grant_role` in `src/testing_cli.rs`, and regenerate
  the stubs.
- **Docs (REQ-214).**
  - Write `Args:`, `Returns:` and `Raises:` in `python/wyrd/stubs/*.pyi`
    for all 126 public callables that lack them.
  - Extend `undocumented_public_members` (`scripts/assemble_stubs.py:47`)
    to require an `Args:` entry per non-`self` parameter, including
    `async def` and module functions.
  - Add `D417` with the Google convention for `python/wyrd/stubs` to Ruff.
- **Journeys.** Update the 31 identity call sites in 17 files, per the
  parity inventory.
  - `test_client_cannot_be_combined_with_transport_options` is obsolete:
    delete it.
  - Service keys come from `cli.issue_key`, which holds `wyrd_default`;
    drop `["agent"]`.
  - `download()` uses `cards.hydrate`.
  - The Python surface needs a `grant_role` journey and a `workload`-gated
    query refusal, under the AC-064 and AC-065 story names, added to
    `fixtures/README.md` in all three SDKs.
- **Persona fixes 7–13.**
  - **7.** Rename the gateway-inference recording fixture to `upstream`
    and reuse `Receiver`/`Delivery`. Replace the self-compare with
    `model_calls(upstream) == 1`.
  - **8.** Use one `binding_ids` helper, moved into support.
  - **9.** Add a `newer_security_reviewer` fixture that the pinning tests
    request. A dict comparison replaces the loop and the
    `sorted(key=lambda…)`.
  - **10.** Comment the first `flush_bifrost()` in each story.
  - **11.** Add one line explaining `interfaces=` at the `StandInModel`
    use.
  - **12.** Move helpers to `tests/integration/support.py`. Nothing
    imports from `conftest`.
  - **13.** The nits:
    - `pytestmark = pytest.mark.integration` per module;
    - one shared `Count`;
    - `StatusCode.ERROR.value`;
    - rename `started` → `observed_with_bifrost`;
    - the gateway-inference server starts with `mutate_env=False` and
      calls pass an explicit client.
  - Card status is read by attribute (point 5), and `CardRef` and `Path`
    are passed directly (point 6).

### W5: TypeScript SDK (`sdks/wyrd-sdk-ts`)

- **Identity (REQ-208, REQ-209).**
  - `Cards.connect({ client })`, `Bifrost.connect({ client, table })`,
    `TableConfig.describe(table, { client })`,
    `Gateway.connect({ client })` and
    `OperatorConnections.connect({ client })`: remove `serverUrl`,
    `credential` and `grpcUrl`.
  - `WyrdState.fromPath(path, { client })` and `startBifrost({ table,
    clientByteLimitBytes })`.
  - The native layer follows.
- **Parity (REQ-210).**
  - `cards.resolveLatest(kind, space, name)`;
  - `bifrost.dropped`;
  - `state.run(alias)`;
  - typed state accessors (`verifier`, `workflow`, `agent`, `prompt`);
  - `Workflow.run` accepts a string input;
  - `Workflow.fromYaml`;
  - `stream(query, params, { deadlineMs })`.
- **Testing CLI.** Add `grantRole` in `native-testing/src/cli.rs` and
  `testing/index.{js,d.ts}`.
- **Docs (REQ-214).** Add `@param`, `@returns` and `@throws` to every public
  method in `src/index.ts` (about 50 take parameters).
- **Journeys.**
  - Update the 25 identity call sites in 11 files.
  - The `bifrost-write` combination-rejection test is obsolete: delete it.
  - Remove every `vi.stubEnv("WYRD_API_KEY", …)` principal switch, using
    a state created with the client.
  - Service keys come from `cli.issueKey`.
  - Add the same new story tests as W4.
- **Regenerate** `wyrd/index.d.ts` and stage it so `ts:napi:check` passes.
  Do not commit.

### W6: Rust SDK journeys (`sdks/wyrd-sdk-rust/tests`)

- Update `start_bifrost*` call sites (`observe_a_run.rs` 3,
  `verify_in_real_time.rs` 5, `scheduled_drift_alerts_operator.rs` 1) to
  states created with a client.
- Use `Cards::hydrate` in support instead of `CardGraphHydrator`.
- Service keys come from the `issue_key` CLI function.
- `caller_without_evals_run_is_refused`: the Service's own key now holds
  `evals:run`, so the refused caller is a machine principal holding only
  `workload`. Use the same input in all three SDKs.
- Add the same new story tests as W4.

### W7: Packet and docs

- **Parity table (AC-062).** Write
  `changes/active/verified-change-contract/review/TASK-017-r2/parity.md`
  with one row per public operation, giving its Rust, Python and
  TypeScript signature and result, plus the exception it uses, if any.
- **Docs site.** `docs/src/content/docs/bifrost/reading-data.svx:86,109`.
- **`TESTING.md`.** Keep its REQ-192 checklist in step with revision 67:
  explicit-client principals and a state created with a client.

### W8: Parity boundary (revision 68, D10)

- **Rust (`crates/shared/wyrd-client`, `sdks/wyrd-sdk-rust/src`).** Add the
  `internal` feature and gate the GAP-row methods in `parity.md` behind it;
  enable it in `wyrd-cli`, `wyrd-mcp`, `wyrd-testing`, `wyrd-sdk-python`, and
  the TypeScript binding crates; drop `Workflows` and
  `PublicWyrdGatewayCaller` from `wyrd_sdk`; add `WorkflowHandle::steps()`
  (or the Rust Workflow type's equivalent). Extend `check:deps` to fail when
  `wyrd-sdk-rust` enables `internal`.
- **Python (W4).** `state.root_ref`; remove the Workflow builders and their
  stubs and tests.
- **TypeScript (W5).** `state.service`; rename `stepIds` to `steps`.
- **Packet (W7).** Resolve every GAP row in `parity.md` against the final
  source.

## Constraints and non-goals

- No compatibility aliases for removed identity arguments; no new runtime
  dependencies (Python typed Cards use stdlib dataclasses).
- Do not weaken, skip, or `ignore` any test; do not hand-edit generated stubs
  or declarations.
- No revoke-role route, no Card-scoped Bifrost read filter, and no
  `POST /v1/principals` gate change.
- Rustdoc every touched Rust item, with `# Errors` (AGENTS.md §16).

## Verification

Run every named test by its exact selector. Then:

- `mise run verify:rust-sdk`, `mise run verify:python-sdk` and
  `mise run verify:typescript-sdk` (including the identity journeys, which
  share Keycloak, so run them one at a time);
- `mise run test:bifrost`;
- `mise run test:principals:integration` (OpenAPI includes the new route);
- `mise run test:cli:journey`;
- `mise run codegen:check`, `mise run check:deps` and
  `mise run check:tenant-isolation`;
- `mise run fmt`, `mise run lints`, `mise run py:lints`,
  `mise run py:typecheck` and `mise run docs:check`;
- `git diff --check`, then a final audit of the tracked and untracked
  diff;
- the R1 grep proof and name-parity script, re-run.

## Implementation evidence

Diff: uncommitted on `wyrd/verified-change-contract/TASK-017` over
`b2d118450`, cumulative with TASK-017-R1. Generated `index.d.ts` and
`docs/public/llms*.txt` are staged because `ts:napi:check` and `docs:check`
compare against the index. Status: implemented.

| Finding / AC | Implementation | Verification | Result |
|---|---|---|---|
| AC-060 (REQ-208) | Rust `Cards::from_env`/`with_client` (`cards/handle.rs:178,191`), `Gateway::from_env`/`with_client` (`gateway.rs:64,75`), `Platform::with_client`; `Cards::new` and `cards/config.rs` deleted; Python `client=None` and TS `{ client }` on every server-facing handle; `client_from_options` gated `internal` (`bifrost/facade.rs:1223`); D11: one ambient door `WyrdClient::from_global`, blank explicit options refused by public name | `bifrost::facade::tests::omitted_server_url_comes_from_the_global_config_file`, `bifrost::facade::tests::blank_explicit_options_are_refused_by_their_public_name`; probe referencing `client_from_options` from `wyrd-sdk-rust` fails E0425 (gated); `client_from_options` absent from `sdks/wyrd-sdk-rust` (grep 0) | PASS |
| AC-061 (REQ-209) | `WyrdState::from_path_with_client` (`state.rs:477`), Python `WyrdState.from_path(path, client=None)`, TS `WyrdState.fromPath(path, { client })`; `start_bifrost*` take no client | `observe::tests::a_state_created_with_a_client_keeps_it_for_every_server_call`; `verify:rust-sdk`, `verify:python-sdk`, `verify:typescript-sdk` exit 0 | PASS |
| AC-062 (REQ-210) | `Cards::hydrate` (`handle.rs:417`), Python `cards.hydrate`, TS `cards.hydrate`; one `describe_table`; `stream(query, params, deadline)`; `Workflow.from_yaml`/`steps`/`name`/`version`/`space` everywhere; `root_ref`/`service`; `record` and `TableConfig.from_arrow(table, schema)`/`from_json_schema` in all three; QueryResult `batches`/`schema`/`terminal`/`num_rows`/`to_bytes` in all three; Python Workflow builders removed; [parity table](parity.md) | `parity.md`: 0 GAP rows (spec revisions 68–70); `cards::handle::tests::hydrate_publishes_a_bundle_the_state_loads`; `workflow::tests::steps_lists_step_ids_in_declaration_order`; name-parity over `fixtures/README.md`: 12 stories, 70 tests × 3 SDKs, 0 mismatches | PASS |
| AC-063 (REQ-193, REQ-211) | Python typed frozen-dataclass Cards (`scripts/gen_card_types.py` → `python/wyrd/_card_types.py`); `CardRef` selectors; Rust `impl AsRef<Path>` / Python `PathLike` / TS string paths; `wyrd_cli` `plan`/`apply`/`get` take `impl AsRef<Path>` | `codegen:check` exit 0; `py:typecheck` exit 0; `verify:python-sdk` (589 unit, 136 integration) exit 0 | PASS |
| AC-064 (REQ-212) | `POST /v1/auth/grant-role` (`routes.rs:674`, wildcard gate, audited); `GrantRoleRequest`/`GrantRoleResponse` (`tenant_principals.rs:129,143`); `wyrd auth grant-role`; `wyrd_cli::commands::grant_role` (`auth/grant_role.rs:65`); Python and TS `testing.cli` grant-role; `grant_role` story in all three SDKs | routes `pg_tests` 12/12 incl. `grant_role_is_idempotent_and_stages_its_allowance`, `grant_role_refuses_non_administrators`, `grant_role_does_not_reach_another_tenants_card`; `test:principals:integration` (OpenAPI) exit 0; `test:cli:journey` 36 passed; `grant_role::only_a_tenant_admin_can_grant_a_role` in all three SDKs | PASS |
| AC-065 (REQ-213) | `wyrd_default` built-in Role (`builtin_roles.rs:72`, `DEFAULT_CARD_ROLE` :91) granted on Card-principal projection (`auth_projection.rs`); migration `20261002000100_workload_role.sql` backfills | `default_card_role_emits_and_verifies_without_tenant_reads`; `seed::pg_tests::backfill_matches_seed_and_grants_existing_card_principals`; `pg_verification_bindings::first_projection_grants_default_role_once_and_revocation_stands`; `grant_role::default_service_key_needs_workload_for_tenant_wide_queries` in all three SDKs | PASS |
| AC-066 (REQ-214) | Python stub gate (`scripts/assemble_stubs.py:50`) plus Ruff D417 (`pyproject.toml:76-85`); TS `@param`/`@returns`/`@throws`; Rust `# Arguments`/`# Errors` across `wyrd-client` | `codegen:check`, `py:lints`, `lints` exit 0; `cargo doc -D warnings` for `wyrd-client` (no features, `internal`, all) and `wyrd-sdk-rust` | PASS |
| AC-067 (REQ-210, W8) | `wyrd-client` `internal` feature (`Cargo.toml:22`) gating raw requests, writer internals, bundle/state introspection, `*_json` observe, `ClientScope`, `QueryResultStream` settlement; enabled by CLI, MCP, testing, Python, and both TS crates; `wyrd_sdk` explicit re-exports without `Workflows`, `PublicWyrdGatewayCaller`, `saved_login`, `auth`, `storage`; unified names; no Workflow builders | `check:deps` (`scripts/checks/deps.sh:79-91`) exit 0, failure proven by hand; `cargo clippy -p wyrd-sdk-rust --no-default-features -D warnings`; Rust journeys build clients through public `WyrdClient::with_config` (`support/mod.rs:84`) | PASS |
| Persona fixes 7–13 | `upstream` rename and `Receiver`; shared status helper and `support.py`; pinning test registers v2 in the test; flush and stand-in loader explained; module-level `pytestmark`; one `Count`; named OTel status constant; split identity/uid tests; descriptive fixture names | `verify:python-sdk` exit 0; R1 grep of 36 story files for `uuid\|json\.loads\|while True\|hashlib\|subprocess\|execFile\|as CardRef\|?? ""`: 0 hits | PASS |
| FIND-GATE-FLUSH | Scribe flush tracks its owned member snapshot: `OwnedMembers` (`assembly.rs:773`), `owns_any` (:1083), `ClaimSlotWait::Await(&OwnedMembers)` (`persistence.rs:1323`), `publish_residue` snapshots before `ready_keys()` (:2341) | `persistence::tests::flush_waits_for_an_outside_claim_over_members_durable_at_drain` (red without the snapshot), `persistence::tests::flush_ignores_an_outside_claim_taken_after_its_snapshot`, `staging_runtime::tests::owns_any_tracks_only_snapshot_members`; `production_closeout::compaction_geometry_exact_rows_and_non_destructive_second_pass`; `test:bifrost` exit 0 (910 + 213 + 108 + journeys) | PASS |

Diagnosis (FIND-GATE-FLUSH):
- **Symptom:** `flush_staged` returned before every drained row had been
  published. Seen in the Forge `compaction_geometry` test.
- **Cause:** the flush tracked only the claims it took itself. The 1 s
  lifecycle tick (`publish_due_claims`) could claim members the flush's drain
  had made durable. Those members were then neither ready nor retryable, so
  the flush exited.
- **Fix site:** `PersistenceWorker::publish_claims` and `publish_residue`.
  Their only caller is `Scribe::flush_staged`; `publish_due_claims` is unchanged.

Final verification, run once each after every write set landed (logs in the
session scratchpad):
- **Static and boundary checks, exit 0:** `fmt`, `lints`, `py:format`,
  `py:lints`, `py:typecheck`, `codegen:check`, `check:deps`,
  `check:tenant-isolation`, `check:examples`, `docs:check`.
- **SDK gates, run serially, exit 0:**
  - `verify:rust-sdk`: 63 journeys and the identity journeys.
  - `verify:python-sdk`: 589 unit and 136 integration tests.
  - `verify:typescript-sdk`: 62 unit and 102 integration tests.
- **Other lanes, exit 0:** `test:bifrost`, `test:principals:integration`,
  `test:cli:journey`.
- **Clippy:** passes with `-D warnings` on `wyrd-client`, `wyrd-sdk-rust`,
  `wyrd-cli`, `wyrd-mcp`, `wyrd-testing`, `wyrd-sdk-python`, `wyrd-sdk-ts`,
  `wyrd-sdk-ts-testing`, `wyrd-server` and `wyrd-rust-examples`.
- **Mirrored story tests:** the three tests mirrored from TypeScript after
  those lanes ran each pass by exact selector:
  `register_and_hydrate::latest_version_resolves_to_the_registered_card`,
  `workflow_loading::text_input_needs_a_declared_input_named_input` and
  `workflow_loading::yaml_workflow_loads_without_resolving_file_targets`.
  That is 3/3 in Rust (nextest `-P journey`) and 3/3 in Python (`-m
  integration`). The TypeScript originals passed in `verify:typescript-sdk`.
- **Diff hygiene:** `git diff --check` and `git diff --cached --check` are
  clean, and every untracked file passes the no-index check. No backup or
  log files are in the tree.

Non-goals held:
- No revoke-role route.
- No MCP grant-role tool.
- No Card-scoped Bifrost read filter.
- No `POST /v1/principals` gate change.
- No compatibility aliases.
- No new runtime dependency.
- No hand-edited generated stubs or declarations.

Risks:
- **Changed migration:** the checksum of
  `20261002000100_workload_role.sql` changed, so long-lived development
  databases must be reset.
- **Refusal-only coverage:** no fixture declares a string `input`, so string
  `Workflow.run` is covered only by its refusal. `state.data` is covered only
  by its kind-mismatch refusal.
- **Example behaviour change:** `examples/python/workflow_gateway.py` now needs
  a `config.toml` binding for `example-gateway`. The three
  `research-and-write` provider examples are nearly identical.
- **Unnameable Rust types:** `ResolvedCredential` payload types, `SpaceName`
  and `CardName` are not re-exported by `wyrd_sdk`. Rust callers of
  `resolve_latest` build names with `.parse()` and let type inference supply
  the type.
- **Abandoned streams:** without `internal`, an abandoned Rust
  `QueryResultStream` is cancelled only when it is dropped, as in Python and
  TypeScript.
