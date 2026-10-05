#### begin imports ####
from collections.abc import Mapping
from typing import Any

#### end of imports ####

class Verification:
    """Tenant-scoped Verification control-plane handle.

    Every call blocks with the GIL released until the server answers. A started
    run is durably enqueued, not finished; poll ``get_run``. A direct
    execution returns its judgment and enqueues nothing.
    """

    def __init__(
        self,
        server_url: str | None = None,
        credential: str | None = None,
    ) -> None:
        """Build a handle; no network call happens here.

        Args:
            server_url: the Wyrd server URL. Resolved from ``WYRD_SERVER_URL``
                and then ``http://localhost:8080`` if omitted.
            credential: the API key or bearer token. Resolved through
                ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
                ``WYRD_API_KEY`` → this server's saved ``wyrd auth login`` →
                ``~/.config/wyrd/credentials.toml``
                ``[default].api_key`` if omitted.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves, or a client configuration error.

        """
        ...

    def get_binding(self, binding_id: str) -> dict[str, Any]:
        """Read one binding's identities, activity, readiness, and cursor.

        Returns the ``VerificationBindingStatus`` wire object: ``binding_id``,
        ``owner_card_uid``, ``subject_card_uid``, ``verifier_uid``, ``active``,
        ``readiness``, ``next_run_at``, ``last_activated_at``, and ``last_run_id``.

        Args:
            binding_id: a UUIDv7 from a Card's
                ``status.verification.binding_ids``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``binding_id`` is not
                a UUIDv7,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``cards:read``, and
                ``WYRD_VERIFICATION_404_BINDING_NOT_FOUND`` for an unknown
                binding.

        """
        ...

    def start_run(self, request: Mapping[str, Any], idempotency_key: str | None = None) -> str:
        """Durably enqueue one manual Drift run and return its run ID.

        ``request`` is the ``StartVerificationRunRequest`` wire object: a
        ``target`` of ``{"kind": "binding", "binding_id": ...}`` or
        ``{"kind": "verifier", "verifier_uid": ..., "subject_card_uid": ...}``
        and an ``input`` of ``{"kind": "drift_window", "start": ..., "end": ...}``.

        Args:
            request: the run target and window.
            idempotency_key: a caller-chosen key, so a retry from any process
                with the same key and request returns the same run ID. Omitted,
                the client mints one per call, which still deduplicates its own
                transport retries.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a malformed request,
                ``WYRD_VERIFICATION_400_INVALID_WINDOW`` or
                ``WYRD_VERIFICATION_400_INVALID_TARGET`` for an unusable run,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``evals:run`` or
                subject scope, ``WYRD_VERIFICATION_404_BINDING_NOT_FOUND``,
                ``WYRD_VERIFICATION_409_VERIFIER_NOT_READY``, and
                ``WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`` when the key was used
                for a different request.

        """
        ...

    def execute(self, request: Mapping[str, Any]) -> dict[str, Any]:
        """Judge supplied input with one exact Verifier and return its judgment.

        ``request`` is the ``ExecuteVerificationRequest`` wire object:
        ``verifier_uid``, ``subject_card_uid``, and an ``input`` of
        ``{"kind": "drift_samples", "columns": {name: [value | None, ...]}}``
        or ``{"kind": "eval_record", "context": {...}, "media": [...]}``.
        Returns the ``ExecuteVerificationResponse`` wire object:
        ``execution_id``, exact ``verifier`` and ``subject`` references,
        ``kind``, ``verdict``, ``summary``, ``counts``, and ``detail``. A
        ``failed`` verdict is a successful return; nothing is enqueued,
        published, or dispatched, and the request is never replayed.

        Args:
            request: the Verifier, subject, and input to judge.

        Raises:
            WyrdError: ``WYRD_VERIFICATION_400_INPUT_INVALID`` for a malformed
                request, ``WYRD_VERIFICATION_413_INPUT_TOO_LARGE`` for an
                exceeded bound, ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``evals:run`` or subject scope,
                ``WYRD_VERIFICATION_404_TARGET_NOT_FOUND`` for an unknown
                Card, ``WYRD_VERIFICATION_409_BASELINE_NOT_READY`` or
                ``WYRD_VERIFICATION_409_BASELINE_LEGACY`` for an unusable
                baseline, ``WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE`` or
                ``WYRD_VERIFICATION_422_INPUT_UNSUPPORTED`` for input the
                Verifier cannot judge, ``WYRD_VERIFICATION_502_DEPENDENCY_FAILED``
                when the judge provider fails, and
                ``WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT`` past the deadline.

        """
        ...

    def get_run(self, run_id: str) -> dict[str, Any]:
        """Read one run's status, requester, result pointer, and dispatches.

        Returns the ``VerificationRunStatus`` wire object: ``run_id``,
        ``status``, ``requested_by_principal_id``, ``result_id``, ``error``,
        and ``dispatches``.

        Args:
            run_id: the UUIDv7 ``start_run()`` returned.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``run_id`` is not a
                UUIDv7,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``cards:read``, and
                ``WYRD_VERIFICATION_404_RUN_NOT_FOUND`` for an unknown run.

        """
        ...

__all__ = ["Verification"]
