"""A FastAPI app that verifies each support answer it receives under its own Run.

It serves the Quickstart's ``support-reply`` Service, pulled once at deploy time:

    wyrd pull --kind Service --name support-reply --version 1.0.0 \
      --output-dir .wyrd-bundle
    uv run uvicorn app:app
"""

from __future__ import annotations

import asyncio

from fastapi import FastAPI, Request
from pydantic import BaseModel
from wyrd.integrations.asgi import RUN_SCOPE_KEY, WyrdMiddleware
from wyrd.state import WyrdState

# ANCHOR: setup
# Load the pulled Service; this reads local files only.
state = WyrdState.from_path(".wyrd-bundle")

app = FastAPI()
# Start the state with the app, drain it at shutdown, and open one Run per request.
app.add_middleware(WyrdMiddleware, state=state)
# ANCHOR_END: setup


class Reply(BaseModel):
    answer: str


# ANCHOR: route
@app.post("/replies")
async def check_reply(reply: Reply, request: Request) -> dict[str, str]:
    # This request's Run, opened by the middleware.
    run = request.scope[RUN_SCOPE_KEY]
    # Verification waits for the verdict, so it runs off the event loop.
    result = await asyncio.to_thread(run.observe.verify, "no-refund-promise", answer=reply.answer)
    return {"run_id": run.run_id, "verdict": result.verdict}
# ANCHOR_END: route
