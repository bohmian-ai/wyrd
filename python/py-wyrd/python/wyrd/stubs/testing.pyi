class WyrdTestServer:
    """Wyrd in-process test server context manager (testing feature only).

    Starts a real Wyrd server bound to a loopback TCP socket backed by an
    embedded Postgres fixture. Use as a context manager; ``bootstrap_service``
    is only valid inside the ``with`` block.
    """

    def __init__(self, cleanup: bool = True, mutate_env: bool = True) -> None: ...
    def __enter__(self) -> WyrdTestServer: ...
    def __exit__(self, exc_type: object, exc_value: object, traceback: object) -> bool: ...
    @property
    def base_url(self) -> str:
        """Base HTTP URL of the bound server (``http://127.0.0.1:<port>``)."""
        ...

    @property
    def api_key(self) -> str:
        """Bootstrap API key seeded at startup."""
        ...

    @property
    def tenant_id(self) -> str:
        """Fixture tenant UUID string."""
        ...

    def bootstrap_service(self, permissions: list[str], name: str = "svc") -> str:
        """Mint a service principal with ``permissions`` roles and return its API key.

        ``permissions`` maps to Wyrd built-in role names (e.g. ``"bifrost_write"``).
        An empty list creates a principal with no grants (useful for negative RBAC
        journeys). Must be called inside the context manager.
        """
        ...

__all__ = ["WyrdTestServer"]
