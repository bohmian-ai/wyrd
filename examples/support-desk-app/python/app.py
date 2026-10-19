"""The support API.

- ``POST /ask`` answers a question and runs the two model-free checks before
  replying.
- ``POST /reviews`` has the agent write a refund decision letter and queues
  the model-judged ``follows-policy`` check on it in the background.
- ``GET /reviews/{execution_id}`` reads a queued check's status and, once it
  is done, its requirement breakdown.

Run it with ``uv run uvicorn app:app``.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from wyrd import WyrdError, verify

from desk import Desk


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncGenerator[None]:
    # ANCHOR: lifespan
    app.state.desk = Desk()
    yield
    # Flush buffered tickets and spans to Wyrd before exiting.
    app.state.desk.state.shutdown()
    # ANCHOR_END: lifespan


app = FastAPI(title="support-desk", lifespan=lifespan)

# What `POST /reviews` asks the agent for: a document, not a chat reply.
LETTER = "Write the customer a refund decision letter in Markdown. Their request: {question}"


class Question(BaseModel):
    question: str
    order_id: str | None = None


@app.exception_handler(WyrdError)
async def wyrd_error(request: Request, error: WyrdError) -> JSONResponse:
    """Reply with Wyrd's own status and stable error code."""
    return JSONResponse({"code": error.code, "message": str(error)}, status_code=error.status)


@app.post("/ask")
async def ask(body: Question, request: Request) -> dict[str, Any]:
    desk: Desk = request.app.state.desk
    run, answer = await desk.answer(body.question)
    ticket = {"question": body.question, "answer": answer, "order_id": body.order_id}
    # Verification blocks until the verdicts are in, so it runs off the event loop.
    results = await asyncio.to_thread(desk.check, run, ticket)
    checks = {
        name: {"verdict": result.verdict, "requirements": result.requirements}
        for name, result in results.items()
    }
    return {"run_id": run.run_id, "answer": answer, "checks": checks}


@app.post("/reviews")
async def review(body: Question, request: Request) -> dict[str, Any]:
    desk: Desk = request.app.state.desk
    run, letter = await desk.answer(LETTER.format(question=body.question))
    execution = await asyncio.to_thread(desk.submit, run, letter)
    return {"run_id": run.run_id, "letter": letter, "execution_id": execution.id}


@app.get("/reviews/{execution_id}")
def review_status(execution_id: str, request: Request) -> dict[str, Any]:
    # ANCHOR: status
    # Any process can pick a check up from its execution ID alone.
    state = verify.Execution.connect(execution_id, request.app.state.desk.client).status()
    # ANCHOR_END: status
    reply: dict[str, Any] = {"execution_id": execution_id, "status": state["status"]}
    if "result" in state:
        outcome = state["result"]["outcome"]
        reply |= {"verdict": outcome["verdict"], "requirements": outcome["requirements"]}
    if "error" in state:
        reply["error"] = state["error"]
    return reply
