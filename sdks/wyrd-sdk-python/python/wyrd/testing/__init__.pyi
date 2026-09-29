# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
class WyrdTestServer:
    """Wyrd in-process test server context manager (testing feature only).

    Starts a real Wyrd server bound to a loopback TCP socket backed by an
    embedded Postgres fixture. Use as a context manager; ``bootstrap_service``
    is only valid inside the ``with`` block.

    When ``mutate_env=True`` (default), the context manager sets
    ``WYRD_SERVER_URL``, ``WYRD_GRPC_URL``, and ``WYRD_API_KEY`` in the process
    environment for the duration of the block and restores original values (or
    removes them) on exit. Use ``mutate_env=False`` when running parallel test
    suites that manage these vars externally.

    ``cleanup`` is reserved for a future teardown-skip feature; currently ignored
    (the server and embedded Postgres are always cleaned up on exit).

    ``audit_publication=False`` keeps the server's audit publisher from retiring
    staged audit rows, for a journey that counts staged decisions.

    ``verification_runtime=True`` composes the verification runtime, so Drift
    baselines fit and verification runs execute in the background.
    ``provider_base_url``, when set, makes the gateway dispatch over HTTP with
    every built-in adapter pointed at that local mock upstream (``OpenAI`` under
    ``/v1``); operator credential bindings read ``WYRD_TEST_GATEWAY_PROVIDER_KEY``.

    ``live_providers`` instead points every built-in adapter at its real
    provider endpoint, which only the opt-in live smoke lane asks for. It
    cannot be combined with ``provider_base_url``.
    """

    def __init__(
        self,
        cleanup: bool = True,
        mutate_env: bool = True,
        audit_publication: bool = True,
        verification_runtime: bool = False,
        provider_base_url: str | None = None,
        live_providers: bool = False,
    ) -> None: ...
    def access_token(self) -> str:
        """Exchange the harness API key for a Wyrd access token."""
        ...

    def flush_bifrost(self) -> None:
        """Flush the server-owned Scribe so published rows become queryable.

        Bounded: a Scribe that cannot settle its staged rows raises instead of
        blocking the interpreter thread forever.
        """
        ...
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

    def bootstrap_service(self, roles: list[str], name: str = "svc") -> str:
        """Mint a service principal with ``roles`` and return its API key.

        ``roles``: Wyrd built-in role names (e.g. ``"bifrost_write"``).
        An empty list creates a principal with no grants (useful for negative RBAC
        journeys). Must be called inside the context manager.
        """
        ...

    def credential_registered_service(self, card_ref: str, roles: list[str]) -> str:
        """Issue an API key for the principal a registered Service Card projects.

        ``card_ref`` is the canonical ``space/Kind/name@version`` identity of an
        already-registered Service. The key carries that Service's real card-ref
        scope, so a run may observe the component Cards its spec references.
        """
        ...

    def make_binding_due(self, binding_id: str) -> None: ...
    def verification_runs(self) -> list[str]: ...
    def retire_fitted_format(self, verifier_uid: str) -> None: ...
    def table_describe_count(self, fqn: str) -> int: ...
    def fail_table_describe(self, fqn: str) -> None: ...
    def restore_table_describe(self) -> None: ...
    def scoped_api_key(self, role: str, permissions: list[str | dict[str, object]]) -> str:
        """Seed ``role`` with exactly ``permissions`` and return a service API key.

        Each permission is a ``"resource:action"`` string or a typed permission
        dict (``resource``, ``action``, ``scope``) naming objects such as one
        gateway model.
        """
        ...

    def revoke_scoped_role(self, role: str) -> None:
        """Revoke ``role`` from the service ``scoped_api_key`` minted for it."""
        ...

    def seed_tenant(self, slug: str) -> str:
        """Provision a second tenant for isolation journeys."""
        ...

    def bootstrap_service_in_tenant(
        self, tenant_id: str, roles: list[str], name: str = "svc"
    ) -> str:
        """Mint a service principal under an explicit tenant."""
        ...

__all__ = ["WyrdTestServer"]
