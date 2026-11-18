# SDK lifecycle and trace-slot ergonomics

Status: finalized

## Problem

A reader who wants an agent's spans checked writes three lifecycle calls, two of
them for reasons they cannot see (`examples/refund-agent/refund_agent.py`):

```python
state.start_telemetry()  # export the agent's spans to Wyrd
state.start_bifrost()    # only because flush() refuses without a writer
...
state.flush()
```

- `flush()` and `shutdown()` drain telemetry *and* the Bifrost writer, and
  `flush()` refuses with `WYRD_SDK_400_BIFROST_NOT_STARTED` when no writer
  started, even when the caller only uses telemetry. Owner:
  `crates/shared/wyrd-client/src/state.rs` `WyrdState::flush` →
  `observe/lifecycle.rs` `BifrostLifecycle::started`.
- `configure()` was modelled on `logfire.configure()` but only fixes Service
  identity and environment. Telemetry is a separate `start_telemetry()`.
- A Verifier whose `inputs` is a Pydantic model / Zod object / schemars struct
  turns every field into a JSON slot (`wyrd-client`
  `authoring/verifier.rs` `InputSlot::fields`), so a model cannot hold a trace
  slot. Readers fall back to `{"reply": str, "trace": InputSlot.trace()}`.

## Desired outcome

The refund example reads as: declare, `configure()`, run the agent, `flush()`,
verify a Pydantic model whose `trace` field is a trace slot. Python, Rust and
TypeScript behave the same.

## Constraints and non-goals

- User decisions: fix `flush` in the SDK; `configure()` installs telemetry by
  default with an opt-out; a model field can declare a trace slot and the
  example uses a Pydantic model.
- Wire contracts unchanged: the marker compiles to the existing `trace` slot.
- No server, schema, migration or auth change.

## Proposed implementation

1. **Drain what was started.** `WyrdState::flush` returns `Ok(())` when no
   writer was started (or a start is in flight); `shutdown` already does. After
   shutdown `flush` still refuses `WYRD_SDK_409_BIFROST_CLOSED`. One change in
   `wyrd-client`; the three SDK wrappers keep settling telemetry first.
2. **`configure()` starts telemetry.** `configure(..., telemetry=True)` fixes
   identity and then installs the providers `start_telemetry()` installs today;
   `start_telemetry()` is removed. An app-owned provider still raises
   `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`, whose remediation stays "pass
   `telemetry=False` and add Wyrd's exporters to your provider".
3. **`Trace` slot marker.** A field typed `wyrd.verify.Trace` (TS
   `verify.Trace(z.string())`, Rust `wyrd_ai::verify::Trace` newtype)
   emits a JSON Schema carrying `"x-wyrd-slot": {"type": "trace"}`.
   `wyrd-client` `InputSlot::json`, which every model, Zod and mapping path
   reaches, replaces a schema carrying the keyword with the canonical slot it
   names. Its value is the trace ID string, so a model instance passed to
   `verify` fills the slot unchanged. Every SDK names it `Trace` (user decision: SDK names stay identical). In TS
   it is a function over the caller's Zod string schema, because TypeScript
   types vanish at runtime and the SDK takes Zod structurally, without a Zod
   dependency.

   Only `trace` gets a marker: `media` values are uploaded `File`/`ArtifactRef`
   objects a model cannot dump, `samples` are server-filled, and `source` is
   an authoring reference. The keyword carries a whole slot, so a later marker
   is one small type.

## Evidence and rationale

- `configure` is local and frozen after first use
  (`state.rs` `WyrdState::configure`; `WYRD_SDK_409_STATE_CONFIG_FROZEN`).
  `start_telemetry` is idempotent; folding it into `configure` makes a second
  `configure()` a refusal, not a no-op.
- Telemetry lives per SDK, not in `wyrd-client`: Python
  `sdks/wyrd-sdk-python/src/state/mod.rs` `start_telemetry` → `wyrd.otel`;
  TS `WyrdState.startTelemetry` → `./otel.js`; Rust
  `sdks/wyrd-sdk-rust/src/otel.rs` free fn `start_telemetry(&state)` returning
  a `Telemetry` handle over a global `tracing` subscriber.

## Decisions and alternatives

- **Writes start the writer on first use (user decision, revised).** `verify`
  and `submit` must never require `start_bifrost()`. The earlier objection
  applied to synchronous enqueue only. `submit`, `run.bifrost.write`/`write_to`
  and `use_table_by_name` are already asynchronous (blocking in Python), so
  the first one starts the writer with default options before recording;
  once running, samples still enqueue synchronously. A state-wide start gate
  makes concurrent first writes start it once and makes an explicit
  `start_bifrost()` in flight awaited, not refused. `start_bifrost()` remains
  for writer options and fail-fast startup. Offline, the first write reports
  the connect error (`WYRD_CLIENT_503_TRANSPORT_DOWN`) instead of
  `WYRD_SDK_400_BIFROST_NOT_STARTED`, which no public call can raise and is
  deleted from the catalog.

- **OTel stays optional.** `configure()` follows the ASGI middleware's
  existing contract (`docs/.../connect/integrate-asgi.md` If it fails): default
  on, `ImportError` naming the `otel` extra when absent, `telemetry=False` to
  opt out. No dependency moves from optional to required.
- **A failed install leaves configuration fixed.** Identity is fixed before
  the providers install, because the Resource needs the environment. Both
  failures (`PROVIDER_EXISTS`, missing extra) are fixed in code and a restart,
  so in-process retry is not designed for.
- **TypeScript `configure()` becomes async.** Telemetry needs a dynamic import
  of the optional peers, so `await state.configure()`.
- **Rust keeps `wyrd_ai::otel::start_telemetry(&state)`.** `configure` is a
  `wyrd-client` method and `wyrd-client` must not depend on OpenTelemetry;
  an extension trait would be a single-implementation trait. Rust's handle
  return is also the idiom (`logfire::configure().finish()` returns one).
- **ASGI middleware** keeps its `telemetry` option and calls the state's
  internal installer, so it works whether or not the app called `configure()`.

## Standards conflicts

None yet.

## Open questions

None material. Resolved by evidence and recorded under decisions.

## Handoff

User asked to implement directly from this design.
