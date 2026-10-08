"""Type fixture pinning the closed Operator connection request and view unions.

`ty` checks this module in the `py:typecheck` lane with unused suppressions as
errors, so every valid literal below must type-check and every
``ty: ignore`` line must be a real rejection. The connection TypedDicts are
stub-only, so they are imported for type checking alone.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from wyrd.operators import (
        CreateOperatorConnectionRequest,
        OperatorConnectionView,
        UpdateOperatorConnectionRequest,
    )


def every_provider_variant_is_accepted() -> tuple[object, ...]:
    creates: list[CreateOperatorConnectionRequest] = [
        {"provider": "slack", "name": "s", "workspace_id": "T1", "bot_token": "x"},
        {"provider": "pager_duty", "name": "p", "integration_key": "x"},
        {
            "provider": "http",
            "name": "h",
            "origin": "https://hooks.example.com",
            "auth": {"scheme": "bearer", "token": "x"},
        },
        {
            "provider": "http",
            "name": "h",
            "origin": "https://hooks.example.com",
            "auth": {"scheme": "basic", "username": "u", "password": "p"},
        },
        {
            "provider": "http",
            "name": "h",
            "origin": "https://hooks.example.com",
            "auth": {"scheme": "header", "name": "X-Api-Key", "value": "x"},
        },
    ]
    updates: list[UpdateOperatorConnectionRequest] = [
        {"provider": "slack"},
        {"provider": "slack", "workspace_id": "T2", "bot_token": "x", "status": "disabled"},
        {"provider": "pager_duty", "integration_key": "x", "status": "active"},
        {"provider": "http", "origin": "https://hooks.example.com"},
        {"provider": "http", "auth": {"scheme": "bearer", "token": "x"}},
    ]
    stamp = "2026-07-01T00:00:00Z"
    views: list[OperatorConnectionView] = [
        {
            "connection_id": "id",
            "name": "s",
            "status": "active",
            "created_at": stamp,
            "updated_at": stamp,
            "provider": "slack",
            "workspace_id": "T1",
        },
        {
            "connection_id": "id",
            "name": "p",
            "status": "disabled",
            "created_at": stamp,
            "updated_at": stamp,
            "provider": "pager_duty",
        },
        {
            "connection_id": "id",
            "name": "h",
            "status": "active",
            "created_at": stamp,
            "updated_at": stamp,
            "provider": "http",
            "origin": "https://hooks.example.com",
            "auth": {"scheme": "header", "name": "X-Api-Key"},
        },
    ]
    return (creates, updates, views)


def invalid_provider_combinations_are_rejected() -> list[object]:
    missing_workspace: CreateOperatorConnectionRequest = {
        "provider": "slack",
        "name": "s",
        "bot_token": "x",
    }  # ty: ignore[invalid-assignment]
    foreign_field: CreateOperatorConnectionRequest = {
        "provider": "pager_duty",
        "name": "p",
        "integration_key": "x",
        "bot_token": "x",
    }  # ty: ignore[invalid-assignment]
    partial_basic: CreateOperatorConnectionRequest = {
        "provider": "http",
        "name": "h",
        "origin": "o",
        "auth": {"scheme": "basic", "username": "u"},
    }  # ty: ignore[invalid-assignment]
    unknown_provider: CreateOperatorConnectionRequest = {"provider": "email", "name": "e"}  # ty: ignore[invalid-assignment]
    renamed: UpdateOperatorConnectionRequest = {"provider": "slack", "name": "renamed"}  # ty: ignore[invalid-assignment]
    crossed: UpdateOperatorConnectionRequest = {
        "provider": "slack",
        "origin": "https://hooks.example.com",
    }  # ty: ignore[invalid-assignment]
    pager_workspace: UpdateOperatorConnectionRequest = {
        "provider": "pager_duty",
        "workspace_id": "T1",
    }  # ty: ignore[invalid-assignment]
    return [
        missing_workspace,
        foreign_field,
        partial_basic,
        unknown_provider,
        renamed,
        crossed,
        pager_workspace,
    ]


def nested_config_is_absent(view: OperatorConnectionView) -> object:
    """Views flatten provider config, so a nested ``config`` key does not exist."""
    return view["config"]  # ty: ignore[invalid-key]


def workspace_requires_narrowing(view: OperatorConnectionView) -> object:
    """``workspace_id`` is only reachable after narrowing to Slack."""
    return view["workspace_id"]  # ty: ignore[invalid-key]


def workspace_is_reachable_after_narrowing(view: OperatorConnectionView) -> object:
    """Narrowing to Slack makes ``workspace_id`` a known key."""
    if view["provider"] == "slack":
        return view["workspace_id"]
    return None
