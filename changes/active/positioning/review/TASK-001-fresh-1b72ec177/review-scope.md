# Fresh TASK-001 acceptance audit

The user explicitly requested a new review, not a remediation closure round.
Review all of `git diff 443a79337...1b72ec177` without carrying forward prior
acceptance conclusions. Prior reports are historical context only.

- Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
- Candidate: `1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
- Authority: candidate `changes/active/positioning/spec.md`, approved revision 5.
- Original task: candidate `changes/active/positioning/tasks/TASK-001-positioning-and-finance-onboarding.md`.
- Applicable current user direction: the examples deliberately omit error-handling
  and formatting scaffolding, expose a typed verify_release function and print
  non-passing requirement reasons. Python requirements remain TypedDict.
- Release/tag publication is a delivery prerequisite, not a task gate under
  approved revision 5; review does not publish.
- CodeGraph root directory absent. Use Git and repository navigation.
- Review reports only may be written; source/HEAD must remain immutable.

## Shared starting navigation

| Surface | Owners, callers, consumers and proof |
|---|---|
| Entry positioning | README, docs PRODUCT, Introduction, Setup, Quickstart; +page.server.ts/load and +page.svelte; CodeFromFile/mdsvex; readme/indexes/shell-links tests and generated llms indexes |
| Finance verification | finance_service and finance_release in Python/Rust/TS, api.py and YAML Cards; WyrdState declaration, Observe::verify, typed results; examples/tests/finance_release.rs and SDK finance/Quickstart/API journeys |
| Policy evidence and history | PDF assets, Artifact extraction/publication, Agent workspace mount company-sharing-policy.pdf.json, file grants and fs.read, Execution.details, stored-verdicts.sql, policy version journeys and fixtures |
| SDK authoring/input projection | wyrd-client authoring/verifier.rs and verify/mod.rs; Python definitions/wrappers/authored stubs; TS definitions/index result builders; model authoring, input projection, result unit/type tests |
| Landing motion and language choice | FinanceFlow, WorkflowStrip, BeatLoop, LangTabs/LangTab/lang.svelte.ts; homepage source highlighting and SSR shell tests; approved task landing-design section |
| Coupled paths and release versions | setup-check rename, setup prompt, SDK manifests/locks, workspace package/path versions; ASGI/local development/declaration tests; docs command checker/generator and GitHub detect-changes test |

Reviewers expand this map against source; it is not complete coverage or a
conclusion. Read applicable nested AGENTS, workflow, maintainer guide and
architecture sections. Evaluate current approved obligations, not superseded
review prescriptions. Historical checks are claims, not fresh execution proof.

## Specialist applicability before discovery

| Contract/invariant | Selection or omission with source basis |
|---|---|
| Full TASK-001 REQ/INV/AC, reader flows, application decisions, native errors, UI accessibility | behavior-rev required for fresh full review; covers entire range and relevant unchanged consumers |
| Broad Rust/shared spec/client, PyO3/stubs, TypeScript SDK, docs/tooling/manifests and journey rules | repo-rev required; multiple tiers change, including mandatory rustdoc and generated/public parity |
| New schema-derived slots, multi-slot model projection, result predicates, shared motion owner | maintainer-rev required; compare new abstractions with existing authoring/input/result/presentation owners |
| Policy extraction, model exposure, fixed Artifact/Verifier identity, old/new execution attribution | evidence domain-rev required; changes PDF source, grants/mounts, fixtures and inspection proof; trace existing storage/authorization owners |
| Shared state and lifecycle | invariant-rev omitted initially: added input projection is value-local; BeatLoop component-local; no server cache/coordination owner changes. Behavior and maintainer cover local lifecycle; evidence specialist covers fixed identity. Escalate on materially complex shared state discovery |
| Runtime recovery and service availability | system-rev omitted initially: cumulative changes affect local SDK preparation/predicates and presentation, not server executor/deployment/transport owners. Manifest changes are workspace version alignment. Behavior reviewer traces native errors and component cleanup; escalate if callers expose changed recovery |
| Security/RBAC, tenancy, persistent engine/concurrency | separate specialists omitted initially: no changed permission evaluator, SQL/RLS, credential, persistence/coordination owner. Evidence specialist traces exact workspace grants and provider disclosure; escalate on changed boundary |

Discovery report paths: task-review-behavior.md, standards-review.md,
maintainer-review.md, domain-review-evidence.md, all in this directory.
A separate fresh ponytail validator follows discovery, even if findings are
empty. Follow-up only for a material uncertainty/conflict or new validator
finding needing separate source validation.
