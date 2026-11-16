# Remediation and optimization review scope

Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling
Prior candidate: 180879328ac9dde94d7504855e821980e5db0f18
Candidate: 2ce7caec7d5a7bd51fb0ba1512f552b0f425ec0f
Original base: 0c81715f2ac52efd2b105e454ee309724df1655f
Review git diff 180879328 2ce7caec7; cumulative context remains available. Continuous ancestry. First remediation round, including subsequent optimizations. Source immutable; no benchmarks/load tests. Exclude preexisting untracked TASK-001-review-002 and gateway-latency-capture directories.

Authority: approved ../../spec.md revision 7; original TASK-005 revision 6 and TASK-006 revision 7 under ../../tasks/; prior ../TASK-005-006-review-001-180879328/ verdict, validated findings, both R1 tasks and completion evidence. Root/nested AGENTS and applicable architecture/testing guides apply. Review actual diffs independently of implementation summaries.

## Navigation and applicability before delegation

| Contract | Owners, producers, consumers, tests | Selected role and reason |
|---|---|---|
| All prior closure, measurement qualification, compact/details availability, Agent export; optimized Agent equivalence | testing capacity tests/verification run/profile; server verification service/routes/pg_verification_routes; skald-agent agent/construct; skald-spec prompt/wire/common, prompt builder, Agent structured/loop/request_builder, runtime dispatch/provider, providers clients, Vala agent block, Gateway request/adapters | Fresh remediation discovery: prior IDs and affected behavior, schema validation, repair, request serialization and telemetry semantics |
| Quota pinning, cancellation, reload and drain | server gateway usage/snapshots/owner, service admin warm callers, pg_invocation_tests | invariant: materially complex shared lifecycle; system: changed retry/recovery and shutdown |
| Rolling upgrade usage, replay, tenant separation | sql migration 20261111000001_gateway_usage_overlap.sql; gateway_admission; pg_gateway_usage; old gateway_admit writers | data domain: persistent cross-version projections changed |
| Token verification, tenant trace privacy/fidelity | auth-verify lib, token_extract/routes, Gate auth; telemetry export/lib; trace tests and action_timeline | security domain: unverified routing and tenant trace propagation changed |
| SharedSchema, borrowed requests, structured output | skald-spec/prompt/agent/runtime/providers and Vala consumers above | maintainer: substantial type/ownership changes; compare existing owners and smallest complete reuse |
| Cross-tier rules and changed rustdoc | every changed Rust owner/test, sql nested AGENTS, SDK Rust action_timeline, architecture | repo: broad tiers, migrations, Python-facing Rust/declaration implications, mandatory docs |
| Production bench isolation and unchanged compact SDK surfaces | prior behavior/security/standards evidence and default feature tree | carry forward unless changed source invalidates; repo/discovery check affected paths |

No new comprehensive first-round audit. Remediation discovery plus scoped specialists cover changed behavior; prior source-backed evidence covers unchanged obligations. No separate Python/TS domain reviewer: no such files changed; repo reviewer checks Rust export implications. Full journeys/gate belong to final change review. Independent fresh validator follows discovery, checking applicability, source, all closure and claims. Follow-up only for material unresolved evidence or validator-origin blocker. Reports only here.
