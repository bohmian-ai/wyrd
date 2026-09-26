# TASK-001 r1 — Domain review: security, authorization, tenant isolation

Reviewer: `domain-rev` (independent). Subject: base `5293546f3`, candidate
`2e09ae81213cb608253b75de276e3c946353ac35`. HEAD was confirmed at the candidate
at the start and end of the review.

## Reviewed boundary

Composite Card registration for `Verifier` Cards and `verified_by` bindings, traced
end to end:
`register_card_http` (`card:write` RBAC verdict, `routes.rs:303-322`) →
`service::register_card` → `validate_request` (`service.rs:1168-1200`) →
`plan_registration_graph` → `resolve_external` / `resolve_card_references` under
`TenantConn` RLS, including the new `validate_effective_bindings`
(`resolve.rs:20-250`) → `write_registration` (audit append on the write
transaction, `recheck_active_card_refs`, `bind_card_references` UID overwrite,
`persist_node` → `persist_outbound_relationships`, `upsert_service_account_from_card`).
Also covered: the canonical `ReferenceSlotVisitor` (`wyrd-spec/src/refs/mod.rs`),
binding validation (`graph/composition.rs:90-246`), `VerifierSpec` / `TriggerSpec` /
`OperatorSpec` strictness, card-bound principal scope minting
(`wyrd-auth/src/card_scope.rs` with `CardKind::is_observation_target`), the loader
validator (`wyrd-loader/src/validate.rs`), removal of `/v1/eval/runs`
(`components/eval`, router, `AppState.eval_runs`), removal of `vala-core::alert_router`,
and the `vala.drift_alerts` drop migration.

## Authority and source coverage

- Spec rev 32: REQ-045/046/047/056, REQ-090–094, REQ-102/103, REQ-109, REQ-113/114,
  REQ-116, REQ-120, REQ-143/144, INV-001/006/012/013/014, AC-004/018/021/022.
- TASK-001 (Implementation Evidence section ignored), AGENTS.md §2/§9/§11,
  `architecture/wyrd-security-posture.md`, `architecture/wyrd-design.md` (principal model).
- Diffs `task001-nogen.diff` and targeted `git diff 5293546f3 HEAD` reads.

Confirmed sound (no finding):

- External refs resolve only through `select_card_uids_by_ref_batch` on a tenant
  `TenantConn` (forced RLS); effective Verifier/Trigger/Operator bodies are loaded with
  `get_card_by_uid` on the same tenant connection. A cross-tenant target resolves to
  nothing and fails `WYRD_REGISTRY_*_UNRESOLVED_DEPENDENCY` before the write
  transaction. The mechanism is the pre-existing generic resolver, so its cross-tenant
  behavior carries over unchanged.
- UID pinning stays server-authoritative. `CardRef::same_identity` ignores `uid`, and
  `bind_ref` / `bind_inline_ref` overwrite any authored `uid` with the resolved one.
  `recheck_active_card_refs` re-verifies external targets inside the write transaction.
- Rejection is atomic. Kind, duplicate, workflow, and activation checks all run before
  `write_registration` opens its transaction. The `allowed` verdict is audited
  standalone exactly once on failure, following the existing pattern.
- Scope minting: `Verifier`, `Trigger`, and `Operator` are excluded from
  `is_observation_target`, so binding refs do not widen a Service or Agent principal's
  signed `card_ref_scope`. Removing Drift and Eval from that set narrows it.
- Registration activates no work. The diff adds no binding projection, run, schedule, or
  dispatch write. `upsert_service_account_from_card` is unchanged.
- Retired surfaces: `/v1/eval/runs`, `eval_router`, `AppState.eval_runs`,
  `vala-core::alert_router`, and `vala.drift_alerts` have no remaining production or
  build references outside historical migrations and two stale doc comments in
  `vala-eval/src/executor.rs:86,119`. Removing them leaves no route without authz; no
  permission or route was added.
- Authorities: no new "internal SYSTEM" claim goes beyond the existing `System`
  principal-kind prose (`wyrd-design.md:145-153`), and `wyrd-security-posture.md` has no
  stale Drift/Eval Card, `publishes_to`, alert-router, or webhook content.

## Verification limits

I ran no builds or tests; that was per instruction. Orchestrator results in
`scratchpad/verify/summary.txt` at review time:

- `cards_integration` rc=0, 23 passed.
- `cli_journey` rc=0, 23 passed with 5 ignored. It includes
  `card_lifecycle::pg_tests::canonical_authored_directory_runs_real_cli_journey`
  and `multi_card_service_get_hydrates_complete_and_metadata_bundles`, which hit the
  happy path of sibling Verifier/Trigger/Operator binding registration.
- `test_shared`, `cards_unit`, `test_sql`, `codegen`, `client_tier`, `pyo3_scope`, and
  `wyrdstate_journey` rc=0.
- `test_wyrd`, `lints`, `ts_*`, and `final_status` were still pending.

The serde behavior cited in DS-3 comes from reading `serde` 1.0.228/229 source
(`private/de.rs` `InternallyTaggedUnitVisitor::visit_map`), not from running it.

## Findings

### DS-1 — MISSING: fail-closed binding-reference flows have no evidence

- **Violated obligation:** AC-018 (fail-closed unresolved, wrong-kind, unauthorized, and
  cross-tenant refs; duplicate Operator/binding rejection), REQ-092, REQ-094/REQ-143
  (workflow `on_failure` rejected before persistence), AGENTS.md §11 (negative flows
  need journey coverage).
- **Location:** `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:50-250`
  (`validate_effective_bindings`, `EffectiveSpecs`, `check_activation`) and
  `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`. The only change there
  renames `Eval` to `Verifier` in the peer-composition test (lines ~1956-1996).
- **Evidence:** outside `error.rs`, `composition.rs`, `resolve.rs`, and the generated TS
  error-code list, no file mentions `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION`,
  `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`, `INVALID_VERIFIER_REF_KIND`, or
  `DUPLICATE_VERIFICATION_BINDING`. No server test registers a `verified_by` binding
  with:
  - a cross-tenant or unresolved Verifier, Trigger, or Operator CardRef;
  - a wrong-kind CardRef;
  - a referenced Operator Card whose action is `workflow` (only the server's
    registry-backed `validate_effective_bindings` catches this);
  - a Drift Verifier on `observations_ready`.

  The existing cross-tenant test (`non_active_and_cross_tenant_dependencies_leave_no_writes`)
  only exercises an Agent→Prompt dependency.
- **Consequence:** the new server-only enforcement code has no test on any branch.
  Examples: an Operator Card registered earlier with `workflow`, then bound by CardRef;
  or a sibling Trigger whose activation does not match its Verifier. A regression that
  let these persist, or that leaked a cross-tenant binding target, would pass every
  current lane.
- **Testable correction:** in `pg_card_registration_route.rs`, add real-server cases that
  register a Service (Service-level, component, and standalone Agent locations) with
  `verified_by`:
  - a Verifier CardRef seeded in another tenant → `UNRESOLVED_DEPENDENCY`;
  - a pre-registered `workflow` Operator Card referenced by CardRef →
    `WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION`;
  - a referenced `schedule` Trigger bound to an Eval Verifier →
    `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH`;
  - a wrong-kind `runs_on` → `INVALID_BINDING_REF_KIND`;
  - a duplicate Operator → the duplicate error;
  - an under-privileged token → `WYRD_PERMISSION_403_DENIED_RBAC`.

  Each case asserts `assert_no_registration_writes`. Add one success case asserting that
  persisted Verifier, Trigger, and Operator refs carry the resolved UIDs and relationship
  rows.

### DS-2 — INCORRECT: `verified_by` on a nested inline Agent bypasses all binding validation but is still resolved, pinned, and persisted

- **Violated obligation:** REQ-092 (wrong-kind refs MUST fail closed), REQ-094/REQ-143
  (workflow `on_failure` MUST be rejected at registration), REQ-090 (bindings live only
  on a Service, a Service component, or a standalone Agent), REQ-056 (no Verifier
  DAG), and the task rule to use one canonical visitor so that resolution, pinning, and
  validation cannot drift.
- **Location:**
  - `crates/wyrd-spec/src/refs/mod.rs:534-543`: `visit_agent` visits `verified_by` and
    is also called for inline Workflow step Agents (`:291`) and for inline Eval
    LLM-judge Agents inside a Verifier (`:345`).
  - Validation looks only at top-level `Spec::Agent`/`Spec::Service` in all four
    places:
    - `crates/wyrd/wyrd-server/src/components/cards/service.rs:1188-1199`
      (`validate_request`);
    - `resolve.rs:121-140` (`owned_bindings`, which feeds `validate_effective_bindings`);
    - `crates/wyrd-spec/src/graph/composition.rs:236-246` (`verification_bindings`);
    - `crates/shared/wyrd-loader/src/validate.rs:64-78`.
- **Evidence:** `AgentSpec.verified_by` is `#[serde(default)]` (`card/agent.rs:47-49`),
  so it deserializes inside any `InlineableRef<AgentSpec>`. For those nested bindings,
  the canonical visitor passes them to:
  - `validate_and_collect_refs`, which resolves them;
  - `bind_card_references`, which UID-pins them;
  - `scope_child_card_refs`, whose output feeds `persist_outbound_relationships`.

  None of the kind, duplicate, workflow, or activation checks see them.
- **Consequence:** a caller with `card:write` can register one of the following, and
  registration returns success:
  - a Workflow Card whose step Agent is inline;
  - an Eval Verifier whose LLM-judge `judge_ref` is inline.

  The inline Agent can declare `verified_by` with any of these:
  - `verifier: {kind: Model, ...}`;
  - an inline or referenced `workflow` Operator in `on_failure`;
  - duplicate Verifiers;
  - a mismatched activation.

  The contract-violating binding persists with UID-pinned refs and relationship edges.
  Because the verifier ref's kind is never checked here, an observation-target kind
  such as `Model` or `Data` also enters that Card's outbound edges and the scope walk.
  This is the exact path the spec requires to fail closed. It also lets a Verifier bind
  Verifiers.
- **Testable correction:** at one shared owner, reject a non-empty `verified_by` on any
  Agent reached as an inline body, with a stable `WYRD_SPEC_400_*` error. One option is
  a visitor-driven check in `wyrd-spec` that both `validate_request` and the loader
  call. The other acceptable option is to route every visited binding list through
  `binding_validation_errors` and `validate_effective_bindings`, but that alone does not
  satisfy REQ-090. Add unit and server tests that register:
  - a Workflow with an inline step Agent carrying `verified_by`;
  - an Eval Verifier whose inline judge Agent carries `verified_by`.

  Both must be refused with no registration writes.

### DS-3 — INCORRECT: an `observations_ready` Trigger silently accepts unknown fields, including secret-bearing ones

- **Violated obligation:** AC-004 (unknown fields and secret-bearing payloads rejected
  before persistence) and REQ-093 (a Trigger contains no thresholds, pass-rate criteria,
  or other verdict matchers).
- **Location:** `crates/wyrd-spec/src/card/trigger.rs:12-43`. The diff removed
  `#[serde(deny_unknown_fields)]` from `TriggerSpec` to allow `#[serde(flatten)]`, and
  `TriggerActivation::ObservationsReady` is a unit variant.
- **Evidence:** serde deserializes an internally tagged unit variant with
  `InternallyTaggedUnitVisitor`, whose `visit_map` drains and ignores every remaining
  entry. The enum-level `deny_unknown_fields` does not apply to unit variants. With the
  outer `deny_unknown_fields` gone, the following deserializes successfully, both as a
  Trigger Card and as an inline `runs_on`:

  ```yaml
  runs_on: {kind: observations_ready, min_pass_rate: 0.9, api_token: sk-live-...}
  ```

  `write_registration` re-serializes the typed spec, so the extra keys are dropped rather
  than stored. `Schedule` and every `OperatorAction` variant are struct variants and
  still reject unknown keys. No test covers unknown fields on Trigger or Operator.
- **Consequence:** the request is not refused. An author's threshold or credential is
  discarded without a diagnostic. The binding then runs with semantics the author did
  not write, and a secret pasted into a Trigger gets a success response instead of a
  refusal. The secret is not persisted.
- **Testable correction:** make the variant strict, for example
  `ObservationsReady {}` under the existing enum `deny_unknown_fields`, which keeps the
  wire shape `{kind: observations_ready}`. Add `wyrd-spec` tests showing that
  `{kind: observations_ready, extra: 1}` and `{kind: schedule, cron: "...", extra: 1}`
  both fail. Add the matching `OperatorSpec` unknown-field case.

## Overall verdict

**FAIL**. DS-2 is a real fail-open path for REQ-092 and REQ-143 wrong-kind and workflow
refusal. DS-1 leaves the new server-side fail-closed enforcement unproven against
AC-018. DS-3 breaks AC-004's unknown-field and secret refusal for one Trigger variant.
The tenant-isolation mechanics (RLS resolution, UID overwrite, in-transaction recheck,
atomic rejection, scope exclusion) and the retirement of `/v1/eval/runs` and the
alert-router are sound.
