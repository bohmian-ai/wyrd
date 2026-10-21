"""The pure ASGI middleware that runs one ``WyrdState`` inside an ASGI application."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable, MutableMapping
from typing import Any

from ...errors import WyrdError
from ...state import WyrdState

Scope = MutableMapping[str, Any]
Message = MutableMapping[str, Any]
Receive = Callable[[], Awaitable[Message]]
Send = Callable[[Message], Awaitable[None]]
ASGIApp = Callable[[Scope, Receive, Send], Awaitable[None]]

RUN_SCOPE_KEY = "wyrd.run"
"""The ASGI scope key holding the request's ``Run``."""

_LOG = logging.getLogger("wyrd.integrations.asgi")


def _diagnostic(error: Exception) -> str:
    """One line naming ``error``: its Wyrd code and message, else its type and message."""
    if isinstance(error, WyrdError):
        return f"{error.code}: {error.message}"
    return f"{type(error).__name__}: {error}"


class WyrdMiddleware:
    """Start a loaded ``WyrdState`` with the application and give each request its own Run.

    Add it as ``app.add_middleware(WyrdMiddleware, state=state)`` in FastAPI or
    Starlette, or wrap any ASGI application as ``WyrdMiddleware(app,
    state=state)``. Lifespan startup starts the state's Bifrost writer and,
    unless ``telemetry=False``, its telemetry; lifespan shutdown drains the
    state after the application's own shutdown. Each ``http`` and
    ``websocket`` call runs inside ``with state.run(alias)``, and the Run is
    at ``scope["wyrd.run"]`` for the application. Other scope types pass
    through. The middleware creates no spans.

    Lifespan must be enabled on the server: without it nothing starts and
    nothing drains. Under Gunicorn each worker runs its own lifespan, so each
    worker owns the resources it starts. A hard kill, a timed-out shutdown,
    or a failed drain can lose queued rows.
    """

    def __init__(
        self,
        app: ASGIApp,
        *,
        state: WyrdState,
        telemetry: bool = True,
        alias: Callable[[Scope], str | None] | None = None,
    ) -> None:
        """Wrap ``app`` with ``state``'s lifecycle.

        Args:
            app: The ASGI application to wrap.
            state: An already loaded state; the middleware never declares or
                registers Cards.
            telemetry: Whether lifespan startup calls ``state.start_telemetry()``.
                ``False`` leaves the tracer provider, its installation, and its
                flushing to the application.
            alias: Given the ASGI scope, returns the Card alias the request's
                Run observes, or ``None`` for the root Service Card.
        """
        self.app = app
        self.state = state
        self.telemetry = telemetry
        self.alias = alias

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        """Serve one ASGI call.

        Args:
            scope: The ASGI connection scope.
            receive: The ASGI receive channel.
            send: The ASGI send channel.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` before the application
                runs when ``alias`` returns an alias this bundle lacks.
        """
        if scope["type"] == "lifespan":
            await self._lifespan(scope, receive, send)
        elif scope["type"] in ("http", "websocket"):
            run = self.state.run(self.alias(scope) if self.alias else None)
            with run:
                await self.app({**scope, RUN_SCOPE_KEY: run}, receive, send)
        else:
            await self.app(scope, receive, send)

    def _start(self) -> None:
        """Start the writer, then telemetry; on a telemetry failure, close the writer and re-raise."""
        self.state.start_bifrost()
        if not self.telemetry:
            return
        try:
            self.state.start_telemetry()
        except Exception:
            try:
                self.state.shutdown()
            except Exception as cleanup:
                _LOG.warning("Wyrd cleanup after failed startup failed: %s", _diagnostic(cleanup))
            raise

    async def _drain(self) -> str | None:
        """Shut the state down off the event loop; return the failure diagnostic, if any."""
        try:
            await asyncio.to_thread(self.state.shutdown)
        except Exception as error:
            _LOG.error("Wyrd shutdown failed: %s", _diagnostic(error))
            return _diagnostic(error)
        return None

    async def _lifespan(self, scope: Scope, receive: Receive, send: Send) -> None:
        """Run Wyrd startup and drain around the application's own lifespan."""
        startup = await receive()
        try:
            await asyncio.to_thread(self._start)
        except Exception as error:
            await send({"type": "lifespan.startup.failed", "message": _diagnostic(error)})
            return
        pending: list[Message] = [startup]
        replied = False

        async def app_receive() -> Message:
            return pending.pop() if pending else await receive()

        async def app_send(message: Message) -> None:
            nonlocal replied
            replied = True
            kind = message["type"]
            if kind == "lifespan.startup.failed" or kind.startswith("lifespan.shutdown."):
                failure = await self._drain()
                if failure and kind == "lifespan.shutdown.complete":
                    message = {"type": "lifespan.shutdown.failed", "message": failure}
            await send(message)

        try:
            await self.app(scope, app_receive, app_send)
        except Exception as error:
            if replied:
                raise
            if not pending:
                # The application read startup, then failed without replying.
                await self._drain()
                await send({"type": "lifespan.startup.failed", "message": _diagnostic(error)})
                return
            # An application without lifespan support raises before reading
            # startup; Wyrd still owns startup and drain.
            await send({"type": "lifespan.startup.complete"})
            await receive()
            failure = await self._drain()
            await send(
                {"type": "lifespan.shutdown.failed", "message": failure}
                if failure
                else {"type": "lifespan.shutdown.complete"}
            )
