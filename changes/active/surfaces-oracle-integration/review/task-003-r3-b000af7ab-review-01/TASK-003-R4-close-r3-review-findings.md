---
id: TASK-003-R4
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [REQ-064, INV-025, AC-022]
depends_on: [TASK-003-R3]
parent_task: TASK-003
remediates: [FIND-TASK-003-R3-1, FIND-TASK-003-R3-2, FIND-TASK-003-R3-3]
---

# Close TASK-003-R3 review findings

Implementation route: `$wyrd-implement`.

## Authority and subject

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9.
- Original tasks: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`, `TASK-004-unify-bifrost-data-root.md`, and `TASK-003-close-repository-integration.md` in the same directory.
- Prior remediation: `TASK-003-R1`, `TASK-003-R2`, `TASK-003-R3` under this change's review directories. Validated R3 review: this directory's `verdict.md` and `findings-validation.md`.
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Reviewed candidate: `b000af7ab704f077a8a4ba3e29c2b968d47d344d`, tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`.
- Last tested source: `f8e887b3038651d2ba82091d642915d6b856d4d6`, tree `eb98062dc854277a248f37d86afe55beb9c3db26`; the candidate's last commit adds review/evidence Markdown only.

## Outcome and corrections

Close three bounded acceptance gaps without altering query behavior or the approved contract. The selected-remote deadline fix, test-only silent peer, local cloud proof, and six formerly missing owner lanes passed their R3 review; preserve them.

### `FIND-TASK-003-R3-1` — final-tree broad gate has not passed

Approved REQ-064/AC-022 and TASK-003 require a passing local broad gate on the final integrated candidate. R3 records killed `mise run gate` attempts and a grouped claim that the gate's 49 dependency tasks passed separately. That is substantial coverage, but no completed aggregate result exists; R3 expressly allows child equivalence for `verify:bifrost` only. The earlier passing gate predates the R3 source correction. Thus the final-tree evidence obligation remains unproved, even though no product-test assertion failure is established.

Run the existing `gate` on one immutable final corrected source commit to exit 0. The gate is a dependency-only `mise` aggregate; serialize its children with `mise run -j 1 gate` and avoid competing compiler jobs so memory pressure does not kill it. Keep every child and selection intact. Record the exact command, result, nonzero test selections, source commit, and tree. Do not add a new aggregate or checker, claim a killed process passed, use an earlier-tree result, or silently substitute grouped child claims. If this exact gate remains impossible, stop and seek explicit approval to change the broad-gate acceptance condition; this task does not grant that waiver.

### `FIND-TASK-003-R3-2` — new test items lack mandatory rustdoc

`crates/wyrd/wyrd-server/src/oracle/forwarding.rs` introduces `Abandon` and `DropProbe` tuple fields without field rustdoc (around lines 118 and 1043). The new `silent_selected_delivery_times_out_once_and_is_cancelled` test (around line 1052) uses assertions and `expect` but lacks `# Panics`. `AGENTS.md` §16 covers private fields and test helpers; passing tests do not waive it.

Document what each tuple field holds and why its drop is observed, and add a concise `# Panics` section describing the cancellation/one-attempt assertions. Retain the existing narrow probes and their behavior. Source audit, `mise run fmt:check`, `mise run lints`, and the existing exact paused-time test prove this correction.

### `FIND-TASK-003-R3-3` — qualified types in new Rust fields and signatures

`architecture/agent-rules.md` requires module-top `use` statements and bare type names in fields, parameters, return types, and bounds. New `SilentForwardPeer` fields, `wait_abandoned`'s return, the `Abandon` and `DropProbe` fields, and `Bifrost::silent_forward_peer_for_test`'s return use qualified paths. The materially changed `route_remote_once` bound still qualifies `VisibilityMode` (`forwarding.rs` around lines 71, 73, 104, 118, 598, 1043; `state.rs` around line 1735). This violates the repository's dependency-manifest convention in directly used production or test paths.

Import only those types at the tops of the owning modules, feature-gated when solely test-support code needs them, and use bare names at the identified type positions. Body-local paths are outside this finding. Do not introduce aliases, modules, dependencies, features, or behavior changes. Prove by source audit, `mise run fmt:check`, `mise run lints`, the exact forwarding unit test, and the owning Bifrost server journey.

## Constraints and proof

- Preserve the captured Oracle deadline, typed timeout, one selected remote attempt, cancellation, peer authentication/tenancy, protected-edge admission, terminal stream, and `test-support` isolation. Do not alter the public query deadline/schema or unrelated storage/Forge behavior.
- Preserve approved `WYRD_STORAGE_URL` and optional endpoint, merge-to-`main` cloud Actions, and local `mise.local.toml` S3/GCS/Azure proof. Post-merge Actions results are not a pre-merge gate. Do not claim Azure SAS or LocalFS upload-duration remediation.
- On one immutable final source tree, run the exact named forwarding unit test through `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::forwarding::tests::silent_selected_delivery_times_out_once_and_is_cancelled)'`, the owning `test:bifrost:journey:server`, `mise run fmt`, `mise run lints`, and the broad `mise run -j 1 gate` to completion. Run `verify:bifrost` or provide its R3-authorized exact same-tree child map.
- Preserve complete final-tree TASK-003/R1/R2/R3 verification, including the six distinct owner lanes (`test:identity:journey`, `test:postgres:{contract,concurrency,roles,inventory}`, `test:bifrost:journey:forge:production-geometry`), focused client/Forge/contract/root tests, storage matrix, cards/CLI/WyrdState and Python/TypeScript journeys, codegen/docs/CI checks, and all three local real-cloud tasks. Record each task's command, result, and nonzero selected count (or explicit non-test check result) with commit/tree. Reuse gate's passing dependency results where a task is genuinely in its closure; do not rerun identical work solely for a second label.
- Run `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<final-candidate>`. Keep Cargo-backed tasks sequential and check host disk/memory before the broad gate. Do not weaken, skip, ignore, allowlist, or filter tests to zero; do not push, merge, release, deploy, or perform final change review.

| Acceptance criterion | Finding |
|---|---|
| The immutable final source tree has a completed passing broad gate and complete nonzero same-tree required lane evidence. | `FIND-TASK-003-R3-1` |
| The two tuple fields and new unit test meet `AGENTS.md` §16 rustdoc, with no behavior change. | `FIND-TASK-003-R3-2` |
| The identified field/signature type positions obey module-top imports and bare-name style, without changing the feature graph or runtime behavior. | `FIND-TASK-003-R3-3` |

Attach evidence to this packet on the tested commit/tree; record a later evidence-only commit separately. Status becomes `IMPLEMENTED` only after all three criteria and the required final-tree verification pass.

## Implementation evidence

Verified candidate: commit `d6de890d83f269eab834fa323f3ebe31f1adc573`,
tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03` (source commit `d6de890d8`).
Every command below ran sequentially on that commit with `HEAD` unchanged. The
commit recording this section adds only this review packet.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Completed passing broad gate and complete nonzero same-tree lane evidence (`FIND-TASK-003-R3-1`) | — | `CARGO_BUILD_JOBS=4 mise run -j 1 gate` → exit 0, `Finished in 2084.38s`, no failed or signal-terminated test. Nonzero selections include `test:rust` nextest lanes 1907, 1272, 649, 410, 113, 102, 44; `test:bifrost` 9/9 lanes (`unit:rust` 222, `unit:python`, `unit:typescript` 6, `integration:redux` 978, `integration:sql` 113, `integration:server` 67, `journey` 7/7 capabilities: sdk 16, forge 13, scribe 21, oracle 28, otlp 10, server 12, mcp 8; `journey:python` 35; `journey:typescript` 16); `py:test:unit` 465; `ts:test:unit` 12; `codegen:check`, `check:ci-selection`, `check:docs`, `check:unwrap-audit`, and every other gate check exit 0 | PASS |
| Tuple fields and new unit test meet §16 rustdoc, no behavior change (`FIND-TASK-003-R3-2`) | `d6de890d8`: field docs on `Abandon.0` and `DropProbe.0`; `# Panics` on `silent_selected_delivery_times_out_once_and_is_cancelled` | Source audit; `mise run fmt:check` (via `fmt` no-diff and `check:bifrost`), `mise run lints` exit 0; `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::forwarding::tests::silent_selected_delivery_times_out_once_and_is_cancelled)'` 1 passed | PASS |
| Identified type positions use module-top imports and bare names, no feature-graph or behavior change (`FIND-TASK-003-R3-3`) | `d6de890d8`: `forwarding.rs` imports `VisibilityMode` and, under `cfg(feature = "test-support")`, `AtomicBool`/`Ordering` and `watch::{Sender, error::RecvError}`; test module imports `AtomicBool`/`Ordering`; `state.rs` imports `SilentForwardPeer` under `test-support`. Bare names in `SilentForwardPeer` fields, `wait_abandoned` return, `Abandon`/`DropProbe` fields, `forward_remote` parameter, `route_remote_once` bound, and `Bifrost::silent_forward_peer_for_test` return | `grep` shows the qualified paths remain only in `use` lines; `mise run lints` (all features) and `cargo check --locked -p wyrd-server --lib --tests` (default features) clean; exact forwarding unit test 1 passed; `mise run test:bifrost:journey:server` 12 passed | PASS |

`verify:bifrost` exact same-tree map: `mise run check:bifrost` exit 0 (fmt
check, scoped Clippy `-D warnings`, oracle-deploy, resource-governance,
object-store-pin, tenant-isolation) + `test:bifrost`, which passed 9/9 lanes
inside the completed `gate` above.

Preserved TASK-003/R1/R2/R3 same-tree lanes (each `mise run <task>` exit 0
unless an exact command is shown):

- Owner lanes: `test:identity:journey` 18; `test:postgres:contract` wrapper
  contract PASS; `test:postgres:concurrency` two isolated lifecycles PASS;
  `test:postgres:roles` 1+1, role audit PASS; `test:postgres:inventory`
  inventory + two negative suites PASS;
  `test:bifrost:journey:forge:production-geometry` 1 passed (450 s).
- Focused: Forge
  `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support,bench-support --test integration -P journey --run-ignored=all -E "test(=forge::compaction_admission::acceptance_unknown_recovers_from_durable_state)"'`
  1; `mise exec -- cargo nextest run --locked -p wyrd-client --test transport -E 'test(/^http::transport_behavior::/)'`
  18; `-p wyrd-storage --all-features --lib -E 'test(/^settings::/)'` 9;
  `-p wyrd-spec --lib -E 'test(/^vala::api::bifrost_wire_tests::/)'` 11;
  `-p wyrd-server --lib -E 'test(/^boot::data_root::tests::/)'` 4.
- Lanes: `test:storage:matrix` all 12 invocations nonzero, passed;
  `test:cards:integration` 6+23; `test:cli:journey` 20;
  `test:wyrdstate:journey` 1; `py:test:testing` 5; `py:test:integration` 55;
  `ts:test:integration` 17; `docs:check`; `lints`. `test:shared` 649,
  `codegen:check`, `check:ci-selection`, and `check:unwrap-audit` are reused
  from the completed gate closure.
- Local real cloud (`mise.local.toml`): `storage:s3:dev`, `storage:gcs:dev`,
  `storage:azure:dev` each handle CRUD 1 + multipart e2e 1 passed.
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..d6de890d83f269eab834fa323f3ebe31f1adc573`
  exit 0.

Environment note: one background runner of the lanes outside the gate was
killed by the host for low memory while starting `test:wyrdstate:journey`.
The 18 lanes before it had already completed and passed; the remaining lanes
then ran to completion in the foreground on the same commit. No killed run is
counted as a pass. `CARGO_BUILD_JOBS=4` limits compiler parallelism only; it
changes no task, test selection, or feature.

Non-goals held: no query deadline, timeout, remote attempt, cancellation,
authentication, tenancy, edge, terminal stream, storage, or Forge behavior
change; no new aggregate, checker, alias, dependency, or feature; no
skipped/ignored/filtered tests or `#[allow]`; no push, merge, release, or
deploy; `mise.local.toml` untracked; unrelated
`changes/active/verified-change-contract/*` edits untouched.

Status: `IMPLEMENTED`.
