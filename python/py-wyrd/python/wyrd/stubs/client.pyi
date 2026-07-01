"""Source stub for wyrd.client."""

#### begin imports ####
import builtins
from typing import Literal, TypedDict

#### end of imports ####

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

    def create(self, request: TrustedIssuerRequest) -> TrustedIssuerView:
        """Register a trusted OIDC issuer and return its redacted view."""
        ...

    def list(self) -> builtins.list[TrustedIssuerView]:
        """List every trusted OIDC issuer for the caller's tenant."""
        ...

    def delete(self, issuer: str, *, cascade: bool = False) -> None:
        """Remove a trusted OIDC issuer; ``cascade`` also drops its bindings."""
        ...

class _WorkloadBindings:
    """The ``client.admin.workload_bindings`` namespace."""

    def create(self, request: WorkloadBindingRequest) -> WorkloadBindingView:
        """Register a workload binding and return its view."""
        ...

    def list(
        self, *, issuer: str | None = None, subject: str | None = None
    ) -> builtins.list[WorkloadBindingView]:
        """List workload bindings, optionally filtered by issuer and subject."""
        ...

    def delete(self, issuer: str, subject: str) -> None:
        """Remove the workload binding for ``issuer`` and ``subject``."""
        ...

class _Admin:
    """The ``client.admin`` namespace."""

    trusted_issuers: _TrustedIssuers
    workload_bindings: _WorkloadBindings

class WyrdClient:
    """Tenant-admin SDK client over the Wyrd ``/v1/admin/*`` HTTP surface."""

    admin: _Admin

    def __init__(self, base_url: str | None = None, api_key: str | None = None) -> None:
        """Assemble a client; ``base_url`` and ``api_key`` override ``WYRD_*`` env."""
        ...

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
