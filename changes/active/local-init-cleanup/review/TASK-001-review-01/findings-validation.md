# TASK-001 independent findings validation

Recommendation: **FIX_REQUIRED**. Both proposals are independently confirmed. Runtime acceptance has credible source and focused proof; the remaining failures are bounded reuse and mandatory rustdoc gates. No specification revision is needed.

## Immutable subject and scope

- Base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
- Candidate: `862a69b187c81a5395bc73b872695a7bb0150330`.
- Approved authority: packet `spec.md`, revision 1, blob `440649c7563dede7724b56f358b3ca1d3ecd1e80`; the authorized working-tree edit only records approval. Original task: `tasks/TASK-001-local-setup-cleanup.md`.
- Read `review-scope.md`, `task-review-behavior.md`, `verification.md`, production/test diff, original task and applicable spec obligations, root and Python SDK AGENTS, task-review skill, Rust/maintainer/agent-surfaces guides, spec-driven-development workflow, operations readiness rules and security identity/tenant/secret rules. CodeGraph is absent; no nested server/CLI AGENTS applies.
- Independently traced full proposed-correction bodies, both capture callers, existing public/private capture alternatives, readiness producer/consumer, CLI no-apply/apply/output/exit paths and gateway read authority. This is proposal validation and focused coverage inspection, not another exhaustive discovery pass.
- All selected reports are present. No prior findings carry forward. No new blocker or material unresolved conflict was found; no follow-up is needed.

## Proposal decisions

### BEH-001 — CONFIRMED

`CapturedLogs` (`wyrd-server/src/test_support.rs:288-332`) owns another complete sink, subscriber installation and writer workflow. Its only callers are the new router test (`http/router.rs:402-403`) and readiness transition test (`components/health/mod.rs:678-679`). Both install capture once on their test thread and read text; neither requires cloning the capture, moving it across threads or separating sink lifetime from guard lifetime.

The full existing `wyrd-testing/src/logs.rs` body supplies that workflow: public `LogCapture::install` creates a thread-default DEBUG-and-above, no-ANSI subscriber, retains its DefaultGuard and provides lossy text read. Its CaptureWriter appends to an Arc/Mutex sink. The existing server dev-dependency (`wyrd-server/Cargo.toml:137`) and public module (`wyrd-testing/src/lib.rs:11`) make it available; `pg_operator_delivery.rs:43,632` already consumes it. The test_support module's dependency-cycle comment concerns constructing AppState through the server harness, not this independent public log fixture through the existing dev edge. The private `operators/keys.rs:741-783` sink is an inspected pre-existing sibling, not the owner to expand or an excuse to retain new duplication.

The writer's poisoned-lock handling differs, but neither required test can poison its private sink; that difference supplies no required behavior. Reuse preserves all assertions and subscriber lifetime. This is concrete redundant machinery under an explicit reuse rule, not a preference among conventional implementations.

### BEH-002 — CONFIRMED

I read every cited body and its rustdoc. `GatewaySummary::serialize` (`wyrd-cli/src/init.rs:386`) propagates arbitrary `S::Error` from Serializer::collect_str without `# Errors`. The new summary unit (`init.rs:1913`) uses assertions and expect; the new transition unit (`health/mod.rs:677`) uses assertions; both omit `# Panics`. Added capture write/flush (`test_support.rs:314,320`) return IO Results without `# Errors`, and write uses lock expect without `# Panics`. The new router helper/test document their panic behavior; the materially changed CLI integration journeys already have panic sections.

Root AGENTS explicitly blocks merge for missing changed-item rustdoc. `agent_docs/rust.md` explicitly includes private helpers and tests and requires Errors/Panics sections. These are mandatory failures even when actual JSON serialization and the regression assertions pass. Deletion under BEH-001 closes the capture's documentation locations. The surviving correction is three accurate rustdoc sections, in the same remediation, without behavior changes or new tests.

## Final deduplicated ledger

### FIND-TASK-001-1

- Discovery source: **BEH-001**; status **CONFIRMED**; classification **DRIFT**.
- Violated obligation: root AGENTS reuse ladder; TASK-001 requires existing owners/fixtures and no new harness; task-review requires whole-workflow reuse.
- Locations: `crates/wyrd/wyrd-server/src/test_support.rs:288-332`, `src/http/router.rs:372,402-403`, `src/components/health/mod.rs:678-679`.
- Source-proven scenario: either required logging regression installs a new capture/subscriber/writer and reads text while available public LogCapture already implements that entire workflow.
- Consequence/materiality: unnecessary second capture owner requires independent maintenance and violates mandatory reuse without supplying missing behavior.
- Smallest correction boundary: delete added CapturedLogs and switch its two callers directly to `wyrd_testing::logs::LogCapture::install()` and `text()`. Preserve assertions, current-thread execution and production code. Do not migrate private sibling sinks or change dependencies/features.
- Burden: deletion of 46 added lines and two caller substitutions; no new abstraction. Merely documenting or shortening the duplicate cannot close the breach.
- Exact focused closure proof: inspect that these two paths use the existing owner, and rerun both logging units with unchanged assertions:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::router::tests::readiness_503_is_not_logged_as_a_failure) | test(=components::health::tests::readiness_transitions_are_logged)'
```

### FIND-TASK-001-2

- Discovery source: **BEH-002**; status **CONFIRMED**; classification **VIOLATION**.
- Violated obligation: root AGENTS mandatory changed-item rustdoc and `agent_docs/rust.md` Rustdoc, including fallible private helpers and panicking tests.
- Locations: `crates/wyrd/wyrd-cli/src/init.rs:386,1913`, `crates/wyrd/wyrd-server/src/components/health/mod.rs:677`, and deleted-under-FIND-1 `src/test_support.rs:314,320`.
- Source-proven scenario: readiness serialization can propagate a serializer error; tests execute assertion/expect failure paths; capture writer can panic on poisoned lock. Required sections are absent.
- Consequence/materiality: explicit merge-blocking documentation gate remains unsatisfied. Runtime PASS cannot waive it.
- Smallest correction boundary: document serializer-error propagation under Errors and the two surviving tests' failure conditions under Panics; delete the duplicate fixture under FIND-1. Preserve implementation and assertions; no unrelated legacy documentation cleanup.
- Burden: three bounded documentation additions, bundled with reuse remediation. No extra test, exemption or product decision.
- Exact focused closure proof: inspect surviving changed bodies against their new sections and confirm duplicate writer deletion. No runtime test is needed solely for prose. If implementation changes, its existing focused proof remains:

```sh
mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=init::tests::gateway_summary_distinguishes_empty_and_failed_reads)'
```

## Reconciled obligation coverage

| Obligation | Independently checked implementation and proof | Result |
|---|---|---|
| REQ-008 / AC-006 current docs base/index paths | Guide Start changes only origin; candidate git grep for retired origin excluding changes returns none; /wyrd/, llms.txt and llms-full.txt remain. Historical/generated records untouched. | PASS |
| REQ-009 / AC-007 quiet startup 503 | build_router uses traced; classifier exempts only /readyz 503 and delegates other statuses/errors. readyz retains cached all_ok predicate and 503 response. Router regression exercises trace layer, readiness 503 and ordinary 503 control. | PASS |
| INV-004 genuine dependency faults/readiness loss/CLI errors | Publisher compares previous/new snapshot before publishing; WARN includes checks on loss, INFO on recovery. Existing dependency WARN branches remain. CLI ready preserves timeout/child-exit errors; run observes children during gateway setup. Transition test asserts warning and pool_acquire reason. | PASS |
| REQ-010 / AC-008 restart names and empty/error distinction | All no-apply paths call stored_deployments. Connection/read errors reach GatewaySummary::read, distinct from successful empty. Managed-secret restart journey asserts existing local-mock while retaining inference/credential/leak checks. Summary unit covers names/empty/error and JSON. | PASS |
| REQ-010 explicit file/first-run behavior | Explicit document still wins; otherwise existing provider offer runs, followed by same apply_deployments_with and readback. No provider request added. Python guided flow approves selected provider, asserts checklist, then lists stored credentials/deployments; supplied file journey covers explicit application. | PASS |
| REQ-011 labelled checklist and preserved paths/outcomes | DevReady labels HTTP/gRPC/UI, storage/Bifrost, settings and credential file; gateway shares status/detail with JSON. MCP results, log location, handoff and Ctrl-C guidance remain in run/print_text. | PASS |
| REQ-011 JSON fields/types and one readiness document | Existing field names retained; gateway remains string via Serialize; run retains one readiness JSON branch. First-run journey parses/asserts gateway alongside endpoint/grpc/ui/tenant/log/mcp. | PASS |
| INV-005 tenancy/configuration/secrets/key claims | Saved authenticated client → Gateway::connect_with → deployment GET → Caller → GatewayAdministration::deployments retains gateway:read and tenant connection. New read maps only deployment names. No provider validation, settings mutation or credential replacement added. Existing journey retains credential bytes and secret-hiding assertions. | PASS |
| Boundaries and non-goals | No dependency/schema/persisted-setting/health-framework/startup-state-machine/card logging or first-run selection change. TASK-002 remains context, not TASK-001 obligation. No weakened assertions, timeout/retry changes or new skips appear. | PASS |
| Existing owner/fixture reuse | Existing capture workflow duplicated. | FAIL: FIND-TASK-001-1 |
| Mandatory changed-item rustdoc | Required Errors/Panics sections absent. | FAIL: FIND-TASK-001-2 |

Runtime PASS rows rely on independently inspected source/assertions and available evidence, not a claimed independent journey rerun. `verification.md` records all three exact changed units passing independently (nextest `6ceabc3a-b77d-49b0-9c3d-652331f87d71`) and clean whitespace checks. TASK-001 supplies four CLI Postgres journeys, two Python local-development journeys, format and lints as passing implementation evidence. I inspected the affected journey paths and retained assertions. No specific unresolved runtime gap justifies another suite or harness in validation.

## Recommendation and limits

Recommend **FIX_REQUIRED**, one bounded remediation for both IDs, then focused closure on the two capture consumers and three surviving documentation additions. Runtime acceptance carries forward unless correction changes its source, authority or dependencies. Neither correction needs a specification revision.

No Cargo build/test, broad suite, provider call or benchmark was run by this validator; root owns execution. HEAD and approved spec blob remain as recorded, and implementation source is unchanged. Risk: reuse/rustdoc gates remain open; supplied journeys were not independently rerun here.
