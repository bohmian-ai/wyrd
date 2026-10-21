---
title: Integrate Wyrd with FastAPI
description: Start a pulled Service with your FastAPI app, verify inside one Run per request, and drain queued evidence at shutdown, including under Gunicorn.
pillar: wyrd
section: Connect
order: 4
status: runnable
---

Add `WyrdMiddleware` to a FastAPI app. It starts your Service's state when the app starts, opens one Run for each request, and drains queued evidence when the app stops.

## What you capture

- One Run per HTTP request or WebSocket connection. Every verification and observation the request makes shares that Run's `run_id`.
- Spans your OpenTelemetry instrumentation records inside the Run, tagged with its `wyrd.run_id` and `wyrd.card_uid`.

## Before you start

- The server from [Set up Wyrd](/get-started/setup/), running.
- The `support-reply` Service from [Run your first verification](/get-started/quickstart/), declared.

## Install

```bash
uv add "wyrd-ai[otel]==0.3.0" fastapi uvicorn
```

Pull the Service into the app's directory. Do this at build or deploy time, not when the app starts:

```bash
uv run wyrd pull --space default --kind Service --name support-reply --version 1.0.0 --output-dir .wyrd-bundle
```

## Usage

Save the app as `app.py`:

<CodeFromFile file="examples/integrate-fastapi/app.py" lang="python" title="app.py" />

Run it:

```bash
uv run uvicorn app:app
```

At startup the middleware starts the state's writer and telemetry before the app starts. At shutdown it waits for the app's own shutdown, then drains, so evidence the app records while stopping is kept. Each request's Run is at `request.scope["wyrd.run"]`.

## Verify it worked

In a second terminal, send an answer that breaks the requirement:

```bash
curl -s localhost:8000/replies -H 'content-type: application/json' -d '{"answer": "You have a guaranteed refund."}'
```

The reply names the request's Run and a `failed` verdict. A second request returns a different `run_id`.

## If it fails

| You see | Fix |
| --- | --- |
| Startup fails with `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | Your app installs its own tracer provider: pass `telemetry=False` and add `wyrd.otel.span_exporter(client)` to that provider |
| Startup fails with an `ImportError` naming `otel` | Install `wyrd-ai[otel]`, or pass `telemetry=False` |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` when the app records | The server runs without lifespan: leave Uvicorn's `--lifespan` at `auto` or set it to `on` |
| `WYRD_SDK_404_UNKNOWN_ALIAS` on every request | `alias` returned a name the bundle does not have |

## Advanced

### Choose the Card per request

By default each Run observes the root Service. Pass `alias`, a function that receives the ASGI scope and returns a Card alias from the bundle, or `None` for the Service. An alias the bundle lacks fails the request before your handler runs.

### Your own OpenTelemetry instrumentation

Add `WyrdMiddleware` after OpenTelemetry's ASGI or FastAPI instrumentation, so Wyrd's middleware is outermost and the instrumentation's spans start inside the Run. The middleware creates no spans of its own. If your app owns its tracer provider, pass `telemetry=False`: the app installs, exports, and flushes that provider, and calls `wyrd.otel.install_run_correlation(provider)` once to tag spans with the Run.

### Gunicorn

Run Gunicorn with Uvicorn workers:

```bash
uv add gunicorn
uv run gunicorn app:app --workers 2 --worker-class uvicorn.workers.UvicornWorker
```

Each worker runs the app's lifespan on its own, so each worker starts and drains its own writer and telemetry. Lifespan must stay enabled; without it nothing starts and nothing drains.

A graceful stop (`SIGTERM`) drains every worker. A worker killed with `SIGKILL`, one that exceeds `--graceful-timeout`, or a failed drain can lose queued rows. A failed drain is logged with its Wyrd error code and reported to the server as a failed shutdown.

## Reference

`WyrdMiddleware(app, *, state, telemetry=True, alias=None)`, from `wyrd.integrations.asgi`:

| Argument | Meaning |
| --- | --- |
| `app` | The ASGI app to wrap; `add_middleware` passes it. |
| `state` | A loaded `WyrdState`. The middleware never declares or registers Cards. |
| `telemetry` | Whether startup calls `state.start_telemetry()`. Default `True`. |
| `alias` | Function from the ASGI scope to the Card alias each Run observes. Default: the root Service. |

Startup or shutdown failures are reported to the server with the Wyrd error code. Other ASGI connection types pass through unchanged.

## Next

[Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/) explains what a Run records and how long it is kept.
