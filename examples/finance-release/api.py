"""A finance API that verifies each report release before it sends the report.

It serves the Quickstart's ``finance-release`` Service, pulled once at deploy
time:

    wyrd pull --kind Service --name finance-release --version 0.1.0 \
      --output-dir .wyrd-bundle
    uv run uvicorn api:api
"""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Annotated

from fastapi import Depends, FastAPI, Request
from fastapi.responses import JSONResponse
# ANCHOR: import
from wyrd import WyrdState
# ANCHOR_END: import
from wyrd import WyrdError
from wyrd.verify import VerificationResult

from finance_service import Release


@asynccontextmanager
async def lifespan(_: FastAPI) -> AsyncIterator[dict]:
    # Each server process loads the Service once, at startup.
    state = WyrdState.from_path(".wyrd-bundle")
    yield {"wyrd": state}
    # Drain what the state recorded before the process exits.
    state.shutdown()


api = FastAPI(lifespan=lifespan)


def wyrd_state(request: Request) -> WyrdState:
    return request.state.wyrd


# ANCHOR: handler
Wyrd = Annotated[WyrdState, Depends(wyrd_state)]


@api.post("/reports/send")
def send_report(release: Release, state: Wyrd):
    with state.run() as run:
        result = run.observe.verify("release-check", release)
    if result.passed:
        return release_report(release, result)
    return hold_for_review(release, result)
# ANCHOR_END: handler


# ANCHOR: decisions
def release_report(release: Release, result: VerificationResult) -> dict:
    # A real API sends the report to its destination here, only after the verdict.
    return {"decision": "released", "run_id": result.run_id, "execution_id": result.execution_id}


def hold_for_review(release: Release, result: VerificationResult) -> dict:
    # Failed and inconclusive verdicts both wait for a person, who sees why.
    return {
        "decision": "held for review",
        "verdict": result.verdict,
        "run_id": result.run_id,
        "execution_id": result.execution_id,
        "requirements": {r["id"]: r["outcome"] for r in result.requirements if r["outcome"] != "passed"},
    }


@api.exception_handler(WyrdError)
def hold_on_error(request: Request, error: WyrdError) -> JSONResponse:
    # No verdict, such as a refused credential or a timeout: nothing was judged, so nothing is sent.
    return JSONResponse({"decision": "held for review", "error": error.code}, status_code=503)
# ANCHOR_END: decisions
