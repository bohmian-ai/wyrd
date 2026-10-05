# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
class WyrdTestServer:
    """Wyrd in-process test server context manager (testing feature only).

    Starts a real Wyrd server bound to a loopback TCP socket backed by an
    embedded Postgres fixture and bootstraps an ``admin`` service key for it.
    Every method and property is valid only inside the ``with`` block; outside
    it they raise ``WyrdError`` ``WYRD_TESTING_500_HARNESS_START``. Exiting
    restores published environment variables, shuts the server down, and never
    suppresses an exception raised inside the block.
    """

    def __init__(
        self,
        cleanup: bool = True,
        mutate_env: bool = True,
        audit_publication: bool = True,
        verification_runtime: bool = False,
        provider_base_url: str | None = None,
        live_providers: bool = False,
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

    def verification_runs(self) -> list[str]:
        """Return every verification run ID of the fixture tenant, oldest first."""
        ...

    def retire_fitted_format(self, verifier_uid: str) -> None:
        """Strip ``format`` from a Drift Verifier's ready fitted profile.

        Simulates a profile fitted under earlier semantics, which the engine
        must refuse rather than rescore.

        Args:
            verifier_uid: the Verifier Card's UID.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` for an invalid UID or
                when the Verifier has no ready fitted profile.

        """
        ...

    def table_describe_count(self, fqn: str) -> int:
        """Count the server's staged, allowed describes of table ``fqn``.

        Proves a writer reused its cached schema. Start the server with
        ``audit_publication=False``, or the publisher retires staged rows and
        shrinks the count.

        Args:
            fqn: the ``"<namespace>.<name>"`` table name.

        """
        ...

    def fail_table_describe(self, fqn: str) -> None:
        """Make every fixture-tenant describe of ``fqn`` fail until restored.

        The server then fails closed with ``WYRD_VALA_500_AUDIT_UNAVAILABLE``
        for that one table. Calling again replaces the previous fault.

        Args:
            fqn: the ``"<namespace>.<name>"`` table name.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` when ``fqn`` holds
                anything but ASCII alphanumerics, ``_``, and ``.``.

        """
        ...

    def restore_table_describe(self) -> None:
        """Remove the ``fail_table_describe()`` fault, if any."""
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

    def ensure_builtin_table(self, namespace: str, name: str) -> None:
        """Create the built-in signal table ``namespace.name`` if it is absent.

        OTLP ingest provisions these tables itself; a journey that writes one
        through the public Arrow batch door calls this first.

        Raises:
            WyrdError: when no built-in table has that name or the catalog
                cannot create it.

        """
        ...

    def prepare_oracle_query_fixture(self, fused: bool = False) -> tuple[str, str]:
        """Ingest a real Oracle query fixture and return ``(table, access_token)``.

        Rows go through the public gRPC ingest path and the token comes from
        the real auth route.

        Args:
            fused: true leaves the rows in Scribe's live tier; false (the
                default) flushes them to sealed storage first.

        """
        ...

    def fail_next_query_after_schema(self) -> None:
        """Make the server cut the next query's stream after its schema frame."""
        ...

    def fail_next_query_after_batch(self) -> None:
        """Make the server cut the next query's stream after its first batch frame."""
        ...

    def stall_next_query_after_schema(self) -> None:
        """Make the server hold the next query's stream after its schema frame.

        Pair with ``wait_query_schema_stall()`` to cancel a query at a known
        point.
        """
        ...

    def wait_query_schema_stall(self) -> str:
        """Block until the stalled query reaches its stall; return its query id.

        Raises:
            WyrdError: when no stall is scheduled or the server drain deadline
                passes first.

        """
        ...

    def bifrost_query_resource_snapshot(self, query_id: str) -> dict[str, int]:
        """Return query ``query_id``'s held admission, memory, peer-slot, and tail-fence counts.

        Raises:
            WyrdError: when the server cannot take an exact snapshot.

        """
        ...

    def wait_bifrost_query_resources_released(
        self, query_id: str, baseline: dict[str, int]
    ) -> dict[str, int]:
        """Block until query ``query_id``'s resources return to ``baseline``.

        Args:
            query_id: the query to watch.
            baseline: a ``bifrost_query_resource_snapshot()`` taken before the
                query ran.

        Returns:
            The final snapshot, equal to ``baseline``.

        Raises:
            WyrdError: ``WYRD_TESTING_500_HARNESS_START`` for a malformed
                baseline, or an error when release takes longer than the
                server drain deadline.

        """
        ...

    def query_denied_token(self) -> str:
        """Return an access token whose principal lacks ``bifrost_query:read``."""
        ...

    def bifrost_read_decision_count(self) -> int:
        """Return the fixture tenant's staged Oracle read-decision audit count.

        Call ``wait_oracle_audit_staged()`` first; Oracle stages decisions in
        the background.
        """
        ...

    def wait_oracle_audit_staged(self, budget_ms: int = 5000) -> int:
        """Block until Oracle's in-flight audit commits finish.

        Args:
            budget_ms: the most milliseconds to wait.

        Returns:
            The decisions still pending; ``0`` once every one is staged.

        Raises:
            WyrdError: when this server hosts no Oracle role.

        """
        ...

__all__ = ["WyrdTestServer"]
