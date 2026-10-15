---
id: SPEC-agents-trace-processor
revision: 2
status: approved
---

# Framework-agnostic agent telemetry

## Objective and approval

Users connect existing agent instrumentation to Wyrd once, execute normally,
and retrieve standard telemetry with optional exact Run/Card correlation.
Native OpenTelemetry instrumentation shares one export path; OpenAI Agents
uses a narrow tracing-processor bridge. Users do not author spans for framework
internals or adopt a Wyrd runtime.

The human owner explicitly approved the preceding proposal on 2026-10-15:
“I agree. revise the spec, approve and write the task (1 task)”. The owner also
requires docsite updates following current standards and conventions. Revision
2 records that approval, superseding r1's OpenAI-only scope.
The owner's same-turn clarification requires the task and scenarios to reuse
the merged ambient-auth mechanisms; REQ-017 records that binding constraint.

## Scope and ownership

- Python: Pydantic AI, Google ADK, and OpenAI Agents recipes and telemetry
  journeys; generic managed and application-owned provider paths.
- TypeScript: retain and complete OpenAI support and generic provider,
  correlation, and lifecycle behavior. No Pydantic/ADK TypeScript adapter.
- Rust: preserve existing generic OTLP/client behavior. No new Rust framework
  adapter or redesign of Rust process-global telemetry ownership.
- Existing authenticated OTLP ingest and Bifrost query contracts remain the
  server boundary. No new route, table, migration, Card kind, or durable event
  payload. Public lifecycle errors use the shared catalog.
- Foreign-runtime glue stays in its SDK; shared authentication, transport,
  tenancy, authorization, and durable behavior retain their existing owners.

## Required behavior

### REQ-010 OpenAI Agents bridge (revised)

Retain Python `wyrd.otel.WyrdAgentsTraceProcessor` and TypeScript
`wyrd-ai.WyrdAgentsTraceProcessor`. Users register one instance explicitly via
framework APIs. Wyrd setup never registers it or patches the framework.
Framework dependencies remain optional.

The bridge emits ordinary spans through the active global OTEL provider:

- Generate valid OTEL IDs; preserve framework trace/parent relationships and
  enclosing OTEL ancestry. Original framework IDs need not become OTEL IDs or
  exported attributes. An unresolved parent uses the known framework root or
  available enclosing context, without invented ancestry.
- Use valid framework start/end timestamps when supplied; missing/invalid
  timestamps fall back to callback time without failing application execution.
- Map model, provider, operation, tool identity, input/output usage, and error
  status where supplied using applicable GenAI conventions. Read late data at
  completion. Unknown span types retain basic name/hierarchy/timing/status;
  never guess provider identity, usage, or other missing metadata.
- Capture Run/Card scope at span start; later completion in another scope does
  not reattribute it. Concurrent callbacks do not corrupt state or mix runs.
- Duplicate starts for a live ID are idempotent; duplicate ends and ends with
  no recorded start are harmless. Shutdown ends unfinished mapped spans with
  ERROR and a fixed payload-free interruption description, clears contexts,
  and is idempotent. Late callbacks after shutdown are ignored.
- Adapter shutdown never closes the provider. Adapter force-flush neither
  finishes active spans nor owns provider flushing; the state/application
  flushes its provider.
- Default mapping excludes prompts, completions, instructions, tool arguments
  and results, raw error messages, and raw error payloads. A safe error type
  and status may be recorded. No bridge content-capture option is required.

### REQ-011 General instrumentation and ergonomic setup

Reuse Python `state.start_telemetry()`, `span_exporter(client)`, and
`install_run_correlation(provider)`, plus TypeScript `state.startTelemetry()`
and `spanExporter(client)`. Managed setup installs one pipeline on an otherwise
unconfigured application. Applications enable native Pydantic instrumentation;
ADK produces its native OTEL spans; OpenAI uses REQ-010.

Generic telemetry needs no Run/Card and retains authenticated publisher
identity with null optional correlation. A Run adds exact `wyrd.run_id` and
`wyrd.card_uid`. Ordinary execution needs no manually authored framework spans.
The user's model provider may be external; Wyrd gateway/MCP use is not required.

### REQ-012 Provider ownership and sampling

Repeated managed setup on the same state is idempotent. An existing provider,
including another state's, is preserved and refused with
`WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`; no identity is silently reused or
replaced. Setup is once per application. Refusal releases any resources created
by the failed setup attempt.

Application-owned providers compose Wyrd's stock exporter with their own
processors and retain lifecycle ownership. Python retains
`install_run_correlation(provider) -> bool`: successful repeat installation is
idempotent; failure remains retryable. TypeScript exposes
`runCorrelationProcessor(): SpanProcessor` from `wyrd-ai/otel` for installation
once in an application provider's construction-time processor list. It creates
no provider/exporter and promises no unsupported post-construction mutation.

Preserve application-owned sampling, resources, propagation, context manager,
and other exporters. Honor non-recording/unsampled spans; never recreate dropped
spans or promise capture independent of sampling. Repeated manual exporter
attachment is application misconfiguration, not a new deduplication registry.

### REQ-013 Managed flush and shutdown

Python/TypeScript state flush first attempts a bounded flush of completed spans
from its managed provider, then attempts the existing native Bifrost drain.
Shutdown uses the same ordering and closes the state writer. Neither permanently
closes the installed global OTEL provider; continued export remains usable.
Flush leaves active spans active. Existing native startup/closed-state refusals
remain applicable.

An OTEL false timeout result, timeout exception, or rejected/failed flush must
not report success or prevent the native drain attempt. Timeout uses shared
catalog code `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`; other provider flush failure
uses `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`. Errors provide scrubbed stage detail
and actionable lifecycle remediation. When both stages fail, OTEL is primary
and the native failure's stable code is retained as secondary detail; native-only
failure retains its code. Use OTEL's configured flush bounds, with no additional
queue, retry policy, or public timeout option.

Application-owned providers are flushed/shut down by the application, never by
state methods. Run exit creates/ends/flushes no span.

### REQ-014 OTEL fidelity, correlation, and content policy

Native spans retain IDs, parentage, links, events, attributes, resources, and
instrumentation scope through the existing OTLP path; Wyrd adds only its optional
Run/Card correlation. Nested Card scopes restore their outer scope. Concurrent
async executions do not leak correlation. Explicit OTEL context propagation
remains supported; arbitrary thread/detached-task propagation is not promised.

Reuse OTEL SDK processors/exporters, OTLP, W3C propagation, and applicable
semantic conventions. Do not rewrite every framework into a proprietary common
schema. Qualify GenAI compatibility against tested dependency versions.

Native instrumentation owns content emission. Recipes configure available
upstream content controls, disclose residual content capture, and state that
Wyrd exports what instrumentation emits. No universal redaction claim. Existing
server payload authorization remains authoritative.

### REQ-015 Docsite and durable authority

Extend the existing Observe task page and relevant Connect/Understand links,
following `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`, and neighboring pages.
Provide actual `wyrd-ai` install prerequisites, one-time managed setup,
application-owned setup, all three Python recipes, TS OpenAI, optional Run
correlation, flush/shutdown, and authenticated query/readback proving success.
Show required framework construction or link a complete prerequisite; undefined
agent/provider placeholders cannot be the only usable procedure.

Explain OpenAI add-versus-replace processor behavior, sampling/content controls,
tested versions, provider conflicts, and flush failure handling. Distinguish
model/gateway/MCP setup from telemetry. Follow existing frontmatter taxonomy,
goal-oriented titles, body headings starting at `##`, `LangTabs`/`LangTab` for
applicable language alternatives, and current internal links. Preserve existing
publication scope/draft flags; publishing draft pages is not authorized.
Regenerate machine-readable indexes/reference output with existing tooling,
never by hand. Update lasting architecture contracts, not progress history.

### REQ-016 Real framework evidence and lane closure

Use actual Python framework execution for all three frameworks and actual TS
OpenAI execution, deterministic local model endpoints, real WyrdTestServer,
and repository-managed Postgres. Framework/provider instrumentation must emit
asserted model spans. Hand-authored generation spans and in-memory exporters
alone are not compatibility evidence.

Cover model/tool success, model/tool failure, streaming completion, optional
Run correlation, nesting/concurrency, enclosing ancestry, both provider paths,
flush while alive, sampling/content controls, denied export, and tenant
isolation. Assert parent IDs, exact Card/Run identity, model and available input/
output usage, terminal error status, and no duplicate logical spans. Query back
through the SDK using existing authenticated OTLP/HTTP and Bifrost. No new MCP
telemetry tool is required.

Supporting unit tests cover callback edges, Python thread safety, registration
retry, and optional imports. Timeout/exception/drain-order and missing-callback
injection may be unit-only: these runtime-local branches cannot be forced
reliably by real IO. Successful flush and denied export still require journeys.
Integrate tests into existing owner and Bifrost capability lanes with their
framework dependencies. No silent skips or uncollected required integrations.

### REQ-017 Reuse merged ambient authentication

Use the merged SPEC-ambient-auth r3 mechanisms rather than new telemetry or
framework auth helpers. Resolve normal clients from existing ambient
configuration/credentials (`WyrdClient()` / `WyrdClient.connect()` and lazy state
resolution); explicit client injection remains available for deliberate identity
selection. Reuse one selected client's shared token/refresh path for its state,
exporter, and any stock client pointed at Wyrd. No copied bearer at setup, new
token cache, manual renewal, or per-framework auth adapter.

Stock provider/MCP clients pointed at Wyrd use Python `client.http_auth` or
TypeScript `client.httpAuth`, which obtain current tokens per request, remove
placeholder provider-key headers, and send `X-Wyrd-Access-Token`. Do not revive
removed `GatewayAuth`, `accessTokenSource`, or standalone `wyrdFetch`. The
OTLP exporter keeps its existing stock-exporter/shared-`access_token` integration;
do not force an httpx/fetch hook into an incompatible exporter transport.
Never send Wyrd credentials to an external model provider.

Recipes lead with ambient configuration and reference the existing stock-client
guide for Wyrd gateway/MCP routing. Python and TS OpenAI journeys must exercise
a stock model client through the real Wyrd gateway using these transports and
ambient Wyrd credentials, with a local upstream. Python native-framework
journeys likewise resolve Wyrd export/query credentials ambiently; external
model calls remain independent. Demonstrate no-credential failure and per-export
token freshness without duplicating the auth subsystem's full test matrix.
Use isolated test configuration directories and repository-managed keys; never
read or modify the developer's actual saved login. Live OIDC is not required.

## Invariants and non-goals

- INV-001 Authentication selects tenant/publisher; Card assertions are
  server-authorized. Run/Card attributes never select credentials or enable
  cross-tenant reads/writes. Reuse shared credential refresh.
- INV-002 No new durable protocol, framework registry/runtime/discovery,
  monkey-patching, universal instrument-all API, bespoke exporter/retry queue,
  or Pydantic/ADK adapter. Preserve existing Rust OTLP and log/metric support;
  their redesign is excluded.
- INV-003 Optional enrichment never changes application results or suppresses
  its exceptions. Explicit setup/lifecycle calls may report actionable errors.
  Never copy payloads/secrets into new metadata, diagnostics, or errors.
- INV-004 Provider ownership stays explicit. Setup does not adopt an existing
  provider; Run scope owns no provider lifecycle.

## Acceptance obligations

All obligations are owned by TASK-001 under revision 2.

| ID | Observable acceptance | Credible evidence |
|---|---|---|
| AC-001 | Three Python frameworks and TS OpenAI export real model/tool activity and query it back | Real SDK → authenticated OTLP → server → SDK journeys |
| AC-003 | Format/lint/types, generated artifacts, boundaries, affected lane selection, scoped tests, and docs pass | Exact focused runs and relevant checks; integrated capability proof |
| AC-007 | Bridge retains hierarchy/timing/model/usage/status and start-time identity; optional imports work | Python/TS journeys plus callback and Python concurrency tests |
| AC-008 | Managed setup is idempotent; owned providers preserve configuration/export and correlate correctly | Both-language provider journeys and retry tests |
| AC-009 | Flush is visible while alive; failure is reported and native drain attempted; active spans/global provider survive | Readback journeys plus deterministic runtime-local failure injection |
| AC-010 | Uncorrelated, nested/concurrent, enclosing-context, error, and streaming executions retain correct identities/data without duplicates | Actual framework journey matrix plus correlation regressions |
| AC-011 | Sampling/content controls hold; denied export persists nothing and cannot cross tenants | Framework sampling/content and authenticated negative journeys |
| AC-012 | Users follow current docsite recipes and verify success; architecture/machine docs agree | Recipe review against journeys, generated checks, `mise run docs:check` |
| AC-013 | Ambient setup, stock-client gateway authentication, credential refusal, and exporter token freshness reuse merged auth correctly | Python/TS OpenAI gateway-to-telemetry journeys, all-framework ambient setup, focused existing transport regressions and exporter auth tests |

## Retention, removal, and replacement

Retain exporters, provider/correlation owners, optional dependencies, bridge,
and focused synthetic correlation tests. Replace duplicate-start overwrite,
empty adapter shutdown, callback-only timing, silently successful flush timeout,
and stale requirement references. Replace synthetic-span-only compatibility
claims with actual framework evidence. No unrelated relocation/dependency
replacement is required.

## Expensive-to-reverse decisions

OTEL/OTLP is the integration boundary; adapters are local runtime glue. Preserve
global-provider shutdown compatibility. The TS correlation factory and the two
catalog-backed lifecycle errors are approved public surfaces. Stock API-key
OTLP configuration remains supported alongside optional shared-client exporter
helpers. The former design sentence “no SDK exporter helper exists” is
superseded and reconciled in this revision's authority edit.

## Open material decisions

None. Private implementation, fixtures, and equivalent focused commands remain
implementation-owned within these boundaries.

## Revision history

- r1 — Approved 2026-10-14; split from SPEC-example-journey-fixes r6 with
  explicit Python/TS OpenAI processors only.
- r2 — Approved 2026-10-15 in this conversation: generic OTEL, three Python
  frameworks, retained TS support, provider/lifecycle correctness, privacy/
  sampling, real journeys, docsite convention compliance, and exactly one task.

## Authority links

- [Repository standards](../../../AGENTS.md)
- [Agent rules](../../../architecture/agent-rules.md)
- [Protocol authority](../../../architecture/wyrd-design.md)
- [Public doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Telemetry observations](../../../architecture/references/domain/telemetry-observations.md)
- [Spec-driven development](../../../architecture/references/languages/spec-driven-development.md)
- [Docs contributing](../../../docs/CONTRIBUTING.md)
- [Docs product conventions](../../../docs/PRODUCT.md)
- [Merged ambient-auth decisions and owners](../../completed/2026/ambient-auth.md)
- [OTEL Trace SDK](https://opentelemetry.io/docs/specs/otel/trace/sdk/)
- [GenAI conventions](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/README.md)
