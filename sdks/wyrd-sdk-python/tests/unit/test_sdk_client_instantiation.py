"""Every connected handle shares one construction vocabulary.

Constructors retain their arguments without IO; ``connect`` connects now; the
first server operation of a constructed handle connects exactly as ``connect``.
"""

import asyncio
import importlib
from collections.abc import Callable
from typing import Any

import pytest
from wyrd import WyrdError
from wyrd.auth import Principals
from wyrd.bifrost import AsyncBifrost, Bifrost
from wyrd.cards import Cards
from wyrd.gateway import Gateway
from wyrd.operators import OperatorConnections

HANDLES = [Bifrost, AsyncBifrost, Cards, Gateway, Principals, OperatorConnections]

FIRST_USE: list[tuple[type, Callable[[Any], object]]] = [
    (Bifrost, lambda handle: handle.producer_count),
    (Cards, lambda handle: handle.data),
    (Gateway, lambda handle: handle.credentials()),
    (Principals, lambda handle: handle.list()),
    (OperatorConnections, lambda handle: handle.list()),
]


@pytest.mark.parametrize("handle", HANDLES)
def test_handles_expose_only_the_common_construction_forms(handle: Any) -> None:
    assert callable(handle.connect)
    for removed in ("from_env", "with_client", "connect_with", "connect_with_config"):
        assert not hasattr(handle, removed)
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module("wyrd.principals")


@pytest.mark.usefixtures("no_credentials")
@pytest.mark.parametrize("handle", [h for h in HANDLES if h is not AsyncBifrost])
def test_connect_is_eager_for_every_handle(handle: Any) -> None:
    with pytest.raises(WyrdError) as captured:
        handle.connect()

    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


@pytest.mark.usefixtures("no_credentials")
@pytest.mark.parametrize(("handle", "first_use"), FIRST_USE)
def test_constructors_are_io_free(handle: type, first_use: Callable[[Any], object]) -> None:
    constructed = handle()

    with pytest.raises(WyrdError) as captured:
        first_use(constructed)

    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


@pytest.mark.usefixtures("no_credentials")
def test_async_bifrost_constructor_does_not_block() -> None:
    async def scenario() -> None:
        AsyncBifrost()
        with pytest.raises(WyrdError) as captured:
            await AsyncBifrost.connect()
        assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"

    asyncio.run(scenario())
