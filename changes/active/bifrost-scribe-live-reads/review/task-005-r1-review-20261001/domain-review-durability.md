# TASK-005 R1 durability and persistent-data review

**Result: PASS.** Subject: `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`. I reviewed the cumulative source and diff, not just R1. No material durability or persistent-data finding.

## Boundary and authority

| Boundary | Governing authority | Source and consumer coverage | Assessment |
| --- | --- | --- | --- |
| Scribe staged ownership and restart | Original TASK-005 Scenario 1, AC 2/3/5; approved spec REQ-012/INV-009; `architecture/bifrost-design.md` staging and telemetry sections; `AGENTS.md` §§9–12 | `scribe/staging_runtime.rs` registration, claim, settlement, restore, publisher handoff; `scribe/persistence.rs` callers; `wyrd-testing/tests/bifrost/scribe/telemetry.rs` and fresh-recorder test | PASS |
| Forge settlement and recovery | Original TASK-005 Scenario 4, AC 2/3; `architecture/bifrost-design.md` maintenance and telemetry sections; `AGENTS.md` §§9–12 | `forge/worker.rs` inline and pooled attempts, failure settlement, cancellation release, prepared reconciliation, telemetry read fallback; Forge task rows and `forge/live_rewrite.rs` journey | PASS |
| Tenant transaction begin | `AGENTS.md` tenancy/server rules; `architecture/bifrost-design.md` tenant ownership; existing SQL contract | `wyrd-sql/src/tenant_conn.rs`, `postgres.rs` and tenant-connection callers in Scribe, Oracle, Forge and catalog | PASS |

I also read `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, and `architecture/references/languages/maintainer-style.md`. `.codegraph/` is absent.

## Durable-state and recovery assessment

- Scribe registers a member only after the stage record is durable. Registration, claim take, settlement, and final restoration now publish backlog from `StagingAssembler` while holding its existing mutex (`staging_runtime.rs:233–254, 295–309, 778–789, 464–495`). The publication path still commits the file and retires the member before settling the claim (`:679–716`). Restart recovery reconstructs authority and claim ownership from stage records before admission and emits the rebuilt backlog. The fresh local recorder test (`:1707`) distinguishes new emission from a stale process-global gauge; the server journey (`scribe/telemetry.rs:314`) checks retained-root restart, readback, replay, publication and final zero gauges. No shadow durable state or new handoff was added.
- Forge's `ForgeSettledAttempt` carries the result returned after the transition commits. Inline and pooled paths feed it to the same close path (`forge/worker.rs:3590–3705, 5460–5500, 3486–3521`); failure settlement returns the committed retry/refusal/failure result (`:8079–8124`). A fatal settlement or lease release still stops the owner, and ambiguous outcomes, no-match release, and retained prepared work still consult the durable task row (`:3712–3779`). The telemetry-only read is absent for known committed results, while durable recovery remains authoritative.
- `begin_bound` boxes the SQLx begin future (`tenant_conn.rs:112–123`) without changing the `AssertSqlSafe` statement, error mapping, tenant binding or transaction scope. Its one-connection Postgres test (`:183–209`) checks failed begin cleanup and transaction-local isolation. The release-build fix is confined to the async future shape.

## Verification limits

The task evidence reports the focused restored-stage and abrupt-restart journeys, Forge recovery journey, SQL tests, and all 147 Bifrost journeys passing. I did not rerun these in this read-only review. The benchmark missed a selective-query latency target; that is a separate task-level performance acceptance issue, with no demonstrated durable-state cause in these paths. The user explicitly deferred `mise run gate` to another branch, so its absence is not a finding here. The concurrency test checks final state under real interleavings; it does not force a particular interleaving, but the mutation and gauge publication share the owner lock in source.

## Proposed findings

None.
