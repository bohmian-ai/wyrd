# TASK-003-R8 — Safe chart brushing and recoverable executor initialization

Kind: remediation. Status: proposed; route directly to `$wyrd-implement`.
Spec: [approved revision 13](../../spec.md). Original task: [TASK-003](../../tasks/TASK-003-shared-ui-foundation.md).
Parent: TASK-003; follows R7. Remediates: **FIND-003-16**, **FIND-003-17**.
Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Reviewed cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533`; immutable candidate: `f8c6f083103d8f55007510a4fbcc75e36b4d9c55`.

## Problem and intended result

Two negative transitions remain in the shared foundation. A successful timeseries whose numeric observations are all null still offers a brush, but releasing it throws while converting absent geometry into dates. A reusable Node executor remembers a failed first connection forever, so a later caller cannot recover after the dependency becomes healthy. Keep each correction at its existing producer: Chart owns geometry-to-range conversion; the Node executor owns its lazy initialization state. These are separate causes packaged in one bounded foundation remediation, not a reason to create shared machinery.

After correction, null-only charts preserve truthful data and cannot throw or emit invalid ranges from brushing. A failed first connection remains a visible failed read, while a later explicit read on the same executor can connect again. Existing normal brushing, successful handle reuse, cancellation, tenant/session authority and SDK transport ownership must continue to work.

[Independent validation](findings-validation.md) records full source/caller traces, executable source probes and minimum correction decisions. [Focused follow-up](followup-review.md) establishes required-host reachability and limits. No new product or architecture decision is needed under REQ-025/030 and the original task.

## FIND-003-16 — Null observations remove the drawable domain before brush conversion

Paths prefixed `ui/` below are relative to `crates/wyrd/wyrd-server/wyrd-ui/`.

At `ui/packages/ui/src/lib/components/Chart.svelte:99–105`, `release` checks gesture width but converts its endpoints with undefined x bounds when no drawable point exists. The producer chain is the existing null-preserving query projection, `loaders.ts::shapeChart:171–199`, and `plot.ts::timeSeries:35–47`. A nonempty timestamp/nullable Float64 result passes schema checks and remains populated. Plotting correctly skips null y observations, leaving no points and therefore no x domain. The component still creates a pointer brush. `WyrdView` supplies an action callback in the required local, remote and AppBridge view paths; a meaningful drag throws `RangeError: Invalid time value` before the host can validate an intent.

For example, an authorized bounded query over `vala.traces.spans` selecting `start_time_unix_nano` and `CAST(NULL AS DOUBLE)`, ordered by timestamp, can return exactly this supported shape. It is neither malformed query data nor a reason to reject nullable numeric schemas. Existing tests brush drawable data; table null and null-series-identity tests do not cover absent geometry. Actual-source probes independently reproduce the exception.

Correct the existing Chart geometry-to-range production boundary so absent or non-increasing finite drawable bounds cannot start or complete an invalid conversion, and emitted intervals satisfy the existing absolute-range contract. Reuse native numeric checks and existing range semantics; local control flow remains implementation-owned. This guard belongs in Chart because Chart produces the arithmetic and public direct-prop consumers need the same protection. Do not move the correction into each host, reject supported null data, manufacture zero observations, replace query success with denial, or change `timeSeries` null handling merely to avoid the failure. Preserve ordinary gaps, signed plots, captions, executed SQL/binds and valid range gestures.

## FIND-003-17 — Failed initialization becomes permanent executor state

At `ui/packages/ui/src/lib/node.ts:16–19`, both lazy SDK handle getters retain their connection promise using nullish assignment. Rejection does not clear that value. All later reads sharing the cache await the original rejection without invoking SDK connect again. Cards get/list share one cache; Bifrost query/list-tables/describe-table share the other. Verification status uses its independent path.

The existing SDK connection owners are fallible and allow a later fresh connection: Cards connects through Rust `WyrdClient::connected` and credential exchange; Bifrost additionally assembles its existing transport for ordinary custom-server clients. Rust stores exchanged credentials only after success. A transient first auth/network/ingest-channel outage therefore need not invalidate the underlying client. The adapter's rejected-promise cache prevents the recovery the existing owner permits.

A user's server can retain the required public `nodeExecutor(client)` across successive loads or `run` calls; its contract is not one-shot, and the integration consumer already retains an executor for multiple reads. The shipped BFF and example construct request-local instances, so they are not permanently disabled across later requests. Keep that scope limit. Existing healthy-connect and cancellation tests do not exercise failed initialization followed by recovery; independent actual-adapter probes show both caches invoke connect only once across two failed calls even when the next connection would succeed.

Correct failed-state retention at both existing initialization producers. Discard rejected initialization so a later caller-initiated read reaches SDK connect again; preserve sharing of in-flight and successful initialization. Keep the first structured error visible. Do not add automatic retries, backoff, configuration, a connection manager, consumer retry guards, SDK credential changes, a new transport or a BFF lifetime change. This is a local cache-state correction, not a new availability policy.

## Constraints and non-goals

- Change the two existing owners and their focused tests. Reuse the current mounted UI and Node SDK mock setup; no new harness, dependency, public field or generated projection is needed.
- Preserve R7 range admission and lexical SQL recognition, exact flat Card identity, original-number ceiling validation, terminal-only results, bounded collection and all prior cancellation fixes.
- Preserve Rust-owned credentials, tenancy, permission and audit behavior. No session authority, SDK transport or durable behavior moves into the UI.
- TASK-027 layout, unrelated PR #205/main content, Ask, saved views, generic mutation dispatch, production MCP registration and publishing remain excluded.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-003-16 | Load a successful nonempty timestamp/nullable-numeric null-only result into rendered Chart/WyrdView and perform a meaningful drag. No exception or invalid range action/reload occurs. Replace it with normal drawable data and verify brushing still emits a valid increasing RFC 3339 range and reloads correctly. A single-instant domain can exercise the same non-increasing-domain invariant without expanding scope. |
| FIND-003-17 | For each cache, first SDK initialization rejects with an existing structured error, and a later read on the same executor/client succeeds through a fresh connect. The first error stays unchanged; subsequent sibling operations share the recovered handle. Existing pre-start, acquisition and active-collection cancellation behavior remains green. |

Follow repository RED/GREEN discipline on the existing test owners. Extend the current Chart scenarios and Node executor mock owner in `ui/src/lib/ui-library.test.ts`; test titles and fixture organization remain implementation-owned. Record the expected regression failures before correcting production code. Focused existing selectors are:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
```

The Node recovery regression may have its own descriptive test title in that same file; record and run its exact selector if added. Finish with the narrow affected-file regression and type/component check:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
```

No Rust, schema/catalog or build-owner change is expected. Do not require a broad aggregate, full live journey, new outage harness or renewed platform qualification for these local corrections. Integrated journey evidence is assessed once at change review. Record actual write set, RED/GREEN results and preserved cancellation/normal-brush proof; later task review still covers the full cumulative candidate against the original task.
