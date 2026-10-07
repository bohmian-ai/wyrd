# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
import os

class WyrdTestServer:
    """Wyrd in-process test server context manager (testing feature only).

    Journeys steer the server through exactly three test controls, each backed
    by the production code path: ``flush_bifrost``, ``wait_for_baseline``, and
    ``make_binding_due``. The remaining members start, address, or credential
    the server.

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
    The same ``/v1`` upstream also serves the verification runtime's ``OpenAI``
    LLM judge, so a continuous Eval journey's judge calls
    ``<root>/v1/chat/completions``.

    ``live_providers`` instead points every built-in adapter at its real
    provider endpoint, which only the opt-in live smoke lane asks for. It
    cannot be combined with ``provider_base_url``.

    ``human_sso=True`` serves the public origin the identity lane's Keycloak
    clients register, so saved user login journeys can sign in against it.
    """

    def __init__(
        self,
        cleanup: bool = True,
        mutate_env: bool = True,
        audit_publication: bool = True,
        verification_runtime: bool = False,
        provider_base_url: str | None = None,
        live_providers: bool = False,
        human_sso: bool = False,
    ) -> None:
        """Create an unstarted server; entering the ``with`` block boots it.

        Args:
            cleanup: reserved for a future teardown-skip feature and currently
                ignored; the server and embedded Postgres are always cleaned up.
            mutate_env: when true, entering sets ``WYRD_SERVER_URL``,
                ``WYRD_GRPC_URL``, and ``WYRD_API_KEY`` for the block and
                exiting restores the original values (or removes them). Pass
                false for parallel suites that manage these variables.
            audit_publication: false keeps the audit publisher from retiring
                staged audit rows, for a journey that counts staged decisions
                such as ``table_describe_count()``.
            verification_runtime: true composes the verification runtime, so
                Drift baselines fit and verification runs execute in the
                background.
            provider_base_url: an absolute URL that every built-in gateway
                adapter targets as a local mock upstream (``OpenAI`` under
                ``/v1``); operator credential bindings read
                ``WYRD_TEST_GATEWAY_PROVIDER_KEY``. The same ``/v1`` upstream
                serves the verification runtime's ``OpenAI`` LLM judge at
                ``<root>/v1/chat/completions``. Omitted, the gateway reaches no
                provider unless ``live_providers`` is set.
            live_providers: true points every built-in adapter at its real
                provider endpoint, for the opt-in live smoke lane only.
            human_sso: true serves the public origin the identity lane's
                Keycloak clients register, so saved user login journeys can
                sign in against it.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` when
                ``provider_base_url`` is not an absolute URL or is combined
                with ``live_providers``.

        """
        ...

    def access_token(self) -> str:
        """Exchange the harness API key for a Wyrd access token.

        Lets a test hand an unmodified OTLP exporter or provider SDK the token
        it needs.
        """
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
        """The ``admin`` service API key bootstrapped on entry."""
        ...

    @property
    def tenant_id(self) -> str:
        """Fixture tenant UUID string."""
        ...

    def bootstrap_service(self, roles: list[str], name: str = "svc") -> str:
        """Mint a fixture-tenant service principal and return its API key.

        Args:
            roles: Wyrd built-in role names (e.g. ``"bifrost_write"``). An
                empty list creates a principal with no grants, for negative
                RBAC journeys.
            name: the service principal's name.

        """
        ...

    def credential_registered_service(self, card_ref: str, roles: list[str]) -> str:
        """Issue an API key for the principal a registered Service Card projects.

        Nothing is minted: registering the Service already created its
        principal, so the key carries that Service's real card-ref scope and a
        run may observe the component Cards its spec references.

        Args:
            card_ref: the canonical ``space/Kind/name@version`` identity of a
                Service already registered in the fixture tenant.
            roles: built-in role names granted to that principal in addition
                to any it already holds.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` for an unparsable
                ``card_ref``; ``WYRD_TESTING_500_HARNESS_BOOTSTRAP`` when the
                Card has no projected principal.

        """
        ...

    def make_binding_due(self, binding_id: str) -> None:
        """Set a verification binding's next run time to database time.

        The scheduler then finds the binding due without moving a clock.

        Args:
            binding_id: the binding's UUIDv7.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` for an invalid ID or
                a failed update.

        """
        ...

    def wait_for_baseline(self, verifier: str, timeout: float) -> None:
        """Return once Drift Verifier ``verifier``'s fitted baseline is ready.

        ``verifier`` is the Verifier Card UID and ``timeout`` is in seconds.
        Raises ``WyrdError`` with ``WYRD_VERIFICATION_409_BASELINE_NOT_READY``
        when ``timeout`` elapses first; its ``details["baseline"]`` is the last
        observed baseline status, the same value ``card.status.verification``
        serves.
        """
        ...

    def scoped_api_key(self, role: str, permissions: list[str | dict[str, object]]) -> str:
        """Seed ``role`` with exactly ``permissions`` and return a service API key.

        Args:
            role: the role to seed; the service is also named ``role``.
            permissions: each a ``"resource:action"`` string granting every
                object of that operation, or a typed permission dict
                (``resource``, ``action``, ``scope``) naming objects such as one
                gateway model.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` for an unparsable
                permission.

        """
        ...

    def revoke_scoped_role(self, role: str) -> None:
        """Revoke ``role`` from the service ``scoped_api_key`` minted for it.

        Access tokens exchanged afterwards resolve the revoked grant set.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` when
                ``scoped_api_key`` never seeded ``role``.

        """
        ...

    def seed_tenant(self, slug: str) -> str:
        """Provision a second tenant for isolation journeys.

        The tenant is seeded with its built-in roles.

        Args:
            slug: the new tenant's slug.

        Returns:
            The new tenant's UUID string, for ``bootstrap_service_in_tenant()``.

        """
        ...

    def bootstrap_service_in_tenant(
        self, tenant_id: str, roles: list[str], name: str = "svc"
    ) -> str:
        """Mint a service principal under an explicit tenant; return its API key.

        Args:
            tenant_id: a tenant UUID string, such as one ``seed_tenant()``
                returned.
            roles: built-in role names, as for ``bootstrap_service()``.
            name: the service principal's name.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` when ``tenant_id``
                does not parse.

        """
        ...

    def activate_human_sso(self, admin_key: str) -> None:
        """Activate the identity lane's Keycloak sign-in for ``admin_key``'s tenant."""
        ...

    def save_human_login(
        self, config_home: str | os.PathLike[str], tenant: str, username: str, password: str
    ) -> None:
        """Save ``username``'s login to ``tenant`` under ``config_home`` as ``wyrd auth login`` does."""
        ...

    def expire_saved_login(self, config_home: str | os.PathLike[str], tenant: str) -> None:
        """Make the saved login stale, so the next client renews it."""
        ...

    def saved_login_is_stale(self, config_home: str | os.PathLike[str], tenant: str) -> bool:
        """Return whether the saved login's access token has expired."""
        ...

    def revoke_saved_login(self, config_home: str | os.PathLike[str], tenant: str) -> None:
        """Revoke the saved login's refresh chain on the server, keeping the record."""
        ...

__all__ = ["WyrdTestServer"]
