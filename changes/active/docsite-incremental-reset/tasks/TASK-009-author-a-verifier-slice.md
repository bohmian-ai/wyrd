---
id: TASK-009
kind: implementation
status: proposed
spec: SPEC-docsite-incremental-reset
spec_revision: 9
requirements: [REQ-012, REQ-011, REQ-002, REQ-004, INV-005, AC-013, AC-012]
depends_on: [TASK-008]
parent_task:
remediates: []
review: manual review by the user
---

## Outcome and Value

A support-agent or Q&A builder sees Wyrd check what they care about: a reply
judged by a model against a policy. Author a Verifier and the LLM gateway page
publish together, because the model judge needs a configured provider.

Before: both pages are drafts built on `examples/support-desk-app`, which no
journey runs as shown. After: Author a Verifier extends the Quickstart's
`support-reply` Service with an assertion, a Python check, and a model judge
from a local `examples/author-a-verifier/`, proven by a journey test; the
gateway page shows only the provider setup the judge needs.

## What Changes and What Must Stay True

- One local example under `examples/author-a-verifier/`: the Verifier YAML files,
  any judge Agent, the Service that binds them, and one complete Python program.
- The tutorial program is Python, with YAML declarations. Rust and TypeScript
  tabs are added only when their journeys exist; that parity is out of scope
  (INV-005).
- The journey uses the test server's local model provider; it needs no provider
  credentials. Readers configure a real provider on the gateway page.
- Rationale in the current draft (cheapest check first, why a pass must cite
  evidence) moves to How verification works.
- `examples/support-desk-app` and its tests are untouched.

## Approach

1. Write the example's Cards and program, extending `support-reply`.
2. Add the journey test that runs the exact program against `WyrdTestServer`
   with the local provider, covering each check passing and failing.
3. Rewrite Author a Verifier around the example in the Quickstart page shape.
4. Trim the gateway page to provider setup, confirmation, and `## If it fails`.
5. Move the rationale; record the released walkthrough; hand both pages to
   the user for review.

## Ordered Implementation Scenarios

### Scenario 1 — The documented program verifies all three check kinds

**Behavior.** Running the example program against a real server declares the
Cards and returns one verdict per requirement: an assertion, a Python check,
and a model judge citing the policy. A reply that breaks each rule fails that
requirement with its reason; a judge verdict without a citation is not a pass.

**RED.** Planned test `test_docsite_author_a_verifier.py` under
`sdks/wyrd-sdk-python/tests/integration/`, executing the example's program
files as the Quickstart journey does, with the local provider scripting the
judge. It fails because the example does not exist. Confirm the selector after
adding it. Command: [author-a-verifier journey](#verification-and-evidence).

**GREEN.** Write the example until the journey passes; rerun the Quickstart
journey, since the example extends its Service.

**REFACTOR.** Share fixtures with `test_docsite_quickstart.py` and
`test_verifier_building_blocks.py` where they already exist; no new harness.

### Scenario 2 — Rerunning the program works without cleanup

**Behavior.** A second run declares nothing new and returns the same verdicts;
an edited Card without a version bump returns `WYRD_REGISTRY_409_SPEC_DRIFT`.

**RED.** Add rerun and drift cases to the same journey; they fail until the
example declares through `WyrdState.declare`.

**GREEN.** Use the existing declaration path; rerun Scenario 1.

**REFACTOR.** None beyond test parameterization.

## Acceptance Criteria

| Obligation | Observation | Proof |
| --- | --- | --- |
| REQ-012, AC-013 | Both pages publish only with the example's journey passing | Scenarios 1–2 |
| REQ-004 | Every displayed snippet is `CodeFromFile` from the tested example | Page source review; `docs:check` |
| REQ-002 | Gateway steps and the program run at 0.3.0 with a real provider | Released walkthrough evidence |
| REQ-011, AC-012 | Guides hold steps, code, and output; rationale moved | Manual review against docs/AGENTS.md |

## Implementation Reference

| Capability | Existing owner | Gap | Extension |
| --- | --- | --- | --- |
| Local model provider | `test_verifier_building_blocks.py`: `WyrdTestServer(verification_runtime=True, provider_base_url=receiver.url)` | Not wired to a docs example | Reuse in the new journey |
| Quickstart example and journey | `examples/quickstart/`, `test_docsite_quickstart.py` | No Python check or judge | Extend `support-reply` in a sibling example |
| Gateway sample | `examples/gateway-setup/` (proven only by `local_dev` journeys) | Not a CI journey | Keep as the released walkthrough's subject |
| CI selection | TASK-004: `examples/` selects docs and the language journeys | None | Use as is |

## Verification and Evidence

- **Author-a-verifier journey (planned):**
  `mise run py:setup && mise run build:server && scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_author_a_verifier.py'`
- **Quickstart journey regression:** same, with `tests/integration/test_docsite_quickstart.py`.
- **Python format and lint:** `mise run py:format && mise run py:lints`.
- **Site gate, once before commit:** `mise run docs:check`.

## Material Stop Conditions

The judge needs a server or SDK change to run from YAML plus a short program
(route through REQ-007's ergonomics rule); the gateway cannot be set up at 0.3.0
without unreleased options.

## Authority Links

- [Approved spec revision 9](../spec.md)
- [Verifier Card reference](../../../../docs/src/content/docs/reference/verifier-card.md)
- [Docs writing rules](../../../../docs/AGENTS.md)
- [Testing](../../../../TESTING.md)
