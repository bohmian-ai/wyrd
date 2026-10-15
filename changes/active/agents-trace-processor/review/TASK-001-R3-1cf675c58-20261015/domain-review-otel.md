# OTEL domain review — PASS

## Immutable subject and boundary

Base `dac4c340f835676adc29f3dcc01f98e1d3d51465`; candidate
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`. HEAD matched the candidate at
entry and after source inspection. This is a cumulative TASK-001 review under
approved spec r2, including the R1, R2 and R3 remediation obligations. The
authorized imported-main attribution changes are reviewed at their telemetry
seams, not classified as unsolicited task scope.

Reviewed boundary: Python and TypeScript framework callbacks → ordinary OTEL
provider/processor/exporter → authenticated OTLP → canonical Gate identity →
Scribe attribution → public SDK readback. Managed/owned provider configuration,
flush, sampling, correlation, payload handling, package prerequisites, recipes
and actual-framework assertions are included.

## Authority and source coverage

- `AGENTS.md`, `architecture/agent-rules.md`, reference router,
  spec-driven-development, maintainer-style, testing-workflows,
  python-api-and-stubs, typescript-guide, and telemetry-observations references.
- Active Wyrd design's authentication, trace context, managed export and
  Observation identity sections; Wyrd doctrine's application/runtime boundary;
  Bifrost design's table/row identity, managed publisher stamps and retry fence.
- Approved `spec.md` r2; original TASK-001 and all three historical remediation
  tasks; previous validation ledger used as hypotheses rather than votes.
- CodeGraph was consulted first for bridge/exporter/correlation/lifecycle and
  Gate identity navigation. Expanded source inspection covered both full bridge
  implementations, surrounding correlation/exporter/setup owners, Python native
  state orchestration and TS state lifecycle, shared settlement regression,
  package declarations/checker, manifests/locks and `mise.toml` selectors.
- Python `tests/unit/state/test_observe_surface.py`; framework `app.py` and
  `test_framework_telemetry.py`; TS `telemetry.test.ts`,
  `agents-trace-processor.test.ts`, and observed-service fixture consumers.
- Gate's three decoded signal ingress methods and shared canonical dispatch;
  `gate/auth.rs::AuthContext::attribution`; Scribe preprocessing logical and
  correlation identities; ingress attribution forwarding and execution-lane
  stamping; existing OTLP maximal trace fidelity and negative attribution
  journey/support/transport helpers. The underlying durable fence is unchanged
  by the bounded R3 patch.
- Observe recipe, docsite conventions/product standards, Connect references and
  lasting architecture contract. Package source and clean-package checker were
  inspected; generated declarations remain source-derived.

Pinned dependency facts were checked against local installed primary source and
locks: Python Agents 0.23.1, OTEL SDK 1.42.1, Pydantic AI slim 2.56.0 and ADK
2.12.0. `ResponseSpanData` has independent safe `usage`, and the ordinary and
streaming Responses producers populate it when full response content is
withheld. The pinned OTEL Python `BatchProcessor.force_flush` explicitly ignores
its timeout and calls synchronous export; bounded lifecycle waiting therefore
has a demonstrated purpose. TS Agents 0.20's Responses producer writes
`_response` when full tracing data is enabled; no absent data is inferred by the
bridge. These facts do not imply compatibility guarantees for untested future
dependency releases.

## Requirement coverage

Results below are source/acceptance coverage assessments. Fresh command results
belong to the orchestrator's `verification.md`.

| Obligation | Implementation and proof inspected | Result |
|---|---|---|
| REQ-010 | Python `otel.py:354-509`; TS `index.ts:2490-2708`: explicit bridge registration, generated OTEL IDs, parent/root/enclosing fallback, valid timestamp conversion, completion-time allowlist mapping, start-time scope, duplicate/missing callbacks and interrupted cleanup. Callback/thread tests and real Agents journeys cover the same owners. | PASS |
| REQ-011 | Existing stock exporters and managed setup retained; actual Pydantic/ADK instrumentation and OpenAI SDK model/tool calls drive export/readback. Generic uncorrelated work remains allowed and publisher-stamped. | PASS |
| REQ-012 | Python native setup mutex/OnceLock and losing-provider shutdown; TS post-import recheck and refused-provider shutdown. Owned correlation factory/installer composes with native processors and leaves provider resources/sampling/lifecycle owned by the application. | PASS |
| REQ-013 | Python `_ManagedTelemetry` bounds caller waiting with at most one in-flight attempt; later calls require a covering attempt. Both runtime state owners attempt OTEL then native drain and preserve shared primary/secondary catalog settlement. No provider shutdown in state shutdown. Live journeys exercise flush/readback before continuing. | PASS |
| REQ-014 | Run context stamped on start, nesting/async isolation, ordinary OTEL pipeline retained. Bridge reads no payload fields and emits payload-free status. Native recipes configure upstream content controls and disclose residual content. Existing maximal OTLP fidelity assertions cover IDs, parentage, timestamps, attributes, links/events, resources and scope. | PASS |
| REQ-015 | Observe installs the actual package/extras and all directly imported TS OTEL packages, constructs each framework, includes ADK runner/session/message, separate provider credentials, managed/owned paths, readback, versions, content/sampling and failure explanations. Existing tabs/draft publication scope retained. | PASS |
| REQ-016 | Three Python parameter cases for managed, owned, execution edges and sampling/privacy/auth; TS actual Agents equivalent. No manually authored generation span is the compatibility proof. Framework subprocess isolates one global provider per process; unit injections stay at runtime-local failure branches. Existing SDK and Bifrost selectors include the files and dev setup supplies frameworks. | PASS |
| REQ-017 | Stock per-export access-token integration retained in Python requests auth and TS exporter async headers; ambient state/client selection and stock gateway `http_auth`/`httpAuth` remain the shared owners. Real gateway test checks external upstream credential isolation and exact missing-credential code. | PASS |
| INV-001 | Card/Run never select auth. Gate authorizes asserted Cards, derives publisher from verified attribution, and now binds both publisher members before Scribe stamping. Denied/cross-tenant journeys require absence through public readback. | PASS |
| INV-002 | No framework discovery, runtime, monkey patch, proprietary transport, exporter or retry queue introduced. Existing logs/metrics exporters and shared signal dispatch retained. | PASS |
| INV-003 | Allowlisted model/tool metadata and fixed/safe error status; native controls and limitations disclosed. Run entry/exit enrichment remains fail-open; setup/lifecycle failures use explicit errors. | PASS |
| INV-004 | Existing provider is refused, not adopted. State lifecycle touches only its own managed provider; Run scope has no provider lifecycle. | PASS |
| AC-001 | Real framework agent calls to local models and real SDK/server/readback assertions; both language surfaces. | PASS |
| AC-003 | Package checker now compiles clean consumer declarations without optional peers and with `skipLibCheck: false`; selector/package/build ownership inspected. Execution remains root-owned. | PASS |
| AC-007 | Real dependency ResponseSpanData direct/fallback/zero usage, callback hierarchy/timing/scope/error tests, optional import cases and Python concurrent callbacks. | PASS |
| AC-008 | Setup repeat/overlap/refusal/loser cleanup tests; owned resource/sampler tests and real owned-provider export/readback. | PASS |
| AC-009 | Actual managed journey pauses after live state flush for readback, writes again on the same native state, retains a held span, and exports after state shutdown. Deterministic held stock exporter proves bounded wait/native drain and subsequent current-call flushing. | PASS |
| AC-010 | Actual errors, streamed completion after scope return, nested/concurrent and uncorrelated turns; exact Run/Card and enclosing ancestry; model/tool counts and span-ID uniqueness. | PASS |
| AC-011 | Real always-off and content-sentinel cases plus denied export and foreign-tenant Card rejection. Native content ownership/residuals explicitly disclosed. | PASS |
| AC-012 | Completed recipes align with actual framework construction; all seven TS OTEL module prerequisites now supplied. Existing generated docs pipeline remains owner. | PASS |
| AC-013 | Gateway positive/negative and current-token unit proof use existing shared auth. Python child emits the actual public exception code and re-raises; parent compares the exact JSON code. | PASS |

## Prior finding closure from current source

| Stable finding | Source-backed closure |
|---|---|
| FIND-TASK-001-1 | Root TS bridge declaration uses local consumed structural types; the clean-package checker compiles generic root APIs/processor without Agents or OTEL peers. Real installed Agents assignability remains tested. |
| FIND-TASK-001-2 | Observe defines installation, shared tool, actual four-framework construction and readback. TS install includes core plus log/metric exporters imported by `otel.ts`. |
| FIND-TASK-001-3 | Python managed app waits for parent readback after flush, then continues; TS queries before shutdown and continues. Both hold an active span and export again after state shutdown. |
| FIND-TASK-001-4 | `_gen_ai` reads independent `data.usage` with full-response fallback, preserving zero and absent model; actual `ResponseSpanData` test covers withheld response. |
| FIND-TASK-001-5 | `_ManagedTelemetry` wraps only lifecycle waiting, retaining ordinary synchronous correlation and stock export ownership; held-export deadline test exercises real BatchSpanProcessor behavior. |
| FIND-TASK-001-6 | TS `startTelemetry` rechecks the same-state field after its dynamic import await and before synchronous installation/publication. |
| FIND-TASK-001-7 | Python attempt sequence captures call entry, waits for obsolete work and requires a newer covering flush. `_stale_flush` ends a late span after prior drain and asserts it exported before the current call returns, for flush and shutdown. |
| FIND-TASK-001-8 | Native Python setup serializes check/install/publication using a GIL-aware mutex; same-state overlapping test observes exactly one installer and no failures. |
| FIND-TASK-001-9 | Gate `otlp_batch_id:133-154` hashes verified subject and current actor; shared `dispatch_canonical:978-991` supplies the same trusted pair as the Scribe frame. Trace/log/metric methods all use that dispatch, across adapters. Derived-ID test distinguishes direct and two actors with identical content; real journey retains five exact pair/Run sets with each marker once across gRPC/HTTP replay. |
| FIND-TASK-001-10 | Shared settlement regression now documents its actual panic conditions at `state.rs:3953-3963`; assertions unchanged. |
| FIND-TASK-001-11 | Python app `__main__` prints `error.code` then re-raises; parent requires exactly `WYRD_CLIENT_401_NO_CREDENTIALS`, retaining actual isolated gateway/client failure path. |
| FIND-TASK-001-12 | OTLP negative journey selects/extracts `sub_principal_id`, `act_principal_id`, `run_id`; groups by all three and asserts exact per-group marker sets. No compatibility column or swallowed query failure added. |

The publisher correction is at the shared implicit-ID producer. It does not
alter native caller-supplied IDs or Scribe logical managed-column exclusion.
This distinction preserves native first-acceptance semantics while separating
OTLP publishers. It needs no downstream guard or second identity/auth owner.

## Proposed findings

None. No material unmet OTEL obligation or distinct reachable regression was
established from the cumulative candidate. This result does not certify all
unrelated imported-main changes.

## Verification limits and residual behavior

No build, test, install, index update, source change, staging or commit was run
by this reviewer. The orchestrator's completed `verification.md` was inspected:
two exact Rust units pass; five TS telemetry unit tests and the packed clean
consumer pass; eight focused Python runtime cases pass; the real delegated
Rust OTLP journey passes; three managed Python framework cases plus the real
ambient gateway/credential-negative case pass; and the TS managed OpenAI
journey passes. No required selected case was skipped. Owned-provider, broader
edge/privacy matrices and broad static/docs lanes retain committed historical
evidence rather than fresh reruns here. No current peer discovery report was
read. Only this report is retained as review output.

Python stock export refusal may be logged even when its provider flush returns
true; the docs disclose this dependency behavior and actual denied-export
journeys assert no persistence. TS Chat Completions supplies no bridge usage,
so omission is correct. Native framework content controls are not universal
redaction. A timed-out Python flush remains in flight but does not accumulate
parallel worker attempts. R3 changes the derived ID, including for direct
publishers; replay straddling old/new deployed derivations can insert again.
The approved remediation does not introduce an upgrade compatibility fence,
and this review does not infer an unapproved stronger delivery guarantee.

Overall OTEL domain result: **PASS**, with final ledger reconciliation owned by
the orchestrator.
