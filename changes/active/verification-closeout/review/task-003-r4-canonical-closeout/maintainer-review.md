# TASK-003 r4 maintainer review

## Subject and coverage

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-task3`; immutable range `7f79fb341..f6c841d57` (`HEAD` was `f6c841d57` at review).
- Authority: `AGENTS.md` §§5, 7–11 and 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/languages/maintainer-style.md`; task and spec under `changes/active/verification-closeout/`.
- Inspected cumulative file list and diff across contracts, server, Bifrost runtime, queue, client, Rust/Python/TypeScript SDKs, generated schemas/declarations, tests, example, documentation, mise, and Postgres scripts. Traced the material new owner paths and callers: `Run::invoke` → shared gateway caller → ingress/admission/capture; Service table declaration → registration/catalog; Eval scoring → result builder/outbox; Card UID attribution → Gate/Scribe/OTLP; cluster task handles → role/process shutdown; Forge reclaim → worker startup; Oracle admission shutdown → memory root notification. Checked the linked journey and focused tests and the public Python/TypeScript declaration changes.
- Available verification is the task evidence's report of `mise run -c gate` passing on 2026-10-09, Scribe 28/28, Forge 22/22, Oracle 50/50, codegen, fmt, and lints. This review did not rerun them.

| Changed surface | Maintainer assessment |
|---|---|
| Card/Service, verification, gateway, Bifrost contracts and generated artifacts | Types and declarations track the UID/run correlation change; materially changed Rust items carry explanatory rustdoc. |
| Server registration, verification, gateway, outbox, boot and state | Changed behavior stays with the owning service/handle; caller paths and targeted tests are findable from those modules. |
| Vala Gate/Scribe/Oracle/Forge, queue, SQL | The changed validation, task lifetimes, reclaim, and admission paths have cohesive owners and explicit failure descriptions; SQL retry proof is separately covered by the finding below. |
| Shared client and Rust/Python/TypeScript SDK projections | Public invoke/telemetry/correlation signatures have corresponding runtime tests, stubs, N-API declarations, and docs. |
| Support-desk example and journey tests | The three examples use the same request/verdict story and public SDK calls; their named journeys are entered into mise. |
| Postgres scripts and mise | Wrapper bootstrap contract test checks container stdin, configured values, SQL failure propagation and cleanup. The changed role rerun weakens its own proof as described below. |

## Material finding

### MAINT-001 — Role idempotency rerun can report success after a SQL error

- **Changed location:** `scripts/postgres/test-roles.sh:27–29`, the new `docker exec ... psql` invocation that runs `roles.sql` a second time.
- **Governing principle:** `architecture/references/languages/maintainer-style.md` §Tests requires a test to prove the outcome a caller cares about. The task's Scenario 6 requires preserving the role-idempotency proof; `AGENTS.md` §11 requires meaningful checks.
- **Evidence and consequence:** The preceding drift setup and final cleanup invocations explicitly pass `-v ON_ERROR_STOP=1` (`:21`, `:40`), as does the changed wrapper's initial bootstrap (`with-test-postgres.sh:117`). The new second run omits it. `psql` can continue after an SQL error and return success when subsequent commands complete, while the assertions at `:30–39` can still observe the initially bootstrapped roles. A maintainer changing `roles.sql` could therefore receive a green `test:postgres:roles` without proving that its second application succeeded.
- **Smallest correction:** Add `--set=ON_ERROR_STOP=1` to the container `psql` rerun, preserving its container client, environment variables, and stdin. The owning `mise run test:postgres:roles` lane should then prove the rerun fails on any SQL error and passes on the current idempotent SQL.

## Non-blocking calibration

No naming, layout, abstraction, or prose preferences were strong enough to propose independently. The review did not infer runtime correctness from source layout or the reported green gate.

**Overall: FAIL** — one material test-proof finding.
