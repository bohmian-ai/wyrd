---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-agent-prompt
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
depends_on: []
---

# Close Agent Prompt registration and public type defects

## Subject and authority

Use `$wyrd-implement` directly. This remains one remediation task covering four required corrections,
subject to the owner-directed risk acceptance below; no new design phase.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt`.
- Approved [spec revision 2](../../spec.md).
- Original [TASK-001](../../tasks/TASK-001-agent-prompt.md).
- Original cumulative base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`.
- Reviewed candidate: `02c06e21bd5fd613341935a843fabe5350ea0a9d`.
- [Verdict](verdict.md), [validated findings](findings-validation.md), and
  [registration follow-up](followup-review.md).

Implement against the reviewed candidate plus this packet. Later review must
reassess the entire original-base-to-remediated-candidate range, the four
required findings and the accepted-risk disposition, not only the latest fix diff. Git scheduling/commits remain caller-owned.

## Owner-directed scope revision

The owner requested: "revise the remediation task. the likelihood of this
happening is infinitely low. we need simple solutions" after discussing
FIND-TASK-001-1 specifically. That finding is **accepted risk, not required
remediation**. This instruction supersedes the earlier lock recommendation and
closure requirement for that interleaving; it does not disprove the review's
source analysis or establish a measured probability.

Do not add a lock, Active-reference recheck, delete-policy change or coordinated
concurrency test for FIND-TASK-001-1. Leave existing transaction, authorization,
tenancy and dependency checks intact. Residual risk: concurrent deletion of a
matching reused Prompt can leave an Agent whose Prompt cannot be hydrated.
This narrow owner-approved exception applies to that registration/deletion
interleaving; it does not waive the other required behavior or findings.
Original discovery/validation reports remain unchanged as historical evidence.

The remaining corrections should extend existing owners directly, with no new
manager, cache, policy, option or test harness solely for this remediation.

## Diagnosis and decision-complete correction

### FIND-TASK-001-2 — Preserve an unchanged exact Agent's selected Prompt

REQ-003/005 require unchanged registration and immutable exact pins.
`service.rs:1130-1140` auto-selects the inline child before validating an existing
exact Agent. After A@Agent1 then B@Agent2 advance the generated Prompt line,
reapplying unchanged Agent1 under a fresh key selects another Prompt version
for A. That new reference changes Agent1's hash and falsely reports spec drift.
The SDK mints a fresh key per invocation, so ordinary apply reaches this path.
Rollback prevents an orphan but does not make this unchanged reapply correct.
Existing tests repeat A before B and only read A afterwards.

In the existing registration owner, use the existing exact Agent and its pinned
registered Prompt when validating an unchanged exact-pin inline submission.
Validate canonical Prompt content and derived kind/name/space using existing
hash/exact-lookup authority, then retain the original exact reference before
normal parent comparison. Do not make an unrelated authored reference
interchangeable with generated inline identity. Genuine Prompt or other parent
field changes under that pin still conflict atomically. Fresh Agent versions
keep normal independent Prompt selection. Do not add deletion-race protection
as part of this fix. Do not globally search historical Prompt content, weaken
spec-drift checking or change normal lock ordering/deduplication policy.

### FIND-TASK-001-3 — Account for this operation's already selected Prompts

REQ-002/003/005 and INV-003 require valid composite registration and reuse.
At `service.rs:1130-1133`, repeated materialization auto-selects through a query
that excludes Pending rows. Within one composite, two exact versions of the
same Agent with identical inline content each try to insert the same generated
Prompt seed version. The live unique index includes Pending, so the second
insert fails with registry unavailable and the complete valid graph rolls back.
A matching authored Prompt sibling at the selected identity causes the same
problem. The actual initial auto version is **0.1.0**: an authored 1.0.0 row on
an empty line is not the unique-collision reproduction and must not be treated
as a requirement to collapse different authored pins.

Extend the existing registration/version owner to include this operation's
already selected same-line Prompt outcomes in normal deduplication and version
allocation. Use current-operation identity and canonical content authority for
both generated and authored selections. Matching content reuses its selected
exact identity; different content selects the next normal version rather than
reusing the wrong body. Eligibility is restricted to this transaction's
selected stable-version Pending rows, alongside normally eligible committed
rows. Do not expose arbitrary Pending rows to other requests/public reads,
change global lifecycle/version semantics, collapse unrelated authored exact
pins, activate before commit or add a replacement graph/version store. Preserve
normal replay outcomes and post-commit activation.

### FIND-TASK-001-4 — Export the public Python result union at runtime

REQ-006/AC-005 and Python stub rules require public import parity.
`sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi:79` advertises HydratedCard,
but runtime `state/__init__.py:10-12` never defines/exports it. An ordinary
`from wyrd.state import HydratedCard` was independently reproduced to raise
ImportError. The static contract hides that import under TYPE_CHECKING.

Define this pure-Python union once in a typed stub-less Python owner using
AgentCard, PromptCard, ModelCard, DataCard and CardEnvelope. Re-export it from
runtime wyrd.state, its __all__, and the authored public stub, following the
existing one-definition rule. Preserve import-cycle safety, exact members,
get/index dispatch and isinstance narrowing. No native class, new dependency,
root-package export expansion or resolver is needed.

### FIND-TASK-001-5 — Complete required Rust documentation in the same fix

The AGENTS.md Rust documentation rule and agent rules remain explicit gates.
Document `crates/wyrd/wyrd-cards/src/agent.rs:33` for its changed retained
PromptCard attachment lifecycle, including unattached and GC-cleared state.
Add substantive `# Panics` sections to all six new server HTTP test items in
`pg_card_registration_route.rs`: `inline_agent_request` (:4029), `native_prompt`
(:4046), `register_inline_agent` (:4057), `outcome_for` (:4075),
`persisted_agent_prompt` (:4085), and `inline_agent_prompt_materializes` (:4108).
Read full bodies and document their actual direct/transitive fixture, IO,
response-shape and assertion assumptions. Keep behavior/assertions intact;
no unrelated rustdoc cleanup or separate style-only remediation round.

## Preserved behavior and scope

The two required registry corrections belong at the existing materialization/
version producer. Reuse the current TenantConn transaction, operation/outcome
authority, version locks, canonical hashes and exact lookups; avoid parallel
identity or version-selection machinery. Keep normal authored
ref/path intent, parent conflict semantics, tenant/RBAC/audit boundaries,
independent Card deletion, Agent/Service principal and verification-binding work,
post-commit activation/replay and immutable stored versions unchanged.

The agreed SDK API and sound Python/TypeScript lookup types remain unchanged.
No state-level agent_prompt method, hidden IO, invented UID, global historical
dedup policy, public Pending visibility option, migration, new feature/dependency,
parallel cache/graph/transport, standalone Workflow registration or external
repository change. Paths are ownership guidance, not a private helper allowlist.

## Acceptance and focused proof

Add the following planned tests to the already confirmed server `integration`
target and Python unit owner; fixture/internal structure stays implementation-owned.
For each changed executable scenario, run the exact focused proof RED, make the
minimal correction GREEN, then rerun earlier green scenarios during REFACTOR.
Use the current candidate for genuine RED; do not run historical code to assign
blame or fabricate the missing original RED history.

1. **FIND-2:** `pg_card_registration_route::inline_agent_prompt_reapply_preserves_pin`:
   exact Agent A@1 -> B@2 -> unchanged A@1 with a fresh key succeeds with original
   Agent/Prompt UIDs and no third Prompt version. Changed inline body or other
   parent fields under A@1 still conflict with no partial writes. Preserve
   sibling binding to the reapplied exact Agent where applicable.
2. **FIND-3:** `pg_card_registration_route::inline_agent_prompt_reuses_transaction_selection`:
   a Service referencing two exact versions of the same Agent with identical
   inline bodies succeeds with one generated Prompt UID and hydrates. A matching
   authored Prompt@0.1.0 plus inline Agent reuses its identity. Different inline
   bodies in repeated same-line selection get different normal versions.
   Conflicting exact authored pins still refuse and roll back.
3. **FIND-4:** `tests/unit/state/test_hydration.py::test_hydrated_card_is_public_runtime_type`:
   import HydratedCard outside TYPE_CHECKING and prove its five public member
   classes. Retain existing positive/negative compile contracts and runtime
   lookup behavior. No extra server journey is needed for this package import.
4. **FIND-5:** static inspection of the field and all six item docs against the
   complete bodies, plus bounded format/rustdoc/lint proof. No artificial
   runtime RED is required for documentation-only corrections.

Exact focused commands (new selectors above are planned additions to existing
confirmed targets; assert tests are selected):

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_card_registration_route::inline_agent_prompt_reapply_preserves_pin)"'
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_card_registration_route::inline_agent_prompt_reuses_transaction_selection)"'
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_hydration.py::test_hydrated_card_is_public_runtime_type
```

Retain original focused registration regressions A and B after the corrections:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_card_registration_route::inline_agent_prompt_materializes)"'
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=register_and_hydrate::agent_prompt_registration_identity)"'
```

Read traces before diagnosing failures. Run Cargo-backed checks sequentially.
Record exact outputs and evidence mapped to the four required FIND IDs. Use the existing
Python setup if a native environment rebuild is genuinely needed; pure-Python
alias/export correction alone does not justify rebuilding every language SDK.

For the expected Rust registration/docs and Python export/stub write set:
`mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run check:deps`, and `git diff --check`.
Run `mise run check:tenant-isolation` if SQL/query paths change. No wire-schema,
OpenAPI or TS declaration change is expected; if one proves necessary, justify
it against approved scope and apply its existing source-driven check. Do not
weaken gates or add a new permanent checker to enforce this remediation.

Full language journey suites and broad aggregates are excluded from this
bounded remediation; integrated change review owns them once. Record genuine
setup blockers with diagnosis, not historical blame. A need to change approved
identity/version/deletion/visibility/public API behavior routes to specification
authority instead of an unapproved workaround.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-2: unchanged exact Agent reapply after line advance keeps original Agent/Prompt; changed Prompt or Agent field under the pin conflicts with no writes; Service binds the reapplied Agent | `service.rs::pin_existing_inline_prompt` pins the generated child to the stored Agent's Prompt version (same kind/name/space only); `persist_node` exact-pin path reuses or raises spec drift | `inline_agent_prompt_reapply_preserves_pin` — RED: 409 `WYRD_REGISTRY_409_SPEC_DRIFT` on unchanged reapply; GREEN | PASS |
| FIND-3: same-line repeated selection in one composite reuses identical bodies (two Agent versions; authored Prompt@0.1.0), allocates successive versions for different bodies, conflicting authored pin refuses atomically | `version_resolve.rs::latest_stable_in_line` counts Pending rows of `SubmittedCardIdentity::operation_id` alongside Active; `resolve_existing` threads it; write loop skips a duplicate outcome for an already selected UID | `inline_agent_prompt_reuses_transaction_selection` — RED: 503 `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`; GREEN | PASS |
| FIND-4: `HydratedCard` importable at runtime with five members | `python/wyrd/state/_hydrated.py` single definition; re-exported by `state/__init__.py`, `__all__`, and stub; contract imports it outside `TYPE_CHECKING` | `test_hydrated_card_is_public_runtime_type` — RED against candidate `__init__.py`: ImportError; GREEN; `py:lints`, `py:typecheck`, `tests/unit/state` (85 passed) | PASS |
| FIND-5: field rustdoc and `# Panics` on six HTTP test items | `wyrd-cards/src/agent.rs` `prompt` field; `pg_card_registration_route.rs` six items plus new helpers/tests | static inspection against bodies; `mise run fmt`, `mise run lints`, `cargo doc` (no diagnostics in touched files) | PASS |
| Regressions A and B retained | — | `inline_agent_prompt_materializes` PASS; `register_and_hydrate::agent_prompt_registration_identity` PASS; all 30 `pg_card_registration_route::*` PASS | PASS |
| FIND-1 accepted risk excluded | no lock, Active recheck, delete-policy change, or concurrency test added | diff audit | PASS |

Checks: `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`, `mise run check:deps`, `mise run check:tenant-isolation`, `git diff --check` — all pass. No wire schema, OpenAPI, or TS declaration change.
Limits: `cargo doc` with `-D warnings` fails on pre-existing private intra-doc links in `wyrd-sql/src/pool.rs` and `postgres.rs`, outside this write set; not a repository lane. Spec-drift for a changed inline body under a pin reports the Prompt's hashes (the Prompt pin conflicts first).
