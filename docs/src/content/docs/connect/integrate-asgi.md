---
title: ASGI apps
description: Add WyrdMiddleware to a FastAPI, Starlette, Django, or Litestar app to start Wyrd with the app, verify inside one Run per request, and drain queued evidence at shutdown.
pillar: wyrd
section: Connect
order: 4
status: runnable
---

Add `WyrdMiddleware` to any ASGI app, such as FastAPI, Starlette, Django, or Litestar. It starts Wyrd when your app starts, wraps each request in its own Run, and sends queued evidence when your app stops.

## What you capture

- One Run per HTTP request or WebSocket connection. Every verdict and row the request produces shares that Run's `run_id`.
- Spans your OpenTelemetry instrumentation records inside the Run, tagged with its `wyrd.run_id` and `wyrd.card_uid`.

## Before you start

- The server from [Set up Wyrd](/get-started/setup/), running.
- The `support-reply` Service from [Run your first verification](/get-started/quickstart/), declared.

## Install

```bash
uv add "wyrd-ai[otel]==0.3.0" uvicorn
```

Add your framework too, such as `uv add fastapi`. Then pull the Service into the app's directory, at build or deploy time rather than when the app starts:

```bash
uv run wyrd pull --space default --kind Service --name support-reply --version 1.0.0 --output-dir .wyrd-bundle
```

## Usage

Each app loads the pulled Service, adds the middleware, and reads the request's Run in its handler:

<Tabs>
<Tab label="FastAPI">

<CodeFromFile file="examples/integrate-fastapi/app.py" lang="python" title="app.py" />

```bash
uv run uvicorn app:app
```

</Tab>
<Tab label="Starlette">

```python title="app.py"
import asyncio

from starlette.applications import Starlette
from starlette.middleware import Middleware
from starlette.responses import JSONResponse
from starlette.routing import Route
from wyrd.integrations.asgi import RUN_SCOPE_KEY, WyrdMiddleware
from wyrd.state import WyrdState

state = WyrdState.from_path(".wyrd-bundle")  # reads local files only


async def check_reply(request):
    run = request.scope[RUN_SCOPE_KEY]  # this request's Run, opened by the middleware
    answer = (await request.json())["answer"]
    # verify waits for the verdict, so it runs off the event loop
    result = await asyncio.to_thread(run.observe.verify, "no-refund-promise", answer=answer)
    return JSONResponse({"run_id": run.run_id, "verdict": result.verdict})


app = Starlette(
    routes=[Route("/replies", check_reply, methods=["POST"])],
    middleware=[Middleware(WyrdMiddleware, state=state)],
)
```

```bash
uv run uvicorn app:app
```

</Tab>
<Tab label="Django">

Wrap the ASGI application in `asgi.py`:

```python title="mysite/asgi.py"
import os

from django.core.asgi import get_asgi_application
from wyrd.integrations.asgi import WyrdMiddleware
from wyrd.state import WyrdState

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")
application = WyrdMiddleware(get_asgi_application(), state=WyrdState.from_path(".wyrd-bundle"))
```

Then read the Run in a view:

```python title="replies/views.py"
import json

from django.http import JsonResponse
from django.views.decorators.csrf import csrf_exempt
from wyrd.integrations.asgi import RUN_SCOPE_KEY


@csrf_exempt  # so the curl below can post without a CSRF token
def check_reply(request):
    run = request.scope[RUN_SCOPE_KEY]  # this request's Run, opened by the middleware
    answer = json.loads(request.body)["answer"]
    result = run.observe.verify("no-refund-promise", answer=answer)
    return JsonResponse({"run_id": run.run_id, "verdict": result.verdict})
```

Add `path("replies", check_reply)` to `urlpatterns` in `urls.py`, then serve it:

```bash
uv run uvicorn mysite.asgi:application
```

</Tab>
<Tab label="Litestar">

```python title="app.py"
import asyncio

from litestar import Litestar, Request, post
from wyrd.integrations.asgi import RUN_SCOPE_KEY, WyrdMiddleware
from wyrd.state import WyrdState

state = WyrdState.from_path(".wyrd-bundle")  # reads local files only


@post("/replies")
async def check_reply(request: Request, data: dict[str, str]) -> dict[str, str]:
    run = request.scope[RUN_SCOPE_KEY]  # this request's Run, opened by the middleware
    # verify waits for the verdict, so it runs off the event loop
    result = await asyncio.to_thread(run.observe.verify, "no-refund-promise", answer=data["answer"])
    return {"run_id": run.run_id, "verdict": result.verdict}


app = WyrdMiddleware(Litestar([check_reply]), state=state)  # wraps any ASGI app
```

```bash
uv run uvicorn app:app
```

</Tab>
</Tabs>

What the middleware does:

| When | It |
| --- | --- |
| The app starts | Starts Wyrd's writer and telemetry, before your app's own startup. If either fails, the app does not start. |
| A request arrives | Opens a Run of the Service, puts it at `request.scope["wyrd.run"]`, and runs your handler inside it. |
| The app stops | Waits for your app's own shutdown, then sends every queued row and span. |

The middleware verifies nothing by itself. Your handler decides what to verify and records it on the request's Run.

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

### Choose the component per request

By default each Run observes the Service. To observe one of its components instead, pass `alias`, a function from the request's ASGI scope to a component alias, or `None` for the Service:

```python
def by_path(scope):
    return "agent" if scope["path"].startswith("/agent/") else None


app.add_middleware(WyrdMiddleware, state=state, alias=by_path)
```

An alias the bundle lacks fails the request before your handler runs.

### WebSockets

A WebSocket connection is one Run for its whole life, so every message on it shares one `run_id`. To judge each turn of a conversation separately and the conversation as a whole, pass a `group_id` to `verify`, as in the [grouped recipe](/build-and-verify/verify/verifier-recipes/#grouped-session).

### Your own OpenTelemetry instrumentation

Add `WyrdMiddleware` after OpenTelemetry's ASGI or framework instrumentation, so Wyrd's middleware is outermost and the instrumentation's spans start inside the Run. The middleware creates no spans of its own. If your app owns its tracer provider, pass `telemetry=False`: the app installs, exports, and flushes that provider, and calls `wyrd.otel.install_run_correlation(provider)` once to tag spans with the Run.

### Gunicorn

Run Gunicorn with Uvicorn workers:

```bash
uv add gunicorn
uv run gunicorn app:app --workers 2 --worker-class uvicorn.workers.UvicornWorker
```

Each worker starts and drains its own writer. Lifespan must stay enabled; without it nothing starts and nothing drains.

A graceful stop (`SIGTERM`) drains every worker. A worker killed with `SIGKILL`, one that exceeds `--graceful-timeout`, or a failed drain can lose queued rows.

## Reference

`WyrdMiddleware(app, *, state, telemetry=True, alias=None)`, from `wyrd.integrations.asgi`:

| Argument | Meaning |
| --- | --- |
| `app` | The ASGI app to wrap; `add_middleware` passes it. |
| `state` | A loaded `WyrdState`. The middleware never declares or registers anything. |
| `telemetry` | Whether startup calls `state.start_telemetry()`. Default `True`. |
| `alias` | Function from the ASGI scope to the component alias each Run observes. Default: the Service. |

`RUN_SCOPE_KEY` is `"wyrd.run"`, the scope key that holds the request's Run. Other ASGI connection types pass through unchanged.

## Next

[Runs and evidence](/understand/runs-observations-and-evidence/) explains what a Run records and how long it is kept.
