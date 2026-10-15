# Immutable cumulative task re-review

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/agent-prompt
Original base: `c1aac2d132b19848ca1940c3907e1fa68a8d08a5`
Previous candidate: `02c06e21bd5fd613341935a843fabe5350ea0a9d`
Current candidate: `a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`
Full acceptance subject: `git diff c1aac2d132b19848ca1940c3907e1fa68a8d08a5 a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`.
Latest fix navigation: `git diff 02c06e21bd5fd613341935a843fabe5350ea0a9d a21a804bc56f039f8a407f6a5b9c4f95905bf3f0` (commits dcf16f862..a21a804bc inclusive).
HEAD must remain at current candidate; source must remain unchanged.

## Authority and prior review

- Approved spec: changes/active/agent-prompt/spec.md revision 2.
- Original task: changes/active/agent-prompt/tasks/TASK-001-agent-prompt.md.
- Prior verdict and validated findings: changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/verdict.md and findings-validation.md.
- Remediation: changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/TASK-001-R1-registration-and-public-type-closure.md; appended implementation evidence is available verification, not acceptance authority.
- Owner explicitly accepted FIND-TASK-001-1 (concurrent deletion of reused generated Prompt) as risk and requested simple solutions, removing its lock/recheck/concurrency-test requirement. This is recorded in the remediation scope and prior verdict addendum. Preserve this direction; do not re-open the same waived interleaving or prescribe its lock/test. Existing auth, tenant and transaction checks still apply.
- Required prior findings to assess independently: FIND-TASK-001-2 exact-pin reapply; -3 current-operation child selection; -4 runtime public Python union export; -5 Rust field/panic docs. Preserve IDs for failures with the same cause. New findings, if any, start after 5 and need demonstrated approved-scope consequence.

## Shared navigation map (starting point, expand independently)

- Original server workflow: components/cards/service.rs::register_card, RegistrationWriter::write, persist_node, inline_prompt_submission, resolve_existing; canonical reference/graph planning and SQL version/transaction/lifecycle owners.
- Remediation seam: service.rs::pin_existing_inline_prompt; caller-local selected outcomes and registration response dedup; wyrd-sql/src/queries/cards/version_resolve.rs::SubmittedCardIdentity, latest_stable_in_line, resolve_version. Inspect all callers and ordinary authored kinds/version modes, not only generated Prompts.
- Original SDK workflow: wyrd-client/src/state.rs typed maps/HydratedAgent/HydratedCard/WyrdState::agent/get/prompt, observe/invoke.rs; Python Card/state hydration and stubs, TS native/state wrapper projections and type contracts; existing graph hydrator and alias index.
- Python remediation: python/wyrd/state/_hydrated.py single union definition, runtime __init__.py and authored __init__.pyi re-export, public typing contract and unit import proof.
- Proof: server pg_card_registration_route exact-pin reapply/current-transaction selection tests plus original HTTP proof; Rust SDK register_and_hydrate original/identity/navigation journeys; Python public union test and typing; original TS navigation/typing and invocation; new/touched Rust test helpers and docs.
- Authorities: AGENTS.md, architecture/agent-rules.md, architecture/references/README.md; spec-driven-development, maintainer-style, testing/implementation execution, protocol/doctrine and applicable language/security/data references.

Read the full cumulative diff and prior finding/remediation evidence; use the latest fix diff only to navigate closure. Prior reports are historical inputs. Current-round reviewers may not read each other's reports or implementation completion summary. No intended verdict is supplied. Do not request historical checkout/baseline testing to assign blame. Static source first; no Cargo-backed builds/tests without orchestrator coordination. Small no-sync public Python import/type inspection is permitted if useful without source/dependency mutation. Do not spawn more agents; write only your exact assigned report.

## Full cumulative paths

```text
architecture/wyrd-design.md
architecture/wyrd-doctrine.mdx
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/TASK-001-R1-registration-and-public-type-closure.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/discovery-comparison.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/domain-review-registry-data.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/domain-review-security-tenancy.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/findings-validation.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/followup-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/maintainer-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/standards-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/subject.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/system-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/task-review-behavior.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/task-review-invariants.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/verdict.md
changes/active/agent-prompt/tasks/TASK-001-agent-prompt.md
crates/shared/wyrd-client/src/observe/invoke.rs
crates/shared/wyrd-client/src/state.rs
crates/wyrd/wyrd-cards/src/agent.rs
crates/wyrd/wyrd-cards/src/prompt.rs
crates/wyrd/wyrd-cli/tests/card_lifecycle.rs
crates/wyrd/wyrd-server/src/components/cards/service.rs
crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs
crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs
docs/src/content/docs/understand/cards-versions-and-relationships.md
fixtures/cards/agent_prompt/desk-prompt.yaml
fixtures/cards/agent_prompt/inline-agent.yaml
fixtures/cards/agent_prompt/path-agent.yaml
fixtures/cards/agent_prompt/prompt-desk.yaml
fixtures/cards/agent_prompt/ref-agent.yaml
sdks/wyrd-sdk-python/python/wyrd/cards/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/prompt/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/state/__init__.py
sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/state/_hydrated.py
sdks/wyrd-sdk-python/src/state/hydrator.rs
sdks/wyrd-sdk-python/src/state/mod.rs
sdks/wyrd-sdk-python/tests/fixtures/bundles/complete/cards/inline_agent/card.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/complete/cards/inline_agent/relationships.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/inline-only-agent/cards/inline_agent/card.yaml
sdks/wyrd-sdk-python/tests/fixtures/bundles/inline-only-agent/cards/inline_agent/relationships.yaml
sdks/wyrd-sdk-python/tests/integration/state/test_state_journey.py
sdks/wyrd-sdk-python/tests/integration/test_register_and_hydrate.py
sdks/wyrd-sdk-python/tests/typecheck/state_contracts.py
sdks/wyrd-sdk-python/tests/unit/runtime/agent/test_agent_journey.py
sdks/wyrd-sdk-python/tests/unit/state/test_hydration.py
sdks/wyrd-sdk-rust/tests/integration/register_and_hydrate.rs
sdks/wyrd-sdk-ts/native/src/cards.rs
sdks/wyrd-sdk-ts/wyrd/index.d.cts
sdks/wyrd-sdk-ts/wyrd/index.d.ts
sdks/wyrd-sdk-ts/wyrd/src/index.ts
sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts
sdks/wyrd-sdk-ts/wyrd/tests/types/card-types.test-d.ts
```

## Latest fix paths

```text
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/TASK-001-R1-registration-and-public-type-closure.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/discovery-comparison.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/domain-review-registry-data.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/domain-review-security-tenancy.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/findings-validation.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/followup-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/maintainer-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/standards-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/subject.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/system-review.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/task-review-behavior.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/task-review-invariants.md
changes/active/agent-prompt/review/TASK-001-02c06e21b-20261015/verdict.md
crates/wyrd/wyrd-cards/src/agent.rs
crates/wyrd/wyrd-server/src/components/cards/service.rs
crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs
crates/wyrd/wyrd-sql/src/queries/cards/version_resolve.rs
sdks/wyrd-sdk-python/python/wyrd/state/__init__.py
sdks/wyrd-sdk-python/python/wyrd/state/__init__.pyi
sdks/wyrd-sdk-python/python/wyrd/state/_hydrated.py
sdks/wyrd-sdk-python/tests/typecheck/state_contracts.py
sdks/wyrd-sdk-python/tests/unit/state/test_hydration.py
```
