---
id: SPEC-verifier-slot-sources
revision: 1
status: draft
---

# Verifier slots independent of activation

## Objective

A nightly Verifier reads the day's parsed documents from Bifrost, prepares an
Agent judgment against a pinned policy PDF, and checks drift in the same
execution. The same exact Verifier can judge caller-supplied values through
verify or submit. Existing drift-only schedules and run-only judgments keep
working. Input shape must not determine how a Verifier runs.

## Rechecked facts and gaps

Checked against this checkout on 2026-10-18. These are current behavior, not
requirements to preserve where the owner explicitly replaces it.

| Evidence | Fact and gap |
|---|---|
| `crates/wyrd-spec/src/card/verifier/validate.rs:244`; `crates/wyrd-spec/src/card/verifier/mod.rs:359` | Registration refuses samples mixed with other slots, drift mixed with other checks (`mixed_evidence`), and steps with samples/drift (`window_steps`). `CheckEvidence` encodes Run/Window. These are the split to remove. Ordinary drift source/feature validation is separate (`validate.rs:415`) and remains useful. |
| `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:39`; `crates/wyrd-spec/src/card/verifier/validate.rs:878`; `crates/wyrd-spec/src/card/trigger.rs:33` | Schedule pairing currently requires a samples slot and an exact binding-owned selector map; only schedule activation exists in code. `group_end` is proposed by the long-horizon spec, not implemented here. |
| `crates/wyrd/wyrd-server/src/verification/drift.rs:690`, `:725`, `:151`, `:180` | `DriftEngine::checks` uses frozen slot keys only to decide which drift requirements to score. It queries `vala.drift.observations` by subject and `[start,end)`, selecting series from the check/profile, not materializing each declared slot. Custom folds a mean; PSI/SPC fold bins/moments and completeness counts. It returns pre-scored reports, not ordinary evidence. |
| `crates/vala/vala-verify/src/blocks/drift.rs:42`, `:76`; `crates/wyrd/wyrd-server/src/verification/runner.rs:396` | Vala can score slot-keyed Arrow samples with requirement-keyed baselines; pre-scored reports take precedence. The scheduled server branch constructs reports and passes an empty evidence map to the ordinary engine; submitted work gets evidence but empty drift state. Removing registration restrictions alone cannot deliver the journey. |
| `crates/wyrd-spec/src/verification/execution.rs:60`, `:197`; `crates/vala/vala-verify/src/blocks/engine.rs:921` | `InputDeclaration` already carries arbitrary inline JSON and outcomes echo declarations. There is no Samples declaration; conformance currently accepts JSON only for JSON slots, so samples cannot yet enter this path. Reuse JSON rather than add a transport or declaration kind. |
| `crates/wyrd-spec/src/card/verifier/reference.rs:118`; `crates/vala/vala-verify/src/blocks/scope.rs:198`; `crates/vala/vala-verify/src/blocks/agent.rs:228`; `crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs:79` | Existing `inputs.<slot>`, indexes, fields and assertion-only wildcards resolve JSON values. Agent placeholders render JSON as text; JSON evidence has located leaves for citations. Samples exposed as JSON can use these paths; no samples-specific reference root is needed. |
| `crates/shared/wyrd-client/src/verify/mod.rs:314`, `:552`; `crates/shared/wyrd-client/src/observe/mod.rs:315`, `:395` | The SDK classifies any samples-bearing Verifier as Window, refuses verify, and flattens submit values into one recorded feature event returning `Recorded`, without an execution. This public recording behavior must survive without classifying Verifiers into modes. |
| `crates/wyrd-spec/src/vala/drift/record.rs:18`; `crates/shared/wyrd-client/src/observe/drift.rs:75`, `:119`, `:151` | Measurements are scalar features, not arbitrary parsed JSON. Existing tall rows carry `record_id`, `created_at`, `series`, `num_value`, `str_value`, `session_id`. Numbers retain a numeric and canonical string projection; booleans and categories retain only strings. Reconstructing arbitrary document objects or original boolean types would invent information. |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs:758`, `:580`; `crates/vala/vala-verify/src/blocks/scope.rs:217` | Direct execution explicitly refuses drift. Enqueue authorizes and stages without loading the Verifier or checking its slots. Missing required evidence currently becomes inconclusive at use, not an admission refusal. Activation-specific admission is a real change; authoritative enqueue validation must also respect long-horizon's no-additional-producer-Postgres-IO constraint. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs:1234`; `crates/wyrd/wyrd-sql/src/queries/drift_baselines.rs:384`; `crates/wyrd/wyrd-server/src/verification/drift.rs:802`; `crates/vala/vala-verify/src/blocks/drift.rs:90` | Registration already enqueues per-requirement distribution baseline fitting without requiring a schedule; exact fitted profiles are readable by Verifier/requirement. Scheduled missing/legacy fitted state can terminate execution; the in-memory adapter instead makes absent fitted state inconclusive. Caller scoring needs that existing fitted state, not a new fitting policy or schedule requirement. |

## Scope and invariants

- **INV-001 — Owner invariant, verbatim:** "Verifiers should be independent of how
  they are run." A Verifier is a pure versioned function over declared inputs.
  Slots describe value shapes and may declare data from other Verifiers, but
  never which activation runs them. Steps and checks do not branch
  on verify, submit, schedule, or group_end. Purity does not promise identical
  stochastic Agent responses; provider and execution provenance still matter.
- **INV-002 — Suppliers compose outside judgment.** Activations supply values;
  a pin is a constant of the exact Verifier version. No activation changes a
  slot's value meaning, rewrites the Verifier, or silently drops requirements.
- **INV-003 — Existing authority and bounds.** Preserve exact versions,
  tenant/subject isolation, accepted caller/delegation or binding-owner
  authority, blocking permissions, non-blocking audit, existing deadlines,
  process/input/query limits, queue replay, outbox loss and retention rules.
  A slot declaration or pin grants no read permission. Bifrost retains its
  ordinary published-plus-live visibility contract; no complete-history claim.

In scope: removing mode restrictions, admitting and supplying ordinary slot
values, shared judgment, parity and regressions. No new Card kind, activation
kind, query language, arbitrary-table selector, execution API, storage pipeline,
or baseline-fitting policy. `group_end` lifecycle remains in the long-horizon
spec; its source ownership is unresolved below. Artifact publication and pins
remain in the [pinned-artifact draft](../verifier-pinned-artifact/spec.md).

## Requirements

**REQ-001 — One Verifier contract.** Allow samples, JSON/media/trace slots,
pinned media, preparation steps, and assert/trace/agent/code/drift checks in
one Verifier. Remove Run/Window admissibility and schedule-required rules from
the Verifier itself. Retain shape-specific rules: drift reads a whole samples
slot with declared measurements; extract reads media; existing dependency,
schema, Agent tool/workspace, citation and reference-position rules still apply.

**REQ-002 — Samples are values.** A complete samples value is a finite ordered
JSON array of the existing tall observation-row objects named in the facts
above, carried by `InputDeclaration::Json { value }`. Preserve their numeric
and categorical projections and record/time identity; select only declared
`features`. This reuses stored row fields and ordinary JSON, without a new
reference grammar or Samples wire kind. For example, one selected row is:

```json
{"record_id":"<record UUID>","created_at":"2026-10-18T12:00:00Z",
 "series":"parsed_document","num_value":null,
 "str_value":"{\"invoice_total\":120}","session_id":null}
```

`inputs.day` exposes the array to Agent/code steps and checks;
`inputs.day[0].str_value` and assertion `each: inputs.day[*].str_value` use
existing selectors. JSON-pointer citations name original rows under that
slot. An Agent may read parsed-document JSON serialized as a string feature;
code preparation may parse it into JSON using existing step outputs. Native
nested documents are not a new measurement type. The motivating fixture
records that string plus a numeric measurement for drift; the drift check
names only its relevant measurements.

For schedules, read every declared samples slot from the bound subject's
observations in the schedule's `[start,end)` window, including features not
used by drift. Present rows in `created_at`, `record_id`, `series` order.
Retain the same selected values for all consumers in that execution; repeated
queries must not give the Agent and drift check different populations. Group
rows by record identity when scoring features, preserving existing incomplete,
duplicate, null, baseline and insufficient-data semantics. An empty window is
an empty value, not an absent supplier or permission to omit a check. Bounds
or a failed read must not silently truncate into a complete/passing value.

**REQ-003 — Caller-supplied samples are allowed.** verify executes a complete
samples array immediately; submit of complete arrays queues one ordinary
execution, including for a drift-only Verifier. Neither needs a schedule or
implicitly records these execution inputs as new drift observations. Other
required slots must also be supplied, with pins added by the server. Preserve
slot names instead of flattening measurements across slots. Validate row
shape, measurement names, types and existing size/numeric limits with errors
at the offending input/row/field. Incomplete measured data remains evidence
to judge under existing drift rules, distinct from a malformed declaration.

This is the minimum extension: arbitrary JSON declarations, local references,
slot-keyed Arrow scoring and baseline ownership already exist; missing
conformance, materialization and server wiring are implementation gaps, not a
reason to forbid an activation. A caller-selected batch represents that
caller's chosen measurement population. It acquires no server-sampled
provenance: retain row times; record a scheduled window only when supplied by
that activation, using existing optional drift-window provenance.

Keep existing SDK submit of a single scalar feature event recording through
the ordinary drift observation path and returning `Recorded`, with no queued
judgment. Complete arrays distinguish execution evidence from that existing
event form without adding a flag or inferring a Verifier mode. This recording
shortcut supplies future observations, not an execution with partially filled
slots; it must not silently discard accompanying invocation evidence. A call
mixing event-form samples with execution inputs is refused at that samples
slot, with remediation to supply a complete array. Ordinary run-only submit
continues to queue. Existing scalar-event verify is not a complete batch and
does not become an implicit one-row judgment.

**REQ-004 — Validate the activation's supply contract.** A Verifier with no
compatible binding may still register and be called with appropriate values.
At binding/Trigger pairing registration, check that the chosen supplier can
fill every required slot after constants are included; refuse the owning
binding with a structured, field-specific error naming its unsupplied slot.
A reusable Trigger alone cannot validate an unknown Verifier; validate their
effective pairing. At verify/submit execution admission, check required slots
and declaration shapes before judgment or enqueue staging, naming
`inputs.<slot>` and any offending nested field. Use existing derive-backed
validation/input errors and remediation, consistently across surfaces.

The minimum schedule supplier supplies samples plus all pinned constants;
it cannot supply an unpinned required JSON/media/trace value. Such a binding
is refused; the Verifier remains valid for another activation. Optional
non-samples slots may remain absent under existing null semantics. There is
no samples-presence prerequisite for scheduling a Verifier whose required
slots are all otherwise supplied. Missing required request values are an
intentional change from today's inconclusive-at-use behavior; empty or
insufficient supplied windows still produce ordinary inconclusive judgments.
Enqueue's authoritative exact-contract admission must preserve the
long-horizon constraint against additional producer Postgres IO; existing
enqueue currently proves neither contract validation nor this combination.

**REQ-005 — Pins and one engine walk.** Apply the sibling pin contract in every
activation: resolve/read/authorize the exact Artifact/path, refuse overrides,
and include identical declaration identity/digest and evidence echo. A pin is
supplied even when marked optional. Scheduled dispatch supplies samples and
pins through ordinary admission, references, steps, checks and decide, with
the same server-owned fitted-baseline context as other activations. Resolve
fitted state by exact Verifier and requirement for every activation; retain
the server's `baseline_not_ready`, `baseline_legacy` and invalid-profile
execution failures consistently rather than silently using the adapter's
absent-state fallback. Custom still needs no fitted baseline. The engine must
not judge drift in isolation and give other consumers an empty evidence map.
An aggregate optimization is permissible only if it judges the same selected
population and preserves complete ordinary evidence for other consumers.
Keep drift statistics/provenance, step/requirement outcomes, citations and
retained input/result identity through existing result recording. Never turn
missing permissions, unavailable evidence or incomplete data into a pass.

**REQ-006 — Cross-spec alignment and surfaces.** Preserve long-horizon
REQ-012's single Verifier-owned feature declaration, mandatory samples slots
(`required: false` refused), no `inputs` field on schedule bindings (including
`{}`), and accounting for every drift requirement. Those are shape and
supplier rules, not a Window Verifier mode. This draft replaces its
Window-only/separate-path premise, not those simplifications. It does not
reinstate redundant schedule selectors or treat a group list as samples.
Group source placement awaits OPEN-001; no implementation may assume both
conflicting contracts are approved. Keep SDKs, HTTP/MCP, hydrated contracts,
served OpenAPI, generated types/schema and authored Python stubs aligned.

## Acceptance criteria and journeys

Primary proof is real SDK → `WyrdTestServer` + repository-managed Postgres →
SDK, across Rust, Python and TypeScript, plus HTTP and MCP discover → act →
observe. Use controlled local provider responses with real production
registration, evidence, authorization, scheduling and result paths. These are
future evidence obligations; no runtime tests were run for this draft.

| ID | Journey and required observable evidence |
|---|---|
| AC-001 | Nightly document review (REQ-001–002, REQ-004–006): upload/register a policy PDF, register the mixed Verifier and schedule binding with no selector map, record document-string and numeric features into Bifrost, flush/shutdown the producer, activate the due schedule, and read the result. Preparation and an Agent judge consume the day's rows and pinned policy; assert/code consumers can resolve the same samples slot. Drift judges its numeric feature in that same execution. Results include all checks/steps, validated PDF/JSON citations and exact pin identity. Exercise both policy conformity and violation; ordinary failure reactions retain their rules. |
| AC-002 | Same exact Verifier, caller batch (REQ-002–005): supply the captured array through verify, then submit → poll/read, omitting the pin. Both execute all declared checks without a schedule or extra observation writes. Controlled judgments and drift statistics agree for identical values and fitted state; activation provenance remains truthful. Queue retries preserve values, pin identity and existing execution/dispatch idempotency. Prove rows are not flattened when slots share feature names. |
| AC-003 | Supply negatives (REQ-001, REQ-003–006): the mixed Verifier registers; a schedule unable to supply an unpinned required slot refuses at the binding/slot, while direct complete input succeeds. Missing required caller slots, malformed rows, undeclared series, oversized batches, event/batch confusion and pin overrides refuse at the offending field before judgment/staging. Schedule `inputs`, even `{}`, and optional samples declarations refuse per long-horizon REQ-012. A pin-only complete binding is not refused for lacking samples. |
| AC-004 | Evidence negatives (INV-003, REQ-002, REQ-005): an empty or insufficient window cannot pass by dropping drift or vacuous assertions; malformed document text cannot pass its dependent judgment. Missing/legacy fitted state has the same server error semantics across activations. Unauthorized Bifrost/Artifact reads, foreign-tenant references, corrupted pin bytes and failed/degraded input reads follow existing refusal/error/inconclusive rules and expose no unauthorized evidence. Read/size/capacity failure cannot return a passing truncated review. Verify and enqueue under-privileged tokens through direct HTTP/MCP as well as SDKs. |
| AC-005 | Regressions (REQ-001, REQ-003, REQ-005–006): existing drift-only event submit → Recorded → producer drain → schedule → retained result/Operator still works, including empty/insufficient windows; existing valid run-only verify and queued submit retain results, citations, errors and replay. The old blanket samples-verify refusal is replaced only for complete batches; omitted required execution inputs now get the explicit admission error. Field-path and conformance tests support, not replace, these journeys. |
| AC-006 | Cross-spec closure (REQ-006, OPEN-001): before approval, resolve group source ownership and reconcile the long-horizon declarations and examples. Before implementation completion, prove the chosen group_end supplier feeds ordinary slots without activation-dependent checks; update owning design/doctrine/references and verify served OpenAPI, runtime MCP catalog/behavior and SDK/schema parity. |

Existing regression homes include
`sdks/wyrd-sdk-rust/tests/integration/scheduled_drift_alerts_operator.rs:139`,
`sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:820`, and
`crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs:1083`.
The latter two encode superseded refusal/missing-input expectations, not
immutable regressions. `mise.toml:466` owns the Rust verification/drift
journey lane; card registration, Python/TypeScript integration, codegen,
served OpenAPI and MCP checks supply complementary evidence. Select final
checks by actual implementation scope under AGENTS.md §11.

## Boundaries and open material decisions

Extend existing owners: `wyrd-spec` for pure slot/wire/validation contracts;
`wyrd-client` for shared projection; server registration/control/runner for
activation admission and authorized supply; Vala for judgment and drift;
existing Bifrost/Scribe and Artifact storage for evidence. No second evaluator,
queue or storage authority. Implementation must explicitly update the
Run/Window passages in Wyrd design §§Verifier, Trigger, Observation identity
and Decisions, doctrine and evaluation/drift references. This two-file draft
does not change those authorities or the long-horizon specification.

**Resolved — group source references stay in the Verifier.** Owner decision: a
group-level Verifier receives inputs from other Verifiers. Declaring
`verifiers.<name>.inputs.<slot>` / `.requirements.<id>.outcome` in its `inputs`
is a declaration of what data the Verifier judges, not of how it is run, so it
is consistent with INV-001. Long-horizon revision 14 REQ-002/011/013 stand
unchanged. INV-001 concerns activation only: verify, submit, schedule and
group_end supply values; none of them restricts slots, steps or checks.

No open material decisions remain.

## Authority and history

Authorities: [AGENTS.md](../../../AGENTS.md) §§2–3, 9, 11, 14;
[agent rules](../../../architecture/agent-rules.md);
[Wyrd design](../../../architecture/wyrd-design.md);
[doctrine](../../../architecture/wyrd-doctrine.mdx);
[Bifrost design](../../../architecture/bifrost-design.md);
[reference router](../../../architecture/references/README.md), including
evaluation, drift, OLAP serving, agent harness, testing and spec workflow.
Related drafts: [pinned Artifact](../verifier-pinned-artifact/spec.md) and
[long-horizon verification](../long-horizon-verification/spec.md), especially
REQ-002/011–014. Owner direction supersedes the mode split for this draft;
approval and authority reconciliation remain outstanding.

| Revision | Date | Status | Change |
|---|---|---|---|
| 1 | 2026-10-18 | draft | Owner-directed activation-independent slots; existing JSON/observation-row batches for caller and scheduled samples; pins in every activation; shared engine walk and supplier admission; preserve event recording and REQ-012; group source ownership open. |
