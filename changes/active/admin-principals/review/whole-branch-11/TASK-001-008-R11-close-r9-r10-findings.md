# TASK-001-008-R11 — Close the validated R9 and R10 findings

## Route and authority

Implement this remediation with `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete cumulative candidate.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14, status `approved`, SHA-256
  `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`.
- Parent remediations:
  - `changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`
  - `changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed cumulative candidate:
  `41e60be61958c92f562fceb5b7a03f40f611bbc8`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-11/findings-validation.md`.
- Remediates: `FIND-admin-principals-R9-2` and
  `FIND-admin-principals-R11-1` through `FIND-admin-principals-R11-8`.

## Outcome

Finish the already approved RFC 8693 flow without inventing another auth,
policy, client, audit, credential, or test architecture. Delegation works in a
production configuration; audit truthfully records the evaluated invoke
decision and the verified actor; directionality is proved; Rust, Python, and
TypeScript can spend the delegated result through their existing Bifrost
facades; and the remaining UUID, documentation, and authority drift is closed.

The five-minute self-contained-JWT revocation window is an approved tradeoff
and is not part of this remediation.

## Findings and decision-complete corrections

### `FIND-admin-principals-R11-1` — production cannot use delegation

The token-exchange route still requires `allow_preview`, while production
configuration validation rejects that same flag. Test servers default it on,
so all existing exchange tests bypass the contradiction. Every production
`on_behalf_of` call is therefore refused before token verification.

Delete the obsolete preview gate and its now-orphaned configuration, environment
input, boot warning, production-validation error, public error entry, generated
artifacts, test-builder switch, and live documentation. Reuse the existing
token route, concrete verifier, `TenantTokenIssuer`, directed policy hook, and
canonical audit writer. Preserve production requirements for a real signing
key, verifier, policy implementation, and audit writer. Add no replacement
flag, feature, endpoint, or compatibility branch.

### `FIND-admin-principals-R11-2` — delegation audit records the wrong permission

Delegation evaluates the existing `invoke` policy action, but the one canonical
audit row stores `auth.token.exchange` in both its operation and permission
fields. Audit history therefore cannot identify the authorization decision
that allowed or denied the exchange.

Keep `operation=auth.token.exchange` and the existing single event, but record
the already evaluated `invoke` action in its permission field for every
delegated allowed, denied, and allowed-then-refused result. Direct token
exchange remains unchanged. Add no event, sink, or vocabulary.

### `FIND-admin-principals-R11-3` — directionality is not proved

Production builds an A-subject/B-actor policy question, but the primary journey
uses an allow-all recording hook and never submits the same two valid tokens in
reverse. The evidence would remain green if A-to-B and B-to-A were equivalent.

Keep the existing `PolicyHook` seam and production request construction. In the
existing primary journey, configure its policy behavior to allow exactly the
seeded A-to-B relation and deny B-to-A, then submit both exchanges through the
real token endpoint. Do not modify the reusable allow-and-record helper or add
a delegation store, production policy type, role, or permission.

### `FIND-admin-principals-R11-4` — Python and TypeScript delegated clients dead-end

Both foreign helpers call the Rust owner and return a delegated `WyrdClient`,
but their public Bifrost constructors create a different client from options.
The delegated authentication state cannot reach any operation, and current
tests prove only validation or unreachable-server failures.

Keep credential machinery on `WyrdClient`; do not add `on_behalf_of` to
`Bifrost`. Preserve Bifrost's current zero-argument/environment behavior in all
three languages, and add only an optional existing-client input for callers
that already hold explicit or delegated authentication state.

The public interfaces and flow are fixed as follows:

```rust
// Normal use: Bifrost resolves Service B from the environment.
let bifrost_b = Bifrost::from_env().await?;

// Delegated use: WyrdClient performs the exchange; Bifrost consumes it.
let service_b = WyrdClient::from_env()?;
let delegated = service_b
    .on_behalf_of(service_a_token, TokenAudience::Bifrost)
    .await?;
let bifrost_as_a = Bifrost::connect(&delegated).await?;
```

Rust already has both Bifrost constructors; do not add another Rust interface.

```python
# Normal use: resolves Service B from the environment exactly as today.
bifrost_b = Bifrost()

# Delegated use: the optional client carries the delegated token.
service_b = WyrdClient()
delegated = service_b.on_behalf_of(service_a_token, audience="bifrost")
bifrost_as_a = Bifrost(client=delegated)
```

Python adds `client: WyrdClient | None = None` to the existing `Bifrost`
constructor. When omitted, the existing environment/config resolution is
unchanged. When supplied, Bifrost uses that client and does not resolve another
credential. `client` may be combined with the existing table binding, but not
with `server_url`, `credential`, or `grpc_url`; conflicting inputs return the
existing stable validation error.

```typescript
// Normal use: resolves Service B from the environment exactly as today.
const bifrostB = await Bifrost.connect();

// Delegated use: the optional client carries the delegated token.
const serviceB = WyrdClient.connect();
const delegated = await serviceB.onBehalfOf(serviceAToken, {
  audience: "bifrost",
});
const bifrostAsA = await Bifrost.connect({ client: delegated });
```

TypeScript adds `client?: WyrdClient` to the existing `Bifrost.connect`
options. Its public options type must make `client` mutually exclusive with
`serverUrl`, `credential`, and `grpcUrl`, while retaining the optional table
binding. When `client` is absent, existing environment/config resolution is
unchanged.

Both foreign projections must call the current Rust
`Bifrost::connect`/`connect_with_table` owner with the wrapped Rust client.
Keep exchange, bearer access, caching, renewal, retry, headers, and transport
private in Rust. Do not add Bifrost delegation methods, broaden Cards or
`WyrdState`, expose raw tokens, duplicate Bifrost, or create a new harness.

Use each existing Python and TypeScript integration harness to prove the same
real A-read/B-read-write delegation scenario. A lower-tier binding test is not
a substitute for these public journeys.

The Python proving scenario must exercise the public workflow directly:

```python
service_b = WyrdClient(server_url=server.url, credential=b_api_key)
delegated = service_b.on_behalf_of(a_access_token, audience="bifrost")
bifrost_as_a = Bifrost(client=delegated)

assert bifrost_as_a.sql("SELECT * FROM allowed.orders").to_arrow().num_rows >= 0
with pytest.raises(WyrdError) as denied:
    bifrost_as_a.write_batch("allowed.orders", batch)
assert denied.value.status == 403

# The same operation uses B's full authority only through B's normal client.
bifrost_as_b = Bifrost(client=service_b)
bifrost_as_b.write_batch("allowed.orders", batch)
```

The TypeScript journey must perform the same sequence through
`WyrdClient.onBehalfOf` and `Bifrost.connect({ client: delegated })`: delegated
read succeeds, delegated write is denied before effect, and the ordinary
Service-B Bifrost client can perform the write. The exact table/row fixtures
come from each existing integration harness rather than a new delegation-only
fixture.

### `FIND-admin-principals-R11-5` — native-ingest audit loses actor B

Gate preserves the delegated chain in its verified `AuthContext`, but the sole
production `PostgresGateAudit` implementation discards that chain while
building both allowed and denied record-write events. Retained audit therefore
attributes the decision to subject A without identifying current actor B.

Reuse the existing delegation-attribution detail and actor-chain projection
already used by HTTP and Oracle audit. Attach it to Gate decisions only when
the verified chain is non-empty so direct-call events remain unchanged.
Preserve Gate authorization and the canonical audit owner; add no sink or
parallel event.

### `FIND-admin-principals-R11-6` — live CLI documentation uses removed `--token`

The shipped query command now accepts only ambient credentials, but the live
Bifrost guide still instructs users to invoke `wyrd query --token`. The example
both fails and reintroduces the secret-in-argv practice R9 removed.

Delete the option from that example and state that `WYRD_ACCESS_TOKEN` or the
normal ambient credential chain must be configured. Do not restore a
compatibility option or add a permanent documentation checker for this one
stale example.

### `FIND-admin-principals-R9-2` — one credential response remains string typed

The card-bound issue-key path creates a UUID credential ID but stringifies it
into `IssueKeyResponse.key_id`; its served schema is consequently an
unconstrained string, outside R9's existing OpenAPI proof.

Use the already installed `Uuid` type in the existing response and pass the
issued UUID directly. Update current consumers and regenerate owned schemas.
Add no identifier wrapper, parser, compatibility field, or route.

### `FIND-admin-principals-R11-7` — R9/R10 Rust documentation is incomplete

R9/R10 added or materially changed private helpers and tests that can panic or
fail but omit the repository-required substantive contract, including required
`# Panics` and `# Errors` sections. Public strict-rustdoc lanes do not inspect
all private and test items.

Perform one inventory limited to Rust items added or materially modified by the
R9/R10 commit range and complete only their missing intent, workflow,
invariant, side-effect, error, panic, and cancellation documentation. Do not
touch unrelated historical items, suppress a lint, or add a permanent audit
script.

### `FIND-admin-principals-R11-8` — TypeScript authority contradicts R10

The TypeScript guide prohibits wrapping the raw shared `WyrdClient`, while the
approved R10 architecture requires a thin N-API projection of that single Rust
owner for authentication and delegation.

Narrowly update the existing guide to permit that shared-client projection and
its composition with existing facades. Preserve the prohibitions on a separate
`QueryClient`, duplicated authentication or transport, and per-call gRPC
transport.

## Constraints and preserved behavior

- Preserve RFC 8693 `subject_token=A`, `actor_token=B`, top-level `sub=A`,
  outer `act.sub=B`, nested actor order, and resource audience binding.
- Preserve semantic authority intersection across subject, actor, and narrower
  resource scope; `act` never grants authority.
- Preserve five-minute, stateless tenant JWTs and database-free Bifrost request
  verification. Do not add revocation introspection, epochs, or caches.
- Preserve one token endpoint, one `TenantTokenIssuer`, one concrete
  `TokenVerifier`, one invoke-policy seam, and one canonical audit path.
- Preserve platform-plane current-state authorization, tenant RLS,
  `TenantConn`/`OperatorPool`, credential lifecycle, refresh behavior, stable
  errors, and audience enforcement.
- Preserve R9 CLI ambient secret sources and successful delegation's actor
  credential in audit only; the delegated JWT remains credential-unattributed.
- Preserve the retained audit-publisher `FOR UPDATE NOWAIT` correction.
- Add no delegation table, role, permission, second policy engine, token
  accessor, client transport, audit sink, compatibility endpoint, or migration.
- Wyrd is unshipped; do not add compatibility behavior.
- Do not broaden documentation work beyond R9/R10 diff-scoped Rust items and
  the two identified live authority/documentation contradictions.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `R11-1` | A production-valid server configuration accepts standard token exchange without any preview setting; obsolete preview config/error/docs/generated residue is absent; real policy/audit/verifier production checks remain |
| `R11-2` | Every delegated allow, deny, and allowed-then-refused event has `operation=auth.token.exchange`, `permission=invoke`, and exactly one canonical row |
| `R11-3` | With valid tokens and one directed policy, A-subject/B-actor succeeds while B-subject/A-actor receives the stable denial, commits one denied invoke decision, and issues no token or effect |
| `R11-4` | Normal `Bifrost()`/`Bifrost.connect()` environment resolution is unchanged; Python `Bifrost(client=delegated)` and TypeScript `Bifrost.connect({ client: delegated })` consume the Rust-owned delegated client; conflicting client/credential/endpoint inputs are rejected; each real SDK journey proves A-authorized read succeeds, B-only write is denied before effect, and direct B still writes |
| `R11-5` | Delegated native ingest retains subject A and actor B in both allowed/denied audit as applicable; a direct B call remains unattributed and can exercise B's own permission |
| `R11-6` | Published query guidance uses the ambient credential chain and contains no secret-valued argument |
| `R9-2` | Card-bound issue-key response, generated schema, served OpenAPI, and consumers use UUID for the credential ID |
| `R11-7` | Every R9/R10-added or materially modified Rust item has its required substantive contract, with an evidence inventory and strict rustdoc/focused tests passing |
| `R11-8` | TypeScript authority permits only the approved thin shared-client projection while retaining all transport/auth duplication bans |

## Focused closure proof

Revise the existing proving scenarios rather than adding parallel suites:

1. Run `query::service_b_acts_for_service_a_with_only_a_table_authority`
   without preview configuration. It must prove A/B exchange, reverse B/A
   denial, delegated read success, HTTP write denial, native-ingest denial
   before effect, direct-B ingest success, audience rejection, and audit
   attribution for A, B, `invoke`, audience, and actor credential.
2. Extend the existing successful, policy-denied, allowed-then-refused, and
   audit-failure token-exchange selectors to prove the exact operation and
   permission fields and preserve one-event/fail-closed behavior.
3. Extend the existing production-validation owner to prove a production
   server with real verifier, policy, audit, and signing configuration no
   longer needs or accepts preview delegation configuration, while existing
   stub/missing-component refusals remain.
4. Extend `credential_ids_publish_their_uuid_contract` and the existing
   issue-key route/journey for the UUID response.
5. Add the explicit delegated-client scenarios above to the existing Python
   and TypeScript integration harnesses, including unchanged environment-only
   Bifrost construction and client/credential conflict refusal, and run them through
   `mise run py:test:integration` and `mise run ts:test:integration` with
   positive named selections.
6. Rerun the existing root CLI secret-option proof and `mise run docs:check`
   after correcting the live query example.
7. Append the diff-scoped Rust documentation inventory and rerun strict rustdoc
   plus the exact focused tests for the documented items.

Every specifically named Rust, Python, or TypeScript test recorded in evidence
must include its literal exact repository-native command, positive selected
count, and result. Do not create a delegation-only harness.

## Broader verification

Run the narrowest owning lanes for all touched surfaces, including:

- `mise run fmt:check`
- `mise run lints`
- `mise run py:format:check`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run py:test:unit`
- `mise run py:test:integration`
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run ts:test:unit`
- `mise run ts:test:integration`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- the existing environment-owning `auth_e2e` command from R10
- `mise run test:bifrost:journey:server`
- `mise run test:bifrost:integration:server`
- `mise run test:shared`
- `mise run test:sql`
- `mise run codegen:check`
- `mise run docs:check`
- strict rustdoc for every affected crate
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Approved `VER-003` forbids substituting or requiring `mise run gate`; do not
run it as remediation evidence. Append one evidence table mapping every
acceptance row to implementation commits, exact focused commands and positive
selected counts, owning lanes, and results.

## Implementation evidence

Candidate: `395a0dee5` on `claude/admin-principals-spec-qfsmjc`. Focused commands
ran through `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run
db:migrate:all:inner && mise exec -- <command>'` where Postgres is required.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `R11-1` production delegation without preview | `c7a87f76c` removes the preview config, error, state, builder switch, docs, and TS error code | `cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::production_auth_carries_no_preview_setting)'`: 1 passed; `cargo nextest run --locked -p wyrd-server --test pg_router_smoke -E 'test(=production_valid_target_serves_token_exchange_without_preview)'`: 1 passed | PASS |
| `R11-2` `auth.token.exchange` / `invoke` on every delegation decision | `4c98bb397` (`exchange_api_key.rs`, `issuance.rs`) | `-p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::<name>)'` each 1 passed for `an_exchange_names_subject_and_actor_and_carries_only_the_intersection`, `a_policy_denied_exchange_commits_one_denied_decision`, `an_allowed_exchange_for_a_missing_actor_commits_one_allowed_decision`, `a_refused_exchange_audit_issues_no_token`; `-p wyrd-server --test pg_openapi_contract -E 'test(=an_unstageable_exchange_audit_answers_with_a_code_the_token_operation_documents)'`: 1 passed | PASS |
| `R11-3` directed A→B succeeds, B→A denied with one denied decision | `bb6788b99` | `cargo nextest run --locked -p wyrd-testing --test server --run-ignored=all -E 'test(=query::service_b_acts_for_service_a_with_only_a_table_authority)'`: 1 passed | PASS |
| `R11-4` Python/TS Bifrost consume a delegated client | `f7655e90f` (Python), `7fec8a115` (TS) | `uv run python -m pytest -q -m integration tests/integration/bifrost/test_bifrost_e2e.py -k 'test_delegated_client_reads_as_a_and_cannot_write_with_b_authority or test_client_cannot_be_combined_with_transport_options'` (in `sdks/wyrd-sdk-python`): 2 passed; `pnpm exec vitest run tests/integration/bifrost-write.test.ts -t "reads as A but cannot write with B's authority through a delegated client"` (in `sdks/wyrd-sdk-ts/wyrd`): 1 passed (covers env-only construction and the client/credential conflict refusal); `mise run py:test:integration`: 57 passed; `mise run ts:test:integration`: 18 passed | PASS |
| `R11-5` delegated native ingest keeps subject A and actor B | `5c17276ed` (`gate_audit.rs`), proved in `bb6788b99` | R11-3 journey above (delegated ingest denied before effect with A/B attribution; direct-B ingest succeeds) | PASS |
| `R11-6` query docs use the ambient credential chain | `7da7b5185` (`reading-data.svx`) | `-p wyrd-cli --lib -E 'test(=cli::tests::no_shipped_command_takes_a_secret_argument)'`: 1 passed; `-E 'test(=cli::tests::the_root_parser_refuses_every_former_secret_option_without_echo)'`: 1 passed; `mise run docs:check`: pass | PASS |
| `R9-2` issued key credential id is a UUID | `710a3fd0a` | `-p wyrd-server --test pg_openapi_contract -E 'test(=credential_ids_publish_their_uuid_contract)'`: 1 passed; `-p wyrd-cli --test cli -E 'test(=auth_issue_key_journey::auth_issue_key_cli_journey)'`: 1 passed (2.7s, real server) | PASS |
| `R11-7` R9/R10 Rust items documented | `bf85bc038` (inventory: its 29-file diff across wyrd-auth-check, wyrd-auth-issue, wyrd-auth-verify, wyrd-client, wyrd-runtime, wyrd-spec, wyrd-auth, wyrd-cli, wyrd-server, wyrd-testing) | `mise run check:docs` (strict `-D missing_docs -D rustdoc::broken_intra_doc_links`): pass; the focused tests above | PASS |
| `R11-8` TS authority permits only the thin shared-client projection | `7da7b5185` (`typescript-guide.md`) | Review of the diff: duplication bans retained; `mise run ts:typecheck`: pass | PASS |

Owning lanes on this candidate: `mise run test:principals:unit` (39 passed),
`mise run test:principals:integration` (77 passed), `mise run test:cli:journey`,
the `auth_e2e` command (8 passed), `mise run test:wyrd` (2024 passed),
`mise run test:sql` (247 passed), `mise run test:identity:journey` (20 passed),
`mise run test:shared` (658 passed), `mise run test:bifrost` (9/9 lanes),
`fmt:check`, `lints`, `py:format:check`, `py:lints`, `py:typecheck`,
`py:test:unit`, `ts:typecheck`, `ts:test:unit`, `codegen:check`, and the
`check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`,
`check:clippy-allow-audit`, `check:tenant-isolation`, and
`check:from-pools-allowlist` boundary checks: all pass.

Harness corrections made while proving this task, outside the R11 findings:
`WyrdTestServer` now traces through the process telemetry stack (`a2dee208b`);
the `WYRD_*_E2E` early-returns, which reported 110 unrun Postgres tests as
passed, are deleted and those suites run in `test:wyrd`/`test:sql`
(`1fec93048`, `395a0dee5`). Non-goals stayed excluded.
