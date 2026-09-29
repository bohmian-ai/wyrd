# TASK-006 R2 Wave 2 Structured Ponytail Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `3593bbc31273673f87159315fbf66a73562d3c99`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-006-r1/`
- Wave 1 inputs: `task-review.md`, `standards-review.md`, `domain-review-eval.md`, `domain-review-data.md`, and `domain-review-security.md`

The candidate remained `HEAD` and unchanged during this validation. No reviewed
source was modified.

## Wave 1 finding validation

| Wave 1 source ID | Status | Resolution |
|---|---|---|
| `REPO-1` | **CONFIRMED** | Retained as `FIND-TASK-006-11`. The new `WyrdTestServer::superuser_pool` is a second forwarding API for a raw `PgPool`, expressly prohibited by `architecture/agent-rules.md`. Its three callers can use the already exposed fixture owner directly; no helper or replacement abstraction is needed. |
| `EVAL-R2-001` | **REVISED** | Retained as `FIND-TASK-006-12`. AC-027's real-seam proof is missing for an unbound prompt media variable, an unsupported valid kind/MIME pairing, and oversized media. The existing real cross-tenant record already proves the “inaccessible or cross-tenant URI” branch, so a second absent-object journey case is not required. Unit tests prove helper behavior but cannot prove continuous-run terminal mapping, result/dispatch suppression, or provider suppression. |

No Wave 1 finding was rejected. No optional suggestion was promoted.

## Caller and reachability validation

### `REPO-1`

`WyrdTestServer::superuser_pool` has exactly three callers, all in
`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`:

- `continuous_eval_runs_the_terminal_matrix` uses it only for the post-ACK
  enqueue-failure trigger;
- `sealed_replay_on_a_later_day_activates_once` uses it only to hold a table
  lock while replay acknowledgements race; and
- `continuous_eval_failures_publish_only_stable_errors` uses it only for a
  temporary restrictive policy and assertion query.

The complete bodies of those tests and the forwarding method were inspected.
The server already exposes `pg_fixture()`, and repository tests already use
`server.pg_fixture().superuser_pool()`. Deleting the new forwarding method and
changing these three expressions preserves the same fixture-owned pool and all
test behavior while removing the redundant API. Moving DDL behind new helpers
would add more surface and is rejected.

### `EVAL-R2-001`

The continuous path is reachable and singular:

`EvalEngine::score` → `ScenarioScoring::score_record` →
`JudgeTaskExecutor::execute` → `SkaldJudgeInvoker::invoke` →
`SkaldJudgeInvoker::bind_media`/`Prompt::render` → local provider.

The full bodies at this boundary were inspected. `score_record` builds bindings
from the committed record. `TenantMedia::resolve` rejects unsupported MIME,
wrong-tenant paths, missing objects, and bodies over the fixed ceiling before
native binding. With no record binding, `Prompt::render` rejects the remaining
`${media:shot}` variable. These implementation paths exist, but the real
journey currently emits only valid media and one foreign-tenant URI. Its record
named `missing` omits assertion context while still supplying valid `shot`
media; it is not a missing-media case. Unsupported MIME and oversized media are
proved only below the SDK/server/runtime seam.

AC-027 explicitly requires negative tests, and TASK-006 requires AC-027 to pass
through real server/provider seams. The gap is therefore task-required, not
speculative test expansion. Reuse the existing terminal-matrix graph, object
store, provider capture, `assert_unresulted`, and body assertions. A new test
harness, production abstraction, absent-object case, or provider mock is
unnecessary.

## Prior-finding closure

| Stable finding | R2 validation | Result |
|---|---|---|
| `FIND-TASK-006-1` | Scribe's existing commit disposition reaches Gate, which activates only the first committed observation; the later-day replay journey checks stored and frozen event time. | CLOSED |
| `FIND-TASK-006-2` | The queue assigns and stores a serialized per-binding observation ordinal once; claims reuse it across retry and restart. | CLOSED |
| `FIND-TASK-006-3` | `StorageHandle::get_object_bounded` retains only the ceiling plus one sentinel byte, and `TenantMedia` rejects overflow before encoding/provider invocation. | CLOSED |
| `FIND-TASK-006-4` | The trace projection reconstructs persisted events, links, attributes, and dropped counts into the existing `SpanRecord`. | CLOSED |
| `FIND-TASK-006-5` | Trace reads use the total order `start_time_unix_nano, span_id`. | CLOSED |
| `FIND-TASK-006-6` | The real SDK journey separates authored creation day from managed receipt day and reads by the frozen managed day. | CLOSED |
| `FIND-TASK-006-7` | Eval reads resolve the persisted tenant SYSTEM principal, grant only the two Eval input tables, and use canonical Oracle authorization/audit. | CLOSED |
| `FIND-TASK-006-8` | Trace reads have a closed event-time range and fixed ceiling-plus-one sentinel before decode/provider work. | CLOSED |
| `FIND-TASK-006-9` | Public and persisted errors use stable fixed text; dependency causes remain in structured diagnostics and media locators are redacted. | CLOSED |
| `FIND-TASK-006-10` | `fan_out_bucket` documents first-error propagation, sibling cancellation, discarded partial outputs, and prior-side-effect limits. | CLOSED |

The two retained R2 findings are new acceptance/repository-rule gaps. They do
not reopen or renumber the ten closed R1 findings.

## Final deduplicated finding ledger

### `FIND-TASK-006-11`

- **Source ID:** `REPO-1`
- **Status / classification:** CONFIRMED / VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` bans raw `PgPool` in
  library signatures and states that the fixture allowlist covers construction,
  never propagation through another signature.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:2676-2691`; callers at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:644,1072,2039`.
- **Evidence:** `WyrdTestServer::superuser_pool` only forwards
  `self.inner.fixture.superuser_pool()`. `WyrdTestServer::pg_fixture` already
  exposes that owning fixture, and no production or other test caller uses the
  new method.
- **Observable consequence:** the test-server library adds a redundant raw
  cross-tenant pool surface that hides the fixture authority and permits later
  tests to bypass the repository's two sanctioned SQL handle types.
- **Decision-complete correction:** delete
  `WyrdTestServer::superuser_pool`; at its three callers, reuse
  `server.pg_fixture().superuser_pool().await?`. Do not add a replacement
  wrapper or move these one-off DDL/assertion operations into new helpers.
- **Focused closure proof:** run the three affected exact Eval journey tests,
  `mise run check:from-pools-allowlist`, `mise run lints`, and
  `git diff --check`.

### `FIND-TASK-006-12`

- **Source ID:** `EVAL-R2-001`
- **Status / classification:** REVISED / MISSING
- **Violated obligation:** AC-027 requires negative tests proving missing
  binding, inaccessible or cross-tenant URI, unsupported kind/MIME, and
  oversized media become visible execution/input errors without a fabricated
  verdict or Operator dispatch. TASK-006 requires AC-027 to pass through the
  real server/provider seam.
- **Location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:719-803,879-895`;
  disconnected lower-level coverage at
  `crates/wyrd/wyrd-server/src/verification/eval.rs:1260-1350` and
  `crates/vala/vala-eval/src/tasks/media.rs:208-223`.
- **Evidence:** the terminal journey proves native valid media and a foreign
  tenant URI. Its `missing` record still supplies valid `shot` media and fails
  on absent assertion context. No real continuous record exercises an unbound
  `${media:shot}`, a valid media kind paired with an unsupported MIME, or an
  object over `MEDIA_LIMIT_BYTES`. The lower-level tests cannot observe run
  settlement, canonical result suppression, dispatch suppression, or the
  production provider boundary.
- **Observable consequence:** the required acceptance lane remains green if
  the continuous adapter regresses any of those three refusal paths into a
  result, dispatch, or provider request.
- **Decision-complete correction:** extend the existing
  `continuous_eval_runs_the_terminal_matrix` data matrix with exactly three
  gated records: no `shot` binding, one supported kind with an incompatible
  MIME, and one stored body over the fixed media ceiling. Keep the existing
  foreign-tenant record as the inaccessible/cross-tenant proof. Reuse the
  current graph, storage handle, provider capture, settlement helpers, and
  assertions; add no production code or new harness.
- **Focused closure proof:** each new gated run settles `errored` with
  `eval_execution_failed`, no result row, no result ID, and no Operator
  dispatch. The existing provider-body invariant must still show that every
  received request contains the valid image bytes and no private URI, thereby
  proving no refused record reached the provider. Run the exact terminal-matrix
  journey and its owning Bifrost server journey lane.

## Validation result

**Two retained findings:** `FIND-TASK-006-11` and `FIND-TASK-006-12`.

Both corrections are bounded, reuse existing owners and fixtures, require no
new dependency or architecture decision, and preserve adjacent durability,
authorization, result-settlement, and provider behavior.

## Verification limits

- This was a static Wave 2 validation. The complete base-to-candidate diff,
  applicable authorities, Wave 1 reports, named correction bodies, and their
  callers were inspected; broad Cargo/Postgres lanes were not rerun within the
  sub-review budget.
- No live cloud store or external provider was used. The existing repository
  object store and local mock provider are the applicable acceptance seams.
- The candidate remained `3593bbc31273673f87159315fbf66a73562d3c99` throughout.
