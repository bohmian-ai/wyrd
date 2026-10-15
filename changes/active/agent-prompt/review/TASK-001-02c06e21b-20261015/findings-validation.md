# Independent structured Ponytail validation

Result: **five retained findings; bounded implementation correction required**. No material specification decision is needed. This is independent validation, not a vote on discovery verdicts.

Subject: base `c1aac2d132b19848ca1940c3907e1fa68a8d08a5` → candidate `02c06e21bd5fd613341935a843fabe5350ea0a9d`; HEAD checked at that candidate. Approved authority is `changes/active/agent-prompt/spec.md` revision 2 and original `tasks/TASK-001-agent-prompt.md`, subordinate to AGENTS.md and agent rules. Read the task-review and Ponytail skills, applicable protocol/doctrine, spec-driven development, maintainer, architecture-pattern and Python-stub authority. CodeGraph preceded code searches. Reviewed cumulative changed source, tests, fixtures, declarations and documentation and all nine supplied discovery/comparison reports, including follow-up. Reviewed source remains unchanged; this report is the only owned file.

## Full proposal dispositions

| Source ID / proposal | Disposition | Source-validated conclusion |
|---|---|---|
| BEH-001 | CONFIRMED | Public stub declares HydratedCard; runtime public package has no such name. Reproduced using the existing environment, without rebuilding or syncing dependencies. Consolidated into FIND-TASK-001-4. |
| INV-REV-002 | CONFIRMED | Same runtime-export cause and correction as BEH-001. No second finding. |
| MNT-001 | CONFIRMED | Same cause; the one-definition/re-export recommendation follows the explicit Python stub rule. No native wrapper is needed. |
| INV-REV-001 | CONFIRMED | Child auto-resolution precedes exact parent comparison; a fresh-key reapply after line advance falsely conflicts. FIND-TASK-001-2. |
| DATA-001 | CONFIRMED | Generated reused dependency bypasses the exact Active/FOR SHARE authority. Version advisory locks and relationship foreign keys do not supply lifecycle protection. FIND-TASK-001-1. |
| DATA-002 | REVISED | Deterministic caller-local Pending collision is real, but its empty-line authored Prompt@1.0.0 reproduction and Fresh 1.0.0 claim are wrong. Seed is 0.1.0. Retain two Agent versions or authored Prompt@0.1.0 reproduction only. FIND-TASK-001-3. |
| STD-001 | CONFIRMED | Changed holder field lacks docs; all six new HTTP proof items omit panic contracts, including the transitively panicking request builder. FIND-TASK-001-5. |
| MNT-002 | CONFIRMED | Its five HTTP items are a subset of STD-001; consolidate, retain the sixth item and holder field from standards review. |
| Follow-up INV-REV-001 / DATA-001 | CONFIRMED | Independently traced below; follow-up adds evidence rather than new independent causes. |
| Follow-up corrected DATA-002 | CONFIRMED | 0.1.0 seed, multiple exact-version siblings and Pending filtering verified from source. It revises DATA-002; no new ID. |
| DATA-002 authored 1.0.0 empty-line unique-collision example | REJECTED | The authored Pending 1.0.0 row does not collide with generated seed 0.1.0. Do not require arbitrary differently pinned authored submissions to collapse to one identity. |

System/security reports proposed no source IDs. Their positive transaction, tenancy, recovery and local-read assessments remain useful, but their broad statements about ordinary registry integrity do not prove the three path-specific conditions below. Standards' typing PASS does not establish runtime import parity. Neither majority PASS nor majority FAIL determines this ledger.

## Final deduplicated finding ledger

### FIND-TASK-001-1 — CONFIRMED — Important — INCORRECT: reused generated dependency lacks lifecycle protection

Sources: DATA-001 and its follow-up. Obligations: REQ-002/003/005, INV-003; preserve existing registered-dependency integrity and independent deletion behavior.

Location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130-1134`.

**Proof.** `RegistrationWriter::write:1100` checks only planned external refs, before inline materialization. A matching independently authored Active `<agent>-prompt` is a permitted REQ-003 name collision and need have no incoming references. `persist_node:1197-1207` takes a version-line advisory lock and uses `resolve_existing:1433-1466`; `version_resolve.rs:48-67` and `get.rs:147-165` perform nonlocking reads. Delete instead takes the target row FOR UPDATE and checks inbound edges (`queries/cards/delete.rs:19-26,77-145`), without the version advisory lock. Delete can hold that row after its zero-inbound decision while materialization reads the previously committed Active value. The writer then assigns its UID as the Agent's Prompt. Relationship insertion checks existence through tenant/UID foreign keys (`relationships.rs:72-103`; migration `20260601000018_card_relationships.sql:19-25`), not Active status. A waiting FK check can resume after the soft-delete commits and still succeed because the tombstone remains. Registration can commit and activate the Agent; `complete_card_inner:1875-1949` validates its own manifests/blob, not dependencies. Public target lookup excludes deleted rows (`get.rs:12-27`), so hydration fails.

**Minimal correction.** At generated dependency selection in the existing registration owner, protect a reused previously committed dependency through existing `recheck_active_card_refs` on the same TenantConn before persisting the Agent. It already checks exact identity/UID/Active and holds FOR SHARE until commit. The consumer guard belongs at this write-time trust/lifecycle boundary, not in hydration. Distinguish a child created/selected by this transaction from a previously committed dependency: transaction-owned Pending children stay eligible under existing composite semantics and must not be forced through an Active-only check. Preserve ordinary rollback, error catalog, row-lock semantics, version selection and independent deletion. Do not change deletion policy, activate before commit, broaden Pending visibility or add a lifecycle owner.

**Closure proof.** A focused real-Postgres registration/delete seam starts with the matching independently authored Prompt, locks it for deletion and establishes zero inbound edges before allowing materialization to proceed. If deletion wins, registration must refuse and roll back; if registration obtains protection first, deletion must refuse after seeing the edge. No live Agent may target the deleted UID. Reuse the existing dependency-race test pattern (`pg_cards_register.rs:821-871`), without sleeps/new harnesses, and drive the generated registration path under a distinct operation key.

### FIND-TASK-001-2 — CONFIRMED — Important — INCORRECT: unchanged exact Agent reapply falsely reports drift

Sources: INV-REV-001 and follow-up. Obligations: REQ-003/005; task scenario 2 exact-pin and unchanged registration behavior.

Location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130-1140`.

**Proof.** Inline child always has `version: None` (`inline_prompt_submission:1574`). For Agent desk@1 with body A, then desk@2 with B, both normally completed, the child line has A@0.1.0 and B@0.1.1. Reapplying unchanged desk@1 under a fresh key auto-selects A@0.1.2: the ordinary version resolver compares only the latest stable row (`version_resolve.rs:83-119`). That new UID-bearing ref changes the normalized parent hash (`persist_node:1185-1190`), and exact parent comparison rejects it (`resolve_existing:1410-1426`). The SDK makes this reachable without special input: `Cards::register_from_path:256-282` calls the registration saga, which mints a new key each time (`cards/saga/mod.rs:58-64`). Same-key replay does not solve fresh apply. Transaction rollback prevents a committed orphan; the consequence is false spec drift. Existing tests repeat A before B and only read A afterwards.

**Minimal correction.** In the existing server registration owner, identify an existing exact Agent before replacing its inline body with a newly selected identity. For an existing exact Agent's generated Prompt relationship, validate the submitted native Prompt against that pinned Prompt using existing canonical identity/hash and exact lookup authority; preserve the original exact ref when content is unchanged. Retain derived kind/name/space rules: this must not make inline submission interchangeable with an unrelated authored ref. Compare the rest of the normalized parent through ordinary exact-pin validation. Genuine content or parent drift still conflicts atomically. New parent versions retain normal child-line selection. Reused committed targets also receive FIND-TASK-001-1 protection. Do not globally search historical content hashes, weaken conflict checking or change lock ordering/ordinary Prompt dedup policy.

**Closure proof.** Extend existing registration proof: pinned A@1 → B@2 → unchanged A@1 with a fresh key succeeds with original Agent/Prompt UIDs and no third Prompt version; changed inline content and changed non-Prompt fields under A@1 still conflict with no partial writes. Include a sibling graph reference to the reapplied Agent where practical so exact sibling binding remains intact.

### FIND-TASK-001-3 — REVISED — Important — INCORRECT: repeated inline materialization ignores this transaction's selected child

Sources: DATA-002 and corrected follow-up. Obligations: REQ-002/003/005, INV-003; valid composite composition and normal identity/conflict rules.

Location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1130-1133`.

**Proof.** A Service with distinct aliases for desk@1.0.0 and desk@2.0.0, both with identical inline content, is valid. Graph identity includes version (`wyrd-spec/src/graph/mod.rs:136-154`, `resolve.rs:994-1017`); composition rejects repeated aliases and peer-only kinds, not these Agents (`graph/composition.rs:54-87,426-435`). Leaves precede Service in `graph/topo.rs:74-139`. The first Agent's generated Prompt is inserted Pending (`service.rs:1209-1221`). Before the sole graph commit at :1163, the second selection cannot see that Pending child through Active-only `latest_stable_in_line`. It chooses seed **0.1.0** again (`wyrd-semver/src/lib.rs:30-31`). The live identity unique index includes Pending (`20260601000014_card_registration_operations.sql:60-62`); insert fails and maps to registry unavailable (`register.rs:290-350`), rolling back the valid graph. An authored same-request Prompt@0.1.0 with matching content also demonstrates collision: Prompt precedes Agent in canonical kind order (`envelope.rs:482-503`). The originally proposed authored@1.0.0 seed example does not.

**Minimal correction.** Extend existing registration/version selection to include the caller's already selected same-line child in this operation, using the existing exact identity/canonical hash authority. Apply normal latest-in-line, dedup and allocation rules to this transaction's selected stable-version Pending rows as well as committed eligible rows; reuse the matching exact child instead of allocating it twice. Scope this eligibility to the current registration operation/transaction. Do not expose arbitrary Pending rows to public lookup or other operations, globally change lifecycle/version policy, dedup unrelated authored exact pins, or replace the graph/version owner. Both generated and explicitly authored same-request selections must participate; a generated-only cache leaves the authored sibling path unresolved. Preserve ordinary conflict behavior and subsequent normal activation/replay outcomes.

**Closure proof.** Focused composite registration tests on an empty line: two pinned same-name Agent versions with identical inline content produce one exact Prompt UID and both pin/hydrate it; authored Prompt@0.1.0 plus matching inline Agent reuses that identity. Also verify changed inline content in a repeated same-line selection allocates distinct normal versions rather than silently reusing different content, while conflicting exact authored pins still refuse/roll back. No requirement to collapse authored@1.0.0 to seed@0.1.0 is retained.

### FIND-TASK-001-4 — CONFIRMED — Important — MISSING: public Python result union cannot be imported

Sources: BEH-001, INV-REV-002, MNT-001. Obligations: REQ-006, AC-005, AGENTS §8, Python API/stub rules.

Location: `sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi:79`; missing runtime export in corresponding `__init__.py:10-12`.

**Proof.** Stub declares the five-holder HydratedCard union used by get/index. Native get dispatch (`src/state/mod.rs:347-360`) returns those correct holders; runtime state package exports only CardEnvelope, HydratedArtifact and WyrdState. Its static contract imports HydratedCard only under TYPE_CHECKING. A consumer's ordinary public import fails before annotations can be used. Independently executed:

```sh
PYTHONDONTWRITEBYTECODE=1 mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python -c 'import wyrd.state as state; print(state.__file__); print("HydratedCard exported:", hasattr(state, "HydratedCard")); from wyrd.state import HydratedCard'
```

Result: existing source package path printed, `HydratedCard exported: False`, then ImportError; exit 1. No dependency synchronization, build, new harness or source mutation.

**Minimal correction.** Define this pure-Python union once in a typed stub-less Python module using the five existing public classes, then re-export through runtime wyrd.state, its __all__, and authored public stub. This follows the explicit one-definition rule, avoids circular imports/restating a union in a stub, and needs no native type, class, resolver or root-package export expansion. Preserve precise union members, existing holders, get/index dispatch and narrowing.

**Closure proof.** Existing Python state/export unit home imports HydratedCard outside TYPE_CHECKING and checks its five public class members; retain positive/negative type contracts. Run that exact test and py:lints/py:typecheck. This is a package import defect, so no new server journey is needed.

### FIND-TASK-001-5 — CONFIRMED — Required documentation gate — VIOLATION: changed Rust field and test panic contracts are undocumented

Sources: STD-001, MNT-002. Obligation: AGENTS Rustdoc rule (headed §17 in current file; discovery calls it §16), agent rules and task-review hard rustdoc gate.

Locations: `crates/wyrd/wyrd-cards/src/agent.rs:33`; `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:4029,4046,4057,4075,4085,4108`.

**Proof/correction.** Changed `PyAgentCard.prompt` stores a PromptCard instead of runtime Prompt but lacks field docs for unattached/hydrated/GC-cleared state (`from_native`, `attach_prompt_card`, `prompt`, `__clear__` inspected). Six new HTTP items have no # Panics: inline_agent_request transitively calls native_prompt, and the remaining five have direct expect/assert/panic paths. Document the field lifecycle and substantive panic assumptions of all six. Keep behavior/assertions and all unrelated fields unchanged. This is an explicit documentation blocker, not a production-panic assertion or a separate style-only remediation round.

**Closure proof.** Inspect these docs against full bodies and run formatting/documentation checks appropriate to the touched files. No runtime test for prose, new harness or unrelated wrapper cleanup.

## Correction scope and sibling preservation

The three registry failures arise in the same materialization producer, but represent independent invariants: pinned-parent normalization, current-operation child selection, and committed-dependency lifecycle protection. Implement them together in the existing registration/version owners, with three distinct closure scenarios. One downstream hydration guard cannot solve any of them. Reuse version locks, canonical hashes, exact lookup, current transaction/operation identity, outcomes and recheck_active_card_refs. Do not add a second graph/store, broad public Pending eligibility, historical dedup policy, reserved generated namespace or cascading ownership.

Traced sibling paths: all production persist_node and resolve_existing callers are this writer; ordinary authored submissions and ref/path siblings use the same persistence/version owners. External references use preflight resolution plus Active row recheck. Both public delete entry points use the same SQL lifecycle row lock; reconciler/completion reuse existing activation/cleanup transitions. Other SQL inserts are test/verification fixtures, not another public registration path requiring redesign. Generated outcomes feed replay_seed, upload initialization and completion; transaction-owned Pending children must continue through that lifecycle. Existing Agent/Service account and verifier binding work in persist_node remains unchanged. Shared hydration validates exact UID-bearing graph closure, Python attaches existing Prompt holders, TS projects the shared selection, and invocation reads that same relationship. Fixing the registration producer preserves those consumers rather than adding per-language repairs.

## Reconciled obligation matrix

| Obligation | Source/proof assessment | Result |
|---|---|---|
| REQ-001 Agent-owned synchronous PromptCard/ref; invocation relationship | Shared HydratedAgent, Python holder attachment, TS thin projection; R/P/T/I source and recorded green proof | PASS |
| REQ-002 all authoring forms, server materialization, same transaction/lifecycle/outcomes | Ordinary paths implemented, generated outcomes included in replay/publication; valid multi-selection composite fails (FIND-3) | FAIL |
| REQ-003 derived name/space/tenant, independent line, immutable old relationships, no cross-Agent sharing | Naming/tenant/version isolation and old-row reads pass; fresh exact-pin reapply and caller-local unchanged reuse fail (FIND-2/3) | FAIL |
| REQ-004 strict typed/generic alias coherence, explicit Prompt alias, no generated Service component | Existing alias index reused; native and language dispatch/narrowing covered | PASS |
| REQ-005 local reads, stable errors and malformed-state refusals | Accessors have no IO; invalid-bundle/unknown/kind cases covered; existing authorization/audit/tenancy retained | PASS for these sub-obligations |
| REQ-005 preserve integrity/conflict/idempotent registration | Delete/reference race and false reapply conflict (FIND-1/2) | FAIL |
| REQ-006 public declarations/exports and holder capability closure | Other projections/callers match; public Python union runtime missing (FIND-4) | FAIL |
| INV-001/002 server-owned durable logic, pure spec boundary | No client durable behavior, hidden writes or PyO3 in spec | PASS |
| INV-003 reuse existing owners | Existing owners reused structurally; incomplete state/lock integration requires extension, not replacement | PASS for architectural boundary; behavior defects above remain |
| INV-004 excluded inline kinds/Workflow/migrations/features/cascade/UI/Bifrost/external repos | Complete changed-path diff remains scoped | PASS |
| AC-001/002 HTTP/SDK registration/identity/edge proof | Ordinary tests and reported green results credible; missing focused regression cases expose FIND-1/2/3 | FAIL for uncovered required behavior |
| AC-003/004 navigation, offline and negative proof | Rust actual shutdown; Python/TS retained-state accessors plus unreachable URL tests; malformed local fixtures and invocation regression | PASS at bounded task scope, with execution limits |
| AC-005 checks/typing/public import closure | Recorded relevant green checks do not prove runtime alias; independent import fails; hard Rustdoc gate fails | FAIL (FIND-4/5) |

## Proof coverage, decisions and limits

All proposed source IDs and the rejected seed example have dispositions. Full cumulative changed surfaces and connected owners supporting these correction boundaries were inspected; the supplied reports cover the remaining source/proof breadth. No unresolved report conflict or missing authority prevents a bounded correction. There is no need for SPEC_REVISION_REQUIRED: approved ordinary versioning, exact pins, independent deletion and composite transaction rules already choose the required outcomes. Remediation may extend narrowly scoped private version selection to account for the caller's transaction, but may not turn it into a general visibility or concurrency policy change.

Recorded A/B/R/P/T/I, owner units, CLI, typing, formatting, codegen and boundary results were inspected as evidence, not rerun or inferred from PASS labels. Those tests do not prove the retained reapply/delete/composite paths. Pre-implementation RED history is explicitly absent and cannot be reconstructed; do not invent it or run historical code. Full affected journey suites remain integrated change-review work, not a remediation requirement. Focused new regression scenarios should be appended to the existing HTTP/Rust registration proof homes; record exact selectors via mise and existing Postgres setup, then run only write-set owner checks. Python import regression uses the existing unit environment; documentation closure is static.

Skipped Cargo builds, live Postgres interleaving reproduction, broad suites and unrelated debt. Residual risk: the three registry defects are source-proven; their fixes still need the focused transaction/race regression proof above.
