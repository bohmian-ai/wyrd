"""Wyrd tenant-admin SDK client.

Ergonomic wrapper over the native ``_WyrdClient``. Exposes the
``client.admin.trusted_issuers`` and ``client.admin.workload_bindings``
namespaces, each round-tripping plain JSON dicts to the ``/admin/*`` routes.
Conflicts and missing records surface as ``WyrdError`` carrying the server's
structured codes (``WYRD_AUTH_409_ADMIN_CONFLICT`` /
``WYRD_AUTH_404_ADMIN_NOT_FOUND``). ``trusted_issuers.create`` sends the client
secret in the request body; it is never returned in any view.
"""

from __future__ import annotations

import builtins
from typing import Literal, TypedDict

from .._wyrd.client import _WyrdClient

ClientAuth = Literal["SecretBasic", "SecretPost", "PrivateKeyJwt", "Public"]
PrincipalKind = Literal["Human", "Workload"]


class ClaimMapping(TypedDict, total=False):
    """Claim-path to principal mapping. ``subject`` is required."""

    subject: str
    email: str
    groups: str


class CardRefPayload(TypedDict, total=False):
    """Card reference a workload binding acts as. ``uid`` is optional."""

    kind: str
    name: str
    version: str
    space: str
    uid: str


class TrustedIssuerRequest(TypedDict, total=False):
    """``trusted_issuers.create`` body. ``client_secret`` is sent, never returned."""

    issuer: str
    expected_audience: str
    client_id: str
    client_auth: ClientAuth
    client_secret: str
    claim_mapping: ClaimMapping
    group_role_map: dict[str, list[str]]
    default_roles: list[str]
    principal_kind: PrincipalKind
    jwks_ttl_secs: int


class TrustedIssuerView(TypedDict):
    """Redacted issuer projection. Never carries the client secret."""

    issuer: str
    jwks_uri: str
    expected_audience: str
    client_id: str
    client_auth: str
    principal_kind: str
    jwks_ttl_secs: int
    claim_mapping: dict[str, object]
    group_role_map: dict[str, object]
    default_roles: list[object]


class WorkloadBindingRequest(TypedDict, total=False):
    """``workload_bindings.create`` body. ``audience`` is optional."""

    issuer: str
    subject: str
    audience: str
    card_ref: CardRefPayload


class WorkloadBindingView(TypedDict):
    """Workload-binding projection."""

    issuer: str
    subject: str
    audience: str | None
    card_ref: dict[str, object]


class _TrustedIssuers:
    """The ``client.admin.trusted_issuers`` namespace."""

    def __init__(self, native: _WyrdClient) -> None:
        self._native = native

    def create(self, request: TrustedIssuerRequest) -> TrustedIssuerView:
        """Register a trusted OIDC issuer and return its redacted view."""
        return self._native.create_trusted_issuer(request)

    def list(self) -> builtins.list[TrustedIssuerView]:
        """List every trusted OIDC issuer for the caller's tenant."""
        return self._native.list_trusted_issuers()

    def delete(self, issuer: str, *, cascade: bool = False) -> None:
        """Remove a trusted OIDC issuer; ``cascade`` also drops its bindings."""
        self._native.delete_trusted_issuer(issuer, cascade)


class _WorkloadBindings:
    """The ``client.admin.workload_bindings`` namespace."""

    def __init__(self, native: _WyrdClient) -> None:
        self._native = native

    def create(self, request: WorkloadBindingRequest) -> WorkloadBindingView:
        """Register a workload binding and return its view."""
        return self._native.create_workload_binding(request)

    def list(
        self, *, issuer: str | None = None, subject: str | None = None
    ) -> builtins.list[WorkloadBindingView]:
        """List workload bindings, optionally filtered by issuer and subject."""
        return self._native.list_workload_bindings(issuer, subject)

    def delete(self, issuer: str, subject: str) -> None:
        """Remove the workload binding for ``issuer`` and ``subject``."""
        self._native.delete_workload_binding(issuer, subject)


class _Admin:
    """The ``client.admin`` namespace."""

    def __init__(self, native: _WyrdClient) -> None:
        self.trusted_issuers = _TrustedIssuers(native)
        self.workload_bindings = _WorkloadBindings(native)


class WyrdClient:
    """Tenant-admin SDK client over the Wyrd ``/admin/*`` HTTP surface."""

    def __init__(self, base_url: str | None = None, api_key: str | None = None) -> None:
        self._native = _WyrdClient(base_url, api_key)
        self.admin = _Admin(self._native)


__all__ = [
    "CardRefPayload",
    "ClaimMapping",
    "ClientAuth",
    "PrincipalKind",
    "TrustedIssuerRequest",
    "TrustedIssuerView",
    "WorkloadBindingRequest",
    "WorkloadBindingView",
    "WyrdClient",
]
