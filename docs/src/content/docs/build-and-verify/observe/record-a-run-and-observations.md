---
title: Record a run and observations
description: Pull your Service's bundle, record rows inside a run, verify or submit them to a Verifier, and read them back by run ID.
pillar: wyrd
section: Build and verify
topic: Observe
order: 1
status: runnable
---

Pull and load the exact Service version you deployed, wrap each request in a run, record what each component saw and did, and hand it to a Verifier with `verify` or `submit`. Verifiers judge these runs, and every row points back to the Card version and run that produced it. This is the path for application code; to load or query tables directly, use the [Bifrost client](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/).

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/) explains the model behind these calls.

## Prerequisites

- The declared `support-desk` Service from [Verify your first agent](/get-started/quickstart/). Its Cards live in `examples/support-desk-app/service/`: the Service declares a `tickets` table with `question` and `answer` columns, binds `no-refund-promise` (one `answer` input) to itself, and binds `ticket-complete` (one `ticket` input) and `follows-policy` (one `letter` file) to its `agent` component.
- A credential bound to that Service version, so the server can attribute what it records. Against the local server from [Set up Wyrd](/get-started/setup/), the saved settings work as they are, as in the quickstart. Elsewhere, a tenant administrator issues one:

  ```bash
  wyrd auth issue-key --kind Service --name support-desk --version 1.0.0 --space default
  ```

  The key prints once. Give it to the service as `WYRD_API_KEY`, alongside `WYRD_SERVER_URL`.
- The SDK for your language: `pip install "wyrd-ai[otel]"`, `npm install wyrd-ai`, or the `wyrd-ai` crate with its `otel` feature.

## 1. Pull the bundle

`wyrd pull` writes the Service and every Card it references, including model weights and datasets, into the directory you name. Do this at build or deploy time, not per request:

```bash
wyrd pull --space default --kind Service --name support-desk --version 1.0.0 \
  --output-dir ./.wyrd-bundle
```

Pull replaces an existing bundle only once the new one is complete, so a failed pull leaves the previous bundle usable. `wyrd get` with the same selector prints the Service's envelope only and writes nothing. The example apps pull from code instead, right after declaring; see step 4 of the [quickstart](/get-started/quickstart/#4-deploy-the-cards).

## 2. Load it and start Bifrost and telemetry

Once, at startup:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="init" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="init" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="startup" title="server.ts" />

</LangTab>
</LangTabs>

`from_path` reads local files only. `start_bifrost` is the first server call: it connects the writer and fails at startup if the server is unreachable or your credential cannot write observations. Telemetry exports spans started inside a run with that run's ID. In Python and TypeScript it is `state.start_telemetry()` (`startTelemetry`). In Rust it is `wyrd_ai::otel::start_telemetry(&state)`, which returns a `Telemetry` handle you keep until shutdown; the Rust app starts it when it begins serving, in step 4.

## 3. Record, verify, and submit

Open a run, then call `observe` on it. Everything in one run shares a `run_id`. The app answers a question in a new run of the Service and records the ticket in the `tickets` table:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="answer" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="answer" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="answer" title="server.ts" />

</LangTab>
</LangTabs>

Then it runs the two model-free Verifiers and waits for their verdicts. `no-refund-promise` is bound to the Service, so it runs on the run itself. `ticket-complete` is bound to the agent, so it runs on `for_card("agent")`, the run's agent view, which shares the same `run_id`:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="check" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="check" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="check" title="server.ts" />

</LangTab>
</LangTabs>

`follows-policy` calls a model judge, so the app queues it on the agent's letter instead of waiting:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" lang="python" region="submit" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="submit" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="submit" title="server.ts" />

</LangTab>
</LangTabs>

| Call | Does | Returns |
| --- | --- | --- |
| `record(table, row)` | Queues one row for a `vala.datasets` table | Nothing; the row is stored at the next flush |
| `verify(name, inputs)` | Runs a Verifier and waits for it | The verdict (`passed`, `failed`, or `inconclusive`) and one result per requirement |
| `submit(name, inputs)` | Queues a Verifier on the server | An execution whose `id` reads the result later |

`verify` and `submit` take a Verifier bound to the Card of the run or view you call them on, and one input per slot that Verifier declares: `answer` for `no-refund-promise`, `ticket` for `ticket-complete`, and the `letter` file for `follows-policy`. `verify.File` fills a file slot. A failed verdict is a normal return, not an error.

Read a submitted result back from any process, by its execution ID alone:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/app.py" lang="python" region="status" title="app.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="status" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="status" title="server.ts" />

</LangTab>
</LangTabs>

`status()` returns the execution's state without waiting; `wait` on the same execution blocks until it is final, up to the timeout you pass.

## 4. Shut down cleanly

Queued rows and spans are not stored yet. Shut down once when the process stops:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/app.py" lang="python" region="lifespan" title="app.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" lang="rust" region="serve" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" lang="typescript" region="shutdown" title="server.ts" />

</LangTab>
</LangTabs>

`shutdown` sends everything still queued and returns once the server has it. A process killed without it can lose its last rows and spans. `flush` does the same without closing the writer, for tests and batch jobs. In Python and TypeScript, both first flush the tracer provider `start_telemetry` installed, and still drain the writer if that flush fails. In Rust, `WyrdState::shutdown` drains only the writer: call `shutdown()` on the `Telemetry` handle first, as above.

## 5. Read a run back

Every row a run produced carries its `run_id`, so one query joins the ticket to the request span, the gateway call, and each verdict:

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/evidence.py" lang="python" region="sql" title="evidence.py" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/evidence.ts" lang="typescript" title="evidence.ts" />

</LangTab>
</LangTabs>

The Rust app runs the same join in its `evidence` function. `Bifrost().trace_timeline(trace_id)` (`traceTimeline` in TypeScript) returns one trace's spans with their parent ids, status, and attributes.

Rows become queryable a short time after `flush` or `shutdown` returns. An empty result right after a flush is not proof they are missing; retry first.

## What can go wrong

| Error code | When | Cause | Fix |
| --- | --- | --- | --- |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` | At the call | `observe` before `start_bifrost` | Start Bifrost at startup |
| `WYRD_SDK_409_BIFROST_CLOSED` | At the call | `observe` after `shutdown` | Shut down only at process exit |
| `WYRD_SDK_404_UNKNOWN_ALIAS` | At the call | `run` or `for_card` with an alias not in the bundle | Use an alias from the Service's `components` |
| `WYRD_SDK_400_INVALID_OBSERVATION` | At the call | Empty or nested drift features, or `record` to a table outside `vala.datasets` | Send flat values; record only to your own tables |
| `WYRD_CLIENT_429_QUEUE_FULL` | At the call | The server is unreachable or slow and the queue filled | Decide whether to drop or retry. The SDK never blocks your request |
| `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` | At `record` | The table was never registered | Declare it in `spec.tables` or register it |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` | At `flush` or `shutdown` | The credential is bound to a different Card than the one recorded about | Use the key issued for this Service version |
| `WYRD_PERMISSION_403_DENIED_RBAC` | At `flush` or `shutdown` | The credential lacks `bifrost_record:write` | Use a `workload` credential |
| `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | At `start_telemetry` | The application already installed a tracer provider | Don't call `start_telemetry`; keep your own provider |
| `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` | At `flush` or `shutdown` | The tracer provider `start_telemetry` installed did not flush in time | Check that the server is reachable. The writer still drained; `details.native_code` names its failure, if any |
| `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` | At `flush` or `shutdown` | That provider failed to flush. In TypeScript this includes a refused span export | Check that the credential may write telemetry. `details.native_code` names any writer failure |

Card-scope and permission refusals arrive at `flush` or `shutdown`, not at the `observe` call, because the server checks them when the batch lands. The Python OpenTelemetry SDK logs a refused span export rather than failing its flush, so look for it in the log. Call `flush` in a smoke test after deploying so a wrong key fails there instead of silently at exit.
