"""A finance API that verifies each report release before it sends the report.

It serves the Quickstart's ``finance-release`` Service, pulled once at deploy
time:

    wyrd pull --kind Service --name finance-release --version 0.1.0 \
      --output-dir .wyrd-bundle
    uv run uvicorn api:api
"""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from fastapi import FastAPI
from pydantic import BaseModel
# ANCHOR: import
from wyrd import WyrdState
# ANCHOR_END: import


class Release(BaseModel):
    report: dict[str, object]
    recipient: dict[str, object]
    destination: dict[str, object]


@asynccontextmanager
async def lifespan(_: FastAPI) -> AsyncIterator[None]:
    yield
    # Drain what the state recorded before the process exits.
    state.shutdown()


api = FastAPI(lifespan=lifespan)

# ANCHOR: handler
state = WyrdState.from_path(".wyrd-bundle")


@api.post("/reports/send")
def send_report(release: Release):
    with state.run() as run:
        r = run.observe.verify("release-check", release)
    if r.passed:
        return release_report(release)
    return hold_for_review(release)
# ANCHOR_END: handler


def release_report(release: Release) -> dict[str, str]:
    # A real API sends the report to its destination here.
    return {"decision": "released"}


def hold_for_review(release: Release) -> dict[str, str]:
    # Failed and inconclusive verdicts both wait for a person.
    return {"decision": "held for review"}
