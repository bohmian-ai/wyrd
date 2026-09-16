---
id: TASK-003-R3
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [REQ-031, REQ-032, REQ-035A, REQ-064, INV-025, AC-007, AC-014, AC-022]
depends_on: [TASK-003-R2]
parent_task: TASK-003
remediates: [FIND-TASK-003-R2-1, FIND-TASK-003-R2-2, FIND-TASK-003-R2-3]
---

# Close TASK-003-R2 review findings

Implementation route: `$wyrd-implement`.

## Authority and immutable subject

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9.
- Original tasks: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`, `TASK-004-unify-bifrost-data-root.md`, and `TASK-003-close-repository-integration.md` in the same directory.
- Prior remediation: `review/tasks-002-r3-004-003-bbfdf35e-review-01/TASK-003-R1-close-cumulative-review-findings.md` and `review/task-003-r1-9c52f8875-review-01/TASK-003-R2-close-r1-review-findings.md`, relative to the active change directory.
- Validated R2 review: `review/task-003-r2-1b2db8634-review-01/{verdict,findings-validation}.md`.
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Reviewed candidate: `1b2db8634bf83c03ba210ebf55700983dd9091e6`, tree `52b3fe3243df7f74ba793eaa5be6caa84fba78ad`.
- Last tested source: `117f668f6046f15dcfb7b197fc53f8557f4c1757`, tree `e97a1e0aa2e0a25368167fcb4fca06792a71e270`; the candidate's last commit only adds the R2 evidence section.

## Outcome

Close the remaining remote-query deadline hole without changing the public
query deadline or protected-edge contract. Complete mandatory rustdoc on the
new edge layer. Produce nonzero passing results for every required local lane
on one immutable final corrected tree, including the six lanes omitted from
the R2 evidence record. Preserve all R1/R2 source corrections and revision-9
local cloud-proof authority.

## Findings and required corrections

### `FIND-TASK-003-R2-3` — selected remote delivery is unbounded after HTTP handoff

The R2 edge timer correctly bounds body collection, authentication, and
capability admission, then yields to Oracle's deadline. On a gateway with no
local Oracle, `ReadyOracleForwarder` selects a remote peer. Its existing
monotonic deadline bounds candidate connection attempts, but the selected
delivery awaits credential acquisition and the initial peer response without
that deadline. A connected silent peer can therefore retain the HTTP
concurrency slot beyond both limits, with no typed query timeout. The local
preparation journey does not exercise this branch.

At the existing remote-forwarding owner, apply the already captured monotonic
query deadline to the selected delivery future through receipt of the first
peer response. On local expiry, return `BifrostError::QueryTimeout`. Preserve
connect-before-delivery selection, signed absolute deadline, one selected
envelope with no delivery retry or successor, existing peer error mapping,
and cancellation by dropping the in-flight future. Do not re-arm the generic
HTTP timer, introduce a second public or configured deadline, or change the
post-header terminal body behavior. Keep the existing local Oracle positive
path and all protected-edge safety layers unchanged.

Proof: exercise a selected connected peer whose delivery does not return,
with a short explicit query deadline. Require the typed timeout, cancellation
of the delivery wait, and no second delivery. Exercise the role-separated
HTTP path (or its existing production-shaped equivalent) so the protected
request settles and releases its concurrency slot. Retain the local
preparation-beyond-edge, pre-Oracle ingress-timeout, and non-query edge-timeout
controls. Use deterministic synchronization, not a 30-second sleep.

### `FIND-TASK-003-R2-2` — new edge service violates mandatory rustdoc

`edge_timeout.rs` introduced an `EdgeTimeout` layer/service with four
undocumented associated types and no `# Errors` section on its fallible
`poll_ready` and `call` methods. `AGENTS.md` §16 and
`architecture/agent-rules.md` require those docs even when compilation and
lint pass.

Document the associated types' roles in the protected stack. Document the
propagated readiness/inner-service failure on `poll_ready` and the
inner-service or edge-expiry error on `call`. Change no behavior, checker, or
abstraction for this item. Prove it by source audit, `mise run fmt`, and
`mise run lints` on the corrected tree.

### `FIND-TASK-003-R2-1` — final-tree proof omits six required owner lanes

The R2 evidence records 31 passing checks at `117f668f6`, including `gate`
and three local real-cloud providers. It does not record the original
TASK-003's identity journey, four dedicated Postgres lanes, or Forge
production-geometry journey on that tree. These have separate `mise` owners
and are outside `gate`; earlier-commit results cannot prove the reviewed
candidate. `verify:bifrost` is also named by the original task, but its exact
same-tree child closure may be demonstrated by already required runs rather
than blindly repeating equivalent work.

After the remote correction and documentation are committed, run the required
matrix sequentially on one immutable final commit/tree. In particular run
and record nonzero selection for each missing owner lane:

```bash
mise run test:identity:journey
mise run test:postgres:contract
mise run test:postgres:concurrency
mise run test:postgres:roles
mise run test:postgres:inventory
mise run test:bifrost:journey:forge:production-geometry
```

Run `mise run verify:bifrost` or provide an exact, same-tree map showing that
its `check:bifrost` and `test:bifrost` children passed through the recorded
commands. Because this task changes server source, rerun the complete
TASK-003-R1/R2 focused and broad verification set, including `mise run gate`,
the affected Bifrost server journey, and the three local real-cloud tasks,
on that new final tree. The 117f668f6 results cannot substitute for tests of
new source. Record every command, nonzero selected count, result, commit, and
tree. Keep Cargo-backed runs sequential; check disk capacity before the broad
gate and use only safe repository-supported cache recovery if necessary.
Do not add an aggregate, waive a failing lane, or filter a test to zero.

## Constraints and acceptance

- Preserve the approved `WYRD_STORAGE_URL` and optional endpoint contract,
  all three storage backends and live-cloud tests, and the merge-to-`main`
  cloud Actions workflow. Local `mise.local.toml` runs are sufficient
  pre-merge cloud proof; no GitHub Actions candidate result is required.
- Preserve the one `HttpTransport` client, bounded JSON control requests,
  unbounded healthy streaming transfers, credential separation, and public
  `request_arrow` API.
- Preserve request ID, default-deny authentication, panic/error mapping,
  load-shed, concurrency, and body-size protections; the query edge timer
  remains active until capability admission.
- Preserve Forge production cancellation, publication, and recovery
  behavior; retain the original shutdown-only recovery assertion and its
  test-support-only post-reconciliation synchronization.
- Do not change the public query deadline/schema, terminal-frame semantics,
  tenant isolation, or the one selected remote attempt. Do not claim this
  task fixes Azure SAS expiry or LocalFS server upload limits.
- Do not weaken, skip, ignore, or allowlist any test or gate; do not merge,
  push, release, deploy, or perform final change review.

| Acceptance criterion | Finding |
|---|---|
| A silent selected remote peer cannot hold pre-header delivery beyond Oracle's captured deadline; expiry is typed, cancels the wait, and starts no successor. Local/role-separated query and protected-edge controls still pass. | `FIND-TASK-003-R2-3` |
| All new edge-layer associated types and fallible methods meet `AGENTS.md` §16 rustdoc requirements, without behavior change. | `FIND-TASK-003-R2-2` |
| The six omitted lanes and the complete required matrix have passing, nonzero, same-tree evidence on one final corrected candidate; `verify:bifrost` is run or exactly mapped to equivalent child proof. | `FIND-TASK-003-R2-1` |

For every specifically named new Rust test, record and run its exact
`mise exec -- cargo nextest run --locked` command with package, target,
features, and exact test expression; include the owning repository-managed
environment wrapper for Postgres or a live server. Run the narrowest owning
`mise` test task for broader proof, then `mise run fmt`, `mise run lints`,
and `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<final-candidate>`.
Keep the evidence on the same immutable final tree and record any evidence-only
commit separately from source.

## Implementation evidence

Verified candidate: commit `f8e887b3038651d2ba82091d642915d6b856d4d6`,
tree `eb98062dc854277a248f37d86afe55beb9c3db26` (source commits `64c29005f`,
`f8e887b30`). Every command below ran sequentially on that commit with `HEAD`
unchanged. The commit recording this section adds only this review packet.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| A silent selected remote peer cannot hold pre-header delivery past Oracle's captured deadline; expiry is typed, cancels the wait, and starts no successor (`FIND-TASK-003-R2-3`) | `64c29005f`: `oracle/forwarding.rs::route_remote_once` bounds `deliver` (credential acquisition + initial `forward_query` response) by the already captured monotonic deadline, maps expiry to `BifrostError::QueryTimeout`; connect-before-delivery, signed deadline, one envelope, and peer error mapping unchanged. `f8e887b30`: `test-support`-only `SilentForwardPeer` parks inbound envelopes in the leader's `forward_query` handler before acceptance | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::forwarding::tests::silent_selected_delivery_times_out_once_and_is_cancelled) \| test(=oracle::forwarding::tests::ready_oracle_forwarding_retry_boundary_is_causal_and_one_cut)'` 2 passed (paused-time; asserts typed timeout, dropped delivery future, one delivery, one connect). `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query::silent_remote_oracle_delivery_yields_typed_query_timeout) \| test(=query::query_edge_timeout_yields_to_oracle_deadline)"'` 2 passed: role-separated cluster, Scribe-only HTTP ingress, 2 s deadline → `WYRD_VALA_504_QUERY_TIMEOUT`, parked peer handler abandoned, exactly 1 envelope across Oracles, follow-up forwarded query on the same ingress succeeds; local preparation-beyond-edge, pre-Oracle query body, and `/v1/cards` edge-timeout controls pass. Negative control: with the delivery timeout removed the new journey failed ("a silent remote peer held the HTTP query past its deadline"). `mise run test:bifrost:journey:server` 12 passed | PASS |
| New edge-layer associated types and fallible methods meet §16 rustdoc, no behavior change (`FIND-TASK-003-R2-2`) | `64c29005f`: `http/middleware/edge_timeout.rs` docs on `Layer::Service`, `Service::{Response, Error, Future}`; `# Errors` on `poll_ready` and `call`; no code change | Source audit; `mise run fmt:check`, `mise run lints` passed | PASS |
| Six omitted owner lanes and complete matrix pass, nonzero, same tree; `verify:bifrost` run (`FIND-TASK-003-R2-1`) | — | `test:identity:journey` 18 passed; `test:postgres:contract` PASS (wrapper contract script); `test:postgres:concurrency` PASS (two concurrent isolated lifecycles); `test:postgres:roles` 1+1 passed, role audit PASS; `test:postgres:inventory` inventory + 2 negative suites PASS; `test:bifrost:journey:forge:production-geometry` 1 passed (469 s). `verify:bifrost`: see note | PASS |
| Full R1/R2 focused and broad verification repeated on the new tree | `64c29005f`, `f8e887b30` | `fmt:check`, `lints`, `codegen:check`, `docs:check`, `check:ci-selection` (32+7), `check:bifrost-oracle-deploy` (2), `check:bifrost-resource-governance`, `check:unwrap-audit` passed. Exact: Forge `forge::compaction_admission::acceptance_unknown_recovers_from_durable_state` (`-p vala-bifrost-redux --features test-support,bench-support --test integration -P journey --run-ignored=all`, Postgres wrapper) 1 passed; `wyrd-client --test transport -E 'test(/^http::transport_behavior::/)'` 18 passed; `wyrd-storage --all-features --lib -E 'test(/^settings::/)'` 9 passed; `wyrd-spec --lib -E 'test(/^vala::api::bifrost_wire_tests::/)'` 11 passed; `wyrd-server --lib -E 'test(/^boot::data_root::tests::/)'` 4 passed. Lanes: `test:shared` 649; `test:storage:matrix` all invocations nonzero, passed; `test:cards:integration` 6+23; `test:cli:journey` 20; `test:wyrdstate:journey` 1; `py:test:testing` 5; `py:test:integration` 55; `ts:test:integration` 17 passed. `gate`: see note | PASS |
| Local real-cloud tasks on the same SHA | untracked `mise.local.toml` | `mise run storage:s3:dev`, `storage:gcs:dev`, `storage:azure:dev`: each handle CRUD 1 passed + multipart e2e 1 passed | PASS |
| Whitespace-clean range | — | `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..f8e887b3038651d2ba82091d642915d6b856d4d6` exit 0 | PASS |

Note — `verify:bifrost` and `gate` exact closure. The host repeatedly killed
long background runs for low system memory (other sessions were compiling
concurrently), so both aggregates were completed as their exact declared
children, sequentially, on the same commit/tree. No child was skipped or
filtered:

- `verify:bifrost` = `check:bifrost` (fmt check, scoped Clippy
  `-D warnings`, oracle-deploy, resource-governance, object-store-pin,
  tenant-isolation: all passed in the `mise run verify:bifrost` run) +
  `test:bifrost` = `scripts/run-bifrost-tests.sh` lanes: `unit:rust` 222,
  `unit:python` 2, `unit:typescript` 6, `integration:redux` 978,
  `integration:sql` 113 (passed in that run before the memory kill);
  `integration:server` 67 (Postgres wrapper + `:inner`); `journey` =
  `test:bifrost:journey:{sdk 16, forge 13, scribe 21, oracle 28, otlp 10,
  server 12, mcp 8}`; `journey:python` 35 (+1 migration); `journey:typescript`
  16 passed.
- `gate` = `test:bifrost` (above) + its 48 other `depends`, each run as
  `mise run <task>`: `check`, `check:skills-sync`, `test:rust` (nonzero
  nextest lanes incl. 1907, 1272, 649, 410, 113, 102, 44),
  `codegen:check`, `cardkind:check`, all `check:*` tier/scope/audit/registry/
  tenant/token/docs/examples/vocab boundary checks, `py:setup`,
  `py:format:check`, `py:lints`, `py:typecheck`, `py:test:unit` 465,
  `ts:napi:check`, `ts:typecheck`, `ts:test:unit` 12,
  `examples:python:datacard`, `check:test-coverage`: all 48 exit 0. A killed
  in-progress `mise run gate` attempt recorded only six `SIGTERM`-cancelled
  Redux tests (signal cancellation, not assertion failures); the same suite
  passed 978/978 on this tree.

Deviation: the role-separated proof needed a silent connected peer; no
existing hook could park the leader before its own deadline governs, so one
`test-support`-only `SilentForwardPeer` switch was added on
`ReadyOracleForwarder` and consulted by the private `forward_query` handler.
Production builds are unchanged. The protected-edge concurrency slot is shown
released by the HTTP request settling and a subsequent query succeeding on the
same ingress; the cluster harness exposes no per-node concurrency limit.

Non-goals held: no public query deadline/schema, terminal-frame, edge-layer,
tenant, storage, or Forge change; no second deadline or configuration; no
Azure SAS or LocalFS upload-limit claim; no skipped/ignored tests or `#[allow]`;
no push, merge, release, or deploy; `mise.local.toml` untracked; unrelated
`changes/active/verified-change-contract/*` edits untouched.

Status: `IMPLEMENTED`.
