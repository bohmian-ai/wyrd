# Repository standards review

**Overall: FAIL.** One source-validated documentation gate remains. No additional security, SQL-capability, portability, SDK-ownership, or generated-shape defect was found in this standards pass. This is repository conformance, not a task-acceptance verdict.

## Immutable subject and scope

- Base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`; HEAD independently matched it.
- Remediation locator: `7cda4e93159ab328de2a74068d5e1978a2590818..3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
- Reviewed cumulative changed-file inventory and source diff, original TASK-003a, prior verdict and findings-validation, amended R1 and appended verification/diagnosis evidence, and revision-18 specification. The prior reports supplied closure hypotheses, not proof. No current-round sibling report was read.
- CodeGraph was used before locating source owners; subsequent exact source/diff/caller reads expanded it. No source edits, tests, Cargo commands, Postgres work, external calls, benchmarks, or new review tooling were performed. Only this report was written.

## Review Findings

### Critical

None.

### Important

**STD-001 — Changed-item rustdoc gate is still incomplete (VIOLATION; preserve FIND-TASK-003a-6).**

Location: `crates/wyrd-spec/src/card/verifier/mod.rs:322–341` (the new `InputSlot` deserializer's `Slot` visitor).

Authority: AGENTS.md §16 requires rustdoc for every new associated type and method, including private items, and an `# Errors` section for every fallible method; agent-rules makes missing/placeholder documentation `BLOCK_BEFORE_MERGE`. This is the explicit documentation exception to the skill's usual behavioral-consequence threshold.

Evidence: `type Value = InputSlot` at line 322 has no rustdoc. `expecting` at line 325 returns `fmt::Result` but documents no formatting-write error. `visit_str` at line 330 only says “A string is a source declaration”; it forwards `SourceText::deserialize` failure without documenting the source grammar/refusal or an `# Errors` section. `visit_map` at line 336 only says “A map is a typed slot”; it forwards `TypedInputSlot::deserialize`, whose tagged, `deny_unknown_fields` derive enforces accepted variants and required fields, without documenting those errors or an `# Errors` section. The outer `deserialize`'s `# Errors` does not document these separate new methods. These methods are reached by the shared `InputSlot` decoding path, not dead helpers. The remote enum preserves the ordinary typed variants while string values retain the source-reference grammar, so their distinct error behavior is precisely the purpose of the change.

Consequence: R1's claim that every changed item now meets the explicit rustdoc completion gate is false. The original three cited FIND-6 locations were corrected, but the same cumulative changed-item obligation remains unsatisfied in the remediation's newly added shared decoder.

Smallest correction: document the visitor's associated output type; explain string source parsing and map typed-slot dispatch, including the applicable decode failures, in substantive rustdoc with `# Errors`; document the formatter-write failure on `expecting`. Keep decoder behavior, public types, and tests unchanged. Batch this with any other validated remediation; no separate refactor, new test, or runtime mechanism is required. Closure is direct source inspection and the normal affected Rust format/lint proof. Retain stable FIND-TASK-003a-6 rather than creating a new issue per helper.

### Suggestions

No optional refactor is proposed. The following literal rule deviations are non-blocking because no material consequence was established: the new test at `reference.rs:1093` puts a `use TriggerSpec` inside the function, contrary to the module-top import rule; newly changed signatures retain qualified types (for example `Visible::pin`'s `sqlx::Error` at `groups.rs:464` and `Support::pending`'s `chrono` types at `pg_verification_groups.rs:272`). These are not additional FINDs and do not determine FAIL.

## Authority coverage

The reference router was read and selected the following complete focused references: doctrine positioning/vocabulary and architecture constraints; architecture patterns; spec-driven development, implementation execution, maintainer style, Rust core, Python API/stubs, TypeScript guide, testing workflows, agent harness and errors; Vala architecture, telemetry/observations, evaluation, OLAP serving, and analytical operations/reliability. Governing architecture sections were followed for doctrine/client model, Verifier/grouping, Bifrost/coordination clock, Trigger, registry authoring/reference forms/errors; Bifrost table identity, durability/visibility, live-tail/Oracle entry, audit/terminal contract, resource invariants and public surface; security principal/authority, authorization, tenant isolation, SSRF and audit. PyO3-specific reference rules do not independently apply: there are no modified PyO3 wrappers, module registrations, or native signatures. DataFusion-plan, Iceberg-maintenance and Arrow-FFI references are not changed domains: this task consumes the existing governed query path rather than changing those engines.

| Changed surface | Governing authority and rule coverage | Inspected source / consumers |
|---|---|---|
| Pure contracts and shared deserialization | AGENTS §§2–8,16; agent-rules; design doctrine, Verifier/Trigger/reference forms; Rust core, errors, architecture patterns | `wyrd-spec` trigger, reference, verifier/mod, verification, execution; typed/source input variants, manual visitor/remote mirror, schema derives and tests |
| Server/group orchestration | AGENTS §§2–6,9–11,15–16; agent-rules; design Verifier/clock; security authority; Rust, evaluation and telemetry references | `verification/{authority,groups,scheduler,claims,runner,engines,hosted,mod,operators,fault}`; cron sibling, claim/settlement, source tuple reader and binding projection |
| Scribe background integration / boot | AGENTS §§2–6,9,15–16; audit and capability rules; Bifrost durability/audit/resource/public contract; analytical reliability | `scribe_outbox`, `boot/mod`, capture/peer-audit callers, runtime test fault owner, `ScheduledQueryCaller`; ACK-to-SQL ordering and original submit handoff |
| SQL and persistent state | AGENTS §§2–5,9,15–16; all SQL capability/RLS/lifecycle rules; design clock and security isolation | Both changed SQL query modules, three cumulative migrations, `DueGroup`/`GroupActivity`/`PinnedGroup`, claim/enqueue/refuse/consume/complete/status; SQL PG test |
| Rust harness and SDK/MCP tests | AGENTS §§11,16; runtime lifetime and external-test rules; testing workflows, maintainer style, agent harness | Server PG grouped/OpenAPI/registration tests, SQL PG tests, testing runtime fixture/caller, Rust verify-in-real-time helpers/foreign runtime processes, MCP grouped media helpers |
| Python tests and authored typing | AGENTS §§8,11,16; Python API/stubs, testing workflows, errors | Public verify stub snapshot projection, support module and verify-in-real-time tests; public imports and pressure/refund/coding consumers |
| TypeScript declarations and tests | AGENTS §§2,8–11; generated-artifact rule; TypeScript guide and testing workflows | Source-derived `card-types.ts`; verify-in-real-time grouped/pressure/refund/coding tests; native/public imports |
| Fixtures | AGENTS §§2,11–12; design exact reference forms; test-integrity rules | All changed grouped YAMLs; six filenames/metadata/reference renames; webhook consumers; three invalid fixtures relocated with negative callers retained and coding-child kind corrected |
| Architecture/docs and execution artifacts | AGENTS §§12,14,16; design hierarchy; spec-driven development, implementation execution, maintainer style | Architecture rev18 wording, user verification/limits docs, generated error catalogue; original task/prior verdict/R1/evidence. Historical review text is evidence and is not product code. |

## Rule-by-rule conformance

PASS below is source-level conformance for the changed surface; supplied green command evidence is distinguished from independently executed proof.

| Applicable rule | Result | Source evidence / boundary |
|---|---|---|
| Native Card/Verifier/Trigger vocabulary, no compatibility aliases or new kinds | PASS | After-only `TriggerActivation::GroupEnd`, `GroupSnapshot` source provenance; six atomic fixture/ref renames; no new route or product entity |
| Durable behavior Rust/server owned, spec declarative and IO/async/PyO3 free | PASS | Spec changes are serde/validation/data; scheduler/queue/outbox own IO; no new dependency or Cargo-feature diff |
| Shared SDK owner; language lifetimes in owning runtimes | PASS | Python/TS changes are tests and contract projections; Rust `foreign_turn` launches actual Python/Node clients and uses `spawn_blocking` for process waiting; no embedded Python/Node runtime in Rust |
| Typed durable identifiers and contracts | PASS | `GroupActivity`, `DueGroup`, `PinnedGroup`, `GroupHandoff` use Card/Execution/Group/Binding identities; SQL UUID/string conversions occur at persistence seams |
| Cohesive retained-state owners, pure synchronous transforms | PASS | Existing Scheduler/Queue/ScribeSink/GroupPoller extended; Sources/Visible/Earliest retain evidence, Reader retains accepted query context; parsing/pinning synchronous; async functions await IO or compose it |
| Actual SQL capability types, construction boundary | PASS | Production queue writes/claims/status accept `&mut TenantConn`; due tenants/counts accept `&OperatorPool`; scheduler/outbox retain `WyrdPostgres`. No new production PgPool/PgConnection/Transaction parameter or field; raw pool fixtures remain test-only. This assessment is from signatures, not merely check:tenant-isolation |
| Caller owns TenantConn commit/rollback | PASS | queue `record_group_activity`, claim/consume/refuse/enqueue/complete never finish passed transactions; ScribeSink opens/commits its own connection; Scheduler opens/commits own claim transactions; claims opens/commits its own |
| RLS rather than tenant predicates; cross-tenant work privileged | PASS | Pending migration enables and forces RLS with tenant/operator policies; tenant SQL uses current tenant and scoped identities, not manual tenant equality filters; operator due query uses OperatorPool |
| PostgreSQL coordination timestamps | PASS | Record activity derives due_at from statement_timestamp + exact stored interval; claim uses statement time; refusal/queue/settlement timestamps database-owned; no new bound process wall clock for coordination |
| Authorization blocks and cannot widen authority; SYSTEM actor only | PASS | BackgroundAuthority preserves recovered owner authority; typed refusal distinguishes unavailable configuration; group read uses Caller::accepted and ScheduledQueryCaller::authenticated; refusal records owner/SYSTEM attribution without accepted authority; status remains ordinary authorized route |
| Audit decisions non-blocking, one Scribe path, no engine audits | PASS | ACK scheduling is sink-side; source calls stage existing outbox records; no request waits for audit write, no new audit SQL table/channel/publisher; scheduler mechanical transitions trace/queue rather than mint audit decisions |
| Secrets/redacted instrumentation and network screening | PASS | New logs contain binding/group/tenant/error identities, no token fields; SQL write instrument skips inputs/authority; child credential goes through process environment; no new production URL-fetch path. Existing Operator screened transport remains owner |
| Portable facilities across supported targets | PASS | std process + Tokio tasks/semaphores/transactions; no new platform cfg, /proc, cgroups, namespaces, platform sandbox prerequisite or OS-only management mechanism |
| Finite concurrency and external/query bounds | PASS | Shared execution semaphore acquired before claim; group per-process sequential pass, bounded tenant/item iteration; Reader uses existing query deadlines/admission/terminal; no custom source ceiling restored. Durable backlog and complete-group reads retain explicitly accepted limits |
| Cancellation, transactions and partial work described | PASS except STD-001 documentation gate | Scheduler handoff races cancellation before its selected commit; failed handoff leaves connection uncommitted; existing queue claims/leases remain. Scheduler public docs state rollback/commit linearization. STD-001 covers new fallible visitor docs |
| Stable errors and one derive-backed catalog | PASS | AuthorityRefusal carries existing WyrdError; GroupPollError is internal thiserror; persisted refusal uses existing VerificationError; Trigger parse produces existing validation-family refusal; no handwritten public catalog or new SDK-only errors |
| Contract serde/schema/SDK parity | PASS | InputSlot typed variants/serde tag/default/deny_unknown_fields match remote mirror; source string remains supported. InlineableRef preserves ref/sibling/inline/path precedence and serialization/schema derives. Trigger schema description and TS duration text agree; GroupSnapshot source-only shape agrees across Rust/TS/authored Python/OpenAPI assertions |
| Generated outputs changed through source | PASS on source/evidence | `card-types.ts` changes follow changed Rust schema/docstrings; error-page line matches derive-backed catalog. R1 records codegen regeneration/check. No hand-written N-API declaration modification |
| Python public typing/import/test structure | PASS | Stub GroupSnapshot mirrors source-only execution group; tests import public modules, use top-level test functions and runtime-owned fixtures. No duplicate durable Python implementation or new wrappers needing six-layer registration |
| TypeScript public typing/import boundaries | PASS | readonly generated interfaces, existing shared facade, explicit helper Promise results; no new exported any/Object/Function contracts or binding engine |
| Test placement, credentials, journey tiers, exact commands | PASS on source/evidence | PostgreSQL tests remain integration PG files; SDK tests drive real public clients/server; MCP media remains MCP journey. Runtime-specific negative tests retained. R1 records exact selected commands/nonzero counts and sequential PG reruns; no new production credentials or live provider unit-test requirement |
| No weakened gates or tests to clear a failure | PASS | No check/allowlist/config weakening in diff; typed InputSlot fix restores error propagation; invalid fixtures moved intact into existing invalid corpus with callers updated rather than deleting negative assertions; coding-child absent-card check now uses actual Verifier kind |
| Complete changed Rust rustdoc, Errors/Panics | FAIL | STD-001. Old Firsts/Visible/activity sites now documented, but new shared visitor is incomplete |
| Top-of-module imports and bare imported signature types | FAIL, non-blocking | Function-local TriggerSpec use and qualified signatures cited above; no demonstrated material consequence; no separate FIND |
| Focused verification and no unsupported benchmark qualification | PASS on supplied evidence | R1 reports P1–P8, C2/C3/C5/C10 and static lanes; keeps REQ-015/AC-009 pending. This reviewer neither reran those commands nor treats their green results as rustdoc proof |
| Immutable review / no unrelated changes | PASS for review conduct | HEAD rechecked at candidate; only new review directory untracked; cumulative git diff --check exits zero; no implementation mutation |

## Prior standards and cleanup closure

- FIND-TASK-003a-6's original locations are closed locally: `groups.rs:392–418` explains input/result keys and earliest-payload retention; `groups.rs:426–429` documents both optional ordering anchors; `pg_verification_groups.rs:299–308` documents activity's invalid-group panic. The stable issue remains open solely because the cumulative changed-item rule also governs the new visitor (STD-001).
- HUMAN-CLEANUP 1: removed fields and claim projections are absent; pending due_at and exact source matcher remain live. No standards objection.
- HUMAN-CLEANUP 2: runner docs describe pinned lists; user docs no longer call schedule the only automatic Trigger; MCP docs describe selected execution provenance. No standards objection.
- HUMAN-CLEANUP 3: filenames, metadata, Service refs and webhook callers change together; invalid fixtures keep their rejection assertions and actual kind. No standards objection.
- FIND-1–5 acceptance is owned by the independent implementation/domain reviews. Standards-relevant seams (typed failure classification, terminal attribution shape, shared validation, runtime-owned multi-language proof, preserved negative assertions) were inspected without substituting this report for that acceptance review.

## Open Questions

None required for the bounded documentation correction.

## Verification Notes

Independently executed: read-only identity/status/diff inspections and `git diff --check 22335c2...3d0b301c5` (exit 0). Reviewed the R1 command table and failure diagnoses; green focused/integration/static results are supplied implementation evidence, not tests run by this reviewer. No broad gate, benchmark, provider calls or Postgres command was run.

Skipped fresh tests/builds under the coordinated review restriction. Accepted visibility, crash-signal loss, outbox/retention loss, continuous postponement, snapshot overlap, durable backlog and fairness limits are not findings; dynamic qualification and benchmarking remain outside this report.
