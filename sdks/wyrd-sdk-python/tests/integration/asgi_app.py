"""The application ``test_asgi_worker_lifecycle`` serves with Gunicorn's Uvicorn workers.

Each worker imports this module after the fork, so each loads its own state
from the downloaded ``observed-service`` bundle and starts it in its own
lifespan. Every request submits one drift sample for the ``model`` Card and
answers with the worker's process id and the request's Run.
"""

from __future__ import annotations

import os
from pathlib import Path

from fastapi import FastAPI, Request
from wyrd.client import WyrdClient
from wyrd.integrations.asgi import RUN_SCOPE_KEY, WyrdMiddleware

from .support import hydrated

state = hydrated(Path(os.environ["WYRD_TEST_BUNDLE"]), WyrdClient())
app = FastAPI()
app.add_middleware(WyrdMiddleware, state=state, telemetry=False, alias=lambda scope: "model")


@app.get("/observe/{latency}")
def observe(latency: float, request: Request) -> dict[str, object]:
    run = request.scope[RUN_SCOPE_KEY]
    run.observe.submit("observed-drift", {"latency": latency})
    return {"pid": os.getpid(), "run_id": run.run_id}
